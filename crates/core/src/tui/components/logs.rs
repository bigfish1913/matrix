//! Logs panel component.

use crate::tui::{LogEntry, LogLevel};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Logs panel component
pub struct LogsPanel;

impl LogsPanel {
    /// Get color for log level
    fn level_color(level: LogLevel) -> Color {
        match level {
            LogLevel::Trace => Color::DarkGray,
            LogLevel::Debug => Color::Gray,
            LogLevel::Info => Color::Green,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        }
    }

    /// Render logs panel
    /// `scroll_offset`: scroll position from top (0 = start)
    /// `viewport_height`: height of the visible area (for wrap calculation, currently unused)
    pub fn render(
        entries: &[LogEntry],
        scroll_offset: u16,
        _viewport_height: u16,
    ) -> Paragraph<'static> {
        let lines: Vec<Line> = entries
            .iter()
            .map(|entry| {
                let time = entry.timestamp.format("%H:%M:%S");
                // Show repeat count if message was duplicated
                let message = if entry.repeat_count > 1 {
                    format!("{} (x{})", entry.message, entry.repeat_count)
                } else {
                    entry.message.clone()
                };
                Line::from(vec![
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:5}", entry.level),
                        Style::default().fg(Self::level_color(entry.level)),
                    ),
                    Span::raw("  "),
                    Span::styled(message, Style::default().fg(Color::White)),
                ])
            })
            .collect();

        // Use the scroll_offset directly - the caller is responsible for calculating the correct value
        Paragraph::new(lines)
            .block(Block::default().title(" Logs ").borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0))
    }

    /// Calculate scroll offset for auto-follow mode to show latest entries at bottom
    pub fn calculate_auto_scroll(total_entries: usize, viewport_height: u16) -> u16 {
        let total = total_entries as u16;
        // If viewport is 0 or all entries fit, no scroll needed
        if viewport_height == 0 || total <= viewport_height {
            0
        } else {
            // Scroll enough to show the last entries at the bottom
            total.saturating_sub(viewport_height)
        }
    }
}
