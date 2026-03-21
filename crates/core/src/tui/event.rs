//! TUI Event handling.

use crate::checkpoint::ReviewReport;
use crate::models::{Question, TaskStatus};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Verbosity level for Claude output display
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerbosityLevel {
    /// Only final results
    Quiet,
    /// Tool names + brief results (default)
    #[default]
    Normal,
    /// Full thinking + tool details
    Verbose,
}

/// Activity type for granular status display
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    ApiCall,
    FileWrite,
    Test,
    Planning,
    Assessing,
    Git,
    Other(&'static str),
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiCall => write!(f, "api"),
            Self::FileWrite => write!(f, "file"),
            Self::Test => write!(f, "test"),
            Self::Planning => write!(f, "plan"),
            Self::Assessing => write!(f, "assess"),
            Self::Git => write!(f, "git"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Execution state for status bar
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutionState {
    #[default]
    Idle,
    Clarifying,
    Generating,
    Running { activity: Activity },
    Completed,
    Failed,
}

impl std::fmt::Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Clarifying => write!(f, "Clarifying"),
            Self::Generating => write!(f, "Generating"),
            Self::Running { activity } => write!(f, "Running:{}", activity),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// Log level for logs panel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => write!(f, "TRACE"),
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

/// Context for structured logging
#[derive(Debug, Clone, Default)]
pub struct LogContext {
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub phase: Option<String>,
}

/// Wrapper for oneshot sender that implements Clone
#[derive(Debug)]
pub struct AnswerSender(Arc<Mutex<Option<tokio::sync::oneshot::Sender<Vec<String>>>>>);

impl AnswerSender {
    pub fn new(sender: tokio::sync::oneshot::Sender<Vec<String>>) -> Self {
        Self(Arc::new(Mutex::new(Some(sender))))
    }

    pub fn send(self, answers: Vec<String>) -> Result<(), Vec<String>> {
        if let Some(sender) = self.0.lock().unwrap().take() {
            sender.send(answers)
        } else {
            Err(answers)
        }
    }
}

impl Clone for AnswerSender {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Wrapper for oneshot sender for boolean confirmations
#[derive(Debug)]
pub struct ConfirmSender(Arc<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>);

impl ConfirmSender {
    pub fn new(sender: tokio::sync::oneshot::Sender<bool>) -> Self {
        Self(Arc::new(Mutex::new(Some(sender))))
    }

    pub fn send(self, confirmed: bool) -> Result<(), bool> {
        if let Some(sender) = self.0.lock().unwrap().take() {
            sender.send(confirmed)
        } else {
            Err(confirmed)
        }
    }
}

impl Clone for ConfirmSender {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Wrapper for oneshot sender for clarification responses
#[derive(Debug)]
pub struct ClarificationSender(Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>);

impl ClarificationSender {
    pub fn new(sender: tokio::sync::oneshot::Sender<String>) -> Self {
        Self(Arc::new(Mutex::new(Some(sender))))
    }

    pub fn send(self, response: String) -> Result<(), String> {
        if let Some(sender) = self.0.lock().unwrap().take() {
            sender.send(response)
        } else {
            Err(response)
        }
    }
}

impl Clone for ClarificationSender {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Channel sender for waiting on question response
#[derive(Debug)]
pub struct QuestionSender(Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>);

impl QuestionSender {
    pub fn new(sender: tokio::sync::oneshot::Sender<String>) -> Self {
        Self(Arc::new(Mutex::new(Some(sender))))
    }

    pub fn send(self, answer: String) -> Result<(), String> {
        if let Some(sender) = self.0.lock().unwrap().take() {
            sender.send(answer).map_err(|e| e.to_string())
        } else {
            Err("Question already answered".to_string())
        }
    }
}

impl Clone for QuestionSender {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Events emitted by orchestrator for TUI consumption
#[derive(Debug, Clone)]
pub enum Event {
    // Task events
    TaskCreated {
        id: String,
        title: String,
        parent_id: Option<String>,
        depth: u32,
        depends_on: Vec<String>,
    },
    TaskStatusChanged {
        id: String,
        status: TaskStatus,
    },
    TaskProgress {
        id: String,
        message: String,
    },

    // Claude output events
    ClaudeThinking {
        task_id: String,
        content: String,
    },
    ClaudeToolUse {
        task_id: String,
        tool_name: String,
        tool_input: Option<String>,
    },
    ClaudeToolResult {
        task_id: String,
        tool_name: String,
        result: String,
        success: bool,
    },
    ClaudeRequest {
        task_id: String,
        prompt: String,
        model: String,
        timeout_secs: u64,
    },
    ClaudeResult {
        task_id: String,
        result: String,
    },

    // Log events
    Log {
        timestamp: chrono::DateTime<chrono::Utc>,
        level: LogLevel,
        message: String,
        task_id: Option<String>,
        task_title: Option<String>,
        phase: Option<String>,
    },

    // Execution state
    ExecutionStateChanged {
        state: ExecutionState,
    },

    // Progress
    ProgressUpdate {
        completed: usize,
        total: usize,
        failed: usize,
        elapsed: Duration,
    },

    // Model info
    ModelChanged {
        model: String,
    },

    // Clarification questions (ask mode)
    ClarificationQuestions {
        questions: Vec<ClarificationQuestion>,
        response_tx: AnswerSender,
    },

    // Resume confirmation
    ResumeConfirm {
        completed: usize,
        pending: usize,
        failed: usize,
        response_tx: ConfirmSender,
    },

    // Clarification task (when Claude generates a question task)
    ClarificationTask {
        task_id: String,
        title: String,
        description: String,
        response_tx: ClarificationSender,
    },

    // Token usage update
    TokenUsageUpdate {
        task_id: String,
        tokens_used: u32,
    },

    // Activity pulse for heartbeat
    ActivityPulse {
        task_id: String,
        activity: Activity,
    },

    // Progress review
    ProgressReview {
        report: ReviewReport,
    },

    // Agent Q&A events
    /// Agent asks a question (blocking pauses task until answered)
    AgentQuestion {
        task_id: String,
        question: Question,
        response_tx: QuestionSender,
    },

    /// User answered a question (from TUI)
    QuestionAnswered {
        question_id: String,
        answer: String,
    },

    /// Agent auto-decided on non-blocking question
    QuestionAutoDecided {
        question_id: String,
        decision: String,
        reason: String,
    },
}

/// A clarification question with multiple choice options
#[derive(Debug, Clone)]
pub struct ClarificationQuestion {
    /// The question text
    pub question: String,
    /// Predefined options to choose from
    pub options: Vec<String>,
    /// Pros for each option (parallel to options)
    pub pros: Vec<String>,
    /// Cons for each option (parallel to options)
    pub cons: Vec<String>,
    /// Index of recommended option (if any)
    pub recommended: Option<usize>,
    /// Reason for recommendation
    pub recommendation_reason: Option<String>,
}

/// Keyboard event for TUI input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    BackTab,
    Left,
    Right,
    Up,
    Down,
    Char(char),
    Backspace,
    Esc,
    Question,
    Enter,
    PageUp,
    PageDown,
}

/// Event type enum for event loop
#[derive(Debug, Clone)]
pub enum TuiEvent {
    Key(Key),
    MouseScroll { delta: i16 }, // Positive = scroll up, Negative = scroll down
    Resize(u16, u16),
    Orchestrator(Event),
    Tick,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbosity_level_default() {
        assert_eq!(VerbosityLevel::default(), VerbosityLevel::Normal);
    }

    #[test]
    fn test_execution_state_display() {
        assert_eq!(
            ExecutionState::Running { activity: Activity::ApiCall }.to_string(),
            "Running:api"
        );
    }

    #[test]
    fn test_activity_display() {
        assert_eq!(Activity::ApiCall.to_string(), "api");
        assert_eq!(Activity::FileWrite.to_string(), "file");
        assert_eq!(Activity::Test.to_string(), "test");
        assert_eq!(Activity::Planning.to_string(), "plan");
        assert_eq!(Activity::Assessing.to_string(), "assess");
        assert_eq!(Activity::Git.to_string(), "git");
        assert_eq!(Activity::Other("custom").to_string(), "custom");
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Info.to_string(), "INFO");
    }
}
