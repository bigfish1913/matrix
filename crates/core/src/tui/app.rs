//! TUI Application state and main loop.

use crate::models::TaskStatus;
use crate::tui::{ClarificationQuestion, Event, EventReceiver, ExecutionState, Key, LogBuffer, VerbosityLevel};
use std::time::{Duration, Instant};

/// State for clarification questions dialog (multiple choice)
#[derive(Debug, Default)]
pub struct ClarificationState {
    pub questions: Vec<ClarificationQuestion>,
    pub answers: Vec<String>,
    pub current_index: usize,
    pub selected_option: usize,  // Currently highlighted option
    pub custom_input: String,    // For "Other" option
    pub is_custom_input: bool,   // Whether user is typing custom input
    pub response_tx: Option<crate::tui::event::AnswerSender>,
}

impl ClarificationState {
    pub fn is_active(&self) -> bool {
        self.response_tx.is_some()
    }

    pub fn finish(&mut self) {
        self.response_tx = None;
        self.questions.clear();
        self.answers.clear();
        self.current_index = 0;
        self.selected_option = 0;
        self.custom_input.clear();
        self.is_custom_input = false;
    }

    /// Get total number of options for current question (including "Other")
    pub fn total_options(&self) -> usize {
        if let Some(q) = self.questions.get(self.current_index) {
            q.options.len() + 1 // +1 for "Other" option
        } else {
            0
        }
    }

