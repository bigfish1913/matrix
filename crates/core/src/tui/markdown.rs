//! Markdown rendering for TUI.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Render markdown text to ratatui Lines
pub fn render_markdown(markdown: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_line: Vec<Span<'static>> = Vec::new();
    let mut current_style = Style::default();
    let mut in_code_block = false;
    let mut in_list = false;
    let mut list_number = 0;
    let mut heading_level = 0;

    let options = Options::empty();
    let parser = Parser::new_ext(markdown, options);

    for event in parser {
        match event {
            Event::Start(Tag::Heading(level, ..)) => {
                heading_level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current_style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
            }
            Event::End(Tag::Heading(..)) => {
                if !current_line.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_line)));
                }
                heading_level = 0;
                current_style = Style::default();
            }
            Event::Start(Tag::Paragraph) => {
                current_style = Style::default();
            }
            Event::End(Tag::Paragraph) => {
                if !current_line.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_line)));
                }
                lines.push(Line::from("")); // Empty line after paragraph
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                current_style = Style::default().fg(Color::Yellow);
                lines.push(Line::from("")); // Empty line before code
            }
            Event::End(Tag::CodeBlock(_)) => {
                if !current_line.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_line)));
                }
                in_code_block = false;
                current_style = Style::default();
                lines.push(Line::from("")); // Empty line after code
            }
            Event::Start(Tag::List(first)) => {
                in_list = true;
                list_number = first.unwrap_or(1);
            }
            Event::End(Tag::List(_)) => {
                in_list = false;
            }
            Event::Start(Tag::Item) => {
                if in_list {
                    let prefix = format!("  {}. ", list_number);
                    current_line.push(Span::styled(prefix, Style::default().fg(Color::Cyan)));
                    list_number += 1;
                }
            }
            Event::End(Tag::Item) => {
                if !current_line.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_line)));
                }
            }
            Event::Start(Tag::Strong) => {
                current_style = current_style.add_modifier(Modifier::BOLD);
            }
            Event::End(Tag::Strong) => {
                current_style = current_style.remove_modifier(Modifier::BOLD);
            }
            Event::Start(Tag::Emphasis) => {
                current_style = current_style.add_modifier(Modifier::ITALIC);
            }
            Event::End(Tag::Emphasis) => {
                current_style = current_style.remove_modifier(Modifier::ITALIC);
            }
            Event::Code(text) => {
                // Inline code
                current_line.push(Span::styled(
                    text.to_string(),
                    Style::default().fg(Color::Yellow),
                ));
            }
            Event::Text(text) => {
                let text = text.to_string();

                // Handle code blocks with line wrapping
                if in_code_block {
                    for line_text in text.lines() {
                        let mut code_line: Vec<Span<'static>> = Vec::new();
                        code_line.push(Span::styled("    ", Style::default().fg(Color::DarkGray)));

                        // Wrap long lines
                        if line_text.len() > width.saturating_sub(4) {
                            let chunks = wrap_text(line_text, width.saturating_sub(4));
                            for (i, chunk) in chunks.into_iter().enumerate() {
                                if i > 0 {
                                    lines.push(Line::from(std::mem::take(&mut code_line)));
                                    code_line.push(Span::styled("    ", Style::default().fg(Color::DarkGray)));
                                }
                                code_line.push(Span::styled(chunk, Style::default().fg(Color::Yellow)));
                            }
                        } else {
                            code_line.push(Span::styled(line_text.to_string(), Style::default().fg(Color::Yellow)));
                        }

                        if !code_line.is_empty() {
                            lines.push(Line::from(code_line));
                        }
                    }
                } else if heading_level > 0 {
                    // Add heading prefix
                    let prefix = "▌".repeat(heading_level.min(3));
                    current_line.push(Span::styled(
                        format!("{} ", prefix),
                        Style::default().fg(Color::Cyan),
                    ));
                    current_line.push(Span::styled(text, current_style));
                } else {
                    // Regular text - handle wrapping
                    let current_len: usize = current_line.iter().map(|s| s.content.len()).sum();
                    if current_len + text.len() > width {
                        lines.push(Line::from(std::mem::take(&mut current_line)));
                    }
                    current_line.push(Span::styled(text, current_style));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if !current_line.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_line)));
                }
            }
            _ => {}
        }
    }

    // Don't forget any remaining content
    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    lines
}

/// Wrap text to specified width
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.len() + word.len() + 1 > width {
            if !current.is_empty() {
                result.push(current.trim().to_string());
                current = String::new();
            }
            // If word is longer than width, just add it
            if word.len() > width {
                result.push(word.to_string());
            } else {
                current = word.to_string();
                current.push(' ');
            }
        } else {
            current.push_str(word);
            current.push(' ');
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    if result.is_empty() {
        vec![text.to_string()]
    } else {
        result
    }
}

/// Simple markdown stripping for plain text display
pub fn strip_markdown(markdown: &str) -> String {
    let mut result = String::new();
    let options = Options::empty();
    let parser = Parser::new_ext(markdown, options);

    for event in parser {
        if let Event::Text(text) = event {
            result.push_str(&text);
        }
    }

    result
}
