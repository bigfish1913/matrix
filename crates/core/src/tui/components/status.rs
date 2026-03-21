//! Status bar component.

use crate::tui::{ExecutionState, VerbosityLevel, Activity};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::{Duration, Instant};

/// Spinner frames for animation
const SPINNER_FRAMES: &[&str] = &["⠋ ", "⠙ ", "⠚ ", "⠞ ", "⠟ "];
/// Pulse indicator frames
const PULSE_FRAMES: &[&str] = &["●", "○"];

/// Status bar component
pub struct StatusBar;

impl StatusBar {
    /// Render status bar with spinner animation and split time display
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        state: ExecutionState,
        current_task: Option<&str>,
        completed: usize,
        total: usize,
        failed: usize,
        total_elapsed: &str,
        task_elapsed: &Duration,
        spinner_frame: usize,
        model: &str,
        verbosity: VerbosityLevel,
        version: &str,
        current_task_tokens: u32,
        total_tokens: u32,
        _last_pulse_time: Option<&Instant>,
    ) -> Paragraph<'static> {
        let state_color = match state {
            ExecutionState::Idle => Color::Gray,
            ExecutionState::Clarifying => Color::Magenta,
            ExecutionState::Generating => Color::Cyan,
            ExecutionState::Running { activity } => match activity {
                Activity::ApiCall => Color::Yellow,
                Activity::Test => Color::Blue,
                Activity::Git => Color::Green,
                Activity::FileWrite => Color::Cyan,
                Activity::Planning => Color::Magenta,
                Activity::Assessing => Color::LightYellow,
                Activity::Other(_) => Color::Yellow,
            },
            ExecutionState::Completed => Color::Green,
            ExecutionState::Failed => Color::Red,
        };

        // Get spinner character based on state and frame
        let spinner = if matches!(
            state,
            ExecutionState::Generating
                | ExecutionState::Clarifying
                | ExecutionState::Running { .. }
        ) {
            SPINNER_FRAMES[spinner_frame % SPINNER_FRAMES.len()]
        } else {
            ""
        };

        // Calculate pulse indicator based on time since last activity
        let pulse_indicator = if matches!(state, ExecutionState::Running { .. }) {
            // Always show pulse indicator when in Running state
            let frame = PULSE_FRAMES[spinner_frame % PULSE_FRAMES.len()];
            format!(" {}", frame)
        } else {
            String::new()
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

        // Format task elapsed time
        let task_elapsed_str = format_duration(*task_elapsed);

        // Build task string with spinner and pulse indicator
        let task_str = if let Some(t) = current_task {
            if !spinner.is_empty() {
                format!(" {} {}{}", spinner, t, pulse_indicator)
            } else {
                format!(" {}{}", t, pulse_indicator)
            }
        } else if !spinner.is_empty() {
            format!(" {} {}", spinner, pulse_indicator)
        } else {
            pulse_indicator
        };

        let verbosity_str = match verbosity {
            VerbosityLevel::Quiet => "Q",
            VerbosityLevel::Normal => "N",
            VerbosityLevel::Verbose => "V",
        };

        let line = Line::from(vec![
            Span::styled(format!("v{} ", version), Style::default().fg(Color::Cyan)),
            Span::styled(
                state.to_string(),
                Style::default()
                    .fg(state_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(task_str, Style::default().fg(Color::White)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Task:", Style::default().fg(Color::DarkGray)),
            Span::styled(task_elapsed_str, Style::default().fg(Color::Yellow)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Total:", Style::default().fg(Color::DarkGray)),
            Span::styled(total_elapsed.to_string(), Style::default().fg(Color::Green)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(progress, Style::default().fg(Color::White)),
            Span::styled(failed_str, Style::default().fg(Color::Red)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Tokens:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}task", current_task_tokens),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}total", total_tokens),
                Style::default().fg(Color::Green),
            ),
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

/// Format duration as MM:SS
fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{:02}:{:02}", mins, secs)
}
