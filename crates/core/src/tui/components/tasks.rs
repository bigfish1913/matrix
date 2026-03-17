// crates/core/src/tui/components/tasks.rs

use crate::models::TaskStatus;
use crate::tui::app::TaskDisplay;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

/// Tasks panel component
pub struct TasksPanel;

impl TasksPanel {
    /// Get status icon for a task
    fn status_icon(status: TaskStatus) -> &'static str {
        match status {
            TaskStatus::Completed => "✓",
            TaskStatus::InProgress => "●",
            TaskStatus::Pending => "○",
            TaskStatus::Failed => "✗",
            TaskStatus::Skipped => "⊘",
        }
    }

    /// Get status color
    fn status_color(status: TaskStatus) -> Color {
        match status {
            TaskStatus::Completed => Color::Green,
            TaskStatus::InProgress => Color::Yellow,
            TaskStatus::Pending => Color::Gray,
            TaskStatus::Failed => Color::Red,
            TaskStatus::Skipped => Color::DarkGray,
        }
    }

    /// Format duration
    fn format_duration(duration: Option<std::time::Duration>) -> String {
        match duration {
            Some(d) => {
                let secs = d.as_secs();
                let mins = secs / 60;
                let secs = secs % 60;
                format!("[{:02}:{:02}]", mins, secs)
            }
            None => String::new(),
        }
    }

    /// Render tasks panel
    pub fn render(tasks: &[TaskDisplay], selected: usize) -> (List<'_>, ListState) {
        let items: Vec<ListItem> = tasks
            .iter()
            .map(|task| {
                let icon = Self::status_icon(task.status);
                let color = Self::status_color(task.status);
                let duration = Self::format_duration(task.duration);
                let status_text = if task.status == TaskStatus::InProgress {
                    "Running".to_string()
                } else if task.status == TaskStatus::Pending {
                    "Pending".to_string()
                } else if task.status == TaskStatus::Failed {
                    "Failed".to_string()
                } else {
                    duration
                };

                let line = Line::from(vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&task.id, Style::default().fg(Color::Cyan)),
                    Span::raw("  "),
                    Span::styled(
                        // Truncate title to fit (Unicode-safe)
                        if task.title.chars().count() > 40 {
                            format!("{}...", task.title.chars().take(37).collect::<String>())
                        } else {
                            task.title.clone()
                        },
                        Style::default().fg(Color::White),
                    ),
                    Span::raw("  "),
                    Span::styled(status_text, Style::default().fg(Color::DarkGray)),
                ]);

                ListItem::new(line)
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(selected));

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Tasks ")
                    .borders(Borders::ALL)
                    .style(Style::default()),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");

        (list, state)
    }
}