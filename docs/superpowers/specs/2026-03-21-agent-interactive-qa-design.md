# Agent Interactive Q&A System Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable agents to ask questions during task execution, with blocking questions pausing tasks until human intervention, and non-blocking questions allowing autonomous decisions with decision logs.

**Architecture:** Extend existing Event system with new AgentQuestion event type, add QuestionStore for persistence, and integrate with TUI using a new Questions tab that reuses Clarification dialog components.

**Tech Stack:** Rust, Tokio, Ratatui, JSON storage

---

## 1. Overview

### 1.1 Problem Statement

During long-running autonomous task execution, agents may encounter situations requiring clarification or decision-making. Currently, there's no mechanism for agents to:
- Ask questions during execution
- Block tasks pending human input
- Make autonomous decisions with audit trails

### 1.2 Solution

Implement an interactive Q&A system where:
- Agents can emit questions with multiple-choice options
- Blocking questions pause task execution until answered
- Non-blocking questions allow agents to decide autonomously with decision logs
- Questions are stored in a persistent queue for manual processing
- TUI provides a dedicated interface for viewing and answering questions

### 1.3 Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Blocking determination | Agent self-marking | Agent has best context on question importance |
| Storage format | JSON file | Consistent with existing TaskStore pattern |
| Notification mechanism | Question queue + manual processing | Allows flexible timing for responses |
| UI integration | New Questions Tab | Separate from task list, dedicated focus |

---

## 2. Data Model

### 2.1 Question Model

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

/// Question model
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
    pub pros: Vec<String>,
    /// Cons for each option (parallel to options)
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
    /// User's answer
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

### 2.2 Storage Location

```
workspace/
└── .matrix/
    ├── tasks/
    │   ├── task-001.json
    │   └── ...
    ├── questions.json      # Question queue
    ├── decisions.log       # Decision log (append-only)
    └── manifest.json
```

### 2.3 questions.json Structure

```json
{
  "questions": [
    {
      "id": "q-001",
      "task_id": "task-003",
      "question": "Which database to use?",
      "options": ["SQLite", "PostgreSQL", "MongoDB"],
      "pros": ["Lightweight", "Powerful", "Flexible"],
      "cons": ["No concurrency", "Requires deployment", "No transactions"],
      "recommended": 0,
      "recommendation_reason": "SQLite is simplest for current scale",
      "blocking": true,
      "status": "pending",
      "created_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

---

## 3. Event System

### 3.1 New Event Types

```rust
pub enum Event {
    // ... existing events ...

