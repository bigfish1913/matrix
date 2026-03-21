# Agent Interactive Q&A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement interactive Q&A system where agents can ask questions during task execution, with blocking questions pausing tasks and non-blocking questions allowing autonomous decisions.

**Architecture:** Extend Event system with AgentQuestion event, add QuestionStore for JSON persistence, create Questions Tab in TUI reusing Clarification dialog components.

**Tech Stack:** Rust, Tokio, Ratatui, JSON storage

---

## File Structure

```
crates/core/src/
├── models/
│   ├── mod.rs           # Modify: export Question
│   └── question.rs      # Create: Question + QuestionStatus
├── store/
│   ├── mod.rs           # Modify: export QuestionStore
│   └── question_store.rs # Create: QuestionStore impl
├── tui/
│   ├── event.rs         # Modify: add question events + QuestionSender
│   ├── app.rs           # Modify: add Questions tab state
│   ├── render.rs        # Modify: add Questions tab rendering
│   ├── mod.rs           # Modify: export new types
│   └── components/
│       ├── mod.rs       # Modify: export QuestionsPanel
│       └── questions.rs # Create: QuestionsPanel component
└── executor/
    └── task_executor.rs # Modify: add ask_question method
```

---

## Task 1: Question Model

**Files:**
- Create: `crates/core/src/models/question.rs`
- Modify: `crates/core/src/models/mod.rs`

### Step 1.1: Create question.rs with imports

```rust
//! Question model for agent interactive Q&A.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static QUESTION_COUNTER: AtomicU64 = AtomicU64::new(1);
```

- [ ] **Create file** `crates/core/src/models/question.rs` with imports

### Step 1.2: Add QuestionStatus enum

```rust
/// Question status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    #[default]
    Pending,      // Waiting for answer
    Answered,     // User answered
    AutoDecided,  // Agent decided (non-blocking)
    Expired,      // Cancelled/outdated
}

impl std::fmt::Display for QuestionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Answered => write!(f, "answered"),
            Self::AutoDecided => write!(f, "auto-decided"),
            Self::Expired => write!(f, "expired"),
        }
    }
}
```

- [ ] **Add QuestionStatus enum** to question.rs

### Step 1.3: Add Question struct

```rust
/// Question model for agent interactive Q&A
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// Question ID, e.g. "q-001"
    pub id: String,
    /// Associated task ID
    pub task_id: String,
    /// Question text
    pub question: String,
    /// Multiple choice options
    pub options: Vec<String>,
    /// Pros for each option (parallel to options)
    #[serde(default)]
    pub pros: Vec<String>,
    /// Cons for each option (parallel to options)
    #[serde(default)]
    pub cons: Vec<String>,
    /// Agent's recommended option index
    pub recommended: Option<usize>,
    /// Reason for recommendation
    pub recommendation_reason: Option<String>,
    /// Whether this blocks task execution
    pub blocking: bool,
    /// Current status
    #[serde(default)]
    pub status: QuestionStatus,
    /// User's answer (option text or custom input)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// Decision log (for auto-decided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_log: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Answer timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<DateTime<Utc>>,
}
```

- [ ] **Add Question struct** to question.rs

### Step 1.4: Add helper methods

```rust
impl Question {
    /// Create a new question
    pub fn new(
        task_id: String,
        question: String,
        options: Vec<String>,
        pros: Vec<String>,
        cons: Vec<String>,
        recommended: Option<usize>,
        recommendation_reason: Option<String>,
        blocking: bool,
    ) -> Self {
        let id = Self::generate_id();
        Self {
            id,
            task_id,
            question,
            options,
            pros,
            cons,
            recommended,
            recommendation_reason,
            blocking,
            status: QuestionStatus::default(),
            answer: None,
            decision_log: None,
            created_at: Utc::now(),
            answered_at: None,
        }
    }

    /// Generate unique question ID
    fn generate_id() -> String {
        let num = QUESTION_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("q-{:03}", num)
    }

    /// Check if question is still pending
    pub fn is_pending(&self) -> bool {
        self.status == QuestionStatus::Pending
    }

    /// Mark as answered
    pub fn answer(&mut self, answer: String) {
        self.status = QuestionStatus::Answered;
        self.answer = Some(answer);
        self.answered_at = Some(Utc::now());
    }

    /// Mark as auto-decided
    pub fn auto_decide(&mut self, decision: String, reason: String) {
        self.status = QuestionStatus::AutoDecided;
        self.answer = Some(decision);
        self.decision_log = Some(reason);
        self.answered_at = Some(Utc::now());
    }
}
```

