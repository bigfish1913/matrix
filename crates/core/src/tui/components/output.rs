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
const MAX_TOOL_INPUT_LINES: usize = 2;
const MAX_TOOL_RESULT_LINES: usize = 5;
const MAX_RESULT_LINES: usize = 10;

/// Output panel component
pub struct OutputPanel;

impl OutputPanel {
    /// Render output panel with content
    /// Each line is prefixed with [Task:xxx] to identify which task it belongs to
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
            .scroll((scroll as u16, 0))
    }

    fn format_output_line(
        line: &OutputLine,
        verbosity: VerbosityLevel,
        width: usize,
    ) -> Vec<Line<'static>> {
        // Get task_id prefix
        let task_prefix = match line {
            OutputLine::Thinking { task_id, .. } => format!("[{}] ", task_id),
            OutputLine::ToolUse { task_id, .. } => format!("[{}] ", task_id),
            OutputLine::ToolResult { task_id, .. } => format!("[{}] ", task_id),
            OutputLine::Result { task_id, .. } => format!("[{}] ", task_id),
        };

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
                            Span::styled(
                                format!("{}[Thinking] ", task_prefix),
                                Style::default().fg(Color::Magenta),
                            ),
                            Span::styled(content.clone(), Style::default().fg(Color::Gray)),
                        ]));
                    } else {
                        // Prefix first line with task_id
                        if let Some(first) = md_lines.first_mut() {
                            let mut new_spans = vec![Span::styled(
                                format!("{}[Thinking] ", task_prefix),
                                Style::default().fg(Color::Magenta),
                            )];
                            new_spans.extend(first.spans.clone());
                            *first = Line::from(new_spans);
                        }
                    }
                    md_lines
                } else {
                    // Show limited preview in normal mode
                    let preview: String = content.chars().take(80).collect();
                    vec![Line::from(vec![
                        Span::styled(
                            format!("{}[Thinking] ", task_prefix),
                            Style::default().fg(Color::Magenta),
                        ),
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
                    Span::styled(
                        format!("{}", task_prefix),
                        Style::default().fg(Color::DarkGray),
                    ),
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

                        for (_i, line) in input.lines().take(max_lines).enumerate() {
                            lines.push(Line::styled(
                                format!("{}  {}", task_prefix, line),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }

                        if input.lines().count() > max_lines {
                            lines.push(Line::styled(
                                format!("{}  ...", task_prefix),
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
                    Span::styled(
                        format!("{}", task_prefix),
                        Style::default().fg(Color::DarkGray),
                    ),
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

                    for (_i, line) in result.lines().take(max_lines).enumerate() {
                        lines.push(Line::styled(
                            format!("{}  {}", task_prefix, line),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }

                    if result.lines().count() > max_lines {
                        lines.push(Line::styled(
                            format!(
                                "{}  ... (+{} more lines)",
                                task_prefix,
                                result.lines().count() - max_lines
                            ),
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
                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        format!("{}", task_prefix),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled("── Result ──", Style::default().fg(Color::Yellow)),
                ])];

                let max_lines = if verbosity >= VerbosityLevel::Verbose {
                    50
                } else {
                    MAX_RESULT_LINES
                };

                let md_lines = render_markdown(content, width);
                for (_i, md_line) in md_lines.iter().take(max_lines).enumerate() {
                    let mut new_line = md_line.clone();
                    // Prefix with task_id (as spacing)
                    new_line.spans.insert(
                        0,
                        Span::styled(task_prefix.clone(), Style::default().fg(Color::DarkGray)),
                    );
                    lines.push(new_line);
                }

                if md_lines.len() > max_lines {
                    lines.push(Line::styled(
                        format!(
                            "{}... (+{} more lines)",
                            task_prefix,
                            md_lines.len() - max_lines
                        ),
                        Style::default().fg(Color::Yellow),
                    ));
                }

                lines
            }
        }
    }
}
