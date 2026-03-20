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

        // Format the message
        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        // Send to TUI
        let timestamp = chrono::Utc::now();
        let _ = self.sender.send(Event::Log {
            timestamp,
            level: log_level,
            message,
        });
    }
}

/// A visitor to extract the message from tracing fields
struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!("{:?}", value));
        } else {
            if !self.0.is_empty() {
                self.0.push_str(", ");
            }
            self.0.push_str(&format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            if !self.0.is_empty() {
                self.0.push_str(", ");
            }
            self.0.push_str(&format!("{}={}", field.name(), value));
        }
    }
}
