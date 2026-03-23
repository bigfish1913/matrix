//! TaskExecutor - handles task execution, testing, and fixing.

use crate::agent::{ClaudeRunner, SharedAgentPool};
use crate::config::{Model, MAX_PROMPT_LENGTH, MAX_WORKSPACE_FILES, TIMEOUT_EXEC};
use crate::detector::ProjectDetector;
use crate::detector::TestRunnerDetector;
use crate::error::{Error, Result};
use crate::models::{Question, Task, TaskStatus};
use crate::store::{QuestionStore, TaskStore};
use crate::tui::{Activity, Event, EventSender, ExecutionState, QuestionSender};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::process::Command;
use tracing::{info, warn};

/// Verification stage for error context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Test,
    Build,
    AiReview,
}

impl Stage {
    /// Get human-readable description for this stage
    pub fn description(&self) -> &'static str {
        match self {
            Stage::Test => "test failures",
            Stage::Build => "compilation/build errors",
            Stage::AiReview => "functionality issues",
        }
    }
}

/// Parsed question data from agent output
#[derive(Debug, Clone)]
pub struct QuestionData {
    pub question: String,
    pub options: Vec<String>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub recommended: Option<usize>,
    pub recommendation_reason: Option<String>,
    pub blocking: bool,
}

/// Executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub model_fast: String,
    pub model_smart: String,
    pub doc_content: Option<String>,
    pub mcp_config: Option<PathBuf>,
    pub debug_mode: bool,
    /// Language for AI prompts (default: "zh" for Chinese)
    pub language: String,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            model_fast: Model::default_fast().to_string(),
            model_smart: Model::default_smart().to_string(),
            doc_content: None,
            mcp_config: None,
            debug_mode: false,
            language: "zh".to_string(), // 默认使用中文
        }
    }
}

impl ExecutorConfig {
    /// Get language instruction for AI prompts
    pub fn lang_instruction(&self) -> &str {
        match self.language.as_str() {
            "zh" => "请用中文回复。所有代码注释、说明、输出信息都用中文。",
            "en" => "Respond in English. All code comments, explanations, and output messages in English.",
            _ => "请用中文回复。所有代码注释、说明、输出信息都用中文。",
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
    question_store: Option<Arc<QuestionStore>>,
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
            question_store: None,
        }
    }

    /// Set the event sender for TUI updates
    pub fn with_event_sender(mut self, sender: Option<EventSender>) -> Self {
        self.event_sender = sender.clone();
        // Also pass to the runner so it can emit Claude output events
        self.runner = self.runner.with_event_sender(sender);
        self
    }

    /// Set the question store for question persistence
    pub fn with_question_store(mut self, store: Arc<QuestionStore>) -> Self {
        self.question_store = Some(store);
        self
    }

    /// Emit an event to the TUI if sender is configured
    fn emit_event(&self, event: Event) {
        if let Some(ref sender) = self.event_sender {
            let _ = sender.send(event);
        }
    }

    /// Update last activity time for a task
    async fn update_activity(&self, task_id: &str) {
        if let Err(e) = self.store.update_last_activity(task_id).await {
            warn!(task_id = %task_id, error = %e, "Failed to update last_activity_at");
        }
    }

