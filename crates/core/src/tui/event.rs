//! TUI Event handling.

use crate::models::TaskStatus;
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

/// Execution state for status bar
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutionState {
    #[default]
    Idle,
    Clarifying,
    Generating,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Clarifying => write!(f, "Clarifying"),
            Self::Generating => write!(f, "Generating"),
            Self::Running => write!(f, "Running"),
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

/// Events emitted by orchestrator for TUI consumption
#[derive(Debug, Clone)]
pub enum Event {
    // Task events
    TaskCreated {
        id: String,
        title: String,
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
    ClaudeResult {
        task_id: String,
        result: String,
    },

    // Log events
    Log {
        timestamp: chrono::DateTime<chrono::Utc>,
        level: LogLevel,
        message: String,
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
}

/// A clarification question with multiple choice options
#[derive(Debug, Clone)]
pub struct ClarificationQuestion {
    /// The question text
    pub question: String,
    /// Predefined options to choose from
    pub options: Vec<String>,
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
}

/// Event type enum for event loop
#[derive(Debug, Clone)]
pub enum TuiEvent {
    Key(Key),
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
        assert_eq!(ExecutionState::Running.to_string(), "Running");
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Info.to_string(), "INFO");
    }
}