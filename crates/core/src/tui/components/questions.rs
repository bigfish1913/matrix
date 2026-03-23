//! Questions panel component for displaying and answering agent questions.

use crate::models::{Question, QuestionStatus};
use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

/// Questions panel component
pub struct QuestionsPanel {
    /// List state for selection
    pub state: ListState,
    /// Whether in answer dialog mode
    pub in_answer_dialog: bool,
    /// Currently selected option in answer dialog
    pub dialog_selection: usize,
    /// Custom input for "Other" option
    pub custom_input: String,
    /// Whether typing custom input
    pub is_custom_input: bool,
}

impl Default for QuestionsPanel {
    fn default() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            state,
            in_answer_dialog: false,
            dialog_selection: 0,
            custom_input: String::new(),
            is_custom_input: false,
        }
    }
}

impl QuestionsPanel {
    /// Create new questions panel
    pub fn new() -> Self {
        Self::default()
    }

    /// Format timestamp for display
    fn format_timestamp(ts: &DateTime<Utc>) -> String {
        ts.format("%H:%M").to_string()
    }

    /// Get status icon for a question
    fn status_icon(status: QuestionStatus, blocking: bool) -> &'static str {
        match status {
            QuestionStatus::Pending => {
                if blocking {
                    "⏳"
                } else {
                    "○"
                }
            }
            QuestionStatus::Answered => "✓",
            QuestionStatus::AutoDecided => "◆",
            QuestionStatus::Expired => "⊘",
        }
    }

    /// Get status color
    fn status_color(status: QuestionStatus) -> Color {
        match status {
            QuestionStatus::Pending => Color::Yellow,
            QuestionStatus::Answered => Color::Green,
            QuestionStatus::AutoDecided => Color::Blue,
            QuestionStatus::Expired => Color::DarkGray,
        }
    }

    /// Move selection up
    pub fn select_up(&mut self, questions_len: usize) {
        if questions_len == 0 {
            return;
        }
        let current = self.state.selected().unwrap_or(0);
        let new_selection = if current == 0 {
            questions_len - 1
        } else {
            current - 1
        };
        self.state.select(Some(new_selection));
    }

    /// Move selection down
    pub fn select_down(&mut self, questions_len: usize) {
        if questions_len == 0 {
            return;
        }
        let current = self.state.selected().unwrap_or(0);
        let new_selection = if current >= questions_len - 1 {
            0
        } else {
            current + 1
        };
        self.state.select(Some(new_selection));
    }

    /// Get currently selected question index
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }

    /// Render the questions panel
    pub fn render(&mut self, frame: &mut Frame, area: Rect, questions: &[Question]) {
        if questions.is_empty() {
            let paragraph = Paragraph::new("No questions yet")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().title(" Questions ").borders(Borders::ALL));
            frame.render_widget(paragraph, area);
            return;
        }

        // Separate pending and answered questions
        let pending: Vec<Question> = questions
            .iter()
            .filter(|q| q.status == QuestionStatus::Pending)
            .cloned()
            .collect();

        // Collect answered questions (non-pending)
        let answered: Vec<Question> = questions
            .iter()
            .filter(|q| q.status != QuestionStatus::Pending)
            .cloned()
            .collect();

        // Build list items with owned data
        let mut items: Vec<ListItem<'static>> = Vec::new();

        // Pending section header
        if !pending.is_empty() {
            items.push(ListItem::new(Line::from(vec![Span::styled(
                "━━━ Pending Questions ━━━",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )])));
            items.push(ListItem::new(Line::from("")));

            for q in &pending {
                let icon = Self::status_icon(q.status, q.blocking);
                let color = Self::status_color(q.status);
                let blocking_indicator = if q.blocking {
                    Span::styled(" blocking", Style::default().fg(Color::Red))
                } else {
                    Span::raw("")
                };

                // Truncate question text
                let question_text: String = if q.question.chars().count() > 40 {
                    format!("{}...", q.question.chars().take(37).collect::<String>())
                } else {
                    q.question.clone()
                };

                let timestamp = Self::format_timestamp(&q.created_at);

                let spans = vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(q.id.clone(), Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}]", timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!(" [{}]", q.task_id),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(" "),
                    Span::styled(question_text, Style::default().fg(Color::White)),
                    blocking_indicator,
                ];

                items.push(ListItem::new(Line::from(spans)));
            }
        }

        // Answered section header
        if !answered.is_empty() {
            if !pending.is_empty() {
                items.push(ListItem::new(Line::from("")));
            }
            items.push(ListItem::new(Line::from(vec![Span::styled(
                "━━━ Answered ━━━",
                Style::default().fg(Color::Green),
            )])));
            items.push(ListItem::new(Line::from("")));

            for q in &answered {
                let icon = Self::status_icon(q.status, q.blocking);
                let color = Self::status_color(q.status);
                let answer_text = q.answer.as_deref().unwrap_or("(no answer)");
                let status_text = match q.status {
                    QuestionStatus::Answered => format!("answered: {}", answer_text),
                    QuestionStatus::AutoDecided => format!("auto-decided: {}", answer_text),
                    QuestionStatus::Expired => "expired".to_string(),
                    QuestionStatus::Pending => unreachable!(),
                };

                // Truncate question text
                let question_text: String = if q.question.chars().count() > 30 {
                    format!("{}...", q.question.chars().take(27).collect::<String>())
                } else {
                    q.question.clone()
                };

                // Show answer on same line if space allows
                let status_display: String = if status_text.chars().count() > 20 {
                    format!("{}...", status_text.chars().take(17).collect::<String>())
                } else {
                    status_text
                };

                // Use answered_at if available, otherwise created_at
                let timestamp = q
                    .answered_at
                    .as_ref()
                    .map(|t| Self::format_timestamp(t))
                    .unwrap_or_else(|| Self::format_timestamp(&q.created_at));

                let spans = vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(q.id.clone(), Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}]", timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!(" [{}]", q.task_id),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(" "),
                    Span::styled(question_text, Style::default().fg(Color::White)),
                    Span::styled(
                        format!(" ✓ {}", status_display),
                        Style::default().fg(Color::DarkGray),
                    ),
                ];

                items.push(ListItem::new(Line::from(spans)));
            }
        }

        // Calculate the offset for the list (skip headers)
        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" Questions ({}) ", questions.len()))
                    .borders(Borders::ALL),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    /// Render the answer dialog
    pub fn render_answer_dialog(&mut self, frame: &mut Frame, area: Rect, question: &Question) {
        // Create centered popup
        let popup_area = centered_rect(85, 80, area);

        // Clear the area
        frame.render_widget(ratatui::widgets::Clear, popup_area);

        let mut lines: Vec<Line> = Vec::new();
        let width = popup_area.width.saturating_sub(4) as usize;

        // Header
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " Answer Question ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        // Question text
        let q_text = format!("▶ {}", question.question);
        lines.push(Line::from(vec![Span::styled(
            q_text,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        // Options
        for (opt_idx, opt) in question.options.iter().enumerate() {
            let is_selected = self.dialog_selection == opt_idx;
            let is_recommended = question.recommended == Some(opt_idx);

            let prefix = if is_selected { "  ◉ " } else { "  ○ " };
            let num = format!("{}. ", opt_idx + 1);

            let style = if is_selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            let opt_text = if opt.chars().count() > width.saturating_sub(10) {
                format!(
                    "{}...",
                    opt.chars()
                        .take(width.saturating_sub(13))
                        .collect::<String>()
                )
            } else {
                opt.clone()
            };

            let mut spans = vec![
                Span::styled(prefix, style),
                Span::styled(num, Style::default().fg(Color::Cyan)),
                Span::styled(opt_text, style),
            ];

            // Add recommendation indicator
            if is_recommended {
                spans.push(Span::styled(" ⏺", Style::default().fg(Color::DarkGray)));
            }

            lines.push(Line::from(spans));

            // Show pros and cons
            if opt_idx < question.pros.len() || opt_idx < question.cons.len() {
                let pro = question.pros.get(opt_idx).map(|s| s.as_str()).unwrap_or("");
                let con = question.cons.get(opt_idx).map(|s| s.as_str()).unwrap_or("");

                let mut info_parts = Vec::new();
                if !pro.is_empty() {
                    info_parts.push(format!("+{}", pro));
                }
                if !con.is_empty() {
                    info_parts.push(format!("-{}", con));
                }
                if !info_parts.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("       "),
                        Span::styled(info_parts.join(" "), Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
        }

        // "Other" option for custom input
        let other_idx = question.options.len();
        let is_other_selected = self.dialog_selection == other_idx;
        let other_prefix = if is_other_selected {
            "  ◉ "
        } else {
            "  ○ "
        };
        let other_style = if is_other_selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(other_prefix, other_style),
            Span::styled(
                format!("{}. ", other_idx + 1),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled("Other (custom input)", other_style),
        ]));

        // Show recommendation reason
        if let Some(reason) = &question.recommendation_reason {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Tip: ", Style::default().fg(Color::DarkGray)),
                Span::styled(reason, Style::default().fg(Color::DarkGray)),
            ]));
        }

        // Custom input field
        if self.is_custom_input {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("┌─ ", Style::default().fg(Color::Magenta)),
                Span::styled("Your answer:", Style::default().fg(Color::Magenta)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("│ ", Style::default().fg(Color::Magenta)),
                Span::styled(&self.custom_input, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(Color::Yellow)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("└─", Style::default().fg(Color::Magenta)),
            ]));
        }

        lines.push(Line::from(""));

        // Help text
        lines.push(Line::from(vec![Span::styled(
            "─".repeat(50.min(area.width as usize - 4)),
            Style::default().fg(Color::DarkGray),
        )]));

        if self.is_custom_input {
            lines.push(Line::from(vec![
                Span::styled(" Enter ", Style::default().fg(Color::Yellow)),
                Span::styled("confirm  ", Style::default().fg(Color::DarkGray)),
                Span::styled(" Esc ", Style::default().fg(Color::Yellow)),
                Span::styled("back", Style::default().fg(Color::DarkGray)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(" ↑↓ ", Style::default().fg(Color::Yellow)),
                Span::styled("navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled(" 1-9 ", Style::default().fg(Color::Yellow)),
                Span::styled("quick select  ", Style::default().fg(Color::DarkGray)),
                Span::styled(" Enter ", Style::default().fg(Color::Yellow)),
                Span::styled("confirm  ", Style::default().fg(Color::DarkGray)),
                Span::styled(" Esc ", Style::default().fg(Color::Yellow)),
                Span::styled("cancel", Style::default().fg(Color::DarkGray)),
            ]));
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", question.id))
                .border_style(Style::default().fg(Color::Cyan)),
        );

        frame.render_widget(paragraph, popup_area);
    }

    /// Get total options including "Other"
    pub fn total_options(&self, question: &Question) -> usize {
        question.options.len() + 1
    }

    /// Check if "Other" is selected
    pub fn is_other_selected(&self, question: &Question) -> bool {
        self.dialog_selection == question.options.len()
    }
}

/// Helper function to create centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_questions_panel_new() {
        let panel = QuestionsPanel::new();
        assert_eq!(panel.state.selected(), Some(0));
        assert!(!panel.in_answer_dialog);
    }

    #[test]
    fn test_select_navigation() {
        let mut panel = QuestionsPanel::new();
        panel.select_down(5);
        assert_eq!(panel.state.selected(), Some(1));
        panel.select_up(5);
        assert_eq!(panel.state.selected(), Some(0));
    }
}
