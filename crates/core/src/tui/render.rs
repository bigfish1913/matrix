// crates/core/src/tui/render.rs

use crate::tui::app::TuiApp;
use crate::tui::components::{LogsPanel, OutputPanel, StatusBar, TabSwitcher, TasksPanel};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    Frame,
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};

pub type MatrixTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

/// Render the TUI
pub fn render_app(frame: &mut Frame, app: &mut TuiApp) {
    // Create main layout: tab switcher + main content + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Tab switcher
            Constraint::Min(10),    // Main content
            Constraint::Length(1),  // Status bar
        ])
        .split(frame.area());

    // Render tab switcher
    let tabs = TabSwitcher::render(app.current_tab);
    frame.render_widget(tabs, chunks[0]);

    // Render main content based on current tab
    match app.current_tab {
        crate::tui::app::Tab::Tasks => {
            let (list, state) = TasksPanel::render(&app.tasks, app.tasks_scroll);
            frame.render_stateful_widget(list, chunks[1], &mut state.clone());
        }
        crate::tui::app::Tab::Output => {
            // Calculate scroll for auto-follow
            let scroll = if app.output_auto_follow {
                app.output_lines.len() as u16  // Scroll to bottom
            } else {
                app.output_scroll
            };
            let paragraph = OutputPanel::render(
                &app.output_lines,
                app.output_task_id.as_deref(),
                app.verbosity,
                scroll,
            );
            frame.render_widget(paragraph, chunks[1]);
        }
        crate::tui::app::Tab::Logs => {
            // Calculate scroll for auto-follow
            let entries = app.log_buffer.get_entries();
            let scroll = if app.logs_auto_follow {
                entries.len() as u16  // Scroll to bottom
            } else {
                app.logs_scroll
            };
            let paragraph = LogsPanel::render(&entries, scroll);
            frame.render_widget(paragraph, chunks[1]);
        }
    }

    // Render status bar
    let status = StatusBar::render(
        app.state,
        app.current_task_id.as_deref(),
        app.completed_count,
        app.total_count,
        app.failed_count,
        &app.elapsed_string(),
        &app.current_model,
        app.verbosity,
    );
    frame.render_widget(status, chunks[2]);

    // Render help overlay if active
    if app.show_help {
        render_help_overlay(frame);
    }
}

fn render_help_overlay(frame: &mut Frame) {
    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);

    let help_text = r#"
╭─────────────────────────────────────╮
│            Keyboard Help            │
├─────────────────────────────────────┤
│  Tab / →     Next tab               │
│  Shift+Tab / ←  Previous tab        │
│  ↑ / ↓       Scroll                 │
│  ?           Show this help         │
│  q / Esc     Quit                   │
╰─────────────────────────────────────╯
"#;

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .style(Style::default().fg(Color::Yellow));

    frame.render_widget(paragraph, area);
}

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