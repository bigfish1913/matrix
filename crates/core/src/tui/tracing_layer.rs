//! Tracing layer for sending logs to TUI.

use crate::tui::{Event, EventSender, LogLevel};
use std::fmt;
use tracing::{Event as TracingEvent, Subscriber};
use tracing_subscriber::{layer::Context, Layer};

/// A tracing layer that sends log events to the TUI
pub struct TuiLogLayer {
    sender: EventSender,
}

impl TuiLogLayer {
    /// Create a new TUI log layer
    pub fn new(sender: EventSender) -> Self {
        Self { sender }
    }
}

impl<S> Layer<S> for TuiLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &TracingEvent, _ctx: Context<'_, S>) {
        // Skip logs from certain noisy modules
        let module = event.metadata().module_path().unwrap_or("");
        if module.contains("tokio")
            || module.contains("hyper")
            || module.contains("mio")
            || module.contains("reqwest")
            || module.contains("h2")
        {
            return;
        }

        // Skip DEBUG and TRACE logs to reduce noise
        let level = *event.metadata().level();
        if level == tracing::Level::TRACE || level == tracing::Level::DEBUG {
            return;
        }

        // Get the log level
        let log_level = match level {
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
            _ => return, // Already filtered above
        };

        // Format the message and extract context
        let mut message = String::new();
        let mut task_id = None;
        let mut task_title = None;
        let mut phase = None;

        let mut visitor = ContextAwareVisitor {
            message: &mut message,
            task_id: &mut task_id,
            task_title: &mut task_title,
            phase: &mut phase,
        };
        event.record(&mut visitor);

        // Send to TUI with context
        let timestamp = chrono::Utc::now();
        let _ = self.sender.send(Event::Log {
            timestamp,
            level: log_level,
            message,
            task_id,
            task_title,
            phase,
        });
    }
}

/// A visitor to extract the message and context from tracing fields
struct ContextAwareVisitor<'a> {
    message: &'a mut String,
    task_id: &'a mut Option<String>,
    task_title: &'a mut Option<String>,
    phase: &'a mut Option<String>,
}

impl<'a> tracing::field::Visit for ContextAwareVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        match field.name() {
            "message" => self.message.push_str(&format!("{:?}", value)),
            "task_id" | "id" => {
                *self.task_id = Some(format!("{:?}", value).trim_matches('"').to_string())
            }
            "title" | "task_title" => {
                *self.task_title = Some(format!("{:?}", value).trim_matches('"').to_string())
            }
            "phase" => *self.phase = Some(format!("{:?}", value).trim_matches('"').to_string()),
            _ => {
                // Include other fields in message
                if !self.message.is_empty() {
                    self.message.push_str(", ");
                }
                self.message
                    .push_str(&format!("{}={:?}", field.name(), value));
            }
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "message" => self.message.push_str(value),
            "task_id" | "id" => *self.task_id = Some(value.to_string()),
            "title" | "task_title" => *self.task_title = Some(value.to_string()),
            "phase" => *self.phase = Some(value.to_string()),
            _ => {
                if !self.message.is_empty() {
                    self.message.push_str(", ");
                }
                self.message
                    .push_str(&format!("{}={}", field.name(), value));
            }
        }
    }
}
