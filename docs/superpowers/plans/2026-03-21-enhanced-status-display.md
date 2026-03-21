# Enhanced Status Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add granular activity states and heartbeat indicator to status bar so users know the program is still running during long operations.

**Architecture:** Extend `ExecutionState` with activity sub-types, add `ActivityPulse` event for heartbeat, update status bar rendering to show activity type and pulse indicator.

**Tech Stack:** Rust, ratatui

---

## File Structure

| File | Purpose |
|------|---------|
| `crates/core/src/tui/event.rs` | Add `Activity` enum, modify `ExecutionState`, add `ActivityPulse` event |
| `crates/core/src/tui/app.rs` | Add pulse tracking fields, handle new events |
| `crates/core/src/tui/components/status.rs` | Update rendering for activity and pulse |
| `crates/core/src/executor/task_executor.rs` | Emit activity states at operation boundaries |

---

## Task 1: Add Activity Enum and Update ExecutionState

**Files:**
- Modify: `crates/core/src/tui/event.rs:20-42`

- [ ] **Step 1: Write the failing test**

Add test for Activity display formatting in `crates/core/src/tui/event.rs` at the bottom of tests module:

```rust
#[cfg(test)]
mod tests {
    // ... existing tests ...

    #[test]
    fn test_activity_display() {
        use super::Activity;
        assert_eq!(Activity::ApiCall.to_string(), "api");
        assert_eq!(Activity::FileWrite.to_string(), "file");
        assert_eq!(Activity::Test.to_string(), "test");
        assert_eq!(Activity::Planning.to_string(), "plan");
        assert_eq!(Activity::Assessing.to_string(), "assess");
        assert_eq!(Activity::Git.to_string(), "git");
        assert_eq!(Activity::Other("custom").to_string(), "custom");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p matrix-core test_activity_display`
Expected: FAIL with "cannot find value `Activity`"

- [ ] **Step 3: Add Activity enum**

Add after `VerbosityLevel` definition (around line 18):

```rust
/// Activity type for granular status display
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    ApiCall,
    FileWrite,
    Test,
    Planning,
    Assessing,
    Git,
    Other(&'static str),
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiCall => write!(f, "api"),
            Self::FileWrite => write!(f, "file"),
            Self::Test => write!(f, "test"),
            Self::Planning => write!(f, "plan"),
            Self::Assessing => write!(f, "assess"),
            Self::Git => write!(f, "git"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}
```

- [ ] **Step 4: Update ExecutionState**

Replace the `ExecutionState` enum (lines 20-29):

```rust
/// Execution state for status bar
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutionState {
    #[default]
    Idle,
    Clarifying,
    Generating,
    Running { activity: Activity },
    Completed,
    Failed,
}
```

- [ ] **Step 5: Update ExecutionState Display impl**

Update the Display impl (lines 31-41):

```rust
impl std::fmt::Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Clarifying => write!(f, "Clarifying"),
            Self::Generating => write!(f, "Generating"),
            Self::Running { activity } => write!(f, "Running:{}", activity),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p matrix-core --lib tui::event`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/tui/event.rs
git commit -m "feat(tui): add Activity enum and update ExecutionState"
```

---

## Task 2: Add ActivityPulse Event

**Files:**
- Modify: `crates/core/src/tui/event.rs:146-248`

- [ ] **Step 1: Add ActivityPulse event variant**

Add to `Event` enum after `TokenUsageUpdate` (around line 244):

```rust
    // Activity pulse for heartbeat
    ActivityPulse {
        task_id: String,
        activity: Activity,
    },
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/tui/event.rs
git commit -m "feat(tui): add ActivityPulse event"
```

---

## Task 3: Update TuiApp for Pulse Tracking

**Files:**
- Modify: `crates/core/src/tui/app.rs`

- [ ] **Step 1: Add pulse tracking fields to TuiApp**

Add fields to `TuiApp` struct (around line 238, after `task_start_time`):

```rust
    // Activity pulse tracking
    pub last_pulse_time: Option<Instant>,
    pub current_activity: Option<Activity>,
```

- [ ] **Step 2: Initialize new fields in TuiApp::new**

Add to `TuiApp::new()` (around line 330):

```rust
            last_pulse_time: None,
            current_activity: None,
```

- [ ] **Step 3: Handle ActivityPulse event in process_event**

Add handler in `process_event` method (after `TokenUsageUpdate` handler, around line 1110):

```rust
            Event::ActivityPulse { task_id, activity } => {
                self.last_pulse_time = Some(Instant::now());
                self.current_activity = Some(activity);
                // Keep current_task_id in sync
                if self.current_task_id.is_none() {
                    self.current_task_id = Some(task_id);
                }
            }
```

- [ ] **Step 4: Update ExecutionStateChanged handler**

Update the `ExecutionStateChanged` handler (around line 1042) to extract activity:

```rust
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
```

- [ ] **Step 5: Add Activity import**

Add `Activity` to imports at top of file (line 6):

```rust
use crate::tui::{
    ClarificationQuestion, ClarificationSender, ConfirmSender, Event, EventReceiver,
    ExecutionState, Key, LogBuffer, LogContext, VerbosityLevel, Activity,
};
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/tui/app.rs
git commit -m "feat(tui): add pulse tracking to TuiApp"
```

---

## Task 4: Update Status Bar Rendering

**Files:**
- Modify: `crates/core/src/tui/components/status.rs`

- [ ] **Step 1: Add imports and constants**

Add at top of file after imports:

```rust
use std::time::Instant;

