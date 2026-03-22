// crates/core/src/tui/components/output.rs

use crate::tui::app::OutputLine;
use crate::tui::markdown::render_markdown;
use crate::tui::VerbosityLevel;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

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
                // Show thinking content in verbose mode
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
                    vec![]
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

                // Show full input in verbose mode, preview in normal mode
                if let Some(input) = tool_input {
                    if verbosity >= VerbosityLevel::Verbose {
                        // Show full input
                        for line in input.lines() {
                            lines.push(Line::styled(
                                format!("  {}", line),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                    } else if !input.is_empty() {
                        // Show preview (first 100 chars)
                        let preview: String = input.chars().take(100).collect();
                        lines.push(Line::styled(
                            format!("  {}", preview),
                            Style::default().fg(Color::DarkGray),
                        ));
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
                let icon = if *success { " " } else { "" };
                let color = if *success { Color::Green } else { Color::Red };

                let mut lines = vec![Line::from(vec![
                    Span::styled("[", Style::default().fg(Color::DarkGray)),
                    Span::styled(tool_name.clone(), Style::default().fg(Color::Cyan)),
                    Span::styled("] ", Style::default().fg(Color::DarkGray)),
                    Span::styled(icon, Style::default().fg(color)),
                ])];

                // Show result content (more in verbose mode)
                if !result.is_empty() {
                    if verbosity >= VerbosityLevel::Verbose {
                        // Full markdown rendering in verbose mode
                        let md_lines = render_markdown(result, width);
                        if !md_lines.is_empty() {
                            for md_line in md_lines {
                                let mut indented_spans: Vec<Span<'static>> =
                                    vec![Span::styled("  ", Style::default().fg(Color::DarkGray))];
                                for span in md_line.spans {
                                    indented_spans.push(span);
                                }
                                lines.push(Line::from(indented_spans));
                            }
                        }
                    } else {
                        // Show first 10 lines in normal mode
                        for line in result.lines().take(10) {
                            lines.push(Line::styled(
                                format!("  {}", line),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                        if result.lines().count() > 10 {
                            lines.push(Line::styled(
                                "  ... (more in verbose mode)",
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                    }
                }

                lines
            }
            OutputLine::Result {
                task_id: _,
                content,
                seq: _,
            } => {
                // Render result as markdown
                let mut lines = vec![Line::styled(
                    "── Result ──",
                    Style::default().fg(Color::Yellow),
                )];
                let md_lines = render_markdown(content, width);
                lines.extend(md_lines);
                lines
            }
        }
    }
}
