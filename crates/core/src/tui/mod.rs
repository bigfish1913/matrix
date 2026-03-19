//! Terminal User Interface for Matrix Orchestrator.

use std::sync::Arc;
use tokio::sync::mpsc;

pub mod app;
pub mod components;
pub mod event;
pub mod render;
pub mod terminal;
pub mod tracing_layer;

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
    pub repeat_count: usize,  // Number of times this message pattern was repeated
}

impl LogBuffer {
    /// Create a new LogBuffer with the specified max entries
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(std::sync::Mutex::new(Vec::new())),
            max_entries,
        }
    }

    /// Extract pattern from message by replacing numbers with placeholder
    fn extract_pattern(message: &str) -> String {
        // Replace sequences of digits with {n}
        let mut result = String::with_capacity(message.len());
        let mut in_number = false;
        for c in message.chars() {
            if c.is_ascii_digit() {
                if !in_number {
                    result.push_str("{n}");
                    in_number = true;
                }
                // Skip digit, already added placeholder
            } else {
                result.push(c);
                in_number = false;
            }
        }
        result
    }

    /// Push a new log entry (deduplicates similar messages with different numbers)
    pub fn push(&self, level: LogLevel, message: String) {
        let mut entries = self.entries.lock().unwrap();
        let pattern = Self::extract_pattern(&message);

        // Check if this matches the pattern of the last message
        if let Some(last) = entries.last_mut() {
            let last_pattern = Self::extract_pattern(&last.message);
            if last.level == level && last_pattern == pattern {
                // Same pattern, increment repeat count and update message to show range
                last.repeat_count += 1;
                return;
            }
        }

        // Add new entry
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level,
            message,
            repeat_count: 1,
        };
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
pub use terminal::{event_stream, init_terminal, restore_terminal, TerminalGuard};
pub use tracing_layer::TuiLogLayer;

#[cfg(test)]
mod tests;