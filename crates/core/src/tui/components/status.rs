//! Status bar component.

use crate::tui::{Activity, ExecutionState, VerbosityLevel};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::{Duration, Instant};

/// Spinner frames - Braille animation for smooth spinning
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Breathing dots - pulsing effect
const BREATHING_DOTS: &[&str] = &["⣀", "⣄", "⣤", "⣶", "⣾", "⣿", "⣾", "⣶"];

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
        last_pulse_time: Option<&Instant>,
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

        // Check if we have recent activity (within 2 seconds)
        let has_recent_pulse = last_pulse_time
            .map(|t| t.elapsed() < Duration::from_secs(2))
            .unwrap_or(false);

        // Spinner animation with breathing effect
        let (spinner, breathing) = if matches!(
            state,
            ExecutionState::Generating | ExecutionState::Clarifying | ExecutionState::Running { .. }
        ) {
            let frame = spinner_frame % SPINNER_FRAMES.len();
            let breath_frame = (spinner_frame / 2) % BREATHING_DOTS.len();
            (SPINNER_FRAMES[frame], BREATHING_DOTS[breath_frame])
        } else {
            ("", "")
        };

        // Activity indicator with activity type
        let activity_indicator = if let ExecutionState::Running { activity } = state {
            let activity_name = match activity {
                Activity::ApiCall => "API",
                Activity::Test => "TEST",
                Activity::Git => "GIT",
                Activity::FileWrite => "WRITE",
                Activity::Planning => "PLAN",
                Activity::Assessing => "ASSESS",
                Activity::Other(_) => "WORK",
            };
            // Add pulse indicator if recent activity
            let pulse = if has_recent_pulse { "●" } else { "○" };
            format!(" {}{} {}", breathing, pulse, activity_name)
        } else if !spinner.is_empty() {
            format!(" {} ", breathing)
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

        // Build task string with activity/spinner indicators
        let task_str = if let Some(t) = current_task {
            if !spinner.is_empty() {
                format!(" {}{} {}", spinner, breathing, t)
            } else {
                format!(" {}", t)
            }
        } else if !spinner.is_empty() {
            format!(" {}{}", spinner, breathing)
        } else {
            String::new()
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
            Span::styled(activity_indicator, Style::default().fg(Color::Yellow)),
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
            Span::styled("Tk:", Style::default().fg(Color::Magenta)),
            Span::styled(
                format!("{}", current_task_tokens),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", total_tokens),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
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
