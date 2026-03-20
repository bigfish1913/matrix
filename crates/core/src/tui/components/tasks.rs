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

/// Item in the task tree (owned)
struct TreeItem {
    id: String,
    title: String,
    status: TaskStatus,
    duration: Option<std::time::Duration>,
    depends_on: Vec<String>,
    depth: u32,
    is_last: bool,
    has_children: bool,
}

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

    /// Get tree prefix based on depth and position
    fn tree_prefix(depth: u32, is_last: bool, _has_children: bool) -> String {
        if depth == 0 {
            return String::new();
        }

        let mut prefix = String::new();

        // Add indentation
        for _ in 0..depth.saturating_sub(1) {
            prefix.push_str("│   ");
        }

        // Add branch symbol
        if is_last {
            prefix.push_str("└── ");
        } else {
            prefix.push_str("├── ");
        }

        prefix
    }

    /// Build tree structure from flat task list
    fn build_tree(tasks: &[TaskDisplay]) -> Vec<TreeItem> {
        // First pass: find all root tasks (no parent)
        let root_ids: Vec<String> = tasks
            .iter()
            .filter(|t| t.parent_id.is_none())
            .map(|t| t.id.clone())
            .collect();

        // Build tree recursively
        let mut result = Vec::new();
        for (idx, root_id) in root_ids.iter().enumerate() {
            let is_last = idx == root_ids.len() - 1;
            if let Some(task) = tasks.iter().find(|t| &t.id == root_id) {
                Self::add_tree_item(tasks, task, 0, is_last, &mut result);
            }
        }
        result
    }

    /// Add a tree item and its children recursively
    fn add_tree_item(
        tasks: &[TaskDisplay],
        task: &TaskDisplay,
        depth: u32,
        is_last: bool,
        result: &mut Vec<TreeItem>,
    ) {
        // Find children of this task
        let children: Vec<&TaskDisplay> = tasks
            .iter()
            .filter(|t| t.parent_id.as_ref() == Some(&task.id))
            .collect();

        let has_children = !children.is_empty();
        result.push(TreeItem {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            duration: task.duration,
            depends_on: task.depends_on.clone(),
            depth,
            is_last,
            has_children,
        });

        // Add children recursively
        for (idx, child) in children.iter().enumerate() {
            let child_is_last = idx == children.len() - 1;
            Self::add_tree_item(tasks, child, depth + 1, child_is_last, result);
        }
    }

    /// Render tasks panel with tree view
    pub fn render(tasks: &[TaskDisplay], selected: usize) -> (List<'static>, ListState) {
        let tree = Self::build_tree(tasks);

        let items: Vec<ListItem<'static>> = tree
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let icon = Self::status_icon(item.status);
                let color = Self::status_color(item.status);
                let duration = Self::format_duration(item.duration);
                let status_text = if item.status == TaskStatus::InProgress {
                    "Running".to_string()
                } else if item.status == TaskStatus::Pending {
                    "Pending".to_string()
                } else if item.status == TaskStatus::Failed {
                    "Failed".to_string()
                } else {
                    duration
                };

                // Tree prefix
                let tree_prefix = Self::tree_prefix(item.depth, item.is_last, item.has_children);

                // Determine if this item is selected
                let is_selected = idx == selected;
                let title_style = if is_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                // Build the line - all owned data
                let mut spans = vec![
                    Span::styled(tree_prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(item.id.clone(), Style::default().fg(Color::Cyan)),
                    Span::raw("  "),
                    Span::styled(
                        // Truncate title to fit (Unicode-safe)
                        if item.title.chars().count() > 35 {
                            format!("{}...", item.title.chars().take(32).collect::<String>())
                        } else {
                            item.title.clone()
                        },
                        title_style,
                    ),
                    Span::raw("  "),
                    Span::styled(status_text, Style::default().fg(Color::DarkGray)),
                ];

                // Show dependency indicator if task has dependencies
                if !item.depends_on.is_empty() {
                    spans.push(Span::styled(
                        format!(" ⬆{}", item.depends_on.len()),
                        Style::default().fg(Color::Magenta),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(selected));

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Tasks (Tree View) ")
                    .borders(Borders::ALL)
                    .style(Style::default()),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");

        (list, state)
    }
}
