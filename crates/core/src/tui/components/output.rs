// crates/core/src/tui/components/output.rs

use crate::tui::app::OutputLine;
use crate::tui::VerbosityLevel;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Output panel component
pub struct OutputPanel;

impl OutputPanel {
    /// Render output panel
    pub fn render(
        lines: &[OutputLine],
        task_id: Option<&str>,
        verbosity: VerbosityLevel,
        scroll: usize,
    ) -> Paragraph<'static> {
        let title = match task_id {
            Some(id) => format!(" Claude Output ({}) ", id),
            None => " Claude Output ".to_string(),
        };

        let text_lines: Vec<Line> = lines
            .iter()
            .skip(scroll)
            .flat_map(|line| Self::format_output_line(line, verbosity))
            .collect();

        Paragraph::new(text_lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false })
    }

    fn format_output_line(line: &OutputLine, verbosity: VerbosityLevel) -> Vec<Line<'static>> {
        match line {
            OutputLine::Thinking { content } => {
                if verbosity == VerbosityLevel::Verbose {
                    vec![Line::from(vec![
                        Span::styled("[Thinking] ", Style::default().fg(Color::Magenta)),
                        Span::styled(content.clone(), Style::default().fg(Color::Gray)),
                    ])]
                } else {
                    vec![]
                }
            }
            OutputLine::ToolUse { tool_name, tool_input } => {
                let input_preview = tool_input
                    .as_ref()
                    .map(|i| format!(" {}", i.chars().take(50).collect::<String>()))
                    .unwrap_or_default();

                vec![Line::from(vec![
                    Span::styled("[", Style::default().fg(Color::DarkGray)),
                    Span::styled(tool_name.clone(), Style::default().fg(Color::Cyan)),
                    Span::styled("]", Style::default().fg(Color::DarkGray)),
                    Span::styled(input_preview, Style::default().fg(Color::DarkGray)),
                ])]
            }
            OutputLine::ToolResult { tool_name, result, success } => {
                let icon = if *success { "✓" } else { "✗" };
                let color = if *success { Color::Green } else { Color::Red };

                let mut lines = vec![Line::from(vec![
                    Span::styled("[", Style::default().fg(Color::DarkGray)),
                    Span::styled(tool_name.clone(), Style::default().fg(Color::Cyan)),
                    Span::styled("] ", Style::default().fg(Color::DarkGray)),
                    Span::styled(icon, Style::default().fg(color)),
                ])];

                // Show result preview in verbose mode
                if verbosity == VerbosityLevel::Verbose && !result.is_empty() {
                    let preview: String = result.lines().take(3).collect::<Vec<_>>().join("\n");
                    if !preview.is_empty() {
                        lines.push(Line::styled(
                            format!("  → {}", preview.replace('\n', "\n  → ")),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }

                lines
            }
            OutputLine::Result { content } => {
                vec![
                    Line::styled("── Result ──", Style::default().fg(Color::Yellow)),
                    Line::styled(content.clone(), Style::default().fg(Color::White)),
                ]
            }
        }
    }
}