use crate::tui::{ExecutionState, VerbosityLevel, Activity};

/// Pulse indicator frames
const PULSE_FRAMES: &[&str] = &["●", "○"];
/// Warning threshold in seconds
const WARNING_THRESHOLD_SECS: u64 = 30;
/// Pulse threshold in seconds (switch from spinner to pulse)
const PULSE_THRESHOLD_SECS: u64 = 2;
```

- [ ] **Step 2: Add pulse parameter to render function signature**

Update the `render` function signature to include pulse parameter:

```rust
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
        last_pulse_time: Option<&Instant>,  // New parameter
    ) -> Paragraph<'static> {
```

- [ ] **Step 3: Add pulse indicator logic**

Add after the spinner logic (around line 52, after the spinner variable):

```rust
        // Calculate pulse indicator based on time since last activity
        let pulse_indicator = if matches!(state, ExecutionState::Running { .. }) {
            let elapsed_secs = last_pulse_time
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(u64::MAX);

            if elapsed_secs > WARNING_THRESHOLD_SECS {
                " ⚠".to_string()
            } else if elapsed_secs > PULSE_THRESHOLD_SECS {
                // Pulsing indicator
                let frame = PULSE_FRAMES[spinner_frame % PULSE_FRAMES.len()];
                format!(" {}", frame)
            } else {
                // Still in active spinner phase
                String::new()
            }
        } else {
            String::new()
        };
```

- [ ] **Step 4: Update state color for Running with activity**

Update the state color match:

```rust
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
```

- [ ] **Step 5: Update task_str to include pulse indicator**

Update the task_str construction:

```rust
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
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/tui/components/status.rs
git commit -m "feat(tui): update status bar with activity and pulse indicator"
```

---

## Task 5: Update Render Call Site

**Files:**
- Modify: `crates/core/src/tui/render.rs`

- [ ] **Step 1: Find StatusBar::render call and update it**

Find the `StatusBar::render` call and add the `last_pulse_time` parameter:

```rust
StatusBar::render(
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
    env!("CARGO_PKG_VERSION"),
    app.current_task_tokens,
    app.total_tokens,
    app.last_pulse_time.as_ref(),  // New parameter
)
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/tui/render.rs
git commit -m "feat(tui): pass last_pulse_time to StatusBar::render"
```

---

## Task 6: Emit Activity States from TaskExecutor

**Files:**
- Modify: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: Add imports**

Update imports at top of file (around line 10):

```rust
use crate::tui::{Event, EventSender, ExecutionState, Activity};
```

- [ ] **Step 2: Add helper methods for emitting activity**

Add helper methods after `emit_event` (around line 84):

```rust
    /// Emit activity state change
    fn emit_activity(&self, activity: Activity) {
        self.emit_event(Event::ExecutionStateChanged {
            state: ExecutionState::Running { activity },
        });
    }

    /// Emit activity pulse
    fn emit_pulse(&self, task_id: &str, activity: Activity) {
        self.emit_event(Event::ActivityPulse {
            task_id: task_id.to_string(),
            activity,
        });
    }
```

- [ ] **Step 3: Emit activity at start of execute method**

In `execute` method (around line 166), add after the info log:

```rust
        info!(task_id = %task.id, title = %task.title, model = %model, "Executing task");

        // Emit activity state for planning
        self.emit_activity(Activity::Planning);
```

- [ ] **Step 4: Emit ApiCall before Claude runner call**

Before the `self.runner.run(...)` call (around line 186), add:

```rust
        // Emit API call activity
        self.emit_activity(Activity::ApiCall);
```

- [ ] **Step 5: Emit Test activity in test method**

In `test` method (around line 284), add after the info log:

```rust
        info!(task_id = %task.id, title = %task.title, "Running tests");

        // Emit test activity
        self.emit_activity(Activity::Test);
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/executor/task_executor.rs
git commit -m "feat(executor): emit activity states during task execution"
```

---

## Task 7: Update Orchestrator for Phase Transitions

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: Add Activity to imports**

Update imports (around line 11):

```rust
use crate::tui::{ClarificationQuestion, ConfirmSender, Event, EventSender, ExecutionState, Activity};
```

- [ ] **Step 2: Update state emissions with activity**

Find and update existing `ExecutionStateChanged` emissions:

- Line ~167: Change to `ExecutionState::Running { activity: Activity::Planning }`
- Line ~179: Change to `ExecutionState::Running { activity: Activity::Planning }`
- Line ~186: Change to `ExecutionState::Running { activity: Activity::Planning }`
- Line ~208: Change to `ExecutionState::Running { activity: Activity::Assessing }`
- Line ~746: Change to `ExecutionState::Running { activity: Activity::Git }`

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "feat(orchestrator): add activity to execution state changes"
```

---

## Task 8: Build and Test

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 2: Build release**

Run: `cargo build --release`
Expected: Build succeeds

- [ ] **Step 3: Install and manual test**

Run: `cargo install --path crates/cli --force`

Then run matrix with a test goal and verify:
- Status bar shows activity type (Running:api, Running:test, etc.)
- Pulse indicator appears during long operations
- Warning indicator appears if stuck

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: resolve any issues found during testing"
```

---

## Summary

This plan adds:
1. Granular activity states (`Activity` enum)
2. Activity pulse mechanism for heartbeat
3. Visual indicators in status bar (activity type + pulse + warning)
4. Executor integration to emit activity states

The implementation follows TDD where practical and makes small, atomic commits at each step.