- [ ] **Add helper methods** to Question impl

### Step 1.5: Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_question_new() {
        let q = Question::new(
            "task-001".to_string(),
            "Which DB?".to_string(),
            vec!["SQLite".to_string(), "PostgreSQL".to_string()],
            vec!["Lightweight".to_string(), "Powerful".to_string()],
            vec!["Limited".to_string(), "Complex".to_string()],
            Some(0),
            Some("Simplest option".to_string()),
            true,
        );
        assert!(q.id.starts_with("q-"));
        assert_eq!(q.task_id, "task-001");
        assert_eq!(q.status, QuestionStatus::Pending);
        assert!(q.blocking);
    }

    #[test]
    fn test_question_answer() {
        let mut q = Question::new(
            "task-001".to_string(),
            "Test?".to_string(),
            vec!["A".to_string()],
            vec![],
            vec![],
            None,
            None,
            true,
        );
        q.answer("A".to_string());
        assert_eq!(q.status, QuestionStatus::Answered);
        assert_eq!(q.answer, Some("A".to_string()));
        assert!(q.answered_at.is_some());
    }

    #[test]
    fn test_question_auto_decide() {
        let mut q = Question::new(
            "task-001".to_string(),
            "Test?".to_string(),
            vec!["A".to_string()],
            vec![],
            vec![],
            Some(0),
            Some("Recommended".to_string()),
            false,
        );
        q.auto_decide("A".to_string(), "Because recommended".to_string());
        assert_eq!(q.status, QuestionStatus::AutoDecided);
        assert!(q.decision_log.is_some());
    }
}
```

- [ ] **Add unit tests** to question.rs

### Step 1.6: Export from mod.rs

In `crates/core/src/models/mod.rs`, add:

```rust
mod question;
pub use question::{Question, QuestionStatus};
```

- [ ] **Add export** to models/mod.rs

### Step 1.7: Run tests

Run: `cargo test -p matrix-core question`

Expected: All tests pass

- [ ] **Run tests** and verify they pass

### Step 1.8: Commit

```bash
git add crates/core/src/models/question.rs crates/core/src/models/mod.rs
git commit -m "feat(models): add Question model for agent interactive Q&A"
```

- [ ] **Commit** Question model

---

## Task 2: QuestionStore

**Files:**
- Create: `crates/core/src/store/question_store.rs`
- Modify: `crates/core/src/store/mod.rs`

### Step 2.1: Create question_store.rs with imports

```rust
//! QuestionStore - persistent storage for questions.

use crate::error::{Error, Result};
use crate::models::{Question, QuestionStatus};
use chrono::Utc;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// Questions file structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct QuestionsFile {
    questions: Vec<Question>,
}
```

- [ ] **Create file** `crates/core/src/store/question_store.rs` with imports

### Step 2.2: Add QuestionStore struct

```rust
/// Question storage manager
pub struct QuestionStore {
    questions_path: PathBuf,
    decisions_path: PathBuf,
}
```

- [ ] **Add QuestionStore struct**

### Step 2.3: Implement new() constructor

```rust
impl QuestionStore {
    /// Create a new QuestionStore
    pub async fn new(questions_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&questions_dir).await?;
        let questions_path = questions_dir.join("questions.json");
        let decisions_path = questions_dir.join("decisions.log");

        let store = Self {
            questions_path,
            decisions_path,
        };

