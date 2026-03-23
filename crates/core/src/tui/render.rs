// crates/core/src/tui/render.rs

use crate::models::TaskStatus;
use crate::tui::app::{OutputLine, Tab, TaskDisplay, TuiApp};
use crate::tui::components::{
    LogsPanel, OutputPanel, QuestionsPanel, StatusBar, TabSwitcher, TasksPanel,
};
use crate::tui::markdown::render_markdown;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
    Frame, Terminal,
};

pub type MatrixTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

/// Render the TUI
pub fn render_app(frame: &mut Frame, app: &mut TuiApp) {
    // Create main layout: tab switcher + main content + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Tab switcher
            Constraint::Min(10),   // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Render tab switcher with additional info
    let tabs = TabSwitcher::render(app.current_tab, app.verbosity);
    frame.render_widget(tabs, chunks[0]);

    // Render main content based on current tab
    match app.current_tab {
        Tab::Logs => {
            let entries = app.log_buffer.get_entries();
            let viewport_height = chunks[1].height.saturating_sub(2);
            app.logs_viewport_height = viewport_height;
            // When auto-follow is on, scroll to show latest entries
            let scroll = if app.logs_auto_follow {
                LogsPanel::calculate_auto_scroll(entries.len(), viewport_height)
            } else {
                app.logs_scroll
            };
            let paragraph = LogsPanel::render(&entries, scroll, viewport_height);
            frame.render_widget(paragraph, chunks[1]);
        }
        Tab::Tasks => {
            let filtered: Vec<TaskDisplay> = app.filtered_tasks().into_iter().cloned().collect();
            let (list, state) =
                TasksPanel::render_with_mode(&filtered, app.tasks_scroll, app.tree_view);
            frame.render_stateful_widget(list, chunks[1], &mut state.clone());

            // Render search box if active
            if app.search.is_active() {
                render_search_box(frame, app);
            }
        }
        Tab::Output => {
            // Render output content directly (no task tabs)
            let viewport_height = chunks[1].height.saturating_sub(2) as usize;
            let total_lines = app.output_lines.len();
            let scroll = if app.output_auto_follow {
                // Calculate proper scroll to show bottom content without overflow
                total_lines.saturating_sub(viewport_height)
            } else {
                app.output_scroll
            };
            let paragraph = OutputPanel::render(&app.output_lines, app.verbosity, scroll);
            frame.render_widget(paragraph, chunks[1]);
        }
        Tab::Events => {
            // Render raw events (verbose mode only)
            let events_text = app.events_buffer.join("\n");
            let viewport_height = chunks[1].height.saturating_sub(2) as usize;
            let total_lines = app.events_buffer.len();
            let scroll = if app.events_auto_follow {
                // Calculate proper scroll to show bottom content without overflow
                total_lines.saturating_sub(viewport_height)
            } else {
                app.events_scroll
            };
            let paragraph = Paragraph::new(events_text)
                .style(Style::default().fg(Color::Gray))
                .scroll((scroll as u16, 0))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Events (Debug) ")
                        .title_style(Style::default().fg(Color::Yellow)),
                );
            frame.render_widget(paragraph, chunks[1]);
        }
        Tab::Meeting => {
            // Render the questions panel
            app.questions_panel.render(frame, chunks[1], &app.questions);
        }
    }

    // Render status bar with progress
    let version = env!("CARGO_PKG_VERSION");
    let task_elapsed = app
        .task_start_time
        .map(|start| start.elapsed())
        .unwrap_or_default();

    // Create status bar with progress bar
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),    // Status text
            Constraint::Length(20), // Progress bar
        ])
        .split(chunks[2]);

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
        app.current_task_tokens,
        app.total_tokens,
        app.last_pulse_time.as_ref(),
    );
    frame.render_widget(status, status_chunks[0]);

    // Progress bar
    if app.total_count > 0 {
        let progress = app.progress_percent();
        let progress_color = if app.failed_count > 0 {
            Color::Yellow
        } else {
            Color::Green
        };
        let gauge = Gauge::default()
            .block(Block::default())
            .gauge_style(Style::default().fg(progress_color))
            .ratio(progress as f64 / 100.0)
            .label(format!(" {}% ", progress));
        frame.render_widget(gauge, status_chunks[1]);
    }

    // Render overlays in order (last = topmost)

    // Help overlay
    if app.show_help {
        render_help_overlay(frame);
    }

    // Task detail panel
    if app.task_detail.is_active() {
        render_task_detail_panel(frame, app);
    }

    // Quit confirmation
    if app.quit_confirm.is_active() {
        render_quit_confirm_dialog(frame);
    }

    // Resume confirmation
    if app.resume_confirm.is_active() {
        render_resume_confirm_dialog(frame, app);
        return;
    }

    // Clarification dialog
    if app.clarification.is_active() {
        render_clarification_dialog(frame, app);
    }

    // Questions answer dialog
    if app.questions_panel.in_answer_dialog {
        if let Some(question) = app.selected_question().cloned() {
            app.questions_panel
                .render_answer_dialog(frame, frame.area(), &question);
        }
    }

    // Pause indicator
    if app.is_paused {
        render_paused_indicator(frame);
    }
}

