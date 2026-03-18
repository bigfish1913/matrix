//! TUI Application state and main loop.

use crate::models::TaskStatus;
use crate::tui::{Event, EventReceiver, ExecutionState, Key, LogBuffer, VerbosityLevel};
use std::time::{Duration, Instant};

/// Current tab being displayed
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Tasks,
    Output,
    Logs,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Tasks => Self::Output,
            Self::Output => Self::Logs,
            Self::Logs => Self::Tasks,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Tasks => Self::Logs,
            Self::Output => Self::Tasks,
            Self::Logs => Self::Output,
        }
    }
}

/// Task display info for UI
#[derive(Debug, Clone)]
pub struct TaskDisplay {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub duration: Option<Duration>,
    pub started_at: Option<Instant>,
}

/// Claude output line
#[derive(Debug, Clone)]
pub enum OutputLine {
    Thinking { content: String },
    ToolUse { tool_name: String, tool_input: Option<String> },
    ToolResult { tool_name: String, result: String, success: bool },
    Result { content: String },
}

/// Main TUI application state
pub struct TuiApp {
    // Tab state
    pub current_tab: Tab,

    // Execution state
    pub state: ExecutionState,
    pub current_task_id: Option<String>,
    pub current_model: String,

    // Progress
    pub completed_count: usize,
    pub total_count: usize,
    pub failed_count: usize,
    pub start_time: Option<Instant>,

    // Tasks
    pub tasks: Vec<TaskDisplay>,
    pub tasks_scroll: usize,

    // Claude output
    pub output_lines: Vec<OutputLine>,
    pub output_scroll: u16,
    pub output_task_id: Option<String>,
    pub output_auto_follow: bool,  // Auto-scroll to bottom on new output

    // Logs
    pub log_buffer: LogBuffer,
    pub logs_scroll: u16,
    pub logs_auto_follow: bool,  // Auto-scroll to bottom on new logs

    // Verbosity
    pub verbosity: VerbosityLevel,

    // Event receiver
    event_receiver: Option<EventReceiver>,

    // Help overlay
    pub show_help: bool,

    // Running flag
    pub running: bool,
}

impl TuiApp {
    pub fn new(verbosity: VerbosityLevel) -> Self {
        Self {
            current_tab: Tab::default(),
            state: ExecutionState::default(),
            current_task_id: None,
            current_model: "haiku".to_string(),
            completed_count: 0,
            total_count: 0,
            failed_count: 0,
            start_time: None,
            tasks: Vec::new(),
            tasks_scroll: 0,
            output_lines: Vec::new(),
            output_scroll: 0,
            output_task_id: None,
            output_auto_follow: true,  // Start with auto-follow enabled
            log_buffer: LogBuffer::default(),
            logs_scroll: 0,
            logs_auto_follow: false,  // Start with auto-follow disabled (no auto-scroll)
            verbosity,
            event_receiver: None,
            show_help: false,
            running: true,
        }
    }

    pub fn with_event_receiver(mut self, receiver: EventReceiver) -> Self {
        self.event_receiver = Some(receiver);
        self
    }

    pub fn with_log_buffer(mut self, buffer: LogBuffer) -> Self {
        self.log_buffer = buffer;
        self
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: Key) {
        if self.show_help {
            if key == Key::Esc || key == Key::Char('?') || key == Key::Char('q') {
                self.show_help = false;
            }
            return;
        }

        match key {
            Key::Tab | Key::Right => {
                self.current_tab = self.current_tab.next();
                self.reset_scroll();
            }
            Key::BackTab | Key::Left => {
                self.current_tab = self.current_tab.prev();
                self.reset_scroll();
            }
            Key::Up => {
                self.scroll_up();
            }
            Key::Down => {
                self.scroll_down();
            }
            Key::Char('?') => {
                self.show_help = true;
            }
            Key::Char('q') | Key::Esc => {
                self.running = false;
            }
            _ => {}
        }
    }

    fn reset_scroll(&mut self) {
        match self.current_tab {
            Tab::Tasks => self.tasks_scroll = 0,
            Tab::Output => {
                self.output_scroll = 0;
                self.output_auto_follow = true;
            }
            Tab::Logs => {
                self.logs_scroll = 0;
                // Keep auto-follow disabled (user preference)
            }
        }
    }

