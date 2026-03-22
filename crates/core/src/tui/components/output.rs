// crates/core/src/tui/components/output.rs

use crate::tui::app::OutputLine;
use crate::tui::markdown::render_markdown;
use crate::tui::VerbosityLevel;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Maximum lines to show for each output type in normal mode
const MAX_THINKING_LINES: usize = 3;
const MAX_TOOL_INPUT_LINES: usize = 2;
const MAX_TOOL_RESULT_LINES: usize = 5;
const MAX_RESULT_LINES: usize = 10;

/// Output panel component
pub struct OutputPanel;

impl OutputPanel {
    /// Render output panel with content
    pub fn render(
        lines: &[OutputLine],
        verbosity: VerbosityLevel,
        scroll: usize,
    ) -> Paragraph<'static> {
        let width = 80; // Approximate width for markdown wrapping
        let text_lines: Vec<Line> = lines
            .iter()
            .flat_map(|line| Self::format_output_line(line, verbosity, width))
            .collect();

        let title = format!(" Claude Output ({} lines) ", text_lines.len());

        Paragraph::new(text_lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0))
    }

    fn format_output_line(
        line: &OutputLine,
        verbosity: VerbosityLevel,
        width: usize,
    ) -> Vec<Line<'static>> {
        match line {
            OutputLine::Thinking {
                task_id: _,
                content,
                seq: _,
            } => {
                // Show thinking content in verbose mode, limited in normal mode
                if verbosity >= VerbosityLevel::Verbose {
                    let mut md_lines = render_markdown(content, width);
                    if md_lines.is_empty() {
                        md_lines.push(Line::from(vec![
                            Span::styled("[Thinking] ", Style::default().fg(Color::Magenta)),
                            Span::styled(content.clone(), Style::default().fg(Color::Gray)),
                        ]));
                    }
                    md_lines
                } else {
                    // Show limited preview in normal mode
                    let preview: String = content.chars().take(80).collect();
                    vec![Line::from(vec![
                        Span::styled("[Thinking] ", Style::default().fg(Color::Magenta)),
                        Span::styled(
                            if content.len() > 80 {
                                format!("{}...", preview)
                            } else {
                                preview
                            },
                            Style::default().fg(Color::Gray),
                        ),
                    ])]
                }
            }
            OutputLine::ToolUse {
                task_id: _,
                tool_name,
                tool_input,
                seq: _,
            } => {
                let mut lines = vec![Line::from(vec![
                    Span::styled("[", Style::default().fg(Color::DarkGray)),
                    Span::styled(tool_name.clone(), Style::default().fg(Color::Cyan)),
                    Span::styled("]", Style::default().fg(Color::DarkGray)),
                ])];

                // Show input preview
                if let Some(input) = tool_input {
                    if !input.is_empty() {
                        let max_lines = if verbosity >= VerbosityLevel::Verbose {
                            input.lines().count()
                        } else {
                            MAX_TOOL_INPUT_LINES
                        };

                        for (i, line) in input.lines().take(max_lines).enumerate() {
                            lines.push(Line::styled(
                                format!("  {}", line),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }

                        if input.lines().count() > max_lines {
                            lines.push(Line::styled(
                                "  ...",
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                    }
                }

                lines
            }
            OutputLine::ToolResult {
                task_id: _,
                tool_name,
                result,
                success,
                seq: _,
            } => {
                let icon = if *success { " OK" } else { " ERR" };
                let color = if *success { Color::Green } else { Color::Red };

                let mut lines = vec![Line::from(vec![
                    Span::styled("[", Style::default().fg(Color::DarkGray)),
                    Span::styled(tool_name.clone(), Style::default().fg(Color::Cyan)),
                    Span::styled("] ", Style::default().fg(Color::DarkGray)),
                    Span::styled(icon, Style::default().fg(color)),
                ])];

                // Show result content with limit
                if !result.is_empty() {
                    let max_lines = if verbosity >= VerbosityLevel::Verbose {
                        50 // Even in verbose mode, limit to 50 lines
                    } else {
                        MAX_TOOL_RESULT_LINES
                    };

                    for (i, line) in result.lines().take(max_lines).enumerate() {
                        lines.push(Line::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }

                    if result.lines().count() > max_lines {
                        lines.push(Line::styled(
                            format!("  ... (+{} more lines)", result.lines().count() - max_lines),
                            Style::default().fg(Color::Yellow),
                        ));
                    }
                }

                lines
            }
            OutputLine::Result {
                task_id: _,
                content,
                seq: _,
            } => {
                // Render result as markdown with limit
                let mut lines = vec![Line::styled(
                    "── Result ──",
                    Style::default().fg(Color::Yellow),
                )];

                let max_lines = if verbosity >= VerbosityLevel::Verbose {
                    50
                } else {
                    MAX_RESULT_LINES
                };

                let md_lines = render_markdown(content, width);
                for (i, md_line) in md_lines.iter().take(max_lines).enumerate() {
                    lines.push(md_line.clone());
                }

                if md_lines.len() > max_lines {
                    lines.push(Line::styled(
                        format!("... (+{} more lines)", md_lines.len() - max_lines),
                        Style::default().fg(Color::Yellow),
                    ));
                }

                lines
            }
        }
    }
}
