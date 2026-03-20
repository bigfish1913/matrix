//! Orchestrator - main coordination engine.

use crate::agent::SharedAgentPool;
use crate::config::{MAX_DEPTH, MAX_RETRIES};
use crate::error::{Error, Result};
use crate::executor::{ExecutorConfig, TaskExecutor};
use crate::models::{Complexity, Task, TaskStatus};
use crate::store::TaskStore;
use crate::tui::event::AnswerSender;
use crate::tui::{ClarificationQuestion, ConfirmSender, Event, EventSender, ExecutionState};
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
    pub event_sender: Option<EventSender>,
    /// Language for AI prompts (default: "zh" for Chinese)
    pub language: String,
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
            ask_mode: true, // 默认开启 ask 模式
            resume: false,
            event_sender: None,
            language: "zh".to_string(), // 默认使用中文
        }
    }
}

/// Main orchestrator
pub struct Orchestrator {
    config: OrchestratorConfig,
    store: Arc<TaskStore>,
    #[allow(dead_code)]
    agent_pool: SharedAgentPool,
    executor: Arc<TaskExecutor>,
    start_time: Option<Instant>,
    /// Track last progress values to avoid duplicate log spam
    last_progress: (usize, usize, usize, usize), // (completed, pending, failed, total)
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
        ).with_event_sender(config.event_sender.clone()));

        Ok(Self {
            config,
            store,
            agent_pool,
            executor,
            start_time: None,
            last_progress: (0, 0, 0, 0),
        })
    }

    /// Emit an event to the TUI if a sender is configured
    fn emit_event(&self, event: Event) {
        if let Some(ref sender) = self.config.event_sender {
            let _ = sender.send(event);
        }
    }

    /// Run the orchestrator
    pub async fn run(&mut self) -> Result<()> {
        self.start_time = Some(Instant::now());
        self.print_header();

        // Ensure .gitignore exists
        self.ensure_gitignore().await?;

        // Phase 0: Interactive clarification (optional)
        let clarification = if self.config.ask_mode && !self.config.resume {
            // Emit Clarifying state before asking questions
            self.emit_event(Event::ExecutionStateChanged {
                state: ExecutionState::Clarifying,
            });
            self.clarify_goal().await?
        } else {
            String::new()
        };

        // Phase 1: Generate or resume tasks
        let total = self.store.total().await?;
        if total > 0 {
            // Found existing tasks - ask user whether to resume or start fresh
            let should_resume = self.confirm_resume().await?;
            if should_resume {
                self.resume_tasks().await?;
            } else {
                // Clear existing tasks and start fresh
                info!("Clearing existing tasks and starting fresh...");
                self.store.clear().await?;
                // Emit Generating state before task generation
                self.emit_event(Event::ExecutionStateChanged {
                    state: ExecutionState::Generating,
                });
                self.generate_tasks(&clarification).await?;
            }
        } else {
            // No existing tasks - generate new ones
            self.emit_event(Event::ExecutionStateChanged {
                state: ExecutionState::Generating,
            });
            self.generate_tasks(&clarification).await?;
        }

        // Emit Running state after tasks are generated/resumed
        self.emit_event(Event::ExecutionStateChanged {
            state: ExecutionState::Running,
        });

        // Emit model info
        self.emit_event(Event::ModelChanged {
            model: "glm-5".to_string(),
        });

        // Show progress
        self.print_progress().await;

        // Run dispatcher
        self.run_dispatcher().await?;

        // Phase 5: Final tests
        self.run_final_tests().await?;

        // Final summary
        self.print_summary().await?;

        // Emit Completed state
        self.emit_event(Event::ExecutionStateChanged {
            state: ExecutionState::Completed,
        });

        Ok(())
    }

    async fn ensure_gitignore(&self) -> Result<()> {
        let gitignore = self.config.workspace.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, DEFAULT_GITIGNORE).await?;
        }
        Ok(())
    }

    /// Phase 0: Interactive clarification with multiple choice questions
    async fn clarify_goal(&self) -> Result<String> {
        info!("Generating clarifying questions...");

        let lang_instruction = match self.config.language.as_str() {
            "zh" => "请用中文提问，选项也用中文。",
            "en" => "Please ask questions and provide options in English.",
            _ => "请用中文提问，选项也用中文。",
        };

        let prompt = format!(
            r#"You are helping plan a software development project.

GOAL: {}
{}

{}

Generate 3-5 concise, targeted clarifying questions.
For each question, provide 3-4 common options to choose from.

Respond ONLY with JSON array:
[
  {{
    "question": "Question text?",
    "options": ["Option 1", "Option 2", "Option 3"]
  }}
]

Example:
[
  {{"question": "项目使用什么编程语言?", "options": ["Rust", "Python", "JavaScript", "Go"]}},
  {{"question": "是否需要数据库支持?", "options": ["是，需要", "不需要", "不确定"]}}
]"#,
            self.config.goal,
            self.config
                .doc_content
                .as_ref()
                .map(|d| format!("DOCUMENT:\n{}", d))
                .unwrap_or_default(),
            lang_instruction
        );

        let result = self
            .executor
            .runner
            .call(&prompt, &self.config.workspace, Some(120), None, None)
            .await?;

        if result.is_error {
            warn!("Could not generate clarifying questions");
            return Ok(String::new());
        }

        // Parse questions with options
        let raw_questions: Vec<RawQuestion> = match serde_json::from_str(&result.text) {
            Ok(q) => q,
            Err(_) => return Ok(String::new()),
        };

        // Convert to ClarificationQuestion
        let questions: Vec<ClarificationQuestion> = raw_questions
            .into_iter()
            .map(|rq| ClarificationQuestion {
                question: rq.question,
                options: rq.options,
            })
            .collect();

        // Check if in TUI mode
        if let Some(ref sender) = self.config.event_sender {
            // TUI mode: send questions via event channel and wait for answers
            let (tx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();
            let _ = sender.send(Event::ClarificationQuestions {
                questions: questions.clone(),
                response_tx: AnswerSender::new(tx),
            });

            // Wait for answers from TUI (with timeout in case TUI is closed)
            match tokio::time::timeout(tokio::time::Duration::from_secs(300), rx).await {
                Ok(Ok(answers)) => {
                    if answers.is_empty() || answers.iter().all(|a| a.trim().is_empty()) {
                        return Ok(String::new());
                    }
                    let formatted: Vec<String> = questions
                        .iter()
                        .zip(answers.iter())
                        .map(|(q, a)| format!("Q: {}\nA: {}", q.question, if a.is_empty() { "(skipped)" } else { a }))
                        .collect();
                    return Ok(formatted.join("\n\n"));
                }
                Ok(Err(_)) => {
                    warn!("Failed to receive clarification answers from TUI");
                    return Ok(String::new());
                }
                Err(_) => {
                    warn!("Timeout waiting for clarification answers from TUI");
                    return Ok(String::new());
                }
            }
        }

        // Non-TUI mode: use stdin/stdout with simple text input
        println!("\n[?] Clarifying Questions\n");

        let mut answers: Vec<String> = Vec::new();
        for (i, q) in questions.iter().enumerate() {
            use std::io::{self, Write};
            println!("  [{}] {}", i + 1, q.question);
            for (j, opt) in q.options.iter().enumerate() {
                println!("      {}. {}", j + 1, opt);
            }
            print!("      Your choice (1-{} or type custom answer): ", q.options.len());
            io::stdout().flush().unwrap();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_ok() {
                let input = input.trim();
                // Check if it's a number selection
                if let Ok(num) = input.parse::<usize>() {
                    if num > 0 && num <= q.options.len() {
                        answers.push(q.options[num - 1].clone());
                    } else {
                        answers.push(input.to_string());
                    }
                } else if !input.is_empty() {
                    answers.push(input.to_string());
                } else {
                    answers.push(String::new());
                }
            }
        }

        let formatted: Vec<String> = questions
            .iter()
            .zip(answers.iter())
            .map(|(q, a)| format!("Q: {}\nA: {}", q.question, if a.is_empty() { "(skipped)" } else { a }))
            .collect();
        Ok(formatted.join("\n\n"))
    }

    /// Ask user to confirm whether to resume or start fresh
    async fn confirm_resume(&self) -> Result<bool> {
        let completed = self.store.count(TaskStatus::Completed).await?;
        let pending = self.store.count(TaskStatus::Pending).await?;
        let failed = self.store.count(TaskStatus::Failed).await?;

        // Check if in TUI mode
        if let Some(ref sender) = self.config.event_sender {
            // TUI mode: send confirmation request via event channel and wait for response
            let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
            let _ = sender.send(Event::ResumeConfirm {
                completed,
                pending,
                failed,
                response_tx: ConfirmSender::new(tx),
            });

            // Wait for user response (with timeout)
            match tokio::time::timeout(tokio::time::Duration::from_secs(300), rx).await {
                Ok(Ok(confirmed)) => {
                    info!(resume = confirmed, "User chose resume option");
                    return Ok(confirmed);
                }
                Ok(Err(_)) => {
                    warn!("Failed to receive resume confirmation from TUI");
                    return Ok(true);  // Default to resume
                }
                Err(_) => {
                    warn!("Timeout waiting for resume confirmation from TUI");
                    return Ok(true);  // Default to resume
                }
            }
        }

        // Non-TUI mode: use stdin/stdout for confirmation
        use std::io::{self, Write};
        println!("\n[!] Found existing tasks:");
        println!("    Completed: {} | Pending: {} | Failed: {}", completed, pending, failed);
        println!();
        print!("    Resume from existing tasks? [Y/n]: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let input = input.trim().to_lowercase();
            // Default to resume (empty input or 'y')
            return Ok(input.is_empty() || input == "y" || input == "yes");
        }

        Ok(true)  // Default to resume
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

        let lang_instruction = match self.config.language.as_str() {
            "zh" => "请用中文编写任务标题和描述。",
            "en" => "Write task titles and descriptions in English.",
            _ => "请用中文编写任务标题和描述。",
        };

        let prompt = format!(
            r#"You are a software project planner. Break down the following goal into development tasks.

PROJECT GOAL: {}

{}

CRITICAL: You MUST respond with ONLY valid JSON. No explanations, no markdown, just the JSON object.
Do NOT include any text before or after the JSON.

IMPORTANT: Generate as many tasks as needed at the SMALLEST possible granularity.
Each task should be completable in a single coding session (30-60 minutes).
Do NOT limit the number of tasks - use as many as necessary for complete coverage.

Use this EXACT format:
{{"tasks": [{{"id": "task-001", "title": "Short title", "description": "Detailed description", "depends_on": []}}]}}

Example response:
{{"tasks": [{{"id": "task-001", "title": "Setup project", "description": "Initialize project structure", "depends_on": []}}, {{"id": "task-002", "title": "Create models", "description": "Define data models", "depends_on": ["task-001"]}}]}}

Now generate tasks for the project goal above. Output ONLY the JSON object:"#,
            self.config.goal, lang_instruction
        );

        let result = self
            .executor
            .runner
            .call(&prompt, &self.config.workspace, Some(120), None, None)
            .await?;

        if result.is_error {
            error!(error = %result.text, "Failed to generate tasks");
            return Err(Error::ClaudeCli(format!("Task generation failed: {}", result.text)));
        }

        // Log the raw response for debugging
        debug!(response = %result.text, "Claude response for task generation");

        // Try to extract JSON from the response
        let text = result.text.trim();

        // Try to find JSON object in the text
        let json_text = if text.starts_with('{') {
            text.to_string()
        } else if let Some(json) = extract_json_from_text(text) {
            json
        } else {
            error!(response = %text, "No JSON found in response");
            return Err(Error::ParseError(format!("No JSON object found in response: {}", &text[..text.len().min(500)])));
        };

        // Parse tasks
        match serde_json::from_str::<TasksResponse>(&json_text) {
            Ok(tasks_response) => {
                info!(count = tasks_response.tasks.len(), "Tasks generated");

                for t in tasks_response.tasks {
                    let mut task = Task::new(t.id, t.title.clone(), t.description);
                    task.depends_on = t.depends_on;
                    self.store.save_task(&task).await?;

                    // Emit TaskCreated event
                    self.emit_event(Event::TaskCreated {
                        id: task.id.clone(),
                        title: task.title.clone(),
                        parent_id: task.parent_id.clone(),
                        depth: task.depth,
                        depends_on: task.depends_on.clone(),
                    });
                }

                self.store.save_manifest(&self.config.goal).await?;
            }
            Err(e) => {
                error!(error = %e, json = %json_text, "Failed to parse tasks JSON");
                return Err(Error::ParseError(format!("Failed to parse tasks: {}. JSON: {}", e, &json_text[..json_text.len().min(500)])));
            }
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

        let lang_instruction = match self.config.language.as_str() {
            "zh" => "请用中文回复。",
            "en" => "Respond in English.",
            _ => "请用中文回复。",
        };

        let prompt = format!(
            r#"You are a senior software engineer evaluating a development task.

TASK: {}
DESCRIPTION: {}
CURRENT DEPTH: {} / {}

{}

Is this task SIMPLE (completable in 30-60 minutes) or COMPLEX (needs splitting)?

IMPORTANT: If splitting, create as many subtasks as needed at the SMALLEST possible granularity.
Each subtask should be completable in a single coding session (30-60 minutes).
Do NOT limit the number of subtasks.

Respond ONLY with JSON:
{{"split": false, "reason": "..."}}
OR if splitting needed:
{{"split": true, "reason": "...", "subtasks": [{{"title": "...", "description": "..."}}]}}"#,
            task.title, task.description, task.depth, MAX_DEPTH, lang_instruction
        );

        let result = self
            .executor
            .runner
            .call(&prompt, &self.config.workspace, Some(120), None, None)
            .await?;

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
                    if self.config.event_sender.is_none() {
                        println!("\nFinal tests passed\n");
                    }
                } else {
                    warn!("Final tests failed");
                    if self.config.event_sender.is_none() {
                        println!("\nFinal tests failed:\n{}\n", stdout);
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Final tests could not run");
            }
        }

        Ok(())
    }

    async fn run_dispatcher(&mut self) -> Result<()> {
        let primary_slots = self.config.num_agents.div_ceil(2);
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
            let completed_ids: HashSet<String> = self
                .store
                .all_tasks()
                .await?
                .into_iter()
                .filter(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped)
                .map(|t| t.id)
                .collect();

            let pending: Vec<Task> = self
                .store
                .pending_tasks()
                .await?
                .into_iter()
                .filter(|t| !dispatched_ids.contains(&t.id))
                .filter(|t| t.depends_on.iter().all(|dep| completed_ids.contains(dep)))
                .collect();

            // Phase 2: Assess complexity and split if needed
            // Evaluate each task before dispatching to determine if splitting is needed
            let mut tasks_to_dispatch: Vec<Task> = Vec::new();
            for mut task in pending {
                // Skip if already being processed
                if dispatched_ids.contains(&task.id) {
                    continue;
                }
                // Only assess tasks that haven't been assessed yet (complexity is unknown)
                // or tasks that are potentially complex
                if task.complexity == Complexity::Unknown || task.complexity == Complexity::Complex {
                    let should_dispatch = self.assess_and_split(&mut task).await?;
                    if should_dispatch {
                        tasks_to_dispatch.push(task);
                    }
                    // If not should_dispatch, task was split into subtasks, skip it
                } else {
                    // Task already assessed as simple, dispatch it
                    tasks_to_dispatch.push(task);
                }
            }

            let primary_pending: Vec<_> = tasks_to_dispatch.iter().filter(|t| t.depth == 0).collect();
            let subtask_pending: Vec<_> = tasks_to_dispatch.iter().filter(|t| t.depth > 0).collect();

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
                let event_sender = self.config.event_sender.clone();

                join_set.spawn(async move {
                    let _ = run_task_pipeline(store, executor, task, event_sender).await;
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
                let event_sender = self.config.event_sender.clone();

                join_set.spawn(async move {
                    let _ = run_task_pipeline(store, executor, task, event_sender).await;
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
        // In TUI mode, use tracing instead of println to avoid interfering with TUI
        if self.config.event_sender.is_some() {
            info!("=== MATRIX - Agent Orchestrator ===");
            info!("Goal:      {}", self.config.goal);
            info!("Workspace: {}", self.config.workspace.display());
            info!("Agents:    {}", self.config.num_agents);
        } else {
            println!();
            println!("=== MATRIX - Agent Orchestrator ===");
            println!();
            println!("Goal:      {}", self.config.goal);
            println!("Workspace: {}", self.config.workspace.display());
            println!("Agents:    {}", self.config.num_agents);
            println!();
        }
    }

    async fn print_progress(&mut self) {
        let total = self.store.total().await.unwrap_or(0);
        let completed = self.store.count(TaskStatus::Completed).await.unwrap_or(0);
        let pending = self.store.count(TaskStatus::Pending).await.unwrap_or(0);
        let failed = self.store.count(TaskStatus::Failed).await.unwrap_or(0);

        let elapsed = self.start_time.map(|t| t.elapsed()).unwrap_or_default();
        let elapsed_str = format_duration(elapsed);

        // Check if progress has changed since last time
        let current_progress = (completed, pending, failed, total);
        let progress_changed = current_progress != self.last_progress;

        // In TUI mode, use tracing instead of println to avoid interfering with TUI
        // Only log if progress has actually changed to avoid spamming the logs panel
        if self.config.event_sender.is_some() {
            if progress_changed {
                info!(
                    "Progress: {} completed | {} pending | {} failed / {} total",
                    completed, pending, failed, total
                );
                self.last_progress = current_progress;
            }

            // Always emit ProgressUpdate event for status bar
            self.emit_event(Event::ProgressUpdate {
                completed,
                total,
                failed,
                elapsed,
            });
        } else {
            println!();
            println!(
                "Progress: {} completed | {} pending | {} failed / {} total [elapsed {}]",
                completed, pending, failed, total, elapsed_str
            );
            println!();
        }
    }

    async fn print_summary(&self) -> Result<()> {
        let completed = self.store.count(TaskStatus::Completed).await?;
        let failed = self.store.count(TaskStatus::Failed).await?;
        let total = self.store.total().await?;

        // In TUI mode, use tracing instead of println to avoid interfering with TUI
        if self.config.event_sender.is_some() {
            info!("{}", "=".repeat(45));
            info!(
                "All tasks processed: {}/{} completed, {} failed",
                completed, total, failed
            );
            info!("{}", "=".repeat(45));
        } else {
            println!();
            println!("{}", "=".repeat(45));
            println!(
                "All tasks processed: {}/{} completed, {} failed",
                completed, total, failed
            );
            println!("{}", "=".repeat(45));
            println!();
        }

        Ok(())
    }
}

/// Run single task pipeline
async fn run_task_pipeline(
    store: Arc<TaskStore>,
    executor: Arc<TaskExecutor>,
    mut task: Task,
    event_sender: Option<EventSender>,
) -> Result<()> {
    let thread_name = format!("thread-{}", task.id);

    // Emit InProgress status
    if let Some(ref sender) = event_sender {
        let _ = sender.send(Event::TaskStatusChanged {
            id: task.id.clone(),
            status: TaskStatus::InProgress,
        });
    }

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
            // Emit Failed status
            if let Some(ref sender) = event_sender {
                let _ = sender.send(Event::TaskStatusChanged {
                    id: task.id.clone(),
                    status: TaskStatus::Failed,
                });
            }
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
            // Emit Failed status
            if let Some(ref sender) = event_sender {
                let _ = sender.send(Event::TaskStatusChanged {
                    id: task.id.clone(),
                    status: TaskStatus::Failed,
                });
            }
            return Ok(());
        }
    }

    // Mark completed
    task.status = TaskStatus::Completed;
    store.save_task(&task).await?;
    info!(task_id = %task.id, "Task completed successfully");

    // Emit Completed status
    if let Some(ref sender) = event_sender {
        let _ = sender.send(Event::TaskStatusChanged {
            id: task.id.clone(),
            status: TaskStatus::Completed,
        });
    }

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

/// Raw question from AI response
#[derive(Debug, serde::Deserialize)]
struct RawQuestion {
    question: String,
    options: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AssessResponse {
    split: bool,
    #[allow(dead_code)]
    reason: String,
    #[serde(default)]
    subtasks: Option<Vec<SubtaskDef>>,
}

#[derive(Debug, serde::Deserialize)]
struct SubtaskDef {
    title: String,
    description: String,
}

/// Extract JSON object from text that may contain other content
fn extract_json_from_text(text: &str) -> Option<String> {
    // Find the first '{'
    let start = text.find('{')?;
    let mut depth = 0;
    let mut end = start;

    for (i, c) in text[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth == 0 && end > start {
        Some(text[start..end].to_string())
    } else {
        None
    }
}
