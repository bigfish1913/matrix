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
        crate::tui::app::Tab::Logs => {
            // Calculate scroll for auto-follow
            let entries = app.log_buffer.get_entries();
            // Calculate viewport height (subtract borders: 2 lines for top/bottom borders)
            let viewport_height = chunks[1].height.saturating_sub(2);
            let scroll = if app.logs_auto_follow {
                LogsPanel::calculate_auto_scroll(entries.len(), viewport_height)
            } else {
                app.logs_scroll
            };
            let paragraph = LogsPanel::render(&entries, scroll, viewport_height);
            frame.render_widget(paragraph, chunks[1]);
        }
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
    }

    // Render status bar
    let version = env!("CARGO_PKG_VERSION");
    let status = StatusBar::render(
        app.state,
        app.current_task_id.as_deref(),
        app.completed_count,
        app.total_count,
        app.failed_count,
        &app.elapsed_string(),
        &app.current_model,
        app.verbosity,
        version,
    );
    frame.render_widget(status, chunks[2]);

    // Render help overlay if active
    if app.show_help {
        render_help_overlay(frame);
    }

    // Render clarification dialog if active
    if app.clarification.is_active() {
        render_clarification_dialog(frame, app);
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

fn render_clarification_dialog(frame: &mut Frame, app: &TuiApp) {
    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let clarification = &app.clarification;

    // Build the content
    let mut content = String::new();
    content.push_str("Clarifying Questions\n");
    content.push_str("═══════════════════\n\n");

    // Show all questions with their answers or status
    for (i, question) in clarification.questions.iter().enumerate() {
        if i < clarification.current_index {
            // Already answered
            let answer = &clarification.answers[i];
            content.push_str(&format!("[✓] {}\n", question));
            if answer.is_empty() {
                content.push_str("    Answer: (skipped)\n\n");
            } else {
                content.push_str(&format!("    Answer: {}\n\n", answer));
            }
        } else if i == clarification.current_index {
            // Current question
            content.push_str(&format!(">>> {}\n",
                question
            ));
            content.push_str(&format!("    Your answer: {}_\n\n", clarification.current_input));
        } else {
            // Future question
            content.push_str(&format!("[ ] {}\n\n", question));
        }
    }

    content.push_str("─────────────────────\n");
    content.push_str("Press Enter to submit answer (blank to skip)\n");
    content.push_str("Press Esc to cancel all questions\n");

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Question {}/{} ", clarification.current_index + 1, clarification.questions.len()))
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}