//! TUI Application state and main loop.

use crate::models::TaskStatus;
use crate::tui::{ClarificationQuestion, ConfirmSender, Event, EventReceiver, ExecutionState, Key, LogBuffer, VerbosityLevel};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// State for resume confirmation dialog
#[derive(Debug, Default)]
pub struct ResumeConfirmState {
    pub completed: usize,
    pub pending: usize,
    pub failed: usize,
    pub selected: bool,  // true = Resume, false = Start Fresh
    pub response_tx: Option<ConfirmSender>,
}

impl ResumeConfirmState {
    pub fn is_active(&self) -> bool {
        self.response_tx.is_some()
    }

    pub fn finish(&mut self) {
        self.response_tx = None;
    }
}

/// State for quit confirmation dialog
#[derive(Debug, Default)]
pub struct QuitConfirmState {
    pub pending: bool,  // Whether there are pending tasks
    pub confirmed: bool, // User confirmed quit
}

impl QuitConfirmState {
    pub fn is_active(&self) -> bool {
        self.pending && !self.confirmed
    }
}

/// State for task detail panel
#[derive(Debug, Default)]
pub struct TaskDetailState {
    pub task_id: Option<String>,
}

impl TaskDetailState {
    pub fn is_active(&self) -> bool {
        self.task_id.is_some()
    }

    pub fn close(&mut self) {
        self.task_id = None;
    }
}

/// State for task search/filter
#[derive(Debug, Default)]
pub struct SearchState {
    pub active: bool,
    pub query: String,
}

impl SearchState {
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
        if !self.active {
            self.query.clear();
        }
    }

    pub fn close(&mut self) {
        self.active = false;
        self.query.clear();
    }
}

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
    pub description: String,
    pub status: TaskStatus,
    pub duration: Option<Duration>,
    pub started_at: Option<Instant>,
    pub parent_id: Option<String>,
    pub depth: u32,
    pub depends_on: Vec<String>,
    pub error: Option<String>,
}

/// Claude output line
#[derive(Debug, Clone)]
pub enum OutputLine {
    Thinking { task_id: String, content: String },
    ToolUse { task_id: String, tool_name: String, tool_input: Option<String> },
    ToolResult { task_id: String, tool_name: String, result: String, success: bool },
    Result { task_id: String, content: String },
}

/// Main TUI application state
pub struct TuiApp {
    // Tab state
    pub current_tab: Tab,

    // Execution state
    pub state: ExecutionState,
    pub current_task_id: Option<String>,
    pub current_model: String,
    pub is_paused: bool,  // Pause execution

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
    pub tree_view: bool,  // Toggle between list and tree view

    // Task search
    pub search: SearchState,

    // Task detail
    pub task_detail: TaskDetailState,

    // Claude output (per-task storage)
    pub output_by_task: HashMap<String, Vec<OutputLine>>,
    pub output_lines: Vec<OutputLine>,  // All output (current view)
    pub output_scroll: u16,
    pub output_task_id: Option<String>,  // Currently viewed task output
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

    // Resume confirmation
    pub resume_confirm: ResumeConfirmState,

    // Quit confirmation
    pub quit_confirm: QuitConfirmState,

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
            is_paused: false,
            completed_count: 0,
            total_count: 0,
            failed_count: 0,
            start_time: None,
            task_start_time: None,
            spinner_frame: 0,
            tasks: Vec::new(),
            tasks_scroll: 0,
            tree_view: false,
            search: SearchState::default(),
            task_detail: TaskDetailState::default(),
            output_by_task: HashMap::new(),
            output_lines: Vec::new(),
            output_scroll: 0,
            output_task_id: None,
            output_auto_follow: true,
            log_buffer: LogBuffer::default(),
            logs_scroll: 0,
            logs_auto_follow: true,
            verbosity,
            event_receiver: None,
            show_help: false,
            clarification: ClarificationState::default(),
            resume_confirm: ResumeConfirmState::default(),
            quit_confirm: QuitConfirmState::default(),
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

