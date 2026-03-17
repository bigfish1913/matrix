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
    pub fn render(entries: &[LogEntry], scroll: usize) -> Paragraph<'static> {
        let lines: Vec<Line> = entries
            .iter()
            .skip(scroll)
            .map(|entry| {
                let time = entry.timestamp.format("%H:%M:%S");
                Line::from(vec![
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:5}", entry.level),
                        Style::default().fg(Self::level_color(entry.level)),
                    ),
                    Span::raw("  "),
                    Span::styled(entry.message.clone(), Style::default().fg(Color::White)),
                ])
            })
            .collect();

        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Logs ")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false })
    }
}