    fn scroll_up(&mut self) {
        match self.current_tab {
            Tab::Tasks => {
                if self.tasks_scroll > 0 {
                    self.tasks_scroll -= 1;
                }
            }
            Tab::Output => {
                self.output_scroll = self.output_scroll.saturating_sub(1);
                // User manually scrolled up, disable auto-follow
                self.output_auto_follow = false;
            }
            Tab::Logs => {
                self.logs_scroll = self.logs_scroll.saturating_sub(1);
                // User manually scrolled up, disable auto-follow
                self.logs_auto_follow = false;
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.current_tab {
            Tab::Tasks => {
                if self.tasks_scroll < self.tasks.len().saturating_sub(1) {
                    self.tasks_scroll += 1;
                }
            }
            Tab::Output => {
                // Check if we're at or near bottom
                let max_scroll = self.output_lines.len() as u16;
                if self.output_scroll < max_scroll {
                    self.output_scroll = self.output_scroll.saturating_add(1);
                }
                // If scrolled near bottom, re-enable auto-follow
                if self.output_scroll >= max_scroll.saturating_sub(5) {
                    self.output_auto_follow = true;
                }
            }
            Tab::Logs => {
                let entries = self.log_buffer.get_entries();
                let max_scroll = entries.len() as u16;
                if self.logs_scroll < max_scroll {
                    self.logs_scroll = self.logs_scroll.saturating_add(1);
                }
                // Don't auto-enable auto-follow for logs (user preference)
            }
        }
    }

    /// Process an orchestrator event
    pub fn process_event(&mut self, event: Event) {
        match event {
            Event::TaskCreated { id, title } => {
                self.tasks.push(TaskDisplay {
                    id,
                    title,
                    status: TaskStatus::Pending,
                    duration: None,
                    started_at: None,
                });
                self.total_count = self.tasks.len();
            }
            Event::TaskStatusChanged { id, status } => {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    // Track when task starts
                    if status == TaskStatus::InProgress && task.started_at.is_none() {
                        task.started_at = Some(Instant::now());
                    }
                    // Calculate duration from per-task start time
                    if status == TaskStatus::Completed {
                        task.duration = task.started_at.map(|s| s.elapsed());
                    }
                    task.status = status;
                }

                // Update counts
                self.completed_count = self.tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
                self.failed_count = self.tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();

                // Track current task
                if status == TaskStatus::InProgress {
                    self.current_task_id = Some(id.clone());
                } else if self.current_task_id.as_ref() == Some(&id) {
                    self.current_task_id = None;
                }
            }
            Event::TaskProgress { id, message } => {
                // Could be shown in output panel
                let _ = (id, message); // For now, ignore
            }
            Event::ClaudeThinking { task_id, content } => {
                if self.verbosity == VerbosityLevel::Verbose {
                    self.output_task_id = Some(task_id);
                    self.output_lines.push(OutputLine::Thinking { content });
                }
            }
            Event::ClaudeToolUse { task_id, tool_name, tool_input } => {
                self.output_task_id = Some(task_id);
                if self.verbosity >= VerbosityLevel::Normal {
                    self.output_lines.push(OutputLine::ToolUse { tool_name, tool_input });
                }
            }
            Event::ClaudeToolResult { task_id, tool_name, result, success } => {
                self.output_task_id = Some(task_id);
                if self.verbosity >= VerbosityLevel::Normal {
                    self.output_lines.push(OutputLine::ToolResult { tool_name, result, success });
                }
            }
            Event::ClaudeResult { task_id, result } => {
                self.output_task_id = Some(task_id);
                self.output_lines.push(OutputLine::Result { content: result });
            }
            Event::Log { timestamp, level, message } => {
                self.log_buffer.push(level, message);
                let _ = timestamp; // LogEntry uses Utc::now()
            }
            Event::ExecutionStateChanged { state } => {
                self.state = state;
                if state == ExecutionState::Running && self.start_time.is_none() {
                    self.start_time = Some(Instant::now());
                }
            }
            Event::ProgressUpdate { completed, total, failed, elapsed } => {
                self.completed_count = completed;
                self.total_count = total;
                self.failed_count = failed;
                let _ = elapsed;
            }
            Event::ModelChanged { model } => {
                self.current_model = model;
            }
        }
    }

    /// Try to receive and process events (non-blocking)
    pub fn poll_events(&mut self) {
        // Collect all events first to avoid borrowing issues
        let events: Vec<Event> = if let Some(ref mut receiver) = self.event_receiver {
            std::iter::from_fn(|| receiver.try_recv().ok()).collect()
        } else {
            Vec::new()
        };
        for event in events {
            self.process_event(event);
        }
    }

    /// Try to receive and process events, returning count of processed events
    pub fn poll_events_count(&mut self) -> usize {
        // Collect all events first to avoid borrowing issues
        let events: Vec<Event> = if let Some(ref mut receiver) = self.event_receiver {
            std::iter::from_fn(|| receiver.try_recv().ok()).collect()
        } else {
            Vec::new()
        };
        let count = events.len();
        for event in events {
            self.process_event(event);
        }
        count
    }

    /// Get elapsed time as formatted string
    pub fn elapsed_string(&self) -> String {
        match self.start_time {
            Some(start) => {
                let elapsed = start.elapsed();
                let secs = elapsed.as_secs();
                let mins = secs / 60;
                let secs = secs % 60;
                format!("{:02}:{:02}", mins, secs)
            }
            None => "00:00".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_navigation() {
        assert_eq!(Tab::Tasks.next(), Tab::Output);
        assert_eq!(Tab::Output.next(), Tab::Logs);
        assert_eq!(Tab::Logs.next(), Tab::Tasks);

        assert_eq!(Tab::Tasks.prev(), Tab::Logs);
        assert_eq!(Tab::Output.prev(), Tab::Tasks);
    }

    #[test]
    fn test_tui_app_new() {
        let app = TuiApp::new(VerbosityLevel::Normal);
        assert_eq!(app.current_tab, Tab::Tasks);
        assert_eq!(app.verbosity, VerbosityLevel::Normal);
        assert!(app.running);
    }

    #[test]
    fn test_handle_key_tab() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.handle_key(Key::Tab);
        assert_eq!(app.current_tab, Tab::Output);
    }

    #[test]
    fn test_handle_key_quit() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.handle_key(Key::Char('q'));
        assert!(!app.running);
    }
}