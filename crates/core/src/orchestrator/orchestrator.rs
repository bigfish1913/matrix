//! Orchestrator - main coordination engine.

use crate::agent::SharedAgentPool;
use crate::config::{MAX_DEPTH, MAX_RETRIES};
use crate::error::{Error, Result};
use crate::executor::{ExecutorConfig, TaskExecutor};
use crate::models::{Complexity, Task, TaskStatus};
use crate::store::TaskStore;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

/// Orchestrator configuration
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub goal: String,
    pub workspace: PathBuf,
    pub tasks_dir: PathBuf,
    pub doc_content: Option<String>,
    pub mcp_config: Option<PathBuf>,
    pub num_agents: usize,
    pub debug_mode: bool,
    pub ask_mode: bool,
    pub resume: bool,
}

impl OrchestratorConfig {
    pub fn new(goal: String, workspace: PathBuf, tasks_dir: PathBuf) -> Self {
        Self {
            goal,
            workspace,
            tasks_dir,
            doc_content: None,
            mcp_config: None,
            num_agents: 1,
            debug_mode: false,
            ask_mode: false,
            resume: false,
        }
    }
}

/// Main orchestrator
pub struct Orchestrator {
    config: OrchestratorConfig,
    store: Arc<TaskStore>,
    agent_pool: SharedAgentPool,
    executor: Arc<TaskExecutor>,
    start_time: Option<Instant>,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub async fn new(config: OrchestratorConfig) -> Result<Self> {
        // Create directories
        fs::create_dir_all(&config.workspace).await?;
        fs::create_dir_all(&config.tasks_dir).await?;

        // Initialize components
        let store = Arc::new(TaskStore::new(config.tasks_dir.clone()).await?);
        let agent_pool = SharedAgentPool::new();

        let executor_config = ExecutorConfig {
            doc_content: config.doc_content.clone(),
            mcp_config: config.mcp_config.clone(),
            debug_mode: config.debug_mode,
            ..Default::default()
        };

        let executor = Arc::new(TaskExecutor::new(
            config.workspace.clone(),
            store.clone(),
            agent_pool.clone(),
            executor_config,
        ));

        Ok(Self {
            config,
            store,
            agent_pool,
            executor,
            start_time: None,
        })
    }

    /// Run the orchestrator
    pub async fn run(&mut self) -> Result<()> {
        self.start_time = Some(Instant::now());
        self.print_header();

        // Ensure .gitignore exists
        self.ensure_gitignore().await?;

        // Phase 0: Interactive clarification (optional)
        let clarification = if self.config.ask_mode && !self.config.resume {
            self.clarify_goal().await?
        } else {
            String::new()
        };

        // Phase 1: Generate or resume tasks
        if self.config.resume && self.store.total().await? > 0 {
            self.resume_tasks().await?;
        } else if !self.config.resume && self.store.total().await? > 0 {
            self.resume_tasks().await?;
        } else {
            self.generate_tasks(&clarification).await?;
        }

        // Show progress
        self.print_progress().await;

        // Run dispatcher
        self.run_dispatcher().await?;

        // Phase 5: Final tests
        self.run_final_tests().await?;

        // Final summary
        self.print_summary().await?;

        Ok(())
    }