fn render_search_box(frame: &mut Frame, app: &TuiApp) {
    let area = Rect {
        x: frame.area().width.saturating_sub(40),
        y: 1,
        width: 38.min(frame.area().width),
        height: 3,
    };
    frame.render_widget(Clear, area);

    let search_text = format!("Search: {}", app.search.query);
    let paragraph = Paragraph::new(search_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" / ")
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}

fn render_output_task_indicator(frame: &mut Frame, task_id: &str, total_tasks: usize) {
    let text = format!(" Viewing: {} | 'a' for all ", task_id);
    let area = Rect {
        x: frame.area().width.saturating_sub(text.len() as u16 + 4),
        y: 1,
        width: (text.len() + 4) as u16,
        height: 1,
    };

    let paragraph = Paragraph::new(text).style(Style::default().fg(Color::Cyan).bg(Color::Black));

    frame.render_widget(paragraph, area);
}

fn render_task_detail_panel(frame: &mut Frame, app: &TuiApp) {
    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(task_id) = &app.task_detail.task_id {
        if let Some(task) = app.tasks.iter().find(|t| &t.id == task_id) {
            // Header
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Task: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &task.id,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(""));

            // Title
            lines.push(Line::from(vec![
                Span::styled("Title: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&task.title, Style::default().fg(Color::White)),
            ]));

            // Status
            let status_color = match task.status {
                TaskStatus::Pending => Color::Yellow,
                TaskStatus::InProgress => Color::Cyan,
                TaskStatus::Completed => Color::Green,
                TaskStatus::Failed => Color::Red,
                TaskStatus::Skipped => Color::DarkGray,
            };
            let status_text = format!("{:?}", task.status);
            lines.push(Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(status_text, Style::default().fg(status_color)),
            ]));

            // Duration
            if let Some(duration) = &task.duration {
                let secs = duration.as_secs();
                let mins = secs / 60;
                let secs = secs % 60;
                lines.push(Line::from(vec![
                    Span::styled("Duration: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{}m {}s", mins, secs),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }

            // Depth
            lines.push(Line::from(vec![
                Span::styled("Depth: ", Style::default().fg(Color::DarkGray)),
                Span::styled(task.depth.to_string(), Style::default().fg(Color::White)),
            ]));

            // Parent
            if let Some(parent) = &task.parent_id {
                lines.push(Line::from(vec![
                    Span::styled("Parent: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(parent, Style::default().fg(Color::Magenta)),
                ]));
            }

            // Dependencies
            if !task.depends_on.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Depends on:",
                    Style::default().fg(Color::DarkGray),
                )]));
                for dep in &task.depends_on {
                    lines.push(Line::from(vec![
                        Span::raw("  → "),
                        Span::styled(dep, Style::default().fg(Color::Yellow)),
                    ]));
                }
            }

            // Description
            if !task.description.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Description:",
                    Style::default().fg(Color::DarkGray),
                )]));
                let desc_lines =
                    render_markdown(&task.description, area.width.saturating_sub(4) as usize);
                lines.extend(desc_lines);
            }

            // Error
            if let Some(error) = &task.error {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Error:",
                    Style::default().fg(Color::Red),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    error,
                    Style::default().fg(Color::Red),
                )]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "Task not found",
                Style::default().fg(Color::Red),
            )]));
        }
    }

    // Help
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(40),
        Style::default().fg(Color::DarkGray),
    )]));
    lines.push(Line::from(vec![
        Span::styled(" Esc/Enter ", Style::default().fg(Color::Yellow)),
        Span::styled("close", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Task Details ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(paragraph, area);
}

fn render_quit_confirm_dialog(frame: &mut Frame) {
    let area = centered_rect(50, 20, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  ⚠️  Tasks are still running!",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Quit anyway? ",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Y ", Style::default().fg(Color::Yellow)),
            Span::styled("quit  ", Style::default().fg(Color::DarkGray)),
            Span::styled(" N/Esc ", Style::default().fg(Color::Yellow)),
            Span::styled("cancel", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Confirm Quit ")
            .border_style(Style::default().fg(Color::Red)),
    );

    frame.render_widget(paragraph, area);
}

fn render_paused_indicator(frame: &mut Frame) {
    let text = " ⏸ PAUSED ";
    let area = Rect {
        x: 2,
        y: frame.area().height.saturating_sub(2),
        width: text.len() as u16,
        height: 1,
    };

    let paragraph = Paragraph::new(text).style(Style::default().fg(Color::Black).bg(Color::Yellow));

    frame.render_widget(paragraph, area);
}

fn render_help_overlay(frame: &mut Frame) {
    let area = centered_rect(65, 70, frame.area());
    frame.render_widget(Clear, area);

    let help_text = r#"
╭───────────────────────────────────────────╮
│              Keyboard Shortcuts            │
├───────────────────────────────────────────┤
│  Tab / →        Next tab                   │
│  Shift+Tab / ←  Previous tab               │
│  ↑ / ↓          Scroll                     │
│  ?              Show this help              │
│  q / Esc        Quit                       │
├───────────────────────────────────────────┤
│  Tasks Tab:                               │
│  Enter          View task details          │
│  /              Search tasks               │
│  t              Toggle tree view           │
├───────────────────────────────────────────┤
│  Output Tab:                              │
│  1-9            View task N output         │
│  a              View all output            │
├───────────────────────────────────────────┤
│  Global:                                  │
│  p              Pause/Resume execution     │
╰───────────────────────────────────────────╯
"#;

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .style(Style::default().fg(Color::Yellow));

    frame.render_widget(paragraph, area);
}

fn render_resume_confirm_dialog(frame: &mut Frame, app: &TuiApp) {
    let area = centered_rect(70, 45, frame.area());
    frame.render_widget(Clear, area);

    let resume = &app.resume_confirm;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  ⚠️  Found existing tasks in workspace",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Stats
    lines.push(Line::from(vec![
        Span::styled("     Completed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            resume.completed.to_string(),
            Style::default().fg(Color::Green),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("     Pending:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            resume.pending.to_string(),
            Style::default().fg(Color::Yellow),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("     Failed:    ", Style::default().fg(Color::DarkGray)),
        Span::styled(resume.failed.to_string(), Style::default().fg(Color::Red)),
    ]));
    lines.push(Line::from(""));

    // Options with pros/cons
    let resume_style = if resume.selected {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let fresh_style = if !resume.selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Resume option
    lines.push(Line::from(vec![
        Span::styled(
            if resume.selected {
                "  → [●] "
            } else {
                "    [ ] "
            },
            resume_style,
        ),
        Span::styled("Resume from existing tasks", resume_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ✓ ", Style::default().fg(Color::Green)),
        Span::styled(
            "Continue progress, no rework",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ✗ ", Style::default().fg(Color::Red)),
        Span::styled(
            "May inherit old context/issues",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(""));

    // Fresh option
    lines.push(Line::from(vec![
        Span::styled(
            if !resume.selected {
                "  → [●] "
            } else {
                "    [ ] "
            },
            fresh_style,
        ),
        Span::styled("Start fresh (clear all tasks)", fresh_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ✓ ", Style::default().fg(Color::Green)),
        Span::styled(
            "Clean slate, fresh AI context",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ✗ ", Style::default().fg(Color::Red)),
        Span::styled("Loses all progress", Style::default().fg(Color::DarkGray)),
    ]));
    lines.push(Line::from(""));

    // Recommendation
    if resume.pending > 0 {
        lines.push(Line::from(vec![
            Span::styled("  💡 ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Recommended: Resume (tasks pending)",
                Style::default().fg(Color::Cyan),
            ),
        ]));
    } else if resume.failed > resume.completed {
        lines.push(Line::from(vec![
            Span::styled("  💡 ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Recommended: Start Fresh (many failures)",
                Style::default().fg(Color::Cyan),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("  💡 ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Recommended: Resume (preserve progress)",
                Style::default().fg(Color::Cyan),
            ),
        ]));
    }
    lines.push(Line::from(""));

    // Help
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(50),
        Style::default().fg(Color::DarkGray),
    )]));
    lines.push(Line::from(vec![
        Span::styled(" ←/→ ", Style::default().fg(Color::Yellow)),
        Span::styled("switch  ", Style::default().fg(Color::DarkGray)),
        Span::styled(" Y/N ", Style::default().fg(Color::Yellow)),
        Span::styled("quick select  ", Style::default().fg(Color::DarkGray)),
        Span::styled(" Enter ", Style::default().fg(Color::Yellow)),
        Span::styled("confirm", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Resume or Start Fresh? ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

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
    let area = centered_rect(85, 80, frame.area());
    frame.render_widget(Clear, area);

    let clarification = &app.clarification;
    let mut lines: Vec<Line> = Vec::new();
    let width = area.width.saturating_sub(4) as usize;

    // Header
    lines.push(Line::from(vec![Span::styled(
        " Clarifying Questions ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Show current question with options including pros/cons
    if let Some(q) = clarification.questions.get(clarification.current_index) {
        // Question
        let q_lines = render_markdown(&q.question, width);
        let highlight_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);

        if q_lines.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("▶ ", highlight_style),
                Span::styled(&q.question, highlight_style),
            ]));
        } else {
            for (idx, q_line) in q_lines.into_iter().enumerate() {
                let mut spans: Vec<Span> = Vec::new();
                if idx == 0 {
                    spans.push(Span::styled("▶ ", highlight_style));
                } else {
                    spans.push(Span::raw("  "));
                }
                for span in q_line.spans {
                    let styled = if span.style.fg.is_none() {
                        Span::styled(span.content, highlight_style)
                    } else {
                        span
                    };
                    spans.push(styled);
                }
                lines.push(Line::from(spans));
            }
        }
        lines.push(Line::from(""));

        // Options with pros/cons
        for (opt_idx, opt) in q.options.iter().enumerate() {
            let is_selected = clarification.selected_option == opt_idx;
            let is_recommended = q.recommended == Some(opt_idx);
            let prefix = if is_selected { "  ◉ " } else { "  ○ " };
            let num = format!("{}. ", opt_idx + 1);

            // Less intrusive styling - no green color for recommended
            let style = if is_selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            let opt_lines = render_markdown(opt, width.saturating_sub(6));
            if opt_lines.is_empty() || opt_lines.len() == 1 {
                let opt_text = if opt_lines.is_empty() {
                    opt.clone()
                } else {
                    opt_lines[0]
                        .spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                };
                let mut spans = vec![
                    Span::styled(prefix, style),
                    Span::styled(num, Style::default().fg(Color::Cyan)),
                    Span::styled(opt_text, style),
                ];
                // Add subtle recommendation indicator
                if is_recommended {
                    spans.push(Span::styled(" ⏺", Style::default().fg(Color::DarkGray)));
                }
                lines.push(Line::from(spans));
            } else {
                for (line_idx, opt_line) in opt_lines.into_iter().enumerate() {
                    if line_idx == 0 {
                        let mut spans = vec![
                            Span::styled(prefix, style),
                            Span::styled(num.clone(), Style::default().fg(Color::Cyan)),
                        ];
                        for span in opt_line.spans {
                            spans.push(Span::styled(span.content, style));
                        }
                        if is_recommended {
                            spans.push(Span::styled(" ⏺", Style::default().fg(Color::DarkGray)));
                        }
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw("       ")];
                        for span in opt_line.spans {
                            spans.push(Span::styled(span.content, style));
                        }
                        lines.push(Line::from(spans));
                    }
                }
            }

            // Show pros and cons for this option (more compact)
            if opt_idx < q.pros.len() || opt_idx < q.cons.len() {
                let pro = q.pros.get(opt_idx).map(|s| s.as_str()).unwrap_or("");
                let con = q.cons.get(opt_idx).map(|s| s.as_str()).unwrap_or("");

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

        // "Other" option
        let other_idx = q.options.len();
        let is_other_selected = clarification.selected_option == other_idx;
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

        // Show recommendation reason at the bottom, more subtle
        if let Some(reason) = &q.recommendation_reason {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Tip: ", Style::default().fg(Color::DarkGray)),
                Span::styled(reason, Style::default().fg(Color::DarkGray)),
            ]));
        }

        // Custom input field
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
                Span::styled(
                    &clarification.custom_input,
                    Style::default().fg(Color::White),
                ),
                Span::styled("█", Style::default().fg(Color::Yellow)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("└─", Style::default().fg(Color::Magenta)),
            ]));
        }

        lines.push(Line::from(""));
    }

    // Progress indicator
    lines.push(Line::from(vec![Span::styled(
        format!(
            " Question {}/{} ",
            clarification.current_index + 1,
            clarification.questions.len()
        ),
        Style::default().fg(Color::DarkGray),
    )]));

    // Help text
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(50.min(area.width as usize - 4)),
        Style::default().fg(Color::DarkGray),
    )]));

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
        .scroll((clarification.scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Clarification ")
                .border_style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(paragraph, area);
}