        // Initialize questions.json if not exists
        if !store.questions_path.exists() {
            store.save_questions(&QuestionsFile::default()).await?;
        }

        Ok(store)
    }
}
```

- [ ] **Implement new() constructor**

### Step 2.4: Implement load/save helpers

```rust
impl QuestionStore {
    /// Load questions from file
    async fn load_questions(&self) -> Result<QuestionsFile> {
        if !self.questions_path.exists() {
            return Ok(QuestionsFile::default());
        }
        let content = fs::read_to_string(&self.questions_path).await?;
        let file: QuestionsFile = serde_json::from_str(&content)?;
        Ok(file)
    }

    /// Save questions to file
    async fn save_questions(&self, file: &QuestionsFile) -> Result<()> {
        let content = serde_json::to_string_pretty(file)?;
        fs::write(&self.questions_path, content).await?;
        debug!(path = %self.questions_path.display(), "Questions saved");
        Ok(())
    }
}
```

- [ ] **Implement load/save helpers**

### Step 2.5: Implement create() method

```rust
impl QuestionStore {
    /// Create a new question
    pub async fn create(&self, question: &Question) -> Result<()> {
        let mut file = self.load_questions().await?;
        file.questions.push(question.clone());
        self.save_questions(&file).await?;
        debug!(question_id = %question.id, "Question created");
        Ok(())
    }
}
```

- [ ] **Implement create() method**

### Step 2.6: Implement query methods

```rust
impl QuestionStore {
    /// Get all questions
    pub async fn all_questions(&self) -> Result<Vec<Question>> {
        let file = self.load_questions().await?;
        Ok(file.questions)
    }

    /// Get all pending questions
    pub async fn pending_questions(&self) -> Result<Vec<Question>> {
        let questions = self.all_questions().await?;
        Ok(questions
            .into_iter()
            .filter(|q| q.status == QuestionStatus::Pending)
            .collect())
    }

    /// Get pending count
    pub async fn pending_count(&self) -> Result<usize> {
        let pending = self.pending_questions().await?;
        Ok(pending.len())
    }

    /// Get questions for a specific task
    pub async fn questions_for_task(&self, task_id: &str) -> Result<Vec<Question>> {
        let questions = self.all_questions().await?;
        Ok(questions
            .into_iter()
            .filter(|q| q.task_id == task_id)
            .collect())
    }

    /// Get question by ID
    pub async fn get(&self, id: &str) -> Result<Option<Question>> {
        let questions = self.all_questions().await?;
        Ok(questions.into_iter().find(|q| q.id == id))
    }
}
```

- [ ] **Implement query methods**

### Step 2.7: Implement answer() method

```rust
impl QuestionStore {
    /// Answer a question
    pub async fn answer(&self, id: &str, answer: &str) -> Result<()> {
        let mut file = self.load_questions().await?;

        if let Some(question) = file.questions.iter_mut().find(|q| q.id == id) {
            question.answer(answer.to_string());
            self.save_questions(&file).await?;
            debug!(question_id = %id, answer = %answer, "Question answered");
            Ok(())
        } else {
            Err(Error::TaskNotFound(format!("Question not found: {}", id)))
        }
    }
}
```

- [ ] **Implement answer() method**

### Step 2.8: Implement record_auto_decision() method

```rust
impl QuestionStore {
    /// Record auto-decision for non-blocking question
    pub async fn record_auto_decision(
        &self,
        id: &str,
        decision: &str,
        reason: &str,
    ) -> Result<()> {
        let mut file = self.load_questions().await?;

        if let Some(question) = file.questions.iter_mut().find(|q| q.id == id) {
            question.auto_decide(decision.to_string(), reason.to_string());
            self.save_questions(&file).await?;

            // Also append to decision log
            self.append_decision_log(question, decision, reason).await?;

            debug!(question_id = %id, decision = %decision, "Auto-decision recorded");
            Ok(())
        } else {
            Err(Error::TaskNotFound(format!("Question not found: {}", id)))
        }
    }
}
```

- [ ] **Implement record_auto_decision() method**

### Step 2.9: Implement append_decision_log() method

```rust
impl QuestionStore {
    /// Append decision to log file
    async fn append_decision_log(
        &self,
        question: &Question,
        decision: &str,
        reason: &str,
    ) -> Result<()> {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
        let entry = format!(
            "[{}] {} | {}\n  Decision: {}\n  Reason: {}\n\n",
            timestamp, question.task_id, question.question, decision, reason
        );

        // Create file if not exists, append otherwise
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.decisions_path)
            .await?
            .write_all(entry.as_bytes())
            .await?;