    /// Check if "Other" option is selected
    pub fn is_other_selected(&self) -> bool {
        let total = self.total_options();
        total > 0 && self.selected_option == total - 1
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Logs,
    Tasks,
    Output,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Logs => Self::Tasks,
            Self::Tasks => Self::Output,
            Self::Output => Self::Logs,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Logs => Self::Output,
            Self::Tasks => Self::Logs,
            Self::Output => Self::Tasks,
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
    pub start_time: Option<Instant>,        // Total elapsed time
    pub task_start_time: Option<Instant>,   // Current task elapsed time

    // Animation
    pub spinner_frame: usize,              // Current spinner frame

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

    // Clarification questions (ask mode)
    pub clarification: ClarificationState,

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
            task_start_time: None,
            spinner_frame: 0,
            tasks: Vec::new(),
            tasks_scroll: 0,
            output_lines: Vec::new(),
            output_scroll: 0,
            output_task_id: None,
            output_auto_follow: true,  // Start with auto-follow enabled
            log_buffer: LogBuffer::default(),
            logs_scroll: 0,
            logs_auto_follow: true,  // Enable auto-follow for logs (auto-scroll to new logs)
            verbosity,
            event_receiver: None,
            show_help: false,
            clarification: ClarificationState::default(),
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
        // Handle clarification questions first
        if self.clarification.is_active() {
            self.handle_clarification_key(key);
            return;
        }

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

    /// Handle keyboard input during clarification questions (multiple choice)
    fn handle_clarification_key(&mut self, key: Key) {
        let total_options = self.clarification.total_options();

        if self.clarification.is_custom_input {
            // User is typing custom input for "Other" option
            match key {
                Key::Enter => {
                    // Save custom input and move to next question
                    let answer = self.clarification.custom_input.clone();
                    self.clarification.answers.push(answer);
                    self.clarification.current_index += 1;
                    self.clarification.custom_input.clear();
                    self.clarification.is_custom_input = false;
                    self.clarification.selected_option = 0;

                    // Check if all questions answered
                    if self.clarification.current_index >= self.clarification.questions.len() {
                        if let Some(tx) = self.clarification.response_tx.take() {
                            let _ = tx.send(self.clarification.answers.clone());
                        }
                        self.clarification.finish();
                    }
                }
                Key::Char(c) => {
                    self.clarification.custom_input.push(c);
                }
                Key::Backspace => {
                    self.clarification.custom_input.pop();
                }
                Key::Esc => {
                    // Cancel custom input, go back to selection
                    self.clarification.is_custom_input = false;
                    self.clarification.custom_input.clear();
                }
                _ => {}
            }
        } else {
            // Normal selection mode
            match key {
                Key::Up => {
                    if self.clarification.selected_option > 0 {
                        self.clarification.selected_option -= 1;
                    }
                }
                Key::Down => {
                    if self.clarification.selected_option < total_options - 1 {
                        self.clarification.selected_option += 1;
                    }
                }
                Key::Char(c) if c.is_ascii_digit() => {
                    // Direct selection by number (1-9)
                    let num = c.to_digit(10).unwrap() as usize;
                    if num > 0 && num <= total_options {
                        self.clarification.selected_option = num - 1;
                        // Auto-confirm selection
                        self.confirm_clarification_option();
                    }
                }
                Key::Enter => {
                    self.confirm_clarification_option();
                }
                Key::Esc => {
                    // Cancel all questions - send empty answers
                    if let Some(tx) = self.clarification.response_tx.take() {
                        let empty_answers: Vec<String> =
                            std::iter::repeat(String::new())
                                .take(self.clarification.questions.len())
                                .collect();
                        let _ = tx.send(empty_answers);
                    }
                    self.clarification.finish();
                }
                _ => {}
            }
        }
    }

    /// Confirm current selection and move to next question
    fn confirm_clarification_option(&mut self) {
        if self.clarification.is_other_selected() {
            // "Other" selected - switch to custom input mode
            self.clarification.is_custom_input = true;
        } else {
            // Regular option selected
            let answer = if let Some(q) = self.clarification.questions.get(self.clarification.current_index) {
                if self.clarification.selected_option < q.options.len() {
                    q.options[self.clarification.selected_option].clone()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            self.clarification.answers.push(answer);
            self.clarification.current_index += 1;
            self.clarification.selected_option = 0;

            // Check if all questions answered
            if self.clarification.current_index >= self.clarification.questions.len() {
                if let Some(tx) = self.clarification.response_tx.take() {
                    let _ = tx.send(self.clarification.answers.clone());
                }
                self.clarification.finish();
            }
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
                // Keep auto-follow enabled (user preference)
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
                // Re-enable auto-follow when scrolled near bottom (consistent with Output panel)
                if self.logs_scroll >= max_scroll.saturating_sub(5) {
                    self.logs_auto_follow = true;
                }
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

                // Track current task and task start time
                if status == TaskStatus::InProgress {
                    self.current_task_id = Some(id.clone());
                    self.task_start_time = Some(Instant::now());  // Reset task timer
                } else if self.current_task_id.as_ref() == Some(&id) {
                    self.current_task_id = None;
                    self.task_start_time = None;  // Clear task timer
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
                // Auto-scroll if auto-follow is enabled
                if self.logs_auto_follow {
                    // Scroll will be calculated in render based on viewport height
                    // Just mark that we need to recalculate
                    self.logs_scroll = u16::MAX; // Signal to recalculate in render
                }
            }
            Event::ExecutionStateChanged { state } => {
                self.state = state;
                // Start timer on any non-idle state (Clarifying, Running, etc.)
                if state != ExecutionState::Idle && self.start_time.is_none() {
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
            Event::ClarificationQuestions { questions, response_tx } => {
                self.clarification.questions = questions;
                self.clarification.answers = Vec::new();
                self.clarification.current_index = 0;
                self.clarification.selected_option = 0;
                self.clarification.custom_input.clear();
                self.clarification.is_custom_input = false;
                self.clarification.response_tx = Some(response_tx);
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
        // New order: Logs -> Tasks -> Output -> Logs
        assert_eq!(Tab::Logs.next(), Tab::Tasks);
        assert_eq!(Tab::Tasks.next(), Tab::Output);
        assert_eq!(Tab::Output.next(), Tab::Logs);

        assert_eq!(Tab::Logs.prev(), Tab::Output);
        assert_eq!(Tab::Tasks.prev(), Tab::Logs);
        assert_eq!(Tab::Output.prev(), Tab::Tasks);
    }

    #[test]
    fn test_tui_app_new() {
        let app = TuiApp::new(VerbosityLevel::Normal);
        assert_eq!(app.current_tab, Tab::Logs);  // Default is now Logs
        assert_eq!(app.verbosity, VerbosityLevel::Normal);
        assert!(app.running);
    }

    #[test]
    fn test_handle_key_tab() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.handle_key(Key::Tab);
        assert_eq!(app.current_tab, Tab::Tasks);  // Logs -> Tasks
    }

    #[test]
    fn test_handle_key_quit() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.handle_key(Key::Char('q'));
        assert!(!app.running);
    }
}