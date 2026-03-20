// crates/core/src/tui/render.rs

use crate::tui::app::TuiApp;
use crate::tui::components::{LogsPanel, OutputPanel, StatusBar, TabSwitcher, TasksPanel};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
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
    let task_elapsed = app.task_start_time
        .map(|start| start.elapsed())
        .unwrap_or_default();
    let status = StatusBar::render(
        app.state,
        app.current_task_id.as_deref(),
        app.completed_count,
        app.total_count,
        app.failed_count,
        &app.elapsed_string(),
        &task_elapsed,
        app.spinner_frame,
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
    let area = centered_rect(80, 70, frame.area());
    frame.render_widget(Clear, area);

    let clarification = &app.clarification;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled(" Clarifying Questions ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));

    // Show all questions
    for (i, q) in clarification.questions.iter().enumerate() {
        if i < clarification.current_index {
            // Already answered
            let answer = if i < clarification.answers.len() {
                &clarification.answers[i]
            } else {
                ""
            };
            lines.push(Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Green)),
                Span::styled(&q.question, Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("→ ", Style::default().fg(Color::Green)),
                Span::styled(answer, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(""));
        } else if i == clarification.current_index {
            // Current question
            let highlight_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
            lines.push(Line::from(vec![
                Span::styled("▶ ", highlight_style),
                Span::styled(&q.question, highlight_style),
            ]));
            lines.push(Line::from(""));

            // Show options
            for (opt_idx, opt) in q.options.iter().enumerate() {
                let is_selected = clarification.selected_option == opt_idx;
                let prefix = if is_selected { "  ◉ " } else { "  ○ " };
                let num = format!("{}. ", opt_idx + 1);
                let style = if is_selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(num, Style::default().fg(Color::Cyan)),
                    Span::styled(opt.clone(), style),
                ]));
            }

            // "Other" option
            let other_idx = q.options.len();
            let is_other_selected = clarification.selected_option == other_idx;
            let other_prefix = if is_other_selected { "  ◉ " } else { "  ○ " };
            let other_style = if is_other_selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(other_prefix, other_style),
                Span::styled(format!("{}. ", other_idx + 1), Style::default().fg(Color::Cyan)),
                Span::styled("Other (custom input)", other_style),
            ]));

            // If in custom input mode, show input field
            if clarification.is_custom_input {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("┌─ ", Style::default().fg(Color::Magenta)),
                    Span::styled("Your answer:", Style::default().fg(Color::Magenta)),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("│ ", Style::default().fg(Color::Magenta)),
                    Span::styled(&clarification.custom_input, Style::default().fg(Color::White)),
                    Span::styled("█", Style::default().fg(Color::Yellow)),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("└─", Style::default().fg(Color::Magenta)),
                ]));
            }

            lines.push(Line::from(""));
        } else {
            // Future question
            lines.push(Line::from(vec![
                Span::styled("○ ", Style::default().fg(Color::DarkGray)),
                Span::styled(&q.question, Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(""));
        }
    }

    // Help text
    lines.push(Line::from(vec![
        Span::styled("─".repeat(50.min(area.width as usize - 4)), Style::default().fg(Color::DarkGray)),
    ]));

    if clarification.is_custom_input {
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
            Span::styled("skip all", Style::default().fg(Color::DarkGray)),
        ]));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Question {}/{} ", clarification.current_index + 1, clarification.questions.len()))
                .border_style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(paragraph, area);
}