//! TaskExecutor - handles task execution, testing, and fixing.

use crate::agent::{ClaudeRunner, SharedAgentPool};
use crate::config::{Model, MAX_PROMPT_LENGTH, MAX_WORKSPACE_FILES, TIMEOUT_EXEC};
use crate::detector::ProjectDetector;
use crate::detector::TestRunnerDetector;
use crate::error::Result;
use crate::models::{Task, TaskStatus};
use crate::store::TaskStore;
use crate::tui::{Event, EventSender};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::process::Command;
use tracing::{info, warn};

/// Executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub model_fast: String,
    pub model_smart: String,
    pub doc_content: Option<String>,
    pub mcp_config: Option<PathBuf>,
    pub debug_mode: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            model_fast: Model::default_fast().to_string(),
            model_smart: Model::default_smart().to_string(),
            doc_content: None,
            mcp_config: None,
            debug_mode: false,
        }
    }
}

/// Task executor
pub struct TaskExecutor {
    workspace: PathBuf,
    store: Arc<TaskStore>,
    pub runner: ClaudeRunner,
    agent_pool: SharedAgentPool,
    config: ExecutorConfig,
    setup_done: bool,
    event_sender: Option<EventSender>,
}

impl TaskExecutor {
    /// Create a new TaskExecutor
    pub fn new(
        workspace: PathBuf,
        store: Arc<TaskStore>,
        agent_pool: SharedAgentPool,
        config: ExecutorConfig,
    ) -> Self {
        let runner = ClaudeRunner::new().with_debug(config.debug_mode);

        Self {
            workspace,
            store,
            runner,
            agent_pool,
            config,
            setup_done: false,
            event_sender: None,
        }
    }

    /// Set the event sender for TUI updates
    pub fn with_event_sender(mut self, sender: Option<EventSender>) -> Self {
        self.event_sender = sender;
        self
    }

    /// Emit an event to the TUI if sender is configured
    fn emit_event(&self, event: Event) {
        if let Some(ref sender) = self.event_sender {
            let _ = sender.send(event);
        }
    }

    /// Setup workspace (install dependencies)
    pub async fn setup_workspace(&mut self) -> Result<()> {
        if self.setup_done {
            return Ok(());
        }
        self.setup_done = true;

        let mut dir = fs::read_dir(&self.workspace).await?;
        let mut files: Vec<_> = Vec::new();
        while let Some(entry) = dir.next_entry().await? {
            files.push(entry);
        }

        if files.is_empty() {
            info!("Empty workspace, skipping setup");
            return Ok(());
        }

        info!("Setting up workspace environment...");

        let info = ProjectDetector::detect(&self.workspace);
        if let Some(cmd) = &info.install_command {
            self.run_install_command(cmd).await?;
        }

        // Check subdirectories
        for subdir in ["backend", "frontend", "server", "client", "api"] {
            let sub = self.workspace.join(subdir);
            if sub.exists() {
                let sub_info = ProjectDetector::detect(&sub);
                if let Some(cmd) = &sub_info.install_command {
                    info!(subdir = subdir, "Running install in subdirectory");
                    self.run_install_command_in_subdir(cmd, subdir).await?;
                }
            }
        }

        Ok(())
    }