        debug!(path = %self.decisions_path.display(), "Decision log appended");
        Ok(())
    }
}
```

- [ ] **Implement append_decision_log() method**

### Step 2.10: Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_question_store_create_and_get() {
        let dir = tempdir().unwrap();
        let store = QuestionStore::new(dir.path().to_path_buf()).await.unwrap();

        let q = Question::new(
            "task-001".to_string(),
            "Test?".to_string(),
            vec!["A".to_string()],
            vec![],
            vec![],
            None,
            None,
            true,
        );

        store.create(&q).await.unwrap();
        let loaded = store.get(&q.id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().question, "Test?");
    }

    #[tokio::test]
    async fn test_question_store_answer() {
        let dir = tempdir().unwrap();
        let store = QuestionStore::new(dir.path().to_path_buf()).await.unwrap();

        let q = Question::new(
            "task-001".to_string(),
            "Test?".to_string(),
            vec!["A".to_string(), "B".to_string()],
            vec![],
            vec![],
            None,
            None,
            true,
        );

        store.create(&q).await.unwrap();
        store.answer(&q.id, "A").await.unwrap();

        let loaded = store.get(&q.id).await.unwrap().unwrap();
        assert_eq!(loaded.status, QuestionStatus::Answered);
        assert_eq!(loaded.answer, Some("A".to_string()));
    }

    #[tokio::test]
    async fn test_question_store_pending_count() {
        let dir = tempdir().unwrap();
        let store = QuestionStore::new(dir.path().to_path_buf()).await.unwrap();

        let q1 = Question::new(
            "task-001".to_string(),
            "Q1?".to_string(),
            vec!["A".to_string()],
            vec![],
            vec![],
            None,
            None,
            true,
        );
        let q2 = Question::new(
            "task-002".to_string(),
            "Q2?".to_string(),
            vec!["B".to_string()],
            vec![],
            vec![],
            None,
            None,
            false,
        );

        store.create(&q1).await.unwrap();
        store.create(&q2).await.unwrap();

        let count = store.pending_count().await.unwrap();
        assert_eq!(count, 2);

        store.answer(&q1.id, "A").await.unwrap();

        let count = store.pending_count().await.unwrap();
        assert_eq!(count, 1);
    }
}
```

- [ ] **Add unit tests**

### Step 2.11: Export from mod.rs

In `crates/core/src/store/mod.rs`, add:

```rust
mod question_store;
pub use question_store::QuestionStore;
```

- [ ] **Add export** to store/mod.rs

### Step 2.12: Run tests

Run: `cargo test -p matrix-core question_store`

Expected: All tests pass

- [ ] **Run tests** and verify they pass

### Step 2.13: Commit

```bash
git add crates/core/src/store/question_store.rs crates/core/src/store/mod.rs
git commit -m "feat(store): add QuestionStore for persistent question storage"
```

- [ ] **Commit** QuestionStore

---

## Task 3: Event Types

**Files:**
- Modify: `crates/core/src/tui/event.rs`

### Step 3.1: Add QuestionSender wrapper

After existing sender types (around line 170), add:

```rust
/// Wrapper for oneshot sender for question responses
#[derive(Debug)]
pub struct QuestionSender(Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>);

impl QuestionSender {
    pub fn new(sender: tokio::sync::oneshot::Sender<String>) -> Self {
        Self(Arc::new(Mutex::new(Some(sender))))
    }

    pub fn send(self, answer: String) -> Result<(), String> {
        if let Some(sender) = self.0.lock().unwrap().take() {
            sender.send(answer)
        } else {
            Err(answer)
        }
    }
}

impl Clone for QuestionSender {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
```