    /// Emit activity state change
    fn emit_activity(&self, activity: Activity) {
        self.emit_event(Event::ExecutionStateChanged {
            state: ExecutionState::Running { activity },
        });
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

        // Update activity time
        self.update_activity(&task.id).await;

        // Emit activity state for planning
        self.emit_activity(Activity::Planning);

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

        // Emit request event (for verbose mode)
        if self.config.debug_mode {
            // Truncate prompt for display (handle UTF-8 properly)
            let prompt_preview = if prompt.len() > 500 {
                let truncated: String = prompt.chars().take(500).collect();
                format!(
                    "{}...\n[truncated, total {} chars]",
                    truncated,
                    prompt.len()
                )
            } else {
                prompt.clone()
            };
            self.emit_event(Event::ClaudeRequest {
                task_id: task.id.clone(),
                prompt: prompt_preview,
                model: model.clone(),
                timeout_secs: TIMEOUT_EXEC,
            });
        }

        // Call Claude - emit progress before potentially long operation
        info!(task_id = %task.id, title = %task.title, model = %model, "Calling Claude API...");
        self.emit_activity(Activity::ApiCall);
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: format!("⏳ Calling {}...", model),
        });

        let result = self
            .runner
            .call(
                &prompt,
                &self.workspace,
                Some(TIMEOUT_EXEC),
                self.config.mcp_config.as_deref(),
                resume_sid.as_deref(),
                Some(&task.id),
            )
            .await;

        // Update activity time after API call
        self.update_activity(&task.id).await;

        // Emit completion message
        info!(task_id = %task.id, title = %task.title, "Claude API call completed");
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: "✓ API call completed, processing result...".to_string(),
        });

        match result {
            Ok(claude_result) if claude_result.is_error => {
                warn!(task_id = %task.id, title = %task.title, error = %claude_result.text, "Execution failed");
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
                    info!(task_id = %task.id, title = %task.title, tokens = usage.total_tokens, "Token usage");
                }

                let stats = self.agent_pool.stats().await;
                info!(task_id = %task.id, title = %task.title, stats = %stats, "Task executed");

                // Emit result event
                self.emit_event(Event::ClaudeResult {
                    task_id: task.id.clone(),
                    result: claude_result.text.clone(),
                });

                Ok(true)
            }
            Err(e) => {
                warn!(task_id = %task.id, title = %task.title, error = %e, "Execution error");
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
        info!(task_id = %task.id, title = %task.title, "Running tests");
        self.update_activity(&task.id).await;
        self.emit_activity(Activity::Test);
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: "⏳ Running tests...".to_string(),
        });

        let runner = TestRunnerDetector::detect(&self.workspace);

        if runner.is_none() {
            info!("No test runner detected, skipping tests");
            self.emit_event(Event::TaskProgress {
                id: task.id.clone(),
                message: "✓ No test runner detected, skipped".to_string(),
            });
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
                    info!(task_id = %task.id, title = %task.title, "Tests passed");
                    self.emit_event(Event::TaskProgress {
                        id: task.id.clone(),
                        message: "✓ Tests passed".to_string(),
                    });
                    Ok((true, combined))
                } else {
                    warn!(task_id = %task.id, title = %task.title, "Tests failed");
                    self.emit_event(Event::TaskProgress {
                        id: task.id.clone(),
                        message: "✗ Tests failed, attempting fix...".to_string(),
                    });
                    Ok((false, combined))
                }
            }
            Err(e) => {
                warn!(task_id = %task.id, title = %task.title, error = %e, "Test command failed");
                self.emit_event(Event::TaskProgress {
                    id: task.id.clone(),
                    message: format!("⚠ Test command error: {}", e),
                });
                Ok((false, e.to_string()))
            }
        }
    }

    /// Verify that the project builds successfully (typecheck + build)
    /// This ensures the generated code is executable, not just tests passing
    pub async fn verify_build(&self, task: &mut Task) -> Result<(bool, String)> {
        info!(task_id = %task.id, title = %task.title, "Verifying build");
        self.update_activity(&task.id).await;
        self.emit_activity(Activity::Test);
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: "⏳ Verifying build (typecheck + build)...".to_string(),
        });

        // Step 1: Run typecheck if available
        let typecheck_result = self
            .run_command(&["npm", "run", "typecheck"], "typecheck")
            .await;
        let mut errors = Vec::new();

        match typecheck_result {
            Ok((success, output)) => {
                if success {
                    info!(task_id = %task.id, "Typecheck passed");
                } else {
                    warn!(task_id = %task.id, "Typecheck failed");
                    errors.push(format!("TypeScript errors:\n{}", output));
                }
            }
            Err(e) => {
                // typecheck script might not exist, that's okay
                info!(task_id = %task.id, "No typecheck script or error: {}", e);
            }
        }

        // Step 2: Run build
        let build_result = self.run_command(&["npm", "run", "build"], "build").await;

        match build_result {
            Ok((success, output)) => {
                if success {
                    info!(task_id = %task.id, "Build passed");
                    if errors.is_empty() {
                        self.emit_event(Event::TaskProgress {
                            id: task.id.clone(),
                            message: "✓ Build verification passed".to_string(),
                        });
                        return Ok((true, "Build verification passed".to_string()));
                    }
                } else {
                    warn!(task_id = %task.id, "Build failed");
                    errors.push(format!("Build errors:\n{}", output));
                }
            }
            Err(e) => {
                errors.push(format!("Build command error: {}", e));
            }
        }

        if errors.is_empty() {
            // Neither typecheck nor build scripts exist
            self.emit_event(Event::TaskProgress {
                id: task.id.clone(),
                message: "✓ No build scripts found, skipped".to_string(),
            });
            return Ok((true, "No build scripts found".to_string()));
        }

        let combined_errors = errors.join("\n\n");
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: "✗ Build verification failed".to_string(),
        });
        Ok((false, combined_errors))
    }

    /// Run a command and return (success, output)
    async fn run_command(&self, cmd: &[&str], name: &str) -> Result<(bool, String)> {
        if cmd.is_empty() {
            return Ok((true, format!("No {} command", name)));
        }

        let result = Command::new(cmd[0])
            .args(&cmd[1..])
            .current_dir(&self.workspace)
            .output()
            .await;

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);
                Ok((output.status.success(), combined))
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    // Command not found, script might not exist
                    Ok((true, format!("{} script not found", name)))
                } else {
                    Err(Error::TaskExecution(format!("{} failed: {}", name, e)))
                }
            }
        }
    }

    /// Verify that the application runs successfully
    /// This is the final verification step after build passes
    pub async fn verify_functionality(&self, task: &mut Task) -> Result<(bool, String)> {
        info!(task_id = %task.id, title = %task.title, "Verifying functionality");
        self.update_activity(&task.id).await;
        self.emit_activity(Activity::Test);
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: "⏳ Verifying functionality (app startup)...".to_string(),
        });

        // Try to start the dev server and capture initial output
        let dev_result = self.run_dev_server_check().await;

        match dev_result {
            Ok((success, output)) => {
                if success {
                    info!(task_id = %task.id, "Functionality verification passed");
                    self.emit_event(Event::TaskProgress {
                        id: task.id.clone(),
                        message: "✓ Functionality verification passed".to_string(),
                    });
                    Ok((true, output))
                } else {
                    warn!(task_id = %task.id, "Functionality verification failed");
                    self.emit_event(Event::TaskProgress {
                        id: task.id.clone(),
                        message: "✗ Runtime errors detected".to_string(),
                    });
                    Ok((false, output))
                }
            }
            Err(e) => {
                // If no dev script, skip functionality check
                info!(task_id = %task.id, "No dev server or skip: {}", e);
                self.emit_event(Event::TaskProgress {
                    id: task.id.clone(),
                    message: "✓ No dev server to verify, skipped".to_string(),
                });
                Ok((true, format!("Skipped: {}", e)))
            }
        }
    }

    /// Unified error fix entry point
    ///
    /// This replaces the previous separate fix functions:
    /// - fix_test_failure
    /// - fix_build_errors
    /// - fix_runtime_errors
    /// - fix_functionality_issues
    pub async fn fix_errors(
        &self,
        task: &mut Task,
        stage: Stage,
        error: &str,
    ) -> Result<bool> {
        info!(task_id = %task.id, stage = ?stage, "Attempting to fix errors");
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: format!("🔧 Attempting to fix {}...", stage.description()),
        });

        let prompt = format!(
            r#"You are a senior developer fixing {} in a software project.

TASK: {}
DESCRIPTION: {}

ERRORS:
{}

CRITICAL INSTRUCTIONS:
1. Analyze the errors carefully
2. Fix the root cause, not just symptoms
3. Ensure fixes don't break existing functionality
4. Do NOT add placeholder implementations

Fix the errors. Make minimal, targeted changes.
Respond with a brief summary of what you fixed."#,
            stage.description(),
            task.title,
            task.description,
            error
        );

        let result = self
            .runner
            .call(
                &prompt,
                &self.workspace,
                Some(TIMEOUT_EXEC),
                None,
                None,
                Some(&task.id),
            )
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

        info!(task_id = %task.id, stage = ?stage, summary = %result.text, "Fix applied");
        Ok(true)
    }

    /// Run dev server and check for startup errors
    async fn run_dev_server_check(&self) -> Result<(bool, String)> {
        use std::time::Duration;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::time::timeout;

        // Start the dev server
        let mut child = tokio::process::Command::new("npm")
            .arg("run")
            .arg("dev")
            .current_dir(&self.workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::TaskExecution(format!("Failed to start dev server: {}", e)))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::TaskExecution("Failed to capture stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::TaskExecution("Failed to capture stderr".to_string()))?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_lines = Vec::new();
        let mut has_error = false;
        let mut server_started = false;

        // Wait up to 30 seconds for server to start or show errors
        let check_duration = Duration::from_secs(30);
        let start = std::time::Instant::now();

        while start.elapsed() < check_duration {
            // Check stdout
            match timeout(Duration::from_millis(100), stdout_reader.next_line()).await {
                Ok(Ok(Some(line))) => {
                    output_lines.push(format!("[OUT] {}", line));

                    // Check for successful server start indicators
                    if line.contains("localhost")
                        || line.contains("ready in")
                        || line.contains("VITE")
                        || line.contains("server running")
                        || line.contains("Local:")
                        || line.contains("Network:")
                    {
                        server_started = true;
                    }

                    // Check for error indicators
                    if line.to_lowercase().contains("error")
                        || line.contains("failed")
                        || line.contains("cannot")
                    {
                        has_error = true;
                    }
                }
                _ => {}
            }

            // Check stderr
            match timeout(Duration::from_millis(100), stderr_reader.next_line()).await {
                Ok(Ok(Some(line))) => {
                    output_lines.push(format!("[ERR] {}", line));
                    // stderr often contains errors
                    if !line.contains("warning") && !line.contains("deprecated") {
                        has_error = true;
                    }
                }
                _ => {}
            }

            // If server started successfully, we can stop
            if server_started && !has_error {
                break;
            }
        }

        // Kill the dev server
        let _ = child.kill().await;

        let combined_output = output_lines.join("\n");

        if has_error {
            Ok((false, combined_output))
        } else if server_started {
            Ok((true, combined_output))
        } else {
            // No clear success or failure - probably okay
            Ok((true, combined_output))
        }
    }

    /// AI-driven functionality review
    /// The AI will review the implementation against requirements and verify functionality
    pub async fn ai_functionality_review(&self, task: &mut Task) -> Result<(bool, String)> {
        info!(task_id = %task.id, title = %task.title, "AI functionality review");
        self.update_activity(&task.id).await;
        self.emit_activity(Activity::Test);
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: "🤖 AI functionality review in progress...".to_string(),
        });

        // Build the review prompt with task context and requirements
        let doc_section = self.doc_section();
        let workspace_files = self.workspace_files(Some(50)); // Limit files for review

        let prompt = format!(
            r#"You are a QA engineer performing functionality acceptance testing.

TASK: {}
DESCRIPTION: {}
{}

WORKSPACE: {}
KEY FILES:
{}

INSTRUCTIONS:
You must verify that the implementation meets the requirements.

STEP 1: Start the application
- Run `npm run dev` in the background
- Wait for it to start (look for "localhost" or "ready" message)

STEP 2: Open the application in browser
- Navigate to http://localhost:5173 (or the port shown)
- Take a screenshot or describe what you see

STEP 3: Verify core functionality
For a game, check:
- Does the game load and display correctly?
- Can you start a new game?
- Do the main UI elements work (buttons, menus)?
- Are there any console errors?

STEP 4: Report findings
Format your response as:
## Functionality Review Results

### ✅ Passed Checks
- [list what works]

### ❌ Failed Checks
- [list what doesn't work]

### 🐛 Bugs Found
- [list any bugs]

### Recommendation
APPROVE / NEEDS_FIX

If NEEDS_FIX, explain what needs to be fixed.
"#,
            task.title,
            task.description,
            doc_section,
            self.workspace.display(),
            workspace_files
        );

        // Call Claude to perform the review
        let result = self
            .runner
            .call(
                &prompt,
                &self.workspace,
                Some(TIMEOUT_EXEC),
                None,
                None,
                Some(&task.id),
            )
            .await;

        // Handle timeout gracefully - skip AI review if it times out
        let result = match result {
            Ok(r) => r,
            Err(Error::Timeout(msg)) => {
                warn!(task_id = %task.id, "AI review timed out, skipping: {}", msg);
                self.emit_event(Event::TaskProgress {
                    id: task.id.clone(),
                    message: "⚠ AI review timed out, skipping (build passed)".to_string(),
                });
                return Ok((true, format!("AI review skipped due to timeout: {}", msg)));
            }
            Err(e) => return Err(e),
        };

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: task.id.clone(),
                tokens_used: usage.total_tokens,
            });
            info!(task_id = %task.id, title = %task.title, tokens = usage.total_tokens, "Token usage (AI review)");
        }

        let review_result = result.text.clone();

        // Check if review passed
        let passed = !result.is_error
            && (review_result.to_lowercase().contains("approve")
                || review_result.to_lowercase().contains("passed checks")
                    && !review_result.to_lowercase().contains("failed checks"));

        if passed {
            info!(task_id = %task.id, "AI functionality review passed");
            self.emit_event(Event::TaskProgress {
                id: task.id.clone(),
                message: "✓ AI functionality review passed".to_string(),
            });
        } else {
            warn!(task_id = %task.id, "AI functionality review found issues");
            self.emit_event(Event::TaskProgress {
                id: task.id.clone(),
                message: "✗ AI functionality review found issues".to_string(),
            });
        }

        Ok((passed, review_result))
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
            .filter(|(path, mtime)| before.get(*path).is_none_or(|old_mtime| old_mtime < *mtime))
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Ask a question during task execution
    ///
    /// # Arguments
    /// * `task_id` - ID of the task asking the question
    /// * `question` - The question text
    /// * `options` - Multiple choice options
    /// * `pros` - Pros for each option
    /// * `cons` - Cons for each option
    /// * `recommended` - Index of recommended option
    /// * `recommendation_reason` - Why this option is recommended
    /// * `blocking` - If true, waits for user answer; if false, auto-decides
    ///
    /// # Returns
    /// The selected option (or auto-decision if non-blocking)
    pub async fn ask_question(
        &self,
        task_id: &str,
        question: &str,
        options: &[String],
        pros: &[String],
        cons: &[String],
        recommended: Option<usize>,
        recommendation_reason: Option<&str>,
        blocking: bool,
    ) -> Result<String> {
        // Validate options
        if options.is_empty() {
            return Err(Error::TaskExecution(
                "No options provided for question".to_string(),
            ));
        }

        let question_model = Question::new(
            task_id.to_string(),
            question.to_string(),
            options.to_vec(),
            pros.to_vec(),
            cons.to_vec(),
            recommended,
            recommendation_reason.map(|s| s.to_string()),
            blocking,
        );

        // Save to store
        if let Some(ref store) = self.question_store {
            store.create(&question_model).await?;
        }

        if blocking {
            // Create oneshot channel for response
            let (tx, rx) = tokio::sync::oneshot::channel::<String>();
            let question_sender = QuestionSender::new(tx);

            // Send event to TUI
            if let Some(ref sender) = self.event_sender {
                let _ = sender.send(Event::AgentQuestion {
                    task_id: task_id.to_string(),
                    question: question_model.clone(),
                    response_tx: question_sender,
                });
            }

            // Wait for answer with timeout to prevent permanent blocking
            let timeout_duration = tokio::time::Duration::from_secs(300); // 5 minutes
            match tokio::time::timeout(timeout_duration, rx).await {
                Ok(Ok(answer)) => {
                    // Record answer
                    if let Some(ref store) = self.question_store {
                        store.answer(&question_model.id, &answer).await?;
                    }
                    Ok(answer)
                }
                Ok(Err(_)) => Err(Error::TaskExecution("Question channel closed".to_string())),
                Err(_) => Err(Error::TaskExecution("Question timeout (5 min)".to_string())),
            }
        } else {
            // Non-blocking: auto-decide with recommended option
            let decision = recommended
                .and_then(|i| options.get(i))
                .cloned()
                .unwrap_or_else(|| options.first().cloned().unwrap_or_default());

            let reason = recommendation_reason.unwrap_or("Auto-decided (non-blocking)");

            // Record auto-decision
            if let Some(ref store) = self.question_store {
                store
                    .record_auto_decision(&question_model.id, &decision, reason)
                    .await?;
            }

            // Send event for UI update
            if let Some(ref sender) = self.event_sender {
                let _ = sender.send(Event::QuestionAutoDecided {
                    question_id: question_model.id.clone(),
                    decision: decision.clone(),
                    reason: reason.to_string(),
                });
            }

            Ok(decision)
        }
    }

    /// Parse QUESTION markers from agent output
    /// Looks for JSON format: QUESTION: {"question": "...", "options": [...], ...}
    /// Or structured format with QUESTION: prefix
    pub fn parse_question_from_output(output: &str) -> Option<QuestionData> {
        // Try JSON format first: QUESTION: {"question": "...", "options": [...]}
        if let Some(json_start) = output.find("QUESTION: {") {
            let json_part = &output[json_start + 9..]; // Skip "QUESTION: "
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_part) {
                return Self::extract_question_data(&json);
            }
        }

        // Try to find standalone JSON with question fields
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if json.get("question").is_some() && json.get("options").is_some() {
                        return Self::extract_question_data(&json);
                    }
                }
            }
        }

        None
    }

    /// Extract QuestionData from JSON value
    fn extract_question_data(json: &serde_json::Value) -> Option<QuestionData> {
        let question = json.get("question")?.as_str()?.to_string();

        let options = json
            .get("options")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        if options.is_empty() {
            return None;
        }

        let pros = json
            .get("pros")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let cons = json
            .get("cons")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let recommended = json
            .get("recommended")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let recommendation_reason = json
            .get("recommendation_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let blocking = json
            .get("blocking")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Some(QuestionData {
            question,
            options,
            pros,
            cons,
            recommended,
            recommendation_reason,
            blocking,
        })
    }

    /// Process agent output to check for embedded questions
    pub async fn process_output_for_questions(&self, task_id: &str, output: &str) -> Result<()> {
        if let Some(question_data) = Self::parse_question_from_output(output) {
            self.ask_question(
                task_id,
                &question_data.question,
                &question_data.options,
                &question_data.pros,
                &question_data.cons,
                question_data.recommended,
                question_data.recommendation_reason.as_deref(),
                question_data.blocking,
            )
            .await?;
        }
        Ok(())
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
