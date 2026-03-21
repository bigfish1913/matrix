//! Orchestrator - main coordination engine.

use crate::agent::SharedAgentPool;
use crate::config::{MAX_DEPTH, MAX_RETRIES};
use crate::error::{Error, Result};
use crate::executor::{ExecutorConfig, TaskExecutor};
use crate::models::{Complexity, Task, TaskStatus};
use crate::store::TaskStore;
use crate::tui::event::{AnswerSender, ClarificationSender};
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

        // Phase 0: Check for existing tasks FIRST
        let total = self.store.total().await?;
        let should_resume = if total > 0 {
            // Found existing tasks - ask user whether to resume or start fresh
            self.confirm_resume().await?
        } else {
            false
        };

        if should_resume {
            // Resume existing tasks - skip clarification
            self.resume_tasks().await?;
        } else {
            // Starting fresh - clear any existing tasks
            if total > 0 {
                info!("Clearing existing tasks and starting fresh...");
                self.store.clear().await?;
            }

            // Ask clarification questions if enabled
            let clarification = if self.config.ask_mode {
                // Emit Clarifying state before asking questions
                self.emit_event(Event::ExecutionStateChanged {
                    state: ExecutionState::Clarifying,
                });
                self.clarify_goal().await?
            } else {
                String::new()
            };

            // Generate project roadmap document (Phase 1.5)
            self.generate_project_roadmap(&clarification).await?;

            // Generate new tasks (Phase 2)
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
        info!(phase = "clarification", "Phase 0: Starting clarification...");

        let lang_instruction = match self.config.language.as_str() {
            "zh" => "请用中文提问，选项也用中文。优缺点和推荐理由也用中文。",
            "en" => "Please ask questions and provide options in English. Pros, cons and recommendations also in English.",
            _ => "请用中文提问，选项也用中文。优缺点和推荐理由也用中文。",
        };

        let prompt = format!(
            r#"You are helping plan a software development project.

GOAL: {}
{}

{}

Generate 3-5 concise, targeted clarifying questions.
For each question, provide 3-4 common options with their pros and cons.
Also recommend the best option with a reason.

Respond ONLY with JSON array:
[
  {{
    "question": "Question text?",
    "options": ["Option 1", "Option 2", "Option 3"],
    "pros": ["Pro for option 1", "Pro for option 2", "Pro for option 3"],
    "cons": ["Con for option 1", "Con for option 2", "Con for option 3"],
    "recommended": 0,
    "recommendation_reason": "Why this option is recommended"
  }}
]

Example:
[
  {{
    "question": "项目使用什么编程语言?",
    "options": ["Rust", "Python", "JavaScript", "Go"],
    "pros": ["高性能，内存安全", "开发快速，生态丰富", "前后端通用", "简洁高效，并发强"],
    "cons": ["学习曲线陡峭", "性能较低", "类型不严格", "生态较小"],
    "recommended": 0,
    "recommendation_reason": "Rust提供最佳的性能和安全性，适合长期维护的项目"
  }},
  {{
    "question": "是否需要数据库支持?",
    "options": ["是，SQLite", "是，PostgreSQL", "不需要", "不确定"],
    "pros": ["轻量，零配置", "功能强大，可扩展", "简单，无依赖", "稍后决定"],
    "cons": ["不适合高并发", "需要额外部署", "数据无法持久化", "可能延迟决策"],
    "recommended": 0,
    "recommendation_reason": "SQLite简单易用，适合中小型项目快速启动"
  }}
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
            warn!("Could not generate clarifying questions: {}", result.text);
            return Ok(String::new());
        }

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: "clarification".to_string(),
                tokens_used: usage.total_tokens,
            });
            info!(phase = "clarification", tokens = usage.total_tokens, "Token usage");
        }

        // Log raw response for debugging
        info!(phase = "clarification", "Clarification response length: {} chars", result.text.len());

        // Try to extract JSON array from response (may be in markdown code block)
        let json_text = if let Some(json) = extract_json_from_markdown(&result.text) {
            json
        } else if let Some(json) = extract_json_array_from_text(&result.text) {
            json
        } else {
            result.text.clone()
        };

        // Parse questions with options
        let raw_questions: Vec<RawQuestion> = match serde_json::from_str::<Vec<RawQuestion>>(&json_text) {
            Ok(q) => {
                info!(phase = "clarification", "Parsed {} questions", q.len());
                q
            }
            Err(e) => {
                warn!(phase = "clarification", "Failed to parse questions JSON: {}", e);
                return Ok(String::new());
            }
        };

        // Convert to ClarificationQuestion
        let questions: Vec<ClarificationQuestion> = raw_questions
            .into_iter()
            .map(|rq| ClarificationQuestion {
                question: rq.question,
                options: rq.options,
                pros: rq.pros,
                cons: rq.cons,
                recommended: rq.recommended,
                recommendation_reason: rq.recommendation_reason,
            })
            .collect();

        // Check if in TUI mode
        if let Some(ref sender) = self.config.event_sender {
            // TUI mode: send questions via event channel and wait for answers
            info!(phase = "clarification", "Sending {} questions to TUI...", questions.len());
            let (tx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();
            match sender.send(Event::ClarificationQuestions {
                questions: questions.clone(),
                response_tx: AnswerSender::new(tx),
            }) {
                Ok(_) => info!(phase = "clarification", "Questions sent to TUI"),
                Err(e) => {
                    warn!("Failed to send ClarificationQuestions event: {}", e);
                    return Ok(String::new());
                }
            }

            // Wait for answers from TUI (with timeout in case TUI is closed)
            info!(phase = "clarification", "Waiting for answers...");
            match tokio::time::timeout(tokio::time::Duration::from_secs(300), rx).await {
                Ok(Ok(answers)) => {
                    info!(phase = "clarification", "Received {} answers", answers.len());
                    if answers.is_empty() || answers.iter().all(|a| a.trim().is_empty()) {
                        warn!(phase = "clarification", "All answers empty, skipping");
                        return Ok(String::new());
                    }
                    let formatted: Vec<String> = questions
                        .iter()
                        .zip(answers.iter())
                        .map(|(q, a): (&ClarificationQuestion, &String)| format!("Q: {}\nA: {}", q.question, if a.is_empty() { "(skipped)" } else { a }))
                        .collect();
                    info!(phase = "clarification", "Completed successfully");
                    return Ok(formatted.join("\n\n"));
                }
                Ok(Err(_)) => {
                    warn!(phase = "clarification", "Failed to receive answers");
                    return Ok(String::new());
                }
                Err(_) => {
                    warn!(phase = "clarification", "Timeout waiting for answers");
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
            .map(|(q, a): (&ClarificationQuestion, &String)| format!("Q: {}\nA: {}", q.question, if a.is_empty() { "(skipped)" } else { a }))
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

    /// Check if a task is a clarification question
    fn is_clarification_task(&self, title: &str, description: &str) -> bool {
        let clarification_keywords = [
            "需要更多信息", "需要信息", "请提供", "请描述", "请详细说明",
            "need more information", "please provide", "please describe",
            "clarification", "question", "询问", "问题",
        ];
        
        let title_lower = title.to_lowercase();
        let desc_lower = description.to_lowercase();
        
        for keyword in &clarification_keywords {
            if title_lower.contains(keyword) || desc_lower.contains(keyword) {
                return true;
            }
        }
        
        false
    }

    /// Generate project roadmap document based on clarification answers
    async fn generate_project_roadmap(&self, clarification: &str) -> Result<()> {
        info!(phase = "generating", "Generating project roadmap...");

        let lang_instruction = match self.config.language.as_str() {
            "zh" => "请用中文编写规划文档。",
            "en" => "Write the planning document in English.",
            _ => "请用中文编写规划文档。",
        };

        let clarification_section = if !clarification.is_empty() {
            format!("\n\nCLARIFICATION (User's answers):\n{}\n", clarification)
        } else {
            String::new()
        };

        let prompt = format!(
            r#"You are a software project planner. Create a detailed project roadmap document.

PROJECT GOAL: {}
{}{}

{}

Create a comprehensive ROADMAP.md document that includes:
1. Project Overview - Brief description of what we're building
2. Architecture Decisions - Key technical choices and rationale
3. Implementation Phases - Logical groupings of work with dependencies
4. Technical Requirements - Libraries, frameworks, tools needed
5. Success Criteria - How to verify each phase is complete

Output ONLY the markdown content for ROADMAP.md (no code blocks, just the raw markdown).
Start with # Project Roadmap"#,
            self.config.goal,
            self.config.doc_content.as_ref().map(|d| format!("\nDOCUMENT:\n{}", d)).unwrap_or_default(),
            clarification_section,
            lang_instruction
        );

        let result = self
            .executor
            .runner
            .call(&prompt, &self.config.workspace, Some(120), None, None)
            .await?;

        if result.is_error {
            warn!("Failed to generate project roadmap: {}", result.text);
            return Ok(()); // Non-fatal, continue without roadmap
        }

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: "generate_roadmap".to_string(),
                tokens_used: usage.total_tokens,
            });
            info!(tokens = usage.total_tokens, "Token usage (generate_roadmap)");
        }

        // Save roadmap to workspace
        let roadmap_path = self.config.workspace.join("ROADMAP.md");
        fs::write(&roadmap_path, &result.text).await?;
        info!("Project roadmap saved to ROADMAP.md");

        // Also log a summary
        let summary: String = result.text.lines().take(20).collect::<Vec<_>>().join("\n");
        info!("Roadmap preview:\n{}", summary);

        Ok(())
    }

    async fn generate_tasks(&self, clarification: &str) -> Result<()> {
        info!(phase = "generating", goal = %self.config.goal, "Generating task list");

        let lang_instruction = match self.config.language.as_str() {
            "zh" => "请用中文编写任务标题和描述。",
            "en" => "Write task titles and descriptions in English.",
            _ => "请用中文编写任务标题和描述。",
        };

        // Include clarification answers if available
        let clarification_section = if !clarification.is_empty() {
            format!("\n\nCLARIFICATION (User's answers to clarifying questions):\n{}\n", clarification)
        } else {
            String::new()
        };

        // Include roadmap if it exists
        let roadmap_path = self.config.workspace.join("ROADMAP.md");
        let roadmap_section = if roadmap_path.exists() {
            if let Ok(roadmap) = tokio::fs::read_to_string(&roadmap_path).await {
                // Truncate if too long
                let truncated = if roadmap.len() > 4000 {
                    format!("{}...\n[truncated]", &roadmap[..4000])
                } else {
                    roadmap
                };
                format!("\n\nPROJECT ROADMAP (Reference this for task breakdown):\n{}\n", truncated)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let prompt = format!(
            r#"You are a software project planner. Break down the following goal into development tasks.

PROJECT GOAL: {}
{}{}{}

CRITICAL: You MUST respond with ONLY valid JSON. No explanations, no markdown, just the JSON object.
Do NOT include any text before or after the JSON.

IMPORTANT CONSTRAINTS:
- Generate at most 10-15 high-level tasks
- Each task should be completable in 1-2 hours
- Focus on logical features, not implementation details
- Combine related work into single tasks
- Quality over quantity - fewer, more comprehensive tasks are better

Use this EXACT format:
{{"tasks": [{{"id": "task-001", "title": "Short title", "description": "Detailed description", "depends_on": []}}]}}

Example response:
{{"tasks": [{{"id": "task-001", "title": "Setup project", "description": "Initialize project structure", "depends_on": []}}, {{"id": "task-002", "title": "Create models", "description": "Define data models", "depends_on": ["task-001"]}}]}}

Now generate tasks for the project goal above. Output ONLY the JSON object:"#,
            self.config.goal,
            self.config.doc_content.as_ref().map(|d| format!("\nDOCUMENT:\n{}", d)).unwrap_or_default(),
            clarification_section,
            roadmap_section
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

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: "generate_tasks".to_string(),
                tokens_used: usage.total_tokens,
            });
            info!(tokens = usage.total_tokens, "Token usage (generate_tasks)");
        }

        // Log the raw response for debugging
        info!("Task generation response length: {} chars", result.text.len());
        info!("Task generation raw response: {}", result.text);

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

                let mut clarification_task = None;
                
                for t in tasks_response.tasks {
                    let mut task = Task::new(t.id, t.title.clone(), t.description.clone());
                    task.depends_on = t.depends_on;
                    
                    // Check if this is a clarification task
                    if self.is_clarification_task(&t.title, &t.description) {
                        task.is_clarification = true;
                        clarification_task = Some(task.clone());
                        info!(task_id = %task.id, "Detected clarification task");
                    }
                    
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
                
                // If there's a clarification task, wait for user response
                if let Some(clar_task) = clarification_task {
                    info!("Waiting for user clarification response...");
                    self.handle_clarification_task(&clar_task).await?;
                }
            }
            Err(e) => {
                error!(error = %e, json = %json_text, "Failed to parse tasks JSON");
                return Err(Error::ParseError(format!("Failed to parse tasks: {}. JSON: {}", e, &json_text[..json_text.len().min(500)])));
            }
        }

        Ok(())
    }

    /// Handle clarification task by waiting for user response
    async fn handle_clarification_task(&self, task: &Task) -> Result<()> {
        // Update task status to in_progress
        let mut task = task.clone();
        task.status = TaskStatus::InProgress;
        self.store.save_task(&task).await?;
        
        // Emit clarification event
        self.emit_event(Event::ExecutionStateChanged {
            state: ExecutionState::Clarifying,
        });
        
        // Check if in TUI mode
        if let Some(ref sender) = self.config.event_sender {
            // TUI mode: send clarification request and wait for response
            let (tx, rx) = tokio::sync::oneshot::channel::<String>();
            let _ = sender.send(Event::ClarificationTask {
                task_id: task.id.clone(),
                title: task.title.clone(),
                description: task.description.clone(),
                response_tx: crate::tui::event::ClarificationSender::new(tx),
            });
            
            // Wait for user response (with timeout)
            info!("Waiting for user clarification response...");
            match tokio::time::timeout(tokio::time::Duration::from_secs(300), rx).await {
                Ok(Ok(response)) => {
                    info!("Received clarification response");
                    // Mark task as completed
                    task.status = TaskStatus::Completed;
                    task.result = Some(response);
                    self.store.save_task(&task).await?;
                }
                Ok(Err(_)) => {
                    warn!("Failed to receive clarification response from TUI");
                    // Mark task as failed
                    task.status = TaskStatus::Failed;
                    task.error = Some("Failed to receive response".to_string());
                    self.store.save_task(&task).await?;
                }
                Err(_) => {
                    warn!("Timeout waiting for clarification response");
                    // Mark task as failed
                    task.status = TaskStatus::Failed;
                    task.error = Some("Timeout waiting for response".to_string());
                    self.store.save_task(&task).await?;
                }
            }
        } else {
            // Non-TUI mode: use stdin/stdout
            println!("\n[?] {}", task.title);
            println!("    {}\n", task.description);
            print!("    Your response: ");
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
            
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_ok() {
                let response = input.trim().to_string();
                if !response.is_empty() {
                    task.status = TaskStatus::Completed;
                    task.result = Some(response);
                } else {
                    task.status = TaskStatus::Failed;
                    task.error = Some("Empty response".to_string());
                }
            } else {
                task.status = TaskStatus::Failed;
                task.error = Some("Failed to read response".to_string());
            }
            self.store.save_task(&task).await?;
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

        // Quick heuristic: skip assessment for obviously simple tasks
        let simple_keywords = [
            "fix", "update", "add", "remove", "rename", "refactor",
            "修复", "更新", "添加", "删除", "重命名", "优化",
        ];
        let title_lower = task.title.to_lowercase();
        let is_simple_title = task.title.len() < 40
            || simple_keywords.iter().any(|k| title_lower.contains(k));
        let is_short_desc = task.description.len() < 100;

        if is_simple_title && is_short_desc {
            info!(task_id = %task.id, "Skipping assessment - appears simple");
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

Is this task SIMPLE (completable in 1-2 hours) or COMPLEX (needs splitting)?

IMPORTANT: Only split if absolutely necessary. Prefer larger tasks over fragmentation.
Maximum 2-3 subtasks if splitting is needed.

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

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: task.id.clone(),
                tokens_used: usage.total_tokens,
            });
            info!(task_id = %task.id, tokens = usage.total_tokens, "Token usage (assess)");
        }

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
        info!(phase = "testing", "Running final tests...");

        let runner = match crate::detector::TestRunnerDetector::detect(&self.config.workspace) {
            Some(r) => r,
            None => {
                info!(phase = "testing", "No test runner detected, skipping");
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
                    info!(phase = "testing", "Final tests passed");
                    if self.config.event_sender.is_none() {
                        println!("\nFinal tests passed\n");
                    }
                } else {
                    warn!(phase = "testing", "Final tests failed");
                    if self.config.event_sender.is_none() {
                        println!("\nFinal tests failed:\n{}\n", stdout);
                    }
                }
            }
            Err(e) => {
                warn!(phase = "testing", error = %e, "Final tests could not run");
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

            tokio::time::sleep(Duration::from_millis(100)).await;
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
            info!(phase = "summary", "{}", "=".repeat(45));
            info!(
                phase = "summary",
                "All tasks processed: {}/{} completed, {} failed",
                completed, total, failed
            );
            info!(phase = "summary", "{}", "=".repeat(45));
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
    #[serde(default)]
    pros: Vec<String>,
    #[serde(default)]
    cons: Vec<String>,
    #[serde(default)]
    recommended: Option<usize>,
    #[serde(default)]
    recommendation_reason: Option<String>,
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

/// Extract JSON array from text that may contain other content
fn extract_json_array_from_text(text: &str) -> Option<String> {
    // Find the first '['
    let start = text.find('[')?;
    let mut depth = 0;
    let mut end = start;

    for (i, c) in text[start..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
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

/// Extract JSON from markdown code block (```json ... ```)
/// Supports both JSON objects and arrays
fn extract_json_from_markdown(text: &str) -> Option<String> {
    // Try to match ```json ... ``` with any JSON content
    let re = regex::Regex::new(r"```(?:json)?\s*\n?([\s\S]*?)\n?\s*```").ok()?;
    let caps = re.captures(text)?;
    let content = caps.get(1)?.as_str().trim();
    
    // Verify it's valid JSON by checking if it starts with { or [
    if content.starts_with('{') || content.starts_with('[') {
        Some(content.to_string())
    } else {
        None
    }
}