    async fn run_install_command(&self, cmd: &[String]) -> Result<()> {
        info!(command = ?cmd, "Running install");

        let result = Command::new(&cmd[0])
            .args(&cmd[1..])
            .current_dir(&self.workspace)
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                info!("Install succeeded");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(stderr = %stderr, "Install had warnings");
            }
            Err(e) => {
                warn!(error = %e, "Install command failed");
            }
        }

        Ok(())
    }

    async fn run_install_command_in_subdir(&self, cmd: &[String], subdir: &str) -> Result<()> {
        let shell_cmd = format!("cd {} && {}", subdir, cmd.join(" "));

        self.run_install_command(&["sh".to_string(), "-c".to_string(), shell_cmd])
            .await
    }

    /// Execute a task
    pub async fn execute(&self, task: &mut Task, thread_name: &str) -> Result<bool> {
        let model = if task.complexity == crate::models::Complexity::Complex {
            &self.config.model_smart
        } else {
            &self.config.model_fast
        };

        info!(task_id = %task.id, title = %task.title, model = %model, "Executing task");

        // Emit task started event
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: format!("Starting execution with model {}", model),
        });

        // Build prompt
        let prompt = self.build_execution_prompt(task);

        // Get session
        let resume_sid = self.agent_pool.get_session(task, thread_name).await;

        // Snapshot workspace before
        let pre_snapshot = self.snapshot_workspace().await?;

        // Update task status
        task.status = TaskStatus::InProgress;
        self.store.save_task(task).await?;

        // Call Claude
        let result = self
            .runner
            .call(
                &prompt,
                &self.workspace,
                Some(TIMEOUT_EXEC),
                self.config.mcp_config.as_deref(),
                resume_sid.as_deref(),
            )
            .await;

        match result {
            Ok(claude_result) if claude_result.is_error => {
                warn!(task_id = %task.id, error = %claude_result.text, "Execution failed");
                task.error = Some(claude_result.text.clone());
                self.emit_event(Event::ClaudeResult {
                    task_id: task.id.clone(),
                    result: format!("Error: {}", claude_result.text),
                });
                Ok(false)
            }
            Ok(claude_result) => {
                // Record modified files
                let post_snapshot = self.snapshot_workspace().await?;
                task.modified_files = self.snapshot_diff(&pre_snapshot, &post_snapshot);

                task.result = Some(claude_result.text.clone());
                if let Some(sid) = &claude_result.session_id {
                    task.session_id = Some(sid.clone());
                    self.agent_pool.record(task, sid, thread_name).await;
                }

                // Emit token usage update if available
                if let Some(usage) = &claude_result.usage {
                    self.emit_event(Event::TokenUsageUpdate {
                        task_id: task.id.clone(),
                        tokens_used: usage.total_tokens,
                    });
                    info!(task_id = %task.id, tokens = usage.total_tokens, "Token usage");
                }

                let stats = self.agent_pool.stats().await;
                info!(task_id = %task.id, stats = %stats, "Task executed");

                // Emit result event
                self.emit_event(Event::ClaudeResult {
                    task_id: task.id.clone(),
                    result: claude_result.text.clone(),
                });

                Ok(true)
            }
            Err(e) => {
                warn!(task_id = %task.id, error = %e, "Execution error");
                task.error = Some(e.to_string());
                self.emit_event(Event::ClaudeResult {
                    task_id: task.id.clone(),
                    result: format!("Execution error: {}", e),
                });
                Ok(false)
            }
        }
    }

    /// Run tests for a task
    pub async fn test(&self, task: &mut Task) -> Result<(bool, String)> {
        info!(task_id = %task.id, "Running tests");

        let runner = TestRunnerDetector::detect(&self.workspace);

        if runner.is_none() {
            info!("No test runner detected, skipping tests");
            return Ok((true, "No test runner detected".to_string()));
        }

        let runner = runner.unwrap();

        // Run tests
        let result = Command::new(&runner.command[0])
            .args(&runner.command[1..])
            .current_dir(&self.workspace)
            .output()
            .await;

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);

                if output.status.success() {
                    info!(task_id = %task.id, "Tests passed");
                    Ok((true, combined))
                } else {
                    warn!(task_id = %task.id, "Tests failed");
                    Ok((false, combined))
                }
            }
            Err(e) => {
                warn!(task_id = %task.id, error = %e, "Test command failed");
                Ok((false, e.to_string()))
            }
        }
    }

    /// Attempt to fix test failures
    pub async fn fix_test_failure(&self, task: &mut Task, test_output: &str) -> Result<bool> {
        info!(task_id = %task.id, "Attempting to fix test failures");

        let prompt = format!(
            r#"You are a senior developer fixing test failures.

TASK: {}
DESCRIPTION: {}

TEST FAILURES:
{}

Fix the failing tests. Make minimal, targeted changes.
Respond with a brief summary of what you fixed."#,
            task.title, task.description, test_output
        );

        let result = self
            .runner
            .call(&prompt, &self.workspace, Some(TIMEOUT_EXEC), None, None)
            .await?;

        if result.is_error {
            warn!(error = %result.text, "Fix attempt failed");
            return Ok(false);
        }

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: task.id.clone(),
                tokens_used: usage.total_tokens,
            });
            info!(task_id = %task.id, tokens = usage.total_tokens, "Token usage (fix)");
        }

        info!(task_id = %task.id, summary = %result.text, "Fix applied");
        Ok(true)
    }

    // Helper methods

    fn build_execution_prompt(&self, task: &Task) -> String {
        let doc_section = self.doc_section();
        let memory_section = self.memory_section();
        let dep_section = self.dependency_section(task);

        let prompt = format!(
            r#"You are an expert software developer implementing a specific task.

TASK: {}
DESCRIPTION: {}
{}
{}
{}
WORKSPACE: {}
CURRENT FILES:
{}

ALREADY COMPLETED:
{}

INSTRUCTIONS:
1. Implement the task completely according to the description
2. Create all necessary files in the workspace directory
3. Production-quality code, no TODOs or placeholders
4. Ensure compatibility with previously completed work
5. Follow patterns and decisions from PROJECT MEMORY

Implement the task now. Work directly in the workspace directory."#,
            task.title,
            task.description,
            doc_section,
            memory_section,
            dep_section,
            self.workspace.display(),
            self.workspace_files(Some(MAX_WORKSPACE_FILES)),
            self.completed_context()
        );

        if prompt.len() > MAX_PROMPT_LENGTH {
            self.truncate_prompt(&prompt, MAX_PROMPT_LENGTH)
        } else {
            prompt
        }
    }

    fn doc_section(&self) -> String {
        if let Some(doc) = &self.config.doc_content {
            format!(
                "\nREFERENCE DOCUMENT:\n{}\n",
                doc.chars().take(5000).collect::<String>()
            )
        } else {
            String::new()
        }
    }

    fn memory_section(&self) -> String {
        let memory_path = self.workspace.join(".claude").join("memory.md");
        if let Ok(content) = std::fs::read_to_string(&memory_path) {
            if content.len() > 100 {
                return format!(
                    "\nPROJECT MEMORY:\n{}\n",
                    content.chars().take(3000).collect::<String>()
                );
            }
        }
        String::new()
    }

    fn dependency_section(&self, task: &Task) -> String {
        if task.depends_on.is_empty() {
            return String::new();
        }
        format!("\nDEPENDS ON: {:?}\n", task.depends_on)
    }

    fn workspace_files(&self, limit: Option<usize>) -> String {
        let limit = limit.unwrap_or(100);
        let mut files: Vec<String> = Vec::new();

        for entry in walkdir::WalkDir::new(&self.workspace)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(&self.workspace) {
                files.push(relative.to_string_lossy().to_string());
            }
        }

        files.sort();

        if files.len() > limit {
            format!(
                "{}\n... ({} more files)",
                files[..limit].join("\n"),
                files.len() - limit
            )
        } else if files.is_empty() {
            "(empty)".to_string()
        } else {
            files.join("\n")
        }
    }

    fn completed_context(&self) -> String {
        "(none yet)".to_string()
    }

    fn truncate_prompt(&self, prompt: &str, max_length: usize) -> String {
        let keep_len = (max_length - 100) / 2;
        format!(
            "{}\n\n... (content truncated) ...\n\n{}",
            &prompt[..keep_len],
            &prompt[prompt.len().saturating_sub(keep_len)..]
        )
    }

    async fn snapshot_workspace(&self) -> Result<HashMap<String, SystemTime>> {
        let mut snapshot = HashMap::new();

        for entry in walkdir::WalkDir::new(&self.workspace)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(&self.workspace) {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        snapshot.insert(relative.to_string_lossy().to_string(), modified);
                    }
                }
            }
        }

        Ok(snapshot)
    }

    fn snapshot_diff(
        &self,
        before: &HashMap<String, SystemTime>,
        after: &HashMap<String, SystemTime>,
    ) -> Vec<String> {
        after
            .iter()
            .filter(|(path, mtime)| {
                before
                    .get(*path)
                    .is_none_or(|old_mtime| old_mtime < *mtime)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_config_default() {
        let config = ExecutorConfig::default();
        assert_eq!(config.model_fast, "haiku");
        assert_eq!(config.model_smart, "sonnet");
        assert!(config.doc_content.is_none());
    }
}