    /// Agent asks a question
    AgentQuestion {
        task_id: String,
        question: Question,
        response_tx: QuestionSender,
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
}
```

### 3.2 QuestionSender

```rust
/// Channel sender for waiting on question response
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

---

## 4. QuestionStore

### 4.1 Interface

```rust
pub struct QuestionStore {
    questions_dir: PathBuf,
    questions_path: PathBuf,
    decisions_path: PathBuf,
}

impl QuestionStore {
    /// Create new QuestionStore
    pub async fn new(questions_dir: PathBuf) -> Result<Self>;

    /// Create a new question
    pub async fn create(&self, question: &Question) -> Result<()>;

    /// Get all pending questions
    pub async fn pending_questions(&self) -> Result<Vec<Question>>;

    /// Get all questions
    pub async fn all_questions(&self) -> Result<Vec<Question>>;

    /// Answer a question
    pub async fn answer(&self, id: &str, answer: &str) -> Result<()>;

    /// Record auto-decision for non-blocking question
    pub async fn record_auto_decision(
        &self,
        id: &str,
        decision: &str,
        reason: &str,
    ) -> Result<()>;

    /// Get questions for a specific task
    pub async fn questions_for_task(&self, task_id: &str) -> Result<Vec<Question>>;

    /// Append decision to log file
    async fn append_decision_log(
        &self,
        question: &Question,
        decision: &str,
        reason: &str,
    ) -> Result<()>;
}
```

---

## 5. Executor Integration

### 5.1 TaskExecutor Extension

```rust
impl TaskExecutor {
    /// Ask user a question (for Agent to call)
    pub async fn ask_question(
        &self,
        task: &Task,
        question: &str,
        options: Vec<String>,
        pros: Vec<String>,
        cons: Vec<String>,
        recommended: Option<usize>,
        recommendation_reason: Option<String>,
        blocking: bool,
    ) -> Result<String> {
        // 1. Create Question object
        let q = Question {
            id: generate_question_id(),
            task_id: task.id.clone(),
            question: question.to_string(),
            options,
            pros,
            cons,
            recommended,
            recommendation_reason: recommendation_reason.clone(),
            blocking,
            status: QuestionStatus::Pending,
            answer: None,
            decision_log: None,
            created_at: Utc::now(),
            answered_at: None,
        };

        if blocking {
            // 2a. For blocking questions, wait for response
            let (tx, rx) = oneshot::channel();
            self.emit_event(Event::AgentQuestion {
                task_id: task.id.clone(),
                question: q,
                response_tx: QuestionSender::new(tx),
            });

            // 3. Block until answered
            let answer = rx.await?;
            Ok(answer)
        } else {
            // 2b. For non-blocking, agent decides and logs
            let decision = self.make_decision(&q)?;
            self.emit_event(Event::QuestionAutoDecided {
                question_id: q.id.clone(),
                decision: decision.clone(),
                reason: recommendation_reason.clone().unwrap_or_default(),
            });
            Ok(decision)
        }
    }

    /// Make autonomous decision based on recommendation
    fn make_decision(&self, question: &Question) -> Result<String> {
        if let Some(idx) = question.recommended {
            if idx < question.options.len() {
                return Ok(question.options[idx].clone());
            }
        }
        // Fallback to first option
        Ok(question.options.first().cloned().unwrap_or_default())
    }
}
```

### 5.2 Agent Prompt Format

Agent outputs questions in this JSON format during execution:

```
When you need to ask a question, output the following JSON format:

QUESTION:
{
  "question": "Which database to use?",
  "options": ["SQLite", "PostgreSQL", "MongoDB"],
  "pros": ["Lightweight, no install", "Powerful features", "Flexible schema"],
  "cons": ["No high concurrency", "Requires deployment", "No transactions"],
  "recommended": 0,
  "reason": "SQLite is most suitable for current project scale",
  "blocking": true
}
```

### 5.3 Execution Flow

```
┌─────────────────────────────────────────────────────────┐
│                    TaskExecutor                          │
│                                                          │
│  ┌──────────┐    Detect QUESTION    ┌─────────────────┐ │
│  │ Claude   │ ───────────────────► │ Parse JSON      │ │
│  │ Execute  │                       └────────┬────────┘ │
│  └──────────┘                                │          │
│                                              ▼          │
│                                     ┌─────────────────┐ │
│                                     │ blocking?       │ │
│                                     └────────┬────────┘ │
│                              ┌───────────────┴────────┐ │
│                              │                        │ │
│                     blocking=true            blocking=false
│                              │                        │ │
│                              ▼                        ▼ │
│                     ┌────────────────┐    ┌────────────┐ │
│                     │ Pause task     │    │ Auto decide│ │
│                     │ Emit event     │    │ Log reason │ │
│                     │ Wait response  │    │ Continue   │ │
│                     └───────┬────────┘    └────────────┘ │
│                             │                           │
│                     User answers                        │
│                             │                           │
│                             ▼                           │
│                     ┌────────────────┐                 │
│                     │ Resume task    │                 │
│                     └────────────────┘                 │
└─────────────────────────────────────────────────────────┘
```

---

## 6. TUI Interface

### 6.1 Questions Tab

New tab added to existing tab order:

```
┌──────────────────────────────────────────────────────────┐
│  Logs  |  Tasks  |  Output  |  Questions (2)            │  ← New
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ━━━ Pending Questions ━━━                               │
│                                                          │
│  ◉ q-001 [task-003] Which database to use?              │
│    ⏳ blocking                                           │
│                                                          │
│  ○ q-002 [task-005] Log output format?                  │
│    ✓ auto-decided: JSON                                  │
│                                                          │
│  ━━━ Answered ━━━                                        │
│                                                          │
│  ○ q-000 [task-001] Project structure?                   │
│    ✓ answered: Monorepo                                  │
│                                                          │
├──────────────────────────────────────────────────────────┤
│  Enter: Answer  |  r: Details  |  Tab: Switch            │
└──────────────────────────────────────────────────────────┘
```

### 6.2 Answer Dialog (Reuse Clarification Component)

```
┌──────────────────────────────────────────────────────────┐
│                   Answer Question                         │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ▶ Which database to use?                                │
│                                                          │
│  ◉ 1. SQLite         ⏺ +Lightweight -No concurrency     │
│  ○  2. PostgreSQL    +Powerful -Requires deployment      │
│  ○  3. MongoDB       +Flexible -No transactions          │
│                                                          │
│  Tip: SQLite is most suitable for current project scale   │
│                                                          │
│  ──────────────────────────────────────────────────────  │
│  ↑↓ Navigate  1-9 Quick select  Enter Confirm  Esc Skip  │
└──────────────────────────────────────────────────────────┘
```

### 6.3 Status Bar Indicator

```
v0.1.6 Running:api ⠋ task-003 | 2 pending questions | ...
                    ↑
              Show count when > 0
```

### 6.4 Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `4` or `Q` | Switch to Questions Tab |
| `Enter` | Answer selected question |
| `r` | View question details |
| `Esc` | Return to previous tab |

---

## 7. Decision Log

### 7.1 Log Format

Non-blocking questions result in decision logs:

**decisions.log**:
```
[2024-01-15 10:35:22] task-005 | Log output format?
  Decision: JSON
  Reason: Easy to parse, integrates with existing monitoring

[2024-01-15 10:42:11] task-007 | Add caching layer?
  Decision: Redis
  Reason: Project already has Redis dependency, reuse reduces complexity
```

### 7.2 Log Entry Structure

```
[timestamp] task_id | question
  Decision: <chosen_option>
  Reason: <reasoning>
```

---

## 8. Implementation Tasks

### Task 1: Question Model

**Files:**
- Create: `crates/core/src/models/question.rs`
- Modify: `crates/core/src/models/mod.rs`

- [ ] **Step 1: Define QuestionStatus enum**
- [ ] **Step 2: Define Question struct with all fields**
- [ ] **Step 3: Add helper methods (new, generate_id)**
- [ ] **Step 4: Add unit tests**
- [ ] **Step 5: Export from mod.rs**

### Task 2: QuestionStore

**Files:**
- Create: `crates/core/src/store/question_store.rs`
- Modify: `crates/core/src/store/mod.rs`

- [ ] **Step 1: Define QuestionStore struct**
- [ ] **Step 2: Implement new() constructor**
- [ ] **Step 3: Implement create() method**
- [ ] **Step 4: Implement pending_questions() method**
- [ ] **Step 5: Implement answer() method**
- [ ] **Step 6: Implement record_auto_decision() method**
- [ ] **Step 7: Implement append_decision_log() method**
- [ ] **Step 8: Add unit tests**
- [ ] **Step 9: Export from mod.rs**

### Task 3: Event Types

**Files:**
- Modify: `crates/core/src/tui/event.rs`

- [ ] **Step 1: Add QuestionStatus to event.rs or import**
- [ ] **Step 2: Define QuestionSender wrapper**
- [ ] **Step 3: Add AgentQuestion event variant**
- [ ] **Step 4: Add QuestionAnswered event variant**
- [ ] **Step 5: Add QuestionAutoDecided event variant**
- [ ] **Step 6: Update any affected pattern matches**

### Task 4: TUI Questions Tab

**Files:**
- Create: `crates/core/src/tui/components/questions.rs`
- Modify: `crates/core/src/tui/components/mod.rs`
- Modify: `crates/core/src/tui/app.rs`
- Modify: `crates/core/src/tui/render.rs`
- Modify: `crates/core/src/tui/event.rs`

- [ ] **Step 1: Add Questions variant to Tab enum**
- [ ] **Step 2: Create QuestionsPanel component**
- [ ] **Step 3: Add questions field to TuiApp**
- [ ] **Step 4: Add question_store field to TuiApp**
- [ ] **Step 5: Implement keyboard handling for Questions tab**
- [ ] **Step 6: Add render logic for Questions tab**
- [ ] **Step 7: Implement answer dialog (reuse Clarification)**
- [ ] **Step 8: Add status bar indicator for pending questions**
- [ ] **Step 9: Export component from mod.rs**

### Task 5: Executor Integration

**Files:**
- Modify: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: Add QUESTION JSON parsing to Claude output handler**
- [ ] **Step 2: Implement ask_question() method**
- [ ] **Step 3: Implement make_decision() helper**
- [ ] **Step 4: Add emit_question event helper**
- [ ] **Step 5: Handle QuestionAnswered event to resume task**

### Task 6: Orchestrator Integration

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: Initialize QuestionStore**
- [ ] **Step 2: Handle AgentQuestion events**
- [ ] **Step 3: Handle QuestionAnswered events**
- [ ] **Step 4: Handle QuestionAutoDecided events**
- [ ] **Step 5: Wire up event channel for questions**

### Task 7: End-to-End Testing

- [ ] **Step 1: Test blocking question flow**
- [ ] **Step 2: Test non-blocking auto-decision flow**
- [ ] **Step 3: Test question persistence across restart**
- [ ] **Step 4: Test decision log writing**
- [ ] **Step 5: Test TUI question answering**

---

## 9. File Change Summary

| File | Change Type |
|------|-------------|
| `models/question.rs` | Create |
| `models/mod.rs` | Modify (add export) |
| `store/question_store.rs` | Create |
| `store/mod.rs` | Modify (add export) |
| `tui/event.rs` | Modify (add events) |
| `tui/app.rs` | Modify (add Questions tab) |
| `tui/render.rs` | Modify (add Questions rendering) |
| `tui/components/questions.rs` | Create |
| `tui/components/mod.rs` | Modify (add export) |
| `executor/task_executor.rs` | Modify (add ask_question) |
| `orchestrator/orchestrator.rs` | Modify (wire up events) |
