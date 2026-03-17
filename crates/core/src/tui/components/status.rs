//! Status bar component.

use crate::tui::{ExecutionState, VerbosityLevel};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Status bar component
pub struct StatusBar;

impl StatusBar {
    /// Render status bar
    pub fn render(
        state: ExecutionState,
        current_task: Option<&str>,
        completed: usize,
        total: usize,
        failed: usize,
        elapsed: &str,
        model: &str,
        verbosity: VerbosityLevel,
    ) -> Paragraph<'static> {
        let state_color = match state {
            ExecutionState::Idle => Color::Gray,
            ExecutionState::Generating => Color::Cyan,
            ExecutionState::Running => Color::Yellow,
            ExecutionState::Completed => Color::Green,
            ExecutionState::Failed => Color::Red,
        };

        let progress = if total > 0 {
            format!("{}/{}", completed, total)
        } else {
            "0/0".to_string()
        };

        let failed_str = if failed > 0 {
            format!(", {} failed", failed)
        } else {
            String::new()
        };

        let task_str = current_task
            .map(|t| format!(" | {}", t))
            .unwrap_or_default();

        let verbosity_str = match verbosity {
            VerbosityLevel::Quiet => "Q",
            VerbosityLevel::Normal => "N",
            VerbosityLevel::Verbose => "V",
        };

        let line = Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Gray)),
            Span::styled(state.to_string(), Style::default().fg(state_color).add_modifier(Modifier::BOLD)),
            Span::styled(task_str, Style::default().fg(Color::Cyan)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(progress, Style::default().fg(Color::White)),
            Span::styled(failed_str, Style::default().fg(Color::Red)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(elapsed.to_string(), Style::default().fg(Color::White)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(model.to_string(), Style::default().fg(Color::Magenta)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(verbosity_str, Style::default().fg(Color::Yellow)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled("?:Help q:Quit", Style::default().fg(Color::DarkGray)),
        ]);

        Paragraph::new(line)
    }
}