- [ ] **Add QuestionSender wrapper** after existing sender types

### Step 3.2: Add question events to Event enum

In the Event enum, add new variants (around line 270):

```rust
pub enum Event {
    // ... existing events ...

    // Question events
    /// Agent asks a question
    AgentQuestion {
        task_id: String,
        question: Box<crate::models::Question>,
        response_tx: Option<QuestionSender>,
    },

    /// User answered a question
    QuestionAnswered {
        question_id: String,
        answer: String,
    },

    /// Agent auto-decided on non-blocking question
    QuestionAutoDecided {
        question_id: String,
        decision: String,
        reason: String,
    },

    /// Questions updated (for TUI refresh)
    QuestionsUpdated,
}
```

- [ ] **Add question events** to Event enum

### Step 3.3: Run build to check for errors

Run: `cargo build -p matrix-core`

Expected: May have unused import warnings, but no errors

- [ ] **Run build** and fix any errors

### Step 3.4: Commit

```bash
git add crates/core/src/tui/event.rs
git commit -m "feat(events): add AgentQuestion and related event types"
```

- [ ] **Commit** event types

---

## Task 4: TUI Questions Tab

**Files:**
- Create: `crates/core/src/tui/components/questions.rs`
- Modify: `crates/core/src/tui/components/mod.rs`
- Modify: `crates/core/src/tui/app.rs`
- Modify: `crates/core/src/tui/render.rs`

### Step 4.1: Create QuestionsPanel component

Create `crates/core/src/tui/components/questions.rs`:

```rust
//! Questions panel component.

use crate::models::{Question, QuestionStatus};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

/// Questions panel component
pub struct QuestionsPanel;

impl QuestionsPanel {
    /// Render questions list
    pub fn render(questions: &[Question], selected: usize) -> (List<'static>, ListState) {
        let mut state = ListState::default();
        state.select(Some(selected));

        let pending: Vec<_> = questions
            .iter()
            .filter(|q| q.status == QuestionStatus::Pending)
            .collect();

        let answered: Vec<_> = questions
            .iter()
            .filter(|q| q.status != QuestionStatus::Pending)
            .collect();

        let mut items: Vec<ListItem> = Vec::new();

        // Pending section
        if !pending.is_empty() {
            items.push(ListItem::new(Line::styled(
                "━━━ Pending Questions ━━━",
                Style::default().fg(Color::Yellow),
            )));
            items.push(ListItem::new(""));

            for q in &pending {
                let status_icon = if q.blocking { "⏳" } else { "○" };
                let status_text = if q.blocking {
                    "blocking"
                } else {
                    "non-blocking"
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} [{}] ", status_icon, q.task_id),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(&q.question, Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(status_text, Style::default().fg(Color::DarkGray)),
                ]);
                items.push(ListItem::new(line));
            }
        }

        // Answered section
        if !answered.is_empty() {
            items.push(ListItem::new(""));
            items.push(ListItem::new(Line::styled(
                "━━━ Answered ━━━",
                Style::default().fg(Color::Green),
            )));
            items.push(ListItem::new(""));

            for q in &answered {
                let status_icon = match q.status {
                    QuestionStatus::Answered => "✓",
                    QuestionStatus::AutoDecided => "◆",
                    _ => "○",
                };

                let answer_text = q.answer.as_deref().unwrap_or("N/A");

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} [{}] ", status_icon, q.task_id),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(&q.question, Style::default().fg(Color::DarkGray)),
                    Span::raw(": "),
                    Span::styled(answer_text, Style::default().fg(Color::White)),
                ]);
                items.push(ListItem::new(line));
            }
        }

        if items.is_empty() {
            items.push(ListItem::new(Line::styled(
                "  No questions yet",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Questions ")
                    .borders(Borders::ALL),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        (list, state)
    }
}
```

