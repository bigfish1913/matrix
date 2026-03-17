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
        // Get the log level
        let level = match *event.metadata().level() {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        };

        // Format the message
        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        // Send to TUI
        let timestamp = chrono::Utc::now();
        let _ = self.sender.send(Event::Log {
            timestamp,
            level,
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
