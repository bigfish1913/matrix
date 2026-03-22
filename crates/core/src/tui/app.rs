//! TUI Application state and main loop.

use crate::models::{Question, QuestionStatus, TaskStatus};
use crate::tui::{
    Activity, ClarificationQuestion, ClarificationSender, ConfirmSender, Event, EventReceiver,
    EventSender, ExecutionState, Key, LogBuffer, LogContext, LogLevel, QuestionSender, VerbosityLevel,
};
use crate::tui::components::QuestionsPanel;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// State for resume confirmation dialog
#[derive(Debug, Default)]
pub struct ResumeConfirmState {
    pub completed: usize,
    pub pending: usize,
    pub failed: usize,
    pub selected: bool, // true = Resume, false = Start Fresh
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
    pub pending: bool,   // Whether there are pending tasks
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

/// State for clarification task (when Claude generates a question task)
#[derive(Debug, Default)]
pub struct ClarificationTaskState {
    pub task_id: Option<String>,
    pub title: String,
    pub description: String,
    pub response: String,
    pub response_tx: Option<crate::tui::event::ClarificationSender>,
}

impl ClarificationTaskState {
    pub fn is_active(&self) -> bool {
        self.response_tx.is_some()
    }

    pub fn finish(&mut self) {
        self.response_tx = None;
        self.task_id = None;
        self.title.clear();
        self.description.clear();
        self.response.clear();
    }
}

/// State for agent question dialog
#[derive(Debug, Default)]
pub struct AgentQuestionState {
    pub task_id: Option<String>,
    pub question: Option<crate::models::Question>,
    pub response_tx: Option<crate::tui::event::QuestionSender>,
}

impl AgentQuestionState {
    pub fn is_active(&self) -> bool {
        self.response_tx.is_some()
    }