- [ ] **Create QuestionsPanel component**

### Step 4.2: Export from components/mod.rs

In `crates/core/src/tui/components/mod.rs`, add:

```rust
mod questions;
pub use questions::QuestionsPanel;
```

- [ ] **Export QuestionsPanel** from mod.rs

### Step 4.3: Add Questions tab to Tab enum in app.rs

In `crates/core/src/tui/app.rs`, modify Tab enum:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Logs,
    Tasks,
    Output,
    Questions,  // New
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
}
```

- [ ] **Add Questions variant** to Tab enum

### Step 4.4: Add question state to TuiApp

In `crates/core/src/tui/app.rs`, add fields to TuiApp struct:

```rust
pub struct TuiApp {
    // ... existing fields ...

    // Questions
    pub questions: Vec<crate::models::Question>,
    pub questions_scroll: usize,
    pub question_answer_mode: bool,  // When true, showing answer dialog
}
```

And initialize in `TuiApp::new()`:

```rust
Self {
    // ... existing fields ...
    questions: Vec::new(),
    questions_scroll: 0,
    question_answer_mode: false,
}
```

- [ ] **Add question fields** to TuiApp

### Step 4.5: Add keyboard handling for Questions tab

In `TuiApp::handle_key()`, add handling for Questions tab:

```rust
Key::Char('4') | Key::Char('Q') => {
    self.current_tab = Tab::Questions;
    self.reset_scroll();
}
```

And in the tab-specific section:

```rust
Tab::Questions => {
    match key {
        Key::Up => {
            if self.questions_scroll > 0 {
                self.questions_scroll -= 1;
            }
        }
        Key::Down => {
            let pending = self.questions.iter()
                .filter(|q| q.status == QuestionStatus::Pending)
                .count();
            if self.questions_scroll < pending.saturating_sub(1) {
                self.questions_scroll += 1;
            }
        }
        Key::Enter => {
            // Enter answer mode
            self.question_answer_mode = true;
        }
        Key::Esc => {
            self.question_answer_mode = false;
        }
        _ => {}
    }
}
```

- [ ] **Add keyboard handling** for Questions tab

### Step 4.6: Add Question event handling in process_event

In `TuiApp::process_event()`, add handlers:

```rust
Event::AgentQuestion { task_id, question, .. } => {
    self.questions.push(*question);
    // Sort: pending first, then by created_at
    self.questions.sort_by(|a, b| {
        let a_pending = a.status == QuestionStatus::Pending;
        let b_pending = b.status == QuestionStatus::Pending;
        b_pending.cmp(&a_pending).then(a.created_at.cmp(&b.created_at))
    });
}
Event::QuestionAnswered { question_id, answer } => {
    if let Some(q) = self.questions.iter_mut().find(|q| q.id == *question_id) {
        q.answer(answer.clone());
    }
}
Event::QuestionAutoDecided { question_id, decision, reason } => {
    if let Some(q) = self.questions.iter_mut().find(|q| q.id == *question_id) {
        q.auto_decide(decision.clone(), reason.clone());
    }
}
Event::QuestionsUpdated => {
    // Signal to reload questions from store
}
```

- [ ] **Add question event handlers** in process_event

### Step 4.7: Add Questions tab rendering in render.rs

In `render_app()`, add the Questions tab case:

```rust
Tab::Questions => {
    let (list, state) = QuestionsPanel::render(&app.questions, app.questions_scroll);
    frame.render_stateful_widget(list, chunks[1], &mut state.clone());

    // Render answer dialog if in answer mode
    if app.question_answer_mode {
        render_answer_dialog(frame, app);
    }
}
```

- [ ] **Add Questions tab rendering**

### Step 4.8: Implement answer dialog (reuse Clarification style)

Add helper function in render.rs:

```rust
fn render_answer_dialog(frame: &mut Frame, app: &TuiApp) {
    use crate::models::QuestionStatus;

    let pending: Vec<_> = app.questions
        .iter()
        .filter(|q| q.status == QuestionStatus::Pending)
        .collect();

    if pending.is_empty() {
        return;
    }

    let question = pending.get(app.questions_scroll).unwrap_or(&pending[0]);

    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();
    let width = area.width.saturating_sub(4) as usize;

    // Header
    lines.push(Line::from(vec![Span::styled(
        " Answer Question ",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Question
    lines.push(Line::from(vec![
        Span::styled("▶ ", Style::default().fg(Color::Yellow)),
        Span::styled(&question.question, Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(""));

    // Options
    for (idx, option) in question.options.iter().enumerate() {
        let is_recommended = question.recommended == Some(idx);
        let style = if is_recommended {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let pro = question.pros.get(idx).map(|s| s.as_str()).unwrap_or("");
        let con = question.cons.get(idx).map(|s| s.as_str()).unwrap_or("");

        let info = if !pro.is_empty() || !con.is_empty() {
            format!(" +{} -{}", pro, con)
        } else {
            String::new()
        };

        lines.push(Line::from(vec![
            Span::styled(format!("  {}. ", idx + 1), Style::default().fg(Color::Cyan)),
            Span::styled(option.clone(), style),
            Span::styled(info, Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Recommendation
    if let Some(reason) = &question.recommendation_reason {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Tip: ", Style::default().fg(Color::DarkGray)),
            Span::styled(reason.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Help
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(50.min(area.width as usize - 4)),
        Style::default().fg(Color::DarkGray),
    )]));
    lines.push(Line::from(vec![
        Span::styled(" 1-9 ", Style::default().fg(Color::Yellow)),
        Span::styled("select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter ", Style::default().fg(Color::Yellow)),
        Span::styled("confirm  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc ", Style::default().fg(Color::Yellow)),
        Span::styled("cancel", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Answer ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(paragraph, area);
}
```

- [ ] **Implement answer dialog** rendering

### Step 4.9: Add pending count to status bar

In `render_app()`, modify status bar to show pending questions:

```rust
// In status bar section, add pending questions indicator
let pending_questions = app.questions.iter()
    .filter(|q| q.status == QuestionStatus::Pending)
    .count();

// Add to status line before help text
if pending_questions > 0 {
    // Show count in status
}
```

- [ ] **Add pending count** to status bar

### Step 4.10: Update TabSwitcher to show Questions

Modify `TabSwitcher::render()` in components/tab_switcher.rs to include Questions tab:

```rust
// Add "Questions" to tab list with count badge if pending > 0
```

- [ ] **Update TabSwitcher** to show Questions tab

### Step 4.11: Run build and test

Run: `cargo build -p matrix-core`

Expected: Compiles successfully

- [ ] **Run build** and fix any errors

### Step 4.12: Commit

```bash
git add crates/core/src/tui/components/questions.rs
git add crates/core/src/tui/components/mod.rs
git add crates/core/src/tui/app.rs
git add crates/core/src/tui/render.rs
git commit -m "feat(tui): add Questions tab with answer dialog"
```

- [ ] **Commit** TUI changes

---

## Task 5: Orchestrator Integration

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

### Step 5.1: Add QuestionStore to Orchestrator

Add field to Orchestrator struct:

```rust
pub struct Orchestrator {
    // ... existing fields ...
    question_store: Arc<QuestionStore>,
}
```

- [ ] **Add QuestionStore field** to Orchestrator

### Step 5.2: Initialize QuestionStore in new()

In `Orchestrator::new()`:

```rust
let question_store = Arc::new(
    QuestionStore::new(config.tasks_dir.clone()).await?
);

Ok(Self {
    // ... existing fields ...
    question_store,
})
```

- [ ] **Initialize QuestionStore** in constructor

### Step 5.3: Handle AgentQuestion event

In the orchestrator, when receiving AgentQuestion event:

```rust
// Store question and notify TUI
self.question_store.create(&question).await?;
self.emit_event(Event::QuestionsUpdated);
```

- [ ] **Handle AgentQuestion** event

### Step 5.4: Handle QuestionAnswered event

When user answers a question:

```rust
self.question_store.answer(&question_id, &answer).await?;
self.emit_event(Event::QuestionAnswered {
    question_id,
    answer,
});
```

- [ ] **Handle QuestionAnswered** event

### Step 5.5: Handle QuestionAutoDecided event

For non-blocking auto-decisions:

```rust
self.question_store
    .record_auto_decision(&question_id, &decision, &reason)
    .await?;
self.emit_event(Event::QuestionAutoDecided {
    question_id,
    decision,
    reason,
});
```

- [ ] **Handle QuestionAutoDecided** event

### Step 5.6: Load pending questions on startup

In `run()` method, after loading tasks:

```rust
// Load any pending questions from previous run
let pending = self.question_store.pending_questions().await?;
for q in pending {
    self.emit_event(Event::AgentQuestion {
        task_id: q.task_id.clone(),
        question: Box::new(q),
        response_tx: None,
    });
}
```

- [ ] **Load pending questions** on startup

### Step 5.7: Run build and test

Run: `cargo build -p matrix-core`

Expected: Compiles successfully

- [ ] **Run build** and fix any errors

### Step 5.8: Commit

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "feat(orchestrator): integrate QuestionStore for question handling"
```

- [ ] **Commit** orchestrator changes

---

## Task 6: CLI Integration

**Files:**
- Modify: `crates/cli/src/main.rs`

### Step 6.1: Add question handling in TUI loop

In `run_tui_loop()`, handle question events:

```rust
// After existing event handling
if let Some(ref question_store) = app.question_store {
    let pending = question_store.pending_questions().await?;
    // Update app.questions if changed
}
```

- [ ] **Add question handling** in TUI loop

### Step 6.2: Wire up answer submission

When user answers a question in TUI:

```rust
// Send answer through event channel
if let Some(ref sender) = app.config.event_sender {
    let _ = sender.send(Event::QuestionAnswered {
        question_id: question.id.clone(),
        answer: selected_answer,
    });
}
```

- [ ] **Wire up answer submission**

### Step 6.3: Run build and test

Run: `cargo build -p matrix-cli`

Expected: Compiles successfully

- [ ] **Run build** and fix any errors

### Step 6.4: Commit

```bash
git add crates/cli/src/main.rs
git commit -m "feat(cli): integrate question handling in TUI"
```

- [ ] **Commit** CLI changes

---

## Task 7: End-to-End Testing

### Step 7.1: Manual test - Question creation

1. Start matrix with a goal that will trigger questions
2. Verify Questions tab appears with correct count
3. Verify question details are displayed correctly

- [ ] **Test question creation** manually

### Step 7.2: Manual test - Answer flow

1. Navigate to Questions tab
2. Select a pending question
3. Press Enter to open answer dialog
4. Select an option and confirm
5. Verify question moves to Answered section

- [ ] **Test answer flow** manually

### Step 7.3: Manual test - Decision log

1. Trigger a non-blocking question
2. Verify auto-decision is recorded
3. Check `.matrix/decisions.log` for entry

- [ ] **Test decision log** manually

### Step 7.4: Manual test - Persistence

1. Create pending questions
2. Quit and restart matrix with --resume
3. Verify questions are still pending

- [ ] **Test persistence** manually

### Step 7.5: Final commit

```bash
git add -A
git commit -m "feat: complete agent interactive Q&A system"
```

- [ ] **Final commit**

---

## Summary

| Task | Files | Estimated Time |
|------|-------|----------------|
| 1. Question Model | 2 files | 30 min |
| 2. QuestionStore | 2 files | 45 min |
| 3. Event Types | 1 file | 15 min |
| 4. TUI Questions Tab | 4 files | 60 min |
| 5. Orchestrator Integration | 1 file | 30 min |
| 6. CLI Integration | 1 file | 20 min |
| 7. E2E Testing | Manual | 30 min |

**Total: ~4 hours**