    async fn ensure_gitignore(&self) -> Result<()> {
        let gitignore = self.config.workspace.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, DEFAULT_GITIGNORE).await?;
        }
        Ok(())
    }

    /// Phase 0: Interactive clarification
    async fn clarify_goal(&self) -> Result<String> {
        info!("Generating clarifying questions...");

        let prompt = format!(
            r#"You are helping plan a software development project.

GOAL: {}
{}

Generate 3-5 concise, targeted clarifying questions.

Respond ONLY with JSON:
{{"questions": ["Question 1?", "Question 2?", "Question 3?"]}}"#,
            self.config.goal,
            self.config.doc_content.as_ref().map(|d| format!("DOCUMENT:\n{}", d)).unwrap_or_default()
        );

        let result = self.executor.runner.call(
            &prompt,
            &self.config.workspace,
            Some(120),
            None,
            None,
        ).await?;

        if result.is_error {
            warn!("Could not generate clarifying questions");
            return Ok(String::new());
        }

        let questions: ClarificationResponse = match serde_json::from_str(&result.text) {
            Ok(q) => q,
            Err(_) => return Ok(String::new()),
        };

        println!("\n[?] Clarifying Questions\n");

        let mut answers: Vec<String> = Vec::new();
        for (i, question) in questions.questions.iter().enumerate() {
            println!("  [{}] {} (press Enter to skip)", i + 1, question);
            answers.push(format!("Q{}: {}", i + 1, question));
        }

        if answers.is_empty() {
            return Ok(String::new());
        }

        Ok(answers.join("\n"))
    }

    async fn resume_tasks(&self) -> Result<()> {
        let total = self.store.total().await?;
        let completed = self.store.count(TaskStatus::Completed).await?;
        let failed = self.store.count(TaskStatus::Failed).await?;
        let pending = self.store.count(TaskStatus::Pending).await?;

        info!(total, completed, failed, pending, "Resuming tasks");

        // Reset stuck in_progress tasks
        let tasks = self.store.all_tasks().await?;
        for mut task in tasks {
            if task.status == TaskStatus::InProgress {
                task.status = TaskStatus::Pending;
                self.store.save_task(&task).await?;
                info!(task_id = %task.id, "Reset stuck task");
            }
        }

        Ok(())
    }

    async fn generate_tasks(&self, _clarification: &str) -> Result<()> {
        info!(goal = %self.config.goal, "Generating task list");

        let prompt = format!(
            r#"You are a software project planner. Break down the following goal into tasks.

PROJECT GOAL: {}

Generate 3-10 tasks. Respond ONLY with JSON:
{{"tasks": [{{"id": "task-001", "title": "...", "description": "...", "depends_on": []}}]}}"#,
            self.config.goal
        );

        let result = self.executor.runner.call(
            &prompt,
            &self.config.workspace,
            Some(120),
            None,
            None,
        ).await?;

        if result.is_error {
            error!(error = %result.text, "Failed to generate tasks");
            return Ok(());
        }

        // Parse tasks
        if let Ok(tasks_response) = serde_json::from_str::<TasksResponse>(&result.text) {
            info!(count = tasks_response.tasks.len(), "Tasks generated");

            for t in tasks_response.tasks {
                let mut task = Task::new(t.id, t.title, t.description);
                task.depends_on = t.depends_on;
                self.store.save_task(&task).await?;
            }

            self.store.save_manifest(&self.config.goal).await?;
        }

        Ok(())
    }

    /// Phase 2: Assess complexity and split if needed
    pub async fn assess_and_split(&self, task: &mut Task) -> Result<bool> {
        if task.depth >= MAX_DEPTH {
            task.complexity = Complexity::Simple;
            self.store.save_task(task).await?;
            return Ok(true);
        }

        info!(task_id = %task.id, "Assessing complexity");

        let prompt = format!(
            r#"You are a senior software engineer evaluating a development task.

TASK: {}
DESCRIPTION: {}
CURRENT DEPTH: {} / {}

Is this task SIMPLE (doable in one claude session) or COMPLEX (needs splitting)?

Respond ONLY with JSON:
{{"split": false, "reason": "..."}}
OR if splitting needed:
{{"split": true, "reason": "...", "subtasks": [{{"title": "...", "description": "..."}}]}}"#,
            task.title,
            task.description,
            task.depth,
            MAX_DEPTH
        );

        let result = self.executor.runner.call(
            &prompt,
            &self.config.workspace,
            Some(120),
            None,
            None,
        ).await?;

        let data: AssessResponse = match serde_json::from_str(&result.text) {
            Ok(d) => d,
            Err(_) => {
                task.complexity = Complexity::Simple;
                self.store.save_task(task).await?;
                return Ok(true);
            }
        };

        if data.split {
            let subtasks = data.subtasks.unwrap_or_default();
            info!(task_id = %task.id, count = subtasks.len(), "Splitting into subtasks");

            for (i, sub) in subtasks.into_iter().enumerate() {
                let sub_id = format!("{}-{}", task.id, i + 1);
                let subtask = Task::subtask(
                    sub_id,
                    sub.title,
                    sub.description,
                    task.id.clone(),
                    task.depth + 1,
                );
                self.store.save_task(&subtask).await?;
                info!(subtask_id = %subtask.id, "Subtask created");
            }

            task.status = TaskStatus::Skipped;
            task.complexity = Complexity::Complex;
            self.store.save_task(task).await?;
            self.store.save_manifest(&self.config.goal).await?;

            return Ok(false);
        }

        task.complexity = Complexity::Simple;
        self.store.save_task(task).await?;
        Ok(true)
    }

    /// Phase 4: Git commit for completed task
    pub async fn git_commit_task(&self, task: &Task) -> Result<()> {
        if task.modified_files.is_empty() {
            debug!(task_id = %task.id, "No files to commit");
            return Ok(());
        }

        info!(task_id = %task.id, files = ?task.modified_files, "Committing changes");

        // Check if git is initialized
        let git_dir = self.config.workspace.join(".git");
        if !git_dir.exists() {
            let output = tokio::process::Command::new("git")
                .args(["init"])
                .current_dir(&self.config.workspace)
                .output()
                .await
                .map_err(|e| Error::Git(format!("Failed to init git: {}", e)))?;

            if !output.status.success() {
                warn!("Git init failed, skipping commit");
                return Ok(());
            }
        }

        // Stage files
        for file in &task.modified_files {
            let _ = tokio::process::Command::new("git")
                .args(["add", file])
                .current_dir(&self.config.workspace)
                .output()
                .await;
        }

        // Create commit
        let message = format!("[{}] {}", task.id, task.title);
        let output = tokio::process::Command::new("git")
            .args(["commit", "-m", &message])
            .current_dir(&self.config.workspace)
            .output()
            .await
            .map_err(|e| Error::Git(format!("Failed to commit: {}", e)))?;

        if output.status.success() {
            info!(task_id = %task.id, "Changes committed");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(stderr = %stderr, "Commit had issues");
        }

        Ok(())
    }

    /// Phase 5: Final tests
    async fn run_final_tests(&self) -> Result<()> {
        info!("Running final tests...");

        let runner = match crate::detector::TestRunnerDetector::detect(&self.config.workspace) {
            Some(r) => r,
            None => {
                info!("No test runner detected, skipping final tests");
                return Ok(());
            }
        };

        let output = tokio::process::Command::new(&runner.command[0])
            .args(&runner.command[1..])
            .current_dir(&self.config.workspace)
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);

                if output.status.success() {
                    info!("Final tests passed");
                    println!("\nFinal tests passed\n");
                } else {
                    warn!("Final tests failed");
                    println!("\nFinal tests failed:\n{}\n", stdout);
                }
            }
            Err(e) => {
                warn!(error = %e, "Final tests could not run");
            }
        }

        Ok(())
    }

    async fn run_dispatcher(&self) -> Result<()> {
        let primary_slots = (self.config.num_agents + 1) / 2;
        let subtask_slots = self.config.num_agents.saturating_sub(primary_slots);
        let deadline = Instant::now() + Duration::from_secs(24 * 3600);

        let mut join_set = JoinSet::new();
        let mut dispatched_ids: HashSet<String> = HashSet::new();
        let mut primary_running = 0usize;
        let mut subtask_running = 0usize;

        while Instant::now() < deadline {
            // Collect completed tasks
            while let Some(res) = join_set.try_join_next() {
                match res {
                    Ok((task_id, depth)) => {
                        dispatched_ids.remove(&task_id);
                        if depth == 0 {
                            primary_running = primary_running.saturating_sub(1);
                        } else {
                            subtask_running = subtask_running.saturating_sub(1);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Task pipeline error");
                    }
                }
            }

            self.print_progress().await;

            // Get schedulable tasks
            let completed_ids: HashSet<String> = self.store.all_tasks().await?
                .into_iter()
                .filter(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped)
                .map(|t| t.id)
                .collect();

            let pending: Vec<Task> = self.store.pending_tasks().await?
                .into_iter()
                .filter(|t| !dispatched_ids.contains(&t.id))
                .filter(|t| t.depends_on.iter().all(|dep| completed_ids.contains(dep)))
                .collect();

            let primary_pending: Vec<_> = pending.iter().filter(|t| t.depth == 0).collect();
            let subtask_pending: Vec<_> = pending.iter().filter(|t| t.depth > 0).collect();

            // Dispatch primary tasks
            for task in primary_pending {
                if primary_running >= primary_slots {
                    break;
                }

                let task_id = task.id.clone();
                dispatched_ids.insert(task_id.clone());
                primary_running += 1;

                info!(task_id = %task.id, "[primary] Dispatched");

                let store = self.store.clone();
                let executor = self.executor.clone();
                let task = task.clone();

                join_set.spawn(async move {
                    let _ = run_task_pipeline(store, executor, task).await;
                    (task_id, 0usize)
                });
            }

            // Dispatch subtasks
            for task in subtask_pending {
                if subtask_running >= subtask_slots {
                    break;
                }

                let task_id = task.id.clone();
                dispatched_ids.insert(task_id.clone());
                subtask_running += 1;

                info!(task_id = %task.id, "[subtask] Dispatched");

                let store = self.store.clone();
                let executor = self.executor.clone();
                let task = task.clone();

                join_set.spawn(async move {
                    let _ = run_task_pipeline(store, executor, task).await;
                    (task_id, 1usize)
                });
            }

            // Check exit conditions
            if dispatched_ids.is_empty() && self.store.pending_tasks().await?.is_empty() {
                info!("All tasks completed");
                break;
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }

    fn print_header(&self) {
        println!();
        println!("=== MATRIX - Agent Orchestrator ===");
        println!();
        println!("Goal:      {}", self.config.goal);
        println!("Workspace: {}", self.config.workspace.display());
        println!("Agents:    {}", self.config.num_agents);
        println!();
    }

    async fn print_progress(&self) {
        let total = self.store.total().await.unwrap_or(0);
        let completed = self.store.count(TaskStatus::Completed).await.unwrap_or(0);
        let pending = self.store.count(TaskStatus::Pending).await.unwrap_or(0);
        let failed = self.store.count(TaskStatus::Failed).await.unwrap_or(0);

        let elapsed = self.start_time.map(|t| t.elapsed()).unwrap_or_default();
        let elapsed_str = format_duration(elapsed);

        println!();
        println!(
            "Progress: {} completed | {} pending | {} failed / {} total [elapsed {}]",
            completed, pending, failed, total, elapsed_str
        );
        println!();
    }

    async fn print_summary(&self) -> Result<()> {
        let completed = self.store.count(TaskStatus::Completed).await?;
        let failed = self.store.count(TaskStatus::Failed).await?;
        let total = self.store.total().await?;

        println!();
        println!("{}", "=".repeat(45));
        println!("All tasks processed: {}/{} completed, {} failed", completed, total, failed);
        println!("{}", "=".repeat(45));
        println!();

        Ok(())
    }
}

/// Run single task pipeline
async fn run_task_pipeline(
    store: Arc<TaskStore>,
    executor: Arc<TaskExecutor>,
    mut task: Task,
) -> Result<()> {
    let thread_name = format!("thread-{}", task.id);

    // Execute
    let success = executor.execute(&mut task, &thread_name).await?;

    if !success {
        if task.retries < MAX_RETRIES {
            task.retries += 1;
            task.status = TaskStatus::Pending;
            store.save_task(&task).await?;
            warn!(task_id = %task.id, attempt = task.retries, "Retrying");
        } else {
            task.status = TaskStatus::Failed;
            store.save_task(&task).await?;
            error!(task_id = %task.id, "Permanently failed");
        }
        return Ok(());
    }

    // Test
    let (tests_passed, test_output) = executor.test(&mut task).await?;

    if !tests_passed {
        // Try to fix
        let fixed = executor.fix_test_failure(&mut task, &test_output).await?;

        if !fixed && task.retries < MAX_RETRIES {
            task.retries += 1;
            task.status = TaskStatus::Pending;
            task.test_failure_context = Some(test_output);
            store.save_task(&task).await?;
            warn!(task_id = %task.id, attempt = task.retries, "Tests failed, retrying");
            return Ok(());
        } else if !fixed {
            task.status = TaskStatus::Failed;
            task.error = Some(format!("Tests failed: {}", test_output));
            store.save_task(&task).await?;
            error!(task_id = %task.id, "Tests failed permanently");
            return Ok(());
        }
    }

    // Mark completed
    task.status = TaskStatus::Completed;
    store.save_task(&task).await?;
    info!(task_id = %task.id, "Task completed successfully");

    Ok(())
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}h{:02}m{:02}s", h, m, s)
    } else {
        format!("{}m{:02}s", m, s)
    }
}

const DEFAULT_GITIGNORE: &str = r#"
# Dependencies
node_modules/
target/
__pycache__/
*.pyc
.env

# IDE
.vscode/
.idea/

# Build
dist/
build/

# Logs
*.log
logs/
"#;

// Helper types

#[derive(Debug, serde::Deserialize)]
struct TasksResponse {
    tasks: Vec<TaskDefinition>,
}

#[derive(Debug, serde::Deserialize)]
struct TaskDefinition {
    id: String,
    title: String,
    description: String,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ClarificationResponse {
    questions: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AssessResponse {
    split: bool,
    reason: String,
    #[serde(default)]
    subtasks: Option<Vec<SubtaskDef>>,
}

#[derive(Debug, serde::Deserialize)]
struct SubtaskDef {
    title: String,
    description: String,
}