    pub fn finish(&mut self) {
        self.response_tx = None;
        self.task_id = None;
        self.question = None;
    }
}

/// State for clarification questions dialog (multiple choice)
#[derive(Debug, Default)]
pub struct ClarificationState {
    pub questions: Vec<ClarificationQuestion>,
    pub answers: Vec<String>,
    pub current_index: usize,
    pub selected_option: usize, // Currently highlighted option
    pub custom_input: String,   // For "Other" option
    pub is_custom_input: bool,  // Whether user is typing custom input
    pub scroll: u16,            // Scroll offset for long content
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
    Questions,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Logs => Self::Tasks,
            Self::Tasks => Self::Output,
            Self::Output => Self::Questions,
            Self::Questions => Self::Logs,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Logs => Self::Questions,
            Self::Tasks => Self::Logs,
            Self::Output => Self::Tasks,
            Self::Questions => Self::Output,
        }
    }

    /// Get next visible tab based on verbosity
    pub fn next_visible(self, verbosity: VerbosityLevel) -> Self {
        let next_tab = self.next();
        // Skip Output tab in non-verbose mode
        if next_tab == Tab::Output && verbosity < VerbosityLevel::Verbose {
            next_tab.next() // Skip to Questions
        } else {
            next_tab
        }
    }

    /// Get previous visible tab based on verbosity
    pub fn prev_visible(self, verbosity: VerbosityLevel) -> Self {
        let prev_tab = self.prev();
        // Skip Output tab in non-verbose mode
        if prev_tab == Tab::Output && verbosity < VerbosityLevel::Verbose {
            prev_tab.prev() // Skip to Tasks
        } else {
            prev_tab
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

/// Claude output line with sequence number for ordering
#[derive(Debug, Clone)]
pub enum OutputLine {
    Thinking {
        task_id: String,
        content: String,
        seq: u64, // Sequence number for ordering
    },
    ToolUse {
        task_id: String,
        tool_name: String,
        tool_input: Option<String>,
        seq: u64,
    },
    ToolResult {
        task_id: String,
        tool_name: String,
        result: String,
        success: bool,
        seq: u64,
    },
    Result {
        task_id: String,
        content: String,
        seq: u64,
    },
}

impl OutputLine {
    pub fn seq(&self) -> u64 {
        match self {
            OutputLine::Thinking { seq, .. } => *seq,
            OutputLine::ToolUse { seq, .. } => *seq,
            OutputLine::ToolResult { seq, .. } => *seq,
            OutputLine::Result { seq, .. } => *seq,
        }
    }
}

/// Main TUI application state
pub struct TuiApp {
    // Tab state
    pub current_tab: Tab,

    // Execution state
    pub state: ExecutionState,
    pub current_task_id: Option<String>,
    pub current_model: String,
    pub is_paused: bool, // Pause execution

    // Progress
    pub completed_count: usize,
    pub total_count: usize,
    pub failed_count: usize,
    pub start_time: Option<Instant>,      // Total elapsed time
    pub task_start_time: Option<Instant>, // Current task elapsed time

    // Activity pulse tracking
    pub last_pulse_time: Option<Instant>,
    pub current_activity: Option<Activity>,

    // Token usage tracking
    pub current_task_tokens: u32, // Tokens used in current task
    pub total_tokens: u32,        // Total tokens used across all tasks

    // Animation
    pub spinner_frame: usize, // Current spinner frame

    // Tasks
    pub tasks: Vec<TaskDisplay>,
    pub tasks_scroll: usize,
    pub tree_view: bool, // Toggle between list and tree view

    // Task search
    pub search: SearchState,

    // Task detail
    pub task_detail: TaskDetailState,

    // Claude output (per-task storage)
    pub output_by_task: HashMap<String, Vec<OutputLine>>,
    pub output_lines: Vec<OutputLine>, // All output (current view)
    pub output_scroll: usize,
    pub output_task_id: Option<String>, // Currently viewed task output
    pub output_auto_follow: bool,       // Auto-scroll to bottom on new output
    pub max_output_lines: usize,        // Maximum lines to keep in output
    pub output_seq_counter: u64,        // Global sequence counter for ordering

    // Logs
    pub log_buffer: LogBuffer,
    pub logs_scroll: u16,
    pub logs_auto_follow: bool, // Auto-scroll to bottom on new logs

    // Questions tab
    pub questions: Vec<Question>,
    pub questions_panel: QuestionsPanel,
    pub questions_scroll: usize,

    // Verbosity
    pub verbosity: VerbosityLevel,

    // Event receiver
    event_receiver: Option<EventReceiver>,
    /// Sender for responses back to orchestrator (question answers)
    response_sender: Option<EventSender>,

    // Help overlay
    pub show_help: bool,

    // Clarification questions (ask mode)
    pub clarification: ClarificationState,

    // Resume confirmation
    pub resume_confirm: ResumeConfirmState,

    // Clarification task (when Claude generates a question task)
    pub clarification_task: ClarificationTaskState,

    // Agent question dialog
    pub agent_question: AgentQuestionState,

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
            current_task_tokens: 0,
            total_tokens: 0,
            spinner_frame: 0,
            last_pulse_time: None,
            current_activity: None,
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
            max_output_lines: 500,
            output_seq_counter: 0,
            log_buffer: LogBuffer::default(),
            logs_scroll: 0,
            logs_auto_follow: true,
            questions: Vec::new(),
            questions_panel: QuestionsPanel::new(),
            questions_scroll: 0,
            verbosity,
            event_receiver: None,
            response_sender: None,
            show_help: false,
            clarification: ClarificationState::default(),
            resume_confirm: ResumeConfirmState::default(),
            clarification_task: ClarificationTaskState::default(),
            agent_question: AgentQuestionState::default(),
            quit_confirm: QuitConfirmState::default(),
            running: true,
        }
    }

    pub fn with_event_receiver(mut self, receiver: EventReceiver) -> Self {
        self.event_receiver = Some(receiver);
        self
    }

    pub fn with_response_sender(mut self, sender: EventSender) -> Self {
        self.response_sender = Some(sender);
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
                    t.id.to_lowercase().contains(&query) || t.title.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    /// Get selected task (based on scroll position)
    pub fn selected_task(&self) -> Option<&TaskDisplay> {
        let filtered = self.filtered_tasks();
        filtered
            .get(self.tasks_scroll.min(filtered.len().saturating_sub(1)))
            .copied()
    }

    /// Get selected question (from questions_panel state)
    pub fn selected_question(&self) -> Option<&Question> {
        let selected = self.questions_panel.state.selected()?;
        self.questions.get(selected)
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
        // Combine all outputs and sort by sequence number
        let mut all_lines: Vec<OutputLine> = Vec::new();
        for lines in self.output_by_task.values() {
            all_lines.extend(lines.clone());
        }
        // Sort by sequence number to maintain chronological order
        all_lines.sort_by_key(|line| line.seq());
        self.output_lines = all_lines;
        self.output_scroll = 0;
        self.output_auto_follow = true;
    }

    /// Add output line with automatic trimming
    fn add_output_line(&mut self, task_id: String, line: OutputLine) {
        // Add to per-task storage
        self.output_by_task
            .entry(task_id.clone())
            .or_default()
            .push(line.clone());

        // Trim per-task storage if needed
        if let Some(lines) = self.output_by_task.get_mut(&task_id) {
            if lines.len() > self.max_output_lines {
                let remove_count = lines.len() - self.max_output_lines;
                lines.drain(0..remove_count);
            }
        }

        // Add to current view if applicable (show in "all" view or matching task view)
        let should_add_to_view = self.output_task_id.is_none() || self.output_task_id.as_ref() == Some(&task_id);
        if should_add_to_view {
            self.output_lines.push(line.clone());
            // Trim current view if needed
            if self.output_lines.len() > self.max_output_lines {
                let remove_count = self.output_lines.len() - self.max_output_lines;
                self.output_lines.drain(0..remove_count);
            }
        }

        // Auto-scroll to bottom if auto_follow is enabled and viewing the matching task or all
        if self.output_auto_follow && should_add_to_view {
            // Use a special marker value (u16::MAX means "scroll to end")
            self.output_scroll = usize::MAX;
        }
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

        // Handle question answer dialog
        if self.questions_panel.in_answer_dialog {
            self.handle_question_answer_key(key);
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
                self.current_tab = self.current_tab.next_visible(self.verbosity);
                self.reset_scroll();
            }
            Key::BackTab | Key::Left => {
                self.current_tab = self.current_tab.prev_visible(self.verbosity);
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
                } else if self.current_tab == Tab::Questions {
                    if let Some(question) = self.selected_question() {
                        if question.status == QuestionStatus::Pending {
                            self.questions_panel.in_answer_dialog = true;
                            self.questions_panel.dialog_selection = 0;
                        }
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

    /// Handle question answer dialog keys
    fn handle_question_answer_key(&mut self, key: Key) {
        let Some(selected) = self.questions_panel.state.selected() else {
            return;
        };
        let Some(question) = self.questions.get(selected).cloned() else {
            return;
        };

        match key {
            Key::Char('q') | Key::Esc => {
                // Cancel dialog
                self.questions_panel.in_answer_dialog = false;
                self.questions_panel.dialog_selection = 0;
            }
            Key::Char('j') | Key::Down => {
                let total = self.questions_panel.total_options(&question);
                self.questions_panel.dialog_selection =
                    (self.questions_panel.dialog_selection + 1) % total;
            }
            Key::Char('k') | Key::Up => {
                let total = self.questions_panel.total_options(&question);
                if self.questions_panel.dialog_selection == 0 {
                    self.questions_panel.dialog_selection = total - 1;
                } else {
                    self.questions_panel.dialog_selection -= 1;
                }
            }
            Key::Char(c) if c.is_ascii_digit() => {
                let num = c.to_digit(10).unwrap_or(0) as usize;
                let total = self.questions_panel.total_options(&question);
                if num > 0 && num <= total {
                    self.questions_panel.dialog_selection = num - 1;
                }
            }
            Key::Enter | Key::Char(' ') => {
                // Submit answer
                let answer = if self.questions_panel.is_other_selected(&question) {
                    self.questions_panel.custom_input.clone()
                } else {
                    question
                        .options
                        .get(self.questions_panel.dialog_selection)
                        .cloned()
                        .unwrap_or_default()
                };

                // Send QuestionAnswered event to orchestrator
                if let Some(ref sender) = self.response_sender {
                    let _ = sender.send(Event::QuestionAnswered {
                        question_id: question.id.clone(),
                        answer: answer.clone(),
                    });
                }

                // Update local state
                if let Some(q) = self.questions.get_mut(selected) {
                    q.answer = Some(answer);
                    q.status = QuestionStatus::Answered;
                }

                self.questions_panel.in_answer_dialog = false;
                self.questions_panel.custom_input.clear();
                self.questions_panel.dialog_selection = 0;
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

    /// Handle TUI events (keyboard, mouse, and tick)
    pub fn handle_tui_event(&mut self, event: crate::tui::TuiEvent) {
        match event {
            crate::tui::TuiEvent::Key(key) => self.handle_key(key),
            crate::tui::TuiEvent::MouseScroll { delta } => self.handle_mouse_scroll(delta),
            crate::tui::TuiEvent::Tick => {
                // Advance spinner frame for animation
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
            }
            _ => {}
        }
    }

    /// Handle mouse scroll
    fn handle_mouse_scroll(&mut self, delta: i16) {
        // Don't scroll if in search mode or confirmation dialogs
        if self.search.is_active() || self.resume_confirm.is_active() || self.quit_confirm.pending {
            return;
        }

        // Don't scroll if clarification dialog is active
        if self.clarification.is_active() {
            return;
        }

        match self.current_tab {
            Tab::Logs => {
                if delta > 0 {
                    // Scroll up
                    self.logs_scroll = self.logs_scroll.saturating_sub(delta as u16);
                    self.logs_auto_follow = false;
                } else {
                    // Scroll down
                    let delta_abs = (-delta) as u16;
                    let entries = self.log_buffer.get_entries();
                    let max_scroll = entries.len() as u16;
                    self.logs_scroll = (self.logs_scroll + delta_abs).min(max_scroll);
                    // Only re-enable auto-follow if content exceeds assumed viewport height
                    // This prevents "jump to top" when content is short
                    if max_scroll > 15 && self.logs_scroll >= max_scroll.saturating_sub(5) {
                        self.logs_auto_follow = true;
                    }
                }
            }
            Tab::Tasks => {
                if delta > 0 {
                    self.tasks_scroll = self.tasks_scroll.saturating_sub(delta as usize);
                } else {
                    let delta_abs = (-delta) as usize;
                    let filtered = self.filtered_tasks();
                    let max_scroll = filtered.len().saturating_sub(1);
                    self.tasks_scroll = (self.tasks_scroll + delta_abs).min(max_scroll);
                }
            }
            Tab::Output => {
                if delta > 0 {
                    self.output_scroll = self.output_scroll.saturating_sub(delta as usize);
                    self.output_auto_follow = false;
                } else {
                    let delta_abs = (-delta) as usize;
                    let max_scroll = self.output_lines.len();
                    self.output_scroll = (self.output_scroll + delta_abs).min(max_scroll);
                    // Re-enable auto-follow if scrolled to bottom
                    if self.output_scroll >= max_scroll.saturating_sub(5) {
                        self.output_auto_follow = true;
                    }
                }
            }
            Tab::Questions => {
                if delta > 0 {
                    self.questions_scroll = self.questions_scroll.saturating_sub(delta as usize);
                    self.questions_panel.select_up(self.questions.len());
                } else {
                    let delta_abs = (-delta) as usize;
                    let max_scroll = self.questions.len().saturating_sub(1);
                    self.questions_scroll = (self.questions_scroll + delta_abs).min(max_scroll);
                    self.questions_panel.select_down(self.questions.len());
                }
            }
        }
    }

    /// Try to quit (with confirmation if tasks running)
    fn try_quit(&mut self) {
        let has_pending = self
            .tasks
            .iter()
            .any(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress);
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
                    self.clarification.scroll = 0; // Reset scroll for new question

                    // Check if all questions answered
                    if self.clarification.current_index >= self.clarification.questions.len() {
                        // Log summary of Q&A to log panel
                        self.log_clarification_summary();

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
                Key::PageUp => {
                    // Scroll up
                    self.clarification.scroll = self.clarification.scroll.saturating_sub(5);
                }
                Key::PageDown => {
                    // Scroll down
                    self.clarification.scroll = self.clarification.scroll.saturating_add(5);
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
                        let empty_answers: Vec<String> = std::iter::repeat(String::new())
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
            let answer = if let Some(q) = self
                .clarification
                .questions
                .get(self.clarification.current_index)
            {
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
            self.clarification.scroll = 0; // Reset scroll for new question

            // Check if all questions answered
            if self.clarification.current_index >= self.clarification.questions.len() {
                // Log summary of Q&A to log panel
                self.log_clarification_summary();

                if let Some(tx) = self.clarification.response_tx.take() {
                    let _ = tx.send(self.clarification.answers.clone());
                }
                self.clarification.finish();
            }
        }
    }

    /// Log summary of clarification Q&A to log panel
    fn log_clarification_summary(&mut self) {
        use crate::tui::LogLevel;

        let context = LogContext {
            phase: Some("clarification".to_string()),
            ..Default::default()
        };

        self.log_buffer
            .push(LogLevel::Info, "━━━ Clarification Summary ━━━".to_string(), context.clone());

        for (i, question) in self.clarification.questions.iter().enumerate() {
            let answer = self
                .clarification
                .answers
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("(skipped)");
            let truncated_q: String = question.question.chars().take(50).collect();
            let q_display = if truncated_q.len() < question.question.len() {
                format!("{}...", truncated_q)
            } else {
                question.question.clone()
            };
            self.log_buffer
                .push(LogLevel::Info, format!("Q{}: {}", i + 1, q_display), context.clone());
            self.log_buffer
                .push(LogLevel::Info, format!("  → {}", answer), context.clone());
        }

        self.log_buffer
            .push(LogLevel::Info, "━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string(), context);

        // Auto-scroll logs to show summary
        if self.logs_auto_follow {
            self.logs_scroll = u16::MAX;
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
            Tab::Questions => {
                self.questions_scroll = 0;
                self.questions_panel.state.select(Some(0));
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
            Tab::Questions => {
                if self.questions_scroll > 0 {
                    self.questions_scroll -= 1;
                    self.questions_panel.select_up(self.questions.len());
                }
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
                let max_scroll = self.output_lines.len();
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
                // Only re-enable auto-follow if content exceeds assumed viewport height
                // This prevents "jump to top" when content is short
                if max_scroll > 15 && self.logs_scroll >= max_scroll.saturating_sub(5) {
                    self.logs_auto_follow = true;
                }
            }
            Tab::Questions => {
                if self.questions_scroll < self.questions.len().saturating_sub(1) {
                    self.questions_scroll += 1;
                    self.questions_panel.select_down(self.questions.len());
                }
            }
        }
    }

    /// Process an orchestrator event
    pub fn process_event(&mut self, event: Event) {
        match event {
            Event::TaskCreated {
                id,
                title,
                parent_id,
                depth,
                depends_on,
            } => {
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

                self.completed_count = self
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Completed)
                    .count();
                self.failed_count = self
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Failed)
                    .count();

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
                // Only show thinking in verbose mode
                if self.verbosity >= VerbosityLevel::Verbose {
                    let seq = self.output_seq_counter;
                    self.output_seq_counter += 1;
                    let line = OutputLine::Thinking {
                        task_id: task_id.clone(),
                        content,
                        seq,
                    };
                    self.add_output_line(task_id, line);
                }
            }
            Event::ClaudeToolUse {
                task_id,
                tool_name,
                tool_input,
            } => {
                if self.verbosity >= VerbosityLevel::Normal {
                    let seq = self.output_seq_counter;
                    self.output_seq_counter += 1;
                    let line = OutputLine::ToolUse {
                        task_id: task_id.clone(),
                        tool_name,
                        tool_input,
                        seq,
                    };
                    self.add_output_line(task_id, line);
                }
            }
            Event::ClaudeToolResult {
                task_id,
                tool_name,
                result,
                success,
            } => {
                if self.verbosity >= VerbosityLevel::Normal {
                    let seq = self.output_seq_counter;
                    self.output_seq_counter += 1;
                    let line = OutputLine::ToolResult {
                        task_id: task_id.clone(),
                        tool_name,
                        result,
                        success,
                        seq,
                    };
                    self.add_output_line(task_id, line);
                }
            }
            Event::ClaudeRequest { task_id: _, prompt: _, model: _, timeout_secs: _ } => {
                // Request logs removed - only show Claude responses, not requests
                // This keeps the output panel clean and focused on actual output
            }
            Event::ClaudeResult { task_id, result } => {
                // Show Claude response in normal mode (not just verbose)
                let header = "═══════════════════════════════════════\n\
                              ═══ Claude Response ═══\n\
                              ═══════════════════════════════════════";
                let seq = self.output_seq_counter;
                self.output_seq_counter += 1;
                let line = OutputLine::Result {
                    task_id: task_id.clone(),
                    content: format!("{}\n\n{}", header, result),
                    seq,
                };
                self.add_output_line(task_id, line);
            }
            Event::Log {
                timestamp,
                level,
                message,
                task_id,
                task_title,
                phase,
            } => {
                let context = LogContext {
                    task_id,
                    task_title,
                    phase,
                };
                self.log_buffer.push(level, message, context);
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
                // Extract activity if Running
                if let ExecutionState::Running { activity } = state {
                    self.current_activity = Some(activity);
                    self.last_pulse_time = Some(Instant::now());
                }
            }
            Event::ProgressUpdate {
                completed,
                total,
                failed,
                elapsed,
            } => {
                self.completed_count = completed;
                self.total_count = total;
                self.failed_count = failed;
                let _ = elapsed;
            }
            Event::ModelChanged { model } => {
                self.current_model = model;
            }
            Event::ClarificationQuestions {
                questions,
                response_tx,
            } => {
                self.clarification.questions = questions;
                self.clarification.answers = Vec::new();
                self.clarification.current_index = 0;
                self.clarification.selected_option = 0;
                self.clarification.custom_input.clear();
                self.clarification.is_custom_input = false;
                self.clarification.response_tx = Some(response_tx);
            }
            Event::ResumeConfirm {
                completed,
                pending,
                failed,
                response_tx,
            } => {
                self.resume_confirm.completed = completed;
                self.resume_confirm.pending = pending;
                self.resume_confirm.failed = failed;
                self.resume_confirm.selected = true;
                self.resume_confirm.response_tx = Some(response_tx);
            }
            Event::ClarificationTask {
                task_id,
                title,
                description,
                response_tx,
            } => {
                self.clarification_task.task_id = Some(task_id);
                self.clarification_task.title = title;
                self.clarification_task.description = description;
                self.clarification_task.response_tx = Some(response_tx);
            }
            Event::TokenUsageUpdate {
                task_id,
                tokens_used,
            } => {
                // Check if this is a new task
                if self.current_task_id.as_ref() != Some(&task_id) {
                    // Reset current task tokens for new task
                    self.current_task_tokens = 0;
                    self.current_task_id = Some(task_id);
                }
                // Update current task tokens
                self.current_task_tokens += tokens_used;
                // Update total tokens
                self.total_tokens += tokens_used;
            }
            Event::ActivityPulse { task_id, activity } => {
                self.last_pulse_time = Some(Instant::now());
                self.current_activity = Some(activity);
                // Keep current_task_id in sync
                if self.current_task_id.is_none() {
                    self.current_task_id = Some(task_id);
                }
            }
            Event::ProgressReview { report } => {
                // Log progress review to logs panel
                for line in report.format().lines() {
                    self.log_buffer.push(
                        LogLevel::Info,
                        line.to_string(),
                        LogContext::default(),
                    );
                }
            }
            Event::AgentQuestion { task_id, question, response_tx } => {
                // Add question to our tracking list if not already present
                let question_found = self.questions.iter().any(|q| q.id == question.id);
                if !question_found {
                    self.questions.push(question.clone());
                }
                // Store question for TUI display and response handling
                self.agent_question.task_id = Some(task_id);
                self.agent_question.question = Some(question);
                self.agent_question.response_tx = Some(response_tx);
            }
            Event::QuestionAnswered { question_id, answer } => {
                // Log that a question was answered
                self.log_buffer.push(
                    LogLevel::Info,
                    format!("Question {} answered: {}", question_id, answer),
                    LogContext::default(),
                );
                // Update the question in our list
                if let Some(q) = self.questions.iter_mut().find(|q| q.id == question_id) {
                    use crate::models::QuestionStatus;
                    q.status = QuestionStatus::Answered;
                    q.answer = Some(answer);
                    q.answered_at = Some(chrono::Utc::now());
                }
            }
            Event::QuestionAutoDecided { question_id, decision, reason } => {
                // Log that an agent auto-decided on a non-blocking question
                self.log_buffer.push(
                    LogLevel::Info,
                    format!("Question {} auto-decided: {} ({})", question_id, decision, reason),
                    LogContext::default(),
                );
                // Update the question in our list
                if let Some(q) = self.questions.iter_mut().find(|q| q.id == question_id) {
                    use crate::models::QuestionStatus;
                    q.status = QuestionStatus::AutoDecided;
                    q.answer = Some(decision);
                    q.decision_log = Some(reason);
                    q.answered_at = Some(chrono::Utc::now());
                }
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
        assert_eq!(Tab::Output.next(), Tab::Questions);
        assert_eq!(Tab::Questions.next(), Tab::Logs);

        assert_eq!(Tab::Logs.prev(), Tab::Questions);
        assert_eq!(Tab::Tasks.prev(), Tab::Logs);
        assert_eq!(Tab::Output.prev(), Tab::Tasks);
        assert_eq!(Tab::Questions.prev(), Tab::Output);
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
