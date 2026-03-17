//! Terminal User Interface for Matrix Orchestrator.

use std::sync::Arc;
use tokio::sync::mpsc;

pub mod app;
pub mod components;
pub mod event;
pub mod render;
pub mod terminal;

/// Channel for orchestrator to send events to TUI
pub type EventSender = mpsc::UnboundedSender<Event>;

/// Channel for TUI to receive events from orchestrator
pub type EventReceiver = mpsc::UnboundedReceiver<Event>;

/// Create an event channel for orchestrator -> TUI communication
pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

/// Log buffer for sharing logs between tracing and TUI
#[derive(Debug, Clone)]
pub struct LogBuffer {
    entries: Arc<std::sync::Mutex<Vec<LogEntry>>>,
    max_entries: usize,
}

/// A single log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub message: String,
}

impl LogBuffer {
    /// Create a new LogBuffer with the specified max entries
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(std::sync::Mutex::new(Vec::new())),
            max_entries,
        }
    }

    /// Push a new log entry
    pub fn push(&self, level: LogLevel, message: String) {
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level,
            message,
        };
        let mut entries = self.entries.lock().unwrap();
        entries.push(entry);
        if entries.len() > self.max_entries {
            entries.remove(0);
        }
    }

    /// Get all log entries
    pub fn get_entries(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().clone()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(1000)
    }
}

pub use app::TuiApp;
pub use event::{Event, ExecutionState, Key, LogLevel, TuiEvent, VerbosityLevel};
pub use render::{render_app, MatrixTerminal};
pub use terminal::{event_stream, init_terminal, restore_terminal};

#[cfg(test)]
mod tests;