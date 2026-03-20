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
    /// Get icon and color for log level
    fn level_style(level: LogLevel) -> (&'static str, Color) {
        match level {
            LogLevel::Trace => ("·", Color::DarkGray),
            LogLevel::Debug => ("○", Color::Gray),
            LogLevel::Info => ("✓", Color::Green),
            LogLevel::Warn => ("⚠", Color::Yellow),
            LogLevel::Error => ("✗", Color::Red),
        }
    }

    /// Render logs panel with grouped display
    /// `scroll_offset`: scroll position from top (0 = start)
    /// `viewport_height`: height of the visible area (for wrap calculation, currently unused)
    pub fn render(
        entries: &[LogEntry],
        scroll_offset: u16,
        _viewport_height: u16,
    ) -> Paragraph<'static> {
        let mut lines: Vec<Line> = Vec::new();
        let mut current_task_id: Option<&str> = None;
        let mut current_phase: Option<&str> = None;

        for entry in entries {
            // Check if we need a group header
            let needs_header = entry.task_id.as_deref() != current_task_id
                || (entry.task_id.is_none() && entry.phase.as_deref() != current_phase);

            if needs_header {
                if let Some(ref task_id) = entry.task_id {
                    // Task group header
                    let title = entry.task_title.as_deref().unwrap_or("");
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("━━━ {}: {} ━━━", task_id, title),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]));
                } else if let Some(ref phase) = entry.phase {
                    // Phase group header
                    let phase_display = match phase.as_str() {
                        "clarification" => "Clarification",
                        "generating" => "Generating",
                        "running" => "Running",
                        "testing" => "Testing",
                        "summary" => "Summary",
                        _ => phase,
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("━━━ {} ━━━", phase_display),
                            Style::default().fg(Color::Magenta),
                        ),
                    ]));
                }

                current_task_id = entry.task_id.as_deref();
                current_phase = entry.phase.as_deref();
            }

            // Format time as HH:MM
            let time = entry.timestamp.format("%H:%M");

            // Get level icon and color
            let (icon, color) = Self::level_style(entry.level);

            // Show repeat count if message was duplicated
            let message = if entry.repeat_count > 1 {
                format!("{} · x{}", entry.message, entry.repeat_count)
            } else {
                entry.message.clone()
            };

            // Build log line with appropriate indentation
            if entry.task_id.is_some() {
                // Task logs: indented
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(message, Style::default().fg(Color::White)),
                ]));
            } else if entry.phase.is_some() {
                // Phase logs: indented
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(message, Style::default().fg(Color::White)),
                ]));
            } else {
                // System logs: no indent, use info icon
                lines.push(Line::from(vec![
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled("ℹ", Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(message, Style::default().fg(Color::White)),
                ]));
            }
        }

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
