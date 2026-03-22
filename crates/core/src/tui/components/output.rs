// crates/core/src/tui/components/output.rs

use crate::tui::app::OutputLine;
use crate::tui::markdown::render_markdown;
use crate::tui::VerbosityLevel;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Output panel component with task tabs
pub struct OutputPanel;

impl OutputPanel {
    /// Render output panel with task tabs and content
    pub fn render(
        lines: &[OutputLine],
        task_id: Option<&str>,
        verbosity: VerbosityLevel,
        scroll: usize,
        task_count: usize,
        has_output: &dyn Fn(&str) -> bool,
    ) -> Paragraph<'static> {
        // Build title with task indicator
        let title = match task_id {
            Some(id) => format!(" Claude Output [{}] ", id),
            None => format!(" Claude Output (All {} tasks) ", task_count),
        };

        let width = 80; // Approximate width for markdown wrapping
        let text_lines: Vec<Line> = lines
            .iter()
            .flat_map(|line| Self::format_output_line(line, verbosity, width))
            .collect();

        Paragraph::new(text_lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0))
    }

    /// Render task tabs for output panel
    pub fn render_task_tabs<'a>(
        tasks: &'a [crate::tui::app::TaskDisplay],
        current_task_id: Option<&str>,
        available_width: u16,
    ) -> Paragraph<'a> {
        let mut spans: Vec<Span<'a>> = vec![Span::styled(
            "Tasks: ",
            Style::default().fg(Color::DarkGray),
        )];

        // Add "All" option
        let is_all = current_task_id.is_none();
        let all_style = if is_all {
            Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(if is_all { "[All]" } else { " All " }, all_style));

        // Add task tabs (limit to fit width)
        let mut used_width = 10u16; // "Tasks: " + "[All]"
        for (idx, task) in tasks.iter().enumerate() {
            if idx >= 9 {
                spans.push(Span::styled(" ...", Style::default().fg(Color::DarkGray)));
                break;
            }

            let tab_text = format!(" {} ", idx + 1);
            let tab_len = tab_text.len() as u16;
            let is_current = current_task_id == Some(&task.id);

            let style = if is_current {
                Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };

            spans.push(Span::styled(tab_text, style));
            used_width += tab_len;

            if used_width > available_width.saturating_sub(10) {
                break;
            }
        }

        // Add hint
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            "1-9:task a:all ↑↓:scroll",
            Style::default().fg(Color::DarkGray),
        ));

        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::BOTTOM))
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