    /// Get filtered tasks based on search query
    pub fn filtered_tasks(&self) -> Vec<&TaskDisplay> {
        if self.search.query.is_empty() {
            self.tasks.iter().collect()
        } else {
            let query = self.search.query.to_lowercase();
            self.tasks
                .iter()
                .filter(|t| {
                    t.id.to_lowercase().contains(&query) ||
                    t.title.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    /// Get selected task (based on scroll position)
    pub fn selected_task(&self) -> Option<&TaskDisplay> {
        let filtered = self.filtered_tasks();
        filtered.get(self.tasks_scroll.min(filtered.len().saturating_sub(1))).copied()
    }

    /// Switch output view to specific task
    pub fn switch_output_to_task(&mut self, task_id: &str) {
        self.output_task_id = Some(task_id.to_string());
        if let Some(lines) = self.output_by_task.get(task_id) {
            self.output_lines = lines.clone();
        } else {
            self.output_lines.clear();
        }
        self.output_scroll = 0;
        self.output_auto_follow = true;
    }

    /// Switch output view to all tasks
    pub fn switch_output_to_all(&mut self) {
        self.output_task_id = None;
        // Combine all outputs sorted by time (approximated by insertion order)
        let task_ids: Vec<String> = self.tasks.iter().map(|t| t.id.clone()).collect();
        let mut all_lines: Vec<OutputLine> = Vec::new();
        for task_id in &task_ids {
            if let Some(lines) = self.output_by_task.get(task_id.as_str()) {
                all_lines.extend(lines.clone());
            }
        }
        self.output_lines = all_lines;
        self.output_scroll = 0;
        self.output_auto_follow = true;
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: Key) {
        // Handle search input mode
        if self.search.is_active() {
            self.handle_search_key(key);
            return;
        }

        // Handle resume confirmation first
        if self.resume_confirm.is_active() {
            self.handle_resume_confirm_key(key);
            return;
        }

        // Handle clarification questions
        if self.clarification.is_active() {
            self.handle_clarification_key(key);
            return;
        }

        // Handle task detail panel
        if self.task_detail.is_active() {
            self.handle_task_detail_key(key);
            return;
        }

        // Handle quit confirmation
        if self.quit_confirm.is_active() {
            self.handle_quit_confirm_key(key);
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
                self.try_quit();
            }
            Key::Char('p') => {
                self.is_paused = !self.is_paused;
            }
            Key::Char('t') => {
                if self.current_tab == Tab::Tasks {
                    self.tree_view = !self.tree_view;
                }
            }
            Key::Char('/') => {
                if self.current_tab == Tab::Tasks {
                    self.search.toggle();
                }
            }
            Key::Enter => {
                if self.current_tab == Tab::Tasks {
                    if let Some(task) = self.selected_task() {
                        self.task_detail.task_id = Some(task.id.clone());
                    }
                }
            }
            Key::Char('a') => {
                if self.current_tab == Tab::Output {
                    self.switch_output_to_all();
                }
            }
            Key::Char(c) if c.is_ascii_digit() => {
                // 1-9: Switch to specific task output
                if self.current_tab == Tab::Output {
                    let num = c.to_digit(10).unwrap() as usize;
                    if num > 0 && num <= self.tasks.len() {
                        let task_id = self.tasks[num - 1].id.clone();
                        self.switch_output_to_task(&task_id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle search input
    fn handle_search_key(&mut self, key: Key) {
        match key {
            Key::Char(c) => {
                self.search.query.push(c);
                // Reset scroll to first match
                self.tasks_scroll = 0;
            }
            Key::Backspace => {
                self.search.query.pop();
            }
            Key::Esc => {
                self.search.close();
            }
            Key::Enter => {
                self.search.close();
            }
            _ => {}
        }
    }

    /// Handle task detail panel keys
    fn handle_task_detail_key(&mut self, key: Key) {
        match key {
            Key::Esc | Key::Char('q') | Key::Enter => {
                self.task_detail.close();
            }
            _ => {}
        }
    }

    /// Handle quit confirmation
    fn handle_quit_confirm_key(&mut self, key: Key) {
        match key {
            Key::Char('y') | Key::Char('Y') | Key::Char('q') => {
                self.quit_confirm.confirmed = true;
                self.running = false;
            }
            Key::Char('n') | Key::Char('N') | Key::Esc => {
                self.quit_confirm.pending = false;
            }
            _ => {}
        }
    }

    /// Try to quit (with confirmation if tasks running)
    fn try_quit(&mut self) {
        let has_pending = self.tasks.iter().any(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress);
        if has_pending && !self.quit_confirm.confirmed {
            self.quit_confirm.pending = true;
        } else {
            self.running = false;
        }
    }

    /// Handle keyboard input during resume confirmation
    fn handle_resume_confirm_key(&mut self, key: Key) {
        match key {
            Key::Left | Key::Right => {
                self.resume_confirm.selected = !self.resume_confirm.selected;
            }
            Key::Char('y') | Key::Char('Y') => {
                if let Some(tx) = self.resume_confirm.response_tx.take() {
                    let _ = tx.send(true);
                }
                self.resume_confirm.finish();
            }
            Key::Char('n') | Key::Char('N') => {
                if let Some(tx) = self.resume_confirm.response_tx.take() {
                    let _ = tx.send(false);
                }
                self.resume_confirm.finish();
            }
            Key::Enter => {
                if let Some(tx) = self.resume_confirm.response_tx.take() {
                    let _ = tx.send(self.resume_confirm.selected);
                }
                self.resume_confirm.finish();
            }
            Key::Esc => {
                // Default to resume on escape
                if let Some(tx) = self.resume_confirm.response_tx.take() {
                    let _ = tx.send(true);
                }
                self.resume_confirm.finish();
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
                self.output_auto_follow = false;
            }
            Tab::Logs => {
                self.logs_scroll = self.logs_scroll.saturating_sub(1);
                self.logs_auto_follow = false;
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.current_tab {
            Tab::Tasks => {
                let filtered = self.filtered_tasks();
                if self.tasks_scroll < filtered.len().saturating_sub(1) {
                    self.tasks_scroll += 1;
                }
            }
            Tab::Output => {
                let max_scroll = self.output_lines.len() as u16;
                if self.output_scroll < max_scroll {
                    self.output_scroll = self.output_scroll.saturating_add(1);
                }
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
                if self.logs_scroll >= max_scroll.saturating_sub(5) {
                    self.logs_auto_follow = true;
                }
            }
        }
    }

    /// Process an orchestrator event
    pub fn process_event(&mut self, event: Event) {
        match event {
            Event::TaskCreated { id, title, parent_id, depth, depends_on } => {
                self.tasks.push(TaskDisplay {
                    id: id.clone(),
                    title,
                    description: String::new(),
                    status: TaskStatus::Pending,
                    duration: None,
                    started_at: None,
                    parent_id,
                    depth,
                    depends_on,
                    error: None,
                });
                self.total_count = self.tasks.len();
                // Initialize output storage for this task
                self.output_by_task.insert(id, Vec::new());
            }
            Event::TaskStatusChanged { id, status } => {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    if status == TaskStatus::InProgress && task.started_at.is_none() {
                        task.started_at = Some(Instant::now());
                    }
                    if status == TaskStatus::Completed {
                        task.duration = task.started_at.map(|s| s.elapsed());
                    }
                    task.status = status;
                }

                self.completed_count = self.tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
                self.failed_count = self.tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();

                if status == TaskStatus::InProgress {
                    self.current_task_id = Some(id.clone());
                    self.task_start_time = Some(Instant::now());
                } else if self.current_task_id.as_ref() == Some(&id) {
                    self.current_task_id = None;
                    self.task_start_time = None;
                }
            }
            Event::TaskProgress { id, message } => {
                // Store progress message in task description or error
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    task.description = message;
                }
            }
            Event::ClaudeThinking { task_id, content } => {
                let line = OutputLine::Thinking { task_id: task_id.clone(), content };
                self.output_by_task.entry(task_id.clone()).or_default().push(line.clone());
                if self.output_task_id.is_none() || self.output_task_id.as_ref() == Some(&task_id) {
                    self.output_lines.push(line);
                }
            }
            Event::ClaudeToolUse { task_id, tool_name, tool_input } => {
                if self.verbosity >= VerbosityLevel::Normal {
                    let line = OutputLine::ToolUse { task_id: task_id.clone(), tool_name, tool_input };
                    self.output_by_task.entry(task_id.clone()).or_default().push(line.clone());
                    if self.output_task_id.is_none() || self.output_task_id.as_ref() == Some(&task_id) {
                        self.output_lines.push(line);
                    }
                }
            }
            Event::ClaudeToolResult { task_id, tool_name, result, success } => {
                if self.verbosity >= VerbosityLevel::Normal {
                    let line = OutputLine::ToolResult { task_id: task_id.clone(), tool_name, result, success };
                    self.output_by_task.entry(task_id.clone()).or_default().push(line.clone());
                    if self.output_task_id.is_none() || self.output_task_id.as_ref() == Some(&task_id) {
                        self.output_lines.push(line);
                    }
                }
            }
            Event::ClaudeResult { task_id, result } => {
                let line = OutputLine::Result { task_id: task_id.clone(), content: result };
                self.output_by_task.entry(task_id.clone()).or_default().push(line.clone());
                if self.output_task_id.is_none() || self.output_task_id.as_ref() == Some(&task_id) {
                    self.output_lines.push(line);
                }
            }
            Event::Log { timestamp, level, message } => {
                self.log_buffer.push(level, message);
                let _ = timestamp;
                if self.logs_auto_follow {
                    self.logs_scroll = u16::MAX;
                }
            }
            Event::ExecutionStateChanged { state } => {
                self.state = state;
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
            Event::ResumeConfirm { completed, pending, failed, response_tx } => {
                self.resume_confirm.completed = completed;
                self.resume_confirm.pending = pending;
                self.resume_confirm.failed = failed;
                self.resume_confirm.selected = true;
                self.resume_confirm.response_tx = Some(response_tx);
            }
        }
    }

    /// Try to receive and process events (non-blocking)
    pub fn poll_events(&mut self) {
        let events: Vec<Event> = if let Some(ref mut receiver) = self.event_receiver {
            std::iter::from_fn(|| receiver.try_recv().ok()).collect()
        } else {
            Vec::new()
        };
        for event in events {
            self.process_event(event);
        }
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

    /// Get progress percentage
    pub fn progress_percent(&self) -> u8 {
        if self.total_count == 0 {
            0
        } else {
            ((self.completed_count + self.failed_count) * 100 / self.total_count) as u8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_navigation() {
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
        assert_eq!(app.current_tab, Tab::Logs);
        assert_eq!(app.verbosity, VerbosityLevel::Normal);
        assert!(app.running);
        assert!(!app.is_paused);
        assert!(!app.tree_view);
    }

    #[test]
    fn test_handle_key_tab() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.handle_key(Key::Tab);
        assert_eq!(app.current_tab, Tab::Tasks);
    }

    #[test]
    fn test_handle_key_pause() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.handle_key(Key::Char('p'));
        assert!(app.is_paused);
        app.handle_key(Key::Char('p'));
        assert!(!app.is_paused);
    }

    #[test]
    fn test_search_toggle() {
        let mut app = TuiApp::new(VerbosityLevel::Normal);
        app.current_tab = Tab::Tasks;
        app.handle_key(Key::Char('/'));
        assert!(app.search.is_active());
    }
}
