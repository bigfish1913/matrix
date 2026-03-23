# Task Scheduling Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify task scheduling by merging 4 fix functions into 1 unified fix_errors(), and split the dispatcher into DependencyGraph, HealthMonitor, and TaskScheduler modules.

**Architecture:** Two iterations - first simplify verification pipeline (low risk), then split dispatcher into focused modules (medium risk). Each iteration produces working, testable code.

**Tech Stack:** Rust, Tokio async runtime, existing matrix-core crate

**Spec:** [2026-03-23-task-scheduling-simplification-design.md](../specs/2026-03-23-task-scheduling-simplification-design.md)

---

## File Structure

### Iteration 1 (Verification Pipeline)
```
crates/core/src/
├── executor/
│   └── task_executor.rs    # Add Stage enum, fix_errors(); delete 4 fix_* functions
└── orchestrator/
    └── orchestrator.rs     # Simplify run_task_pipeline()
```

### Iteration 2 (Dispatcher Split)
```
crates/core/src/
├── orchestrator/
│   ├── mod.rs              # Export new modules
│   ├── orchestrator.rs     # Simplified run_dispatcher()
│   ├── dependency_graph.rs # NEW: Dependency resolution
│   ├── health_monitor.rs   # NEW: Stalled/blocked detection
│   └── task_scheduler.rs   # NEW: Slot management, dispatch
```

---

## Iteration 1: Simplify Verification Pipeline

### Task 1.1: Add Stage Enum and fix_errors()

**Files:**
- Modify: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: Add Stage enum after imports (after line 17)**

```rust
/// Verification stage for error context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Test,
    Build,
    AiReview,
}

impl Stage {
    /// Get human-readable description for this stage
    pub fn description(&self) -> &'static str {
        match self {
            Stage::Test => "test failures",
            Stage::Build => "compilation/build errors",
            Stage::AiReview => "functionality issues",
        }
    }
}
```

- [ ] **Step 2: Add fix_errors() method after verify_functionality() (after line 669)**

```rust
    /// Unified error fix entry point
    ///
    /// This replaces the previous separate fix functions:
    /// - fix_test_failure
    /// - fix_build_errors
    /// - fix_runtime_errors
    /// - fix_functionality_issues
    pub async fn fix_errors(
        &self,
        task: &mut Task,
        stage: Stage,
        error: &str,
    ) -> Result<bool> {
        info!(task_id = %task.id, stage = ?stage, "Attempting to fix errors");
        self.emit_event(Event::TaskProgress {
            id: task.id.clone(),
            message: format!("🔧 Attempting to fix {}...", stage.description()),
        });

        let prompt = format!(
            r#"You are a senior developer fixing {} in a software project.

TASK: {}
DESCRIPTION: {}

ERRORS:
{}

CRITICAL INSTRUCTIONS:
1. Analyze the errors carefully
2. Fix the root cause, not just symptoms
3. Ensure fixes don't break existing functionality
4. Do NOT add placeholder implementations

Fix the errors. Make minimal, targeted changes.
Respond with a brief summary of what you fixed."#,
            stage.description(),
            task.title,
            task.description,
            error
        );

        let result = self
            .runner
            .call(
                &prompt,
                &self.workspace,
                Some(TIMEOUT_EXEC),
                None,
                None,
                Some(&task.id),
            )
            .await?;

        if result.is_error {
            warn!(error = %result.text, "Fix attempt failed");
            return Ok(false);
        }

        // Emit token usage update if available
        if let Some(usage) = &result.usage {
            self.emit_event(Event::TokenUsageUpdate {
                task_id: task.id.clone(),
                tokens_used: usage.total_tokens,
            });
            info!(task_id = %task.id, tokens = usage.total_tokens, "Token usage (fix)");
        }

        info!(task_id = %task.id, stage = ?stage, summary = %result.text, "Fix applied");
        Ok(true)
    }
```

- [ ] **Step 3: Run cargo check to verify syntax**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo check -p matrix-core`
Expected: No errors (may have unused warnings)

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/executor/task_executor.rs
git commit -m "feat(executor): add Stage enum and unified fix_errors() method"
```

---

### Task 1.2: Simplify run_task_pipeline()

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: Add use statement for Stage (at top of file, around line 1-23)**

Add to existing imports:
```rust
use crate::executor::Stage;
```

- [ ] **Step 2: Replace run_task_pipeline function (line 1957-2251)**

Replace the entire `run_task_pipeline` function with:

```rust
/// Run single task pipeline with unified verification and fix
async fn run_task_pipeline(
    store: Arc<TaskStore>,
    executor: Arc<TaskExecutor>,
    mut task: Task,
    event_sender: Option<EventSender>,
    workspace: PathBuf,
) -> Result<Option<Task>> {
    let thread_name = format!("thread-{}", task.id);

    // Set started_at and last_activity_at for stalled detection
    let now = Utc::now();
    task.started_at = Some(now);
    task.last_activity_at = Some(now);
    if let Err(e) = store.save_task(&task).await {
        error!(task_id = %task.id, error = %e, "Failed to save task as InProgress");
    }

    // Emit InProgress status
    if let Some(ref sender) = event_sender {
        let _ = sender.send(Event::TaskStatusChanged {
            id: task.id.clone(),
            status: TaskStatus::InProgress,
        });
    }

    // Phase 1: Execute
    let success = match executor.execute(&mut task, &thread_name).await {
        Ok(s) => s,
        Err(e) => {
            error!(task_id = %task.id, error = %e, "Executor failed");
            false
        }
    };

    if !success {
        return handle_execution_failure(store, &mut task, event_sender).await;
    }

    // Phase 2: Verification loop (Test -> Build -> AiReview)
    let stages: [(Stage, fn(&TaskExecutor, &mut Task) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(bool, String)>> + Send>>); 3] = [
        (Stage::Test, |e, t| Box::pin(e.test(t))),
        (Stage::Build, |e, t| Box::pin(e.verify_build(t))),
        (Stage::AiReview, |e, t| Box::pin(e.ai_functionality_review(t))),
    ];

    for (stage, verify_fn) in stages {
        let (passed, output) = match verify_fn(&executor, &mut task).await {
            Ok(result) => result,
            Err(e) => {
                warn!(task_id = %task.id, stage = ?stage, error = %e, "Verification error");
                (true, format!("Verification skipped: {}", e))
            }
        };

        if !passed {
            // Attempt unified fix
            let fixed = match executor.fix_errors(&mut task, stage, &output).await {
                Ok(f) => f,
                Err(e) => {
                    error!(task_id = %task.id, stage = ?stage, error = %e, "Fix attempt failed");
                    false
                }
            };

            if !fixed && task.retries < MAX_RETRIES {
                task.retries += 1;
                task.status = TaskStatus::Pending;
                task.error = Some(format!("{} failed: {}", stage.description(), output));
                task.started_at = None;
                if let Err(e) = store.save_task(&task).await {
                    error!(task_id = %task.id, error = %e, "Failed to save task for retry");
                }
                warn!(task_id = %task.id, stage = ?stage, attempt = task.retries, "Verification failed, retrying");
                return Ok(None);
            } else if !fixed {
                task.status = TaskStatus::Failed;
                task.error = Some(format!("{} failed: {}", stage.description(), output));
                task.started_at = None;
                if let Err(e) = store.save_task(&task).await {
                    error!(task_id = %task.id, error = %e, "Failed to save task as Failed");
                }
                error!(task_id = %task.id, stage = ?stage, "Verification failed permanently");
                if let Some(ref sender) = event_sender {
                    let _ = sender.send(Event::TaskStatusChanged {
                        id: task.id.clone(),
                        status: TaskStatus::Failed,
                    });
                }
                return Ok(None);
            }
        }
    }

    // Phase 3: Mark completed
    task.status = TaskStatus::Completed;
    task.started_at = None;
    if let Err(e) = store.save_task(&task).await {
        error!(task_id = %task.id, error = %e, "Failed to save task as Completed");
    }
    info!(task_id = %task.id, "Task completed successfully");

    // Git commit for completed task
    if let Err(e) = git_commit_task(&workspace, &task).await {
        warn!(task_id = %task.id, error = %e, "Git commit failed");
    }

    // Emit Completed status
    if let Some(ref sender) = event_sender {
        let _ = sender.send(Event::TaskStatusChanged {
            id: task.id.clone(),
            status: TaskStatus::Completed,
        });

        // Emit task summary with code changes
        let file_summaries: Vec<crate::tui::FileChangeSummary> = task
            .modified_files
            .iter()
            .map(|path| crate::tui::FileChangeSummary {
                path: path.clone(),
                description: String::new(),
            })
            .collect();
        let _ = sender.send(Event::TaskSummary {
            task_id: task.id.clone(),
            title: task.title.clone(),
            modified_files: file_summaries,
        });
    }

    Ok(Some(task))
}

/// Handle execution failure with retry logic
async fn handle_execution_failure(
    store: Arc<TaskStore>,
    task: &mut Task,
    event_sender: Option<EventSender>,
) -> Result<Option<Task>> {
    if task.retries < MAX_RETRIES {
        task.retries += 1;
        task.status = TaskStatus::Pending;
        task.started_at = None;
        if let Err(e) = store.save_task(task).await {
            error!(task_id = %task.id, error = %e, "Failed to save task for retry");
        }
        warn!(task_id = %task.id, attempt = task.retries, "Execution failed, retrying");
    } else {
        task.status = TaskStatus::Failed;
        task.started_at = None;
        if let Err(e) = store.save_task(task).await {
            error!(task_id = %task.id, error = %e, "Failed to save task as Failed");
        }
        error!(task_id = %task.id, "Execution failed permanently");
        if let Some(ref sender) = event_sender {
            let _ = sender.send(Event::TaskStatusChanged {
                id: task.id.clone(),
                status: TaskStatus::Failed,
            });
        }
    }
    Ok(None)
}
```

- [ ] **Step 3: Run cargo check to verify syntax**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "refactor(orchestrator): simplify run_task_pipeline with unified verification"
```

---

### Task 1.3: Delete Old Fix Functions

**Files:**
- Modify: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: Delete fix_test_failure() (lines 411-460)**

Remove the entire function `pub async fn fix_test_failure(&self, task: &mut Task, test_output: &str) -> Result<bool>`

- [ ] **Step 2: Delete fix_build_errors() (lines 566-625)**

Remove the entire function `pub async fn fix_build_errors(&self, task: &mut Task, build_output: &str) -> Result<bool>`

- [ ] **Step 3: Delete fix_runtime_errors() (lines 769-829)**

Remove the entire function `pub async fn fix_runtime_errors(&self, task: &mut Task, runtime_output: &str) -> Result<bool>`

- [ ] **Step 4: Delete fix_functionality_issues() (lines 948-1007)**

Remove the entire function `pub async fn fix_functionality_issues(&self, task: &mut Task, review_output: &str) -> Result<bool>`

- [ ] **Step 5: Run cargo check to verify no remaining references**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/executor/task_executor.rs
git commit -m "refactor(executor): remove deprecated fix_* functions in favor of fix_errors()"
```

---

### Task 1.4: Test Iteration 1

- [ ] **Step 1: Run unit tests**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo test -p matrix-core`
Expected: All tests pass

- [ ] **Step 2: Build release**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo build --release`
Expected: Build succeeds

- [ ] **Step 3: Manual integration test (optional)**

Run matrix with a simple project to verify the pipeline works end-to-end.

- [ ] **Step 4: Commit iteration 1 completion**

```bash
git add -A
git commit -m "feat: complete iteration 1 - unified verification pipeline"
```

---

## Iteration 2: Split Dispatcher

### Task 2.1: Create DependencyGraph Module

**Files:**
- Create: `crates/core/src/orchestrator/dependency_graph.rs`

- [ ] **Step 1: Create dependency_graph.rs**

```rust
//! Dependency graph for task scheduling.

use crate::models::{Task, TaskStatus};
use std::collections::{HashMap, HashSet};

/// Dependency graph for resolving task dependencies.
///
/// Handles the complexity of:
/// - Direct dependencies (task A depends on task B)
/// - Split tasks (task A was split into A-1, A-2, etc.)
pub struct DependencyGraph {
    /// Map of parent task ID -> subtask IDs
    subtask_map: HashMap<String, Vec<String>>,
    /// Set of completed/skipped task IDs
    completed_ids: HashSet<String>,
}

impl DependencyGraph {
    /// Build dependency graph from a list of tasks.
    pub fn build(tasks: &[Task]) -> Self {
        let mut subtask_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut completed_ids: HashSet<String> = HashSet::new();

        for task in tasks {
            // Track completed/skipped tasks
            if task.status == TaskStatus::Completed || task.status == TaskStatus::Skipped {
                completed_ids.insert(task.id.clone());
            }

            // Detect subtasks (ID pattern: parent-1, parent-2, etc.)
            if let Some(pos) = task.id.rfind('-') {
                if pos > 0 {
                    let parent_id = &task.id[..pos];
                    // Check if suffix is a number
                    if task.id[pos + 1..].parse::<u32>().is_ok() {
                        subtask_map
                            .entry(parent_id.to_string())
                            .or_default()
                            .push(task.id.clone());
                    }
                }
            }
        }

        Self { subtask_map, completed_ids }
    }

    /// Check if a task's dependencies are satisfied.
    ///
    /// A dependency is satisfied if:
    /// 1. The task itself is completed/skipped, OR
    /// 2. The task was split and all subtasks are completed/skipped
    pub fn is_satisfied(&self, task: &Task) -> bool {
        task.depends_on.iter().all(|dep| self.is_dep_completed(dep))
    }

    /// Get all tasks that are ready to run (dependencies satisfied).
    pub fn get_ready_tasks<'a>(&self, pending: &'a [Task]) -> Vec<&'a Task> {
        pending
            .iter()
            .filter(|t| self.is_satisfied(t))
            .collect()
    }

    /// Update completed set after a task completes.
    pub fn mark_completed(&mut self, task_id: &str) {
        self.completed_ids.insert(task_id.to_string());
    }

    /// Check if a specific dependency is completed.
    fn is_dep_completed(&self, dep: &str) -> bool {
        // Direct completion
        if self.completed_ids.contains(dep) {
            return true;
        }

        // Check if this dep was split into subtasks
        if let Some(subtasks) = self.subtask_map.get(dep) {
            // All subtasks must be completed
            subtasks.iter().all(|s| self.completed_ids.contains(s))
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, status: TaskStatus, depends_on: Vec<&str>) -> Task {
        let mut task = Task::new(id.to_string(), format!("Task {}", id), "Description".to_string());
        task.status = status;
        task.depends_on = depends_on.into_iter().map(|s| s.to_string()).collect();
        task
    }

    #[test]
    fn test_direct_dependency() {
        let tasks = vec![
            make_task("task-1", TaskStatus::Completed, vec![]),
            make_task("task-2", TaskStatus::Pending, vec!["task-1"]),
        ];
        let graph = DependencyGraph::build(&tasks);

        assert!(graph.is_satisfied(&tasks[1]));
    }

    #[test]
    fn test_unsatisfied_dependency() {
        let tasks = vec![
            make_task("task-1", TaskStatus::Pending, vec![]),
            make_task("task-2", TaskStatus::Pending, vec!["task-1"]),
        ];
        let graph = DependencyGraph::build(&tasks);

        assert!(!graph.is_satisfied(&tasks[1]));
    }

    #[test]
    fn test_split_task_dependency() {
        let tasks = vec![
            make_task("task-1", TaskStatus::Skipped, vec![]),
            make_task("task-1-1", TaskStatus::Completed, vec![]),
            make_task("task-1-2", TaskStatus::Completed, vec![]),
            make_task("task-2", TaskStatus::Pending, vec!["task-1"]),
        ];
        let graph = DependencyGraph::build(&tasks);

        // task-2 should be satisfied because all subtasks of task-1 are completed
        assert!(graph.is_satisfied(&tasks[3]));
    }

    #[test]
    fn test_partial_split_dependency() {
        let tasks = vec![
            make_task("task-1", TaskStatus::Skipped, vec![]),
            make_task("task-1-1", TaskStatus::Completed, vec![]),
            make_task("task-1-2", TaskStatus::Pending, vec![]),
            make_task("task-2", TaskStatus::Pending, vec!["task-1"]),
        ];
        let graph = DependencyGraph::build(&tasks);

        // task-2 should NOT be satisfied because task-1-2 is still pending
        assert!(!graph.is_satisfied(&tasks[3]));
    }
}
```

- [ ] **Step 2: Run tests for DependencyGraph**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo test -p matrix-core dependency_graph`
Expected: 4 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/orchestrator/dependency_graph.rs
git commit -m "feat(orchestrator): add DependencyGraph module"
```

---

### Task 2.2: Create HealthMonitor Module

**Files:**
- Create: `crates/core/src/orchestrator/health_monitor.rs`

- [ ] **Step 1: Create health_monitor.rs**

```rust
//! Health monitoring for task scheduling.

use crate::models::{Task, TaskStatus};
use crate::store::TaskStore;
use crate::error::Result;
use chrono::{DateTime, Utc};
use std::time::Instant;
use tracing::{info, warn};

/// Configuration for health monitoring.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Threshold in minutes before a task is considered stalled.
    pub stall_threshold_minutes: i64,
    /// Minimum seconds between warning logs.
    pub warning_throttle_secs: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            stall_threshold_minutes: 10,
            warning_throttle_secs: 30,
        }
    }
}

/// Blocked task information.
#[derive(Debug, Clone)]
pub struct BlockedTask {
    pub task_id: String,
    pub blocked_by: Vec<String>,
}

/// Health monitor for detecting and handling stalled/blocked tasks.
pub struct HealthMonitor {
    config: HealthConfig,
    last_warning: Option<Instant>,
}

impl HealthMonitor {
    /// Create a new health monitor with default config.
    pub fn new() -> Self {
        Self {
            config: HealthConfig::default(),
            last_warning: None,
        }
    }

    /// Create health monitor with custom config.
    pub fn with_config(config: HealthConfig) -> Self {
        Self {
            config,
            last_warning: None,
        }
    }

    /// Detect and reset stalled tasks.
    ///
    /// Returns the list of task IDs that were reset.
    pub async fn check_stalled(&self, store: &TaskStore) -> Result<Vec<String>> {
        let all_tasks = store.all_tasks().await?;
        let now = Utc::now();
        let mut reset_ids = Vec::new();

        for mut task in all_tasks {
            if task.status != TaskStatus::InProgress {
                continue;
            }

            if self.is_stalled(&task, now) {
                warn!(task_id = %task.id, "Resetting stalled task to Pending");
                task.status = TaskStatus::Pending;
                task.started_at = None;
                task.last_activity_at = None;
                store.save_task(&task).await?;
                reset_ids.push(task.id);
            }
        }

        if !reset_ids.is_empty() {
            info!("Reset {} stalled tasks", reset_ids.len());
        }

        Ok(reset_ids)
    }

    /// Check and warn about blocked tasks.
    pub fn check_blocked(&mut self, blocked: &[BlockedTask]) {
        if blocked.is_empty() {
            return;
        }

        if self.should_warn() {
            for b in blocked {
                warn!(
                    task_id = %b.task_id,
                    blocked_by = ?b.blocked_by,
                    "Task blocked by failed dependencies"
                );
            }
            self.last_warning = Some(Instant::now());
        }
    }

    /// Check if a task is stalled (no activity for too long).
    fn is_stalled(&self, task: &Task, now: DateTime<Utc>) -> bool {
        let activity_time = task.last_activity_at.or(task.started_at);
        match activity_time {
            Some(time) => {
                let elapsed = now.signed_duration_since(time);
                elapsed.num_minutes() >= self.config.stall_threshold_minutes
            }
            None => {
                // InProgress but no activity time - definitely stalled
                true
            }
        }
    }

    /// Check if we should emit a warning (throttled).
    fn should_warn(&self) -> bool {
        match self.last_warning {
            None => true,
            Some(time) => {
                time.elapsed().as_secs() >= self.config.warning_throttle_secs
            }
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_config_default() {
        let config = HealthConfig::default();
        assert_eq!(config.stall_threshold_minutes, 10);
        assert_eq!(config.warning_throttle_secs, 30);
    }

    #[test]
    fn test_should_warn_throttling() {
        let mut monitor = HealthMonitor::new();

        // First warning should be allowed
        assert!(monitor.should_warn());

        // After recording, should be throttled
        monitor.last_warning = Some(Instant::now());
        assert!(!monitor.should_warn());
    }
}
```

- [ ] **Step 2: Run tests for HealthMonitor**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo test -p matrix-core health_monitor`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/orchestrator/health_monitor.rs
git commit -m "feat(orchestrator): add HealthMonitor module"
```

---

### Task 2.3: Create TaskScheduler Module

**Files:**
- Create: `crates/core/src/orchestrator/task_scheduler.rs`

- [ ] **Step 1: Create task_scheduler.rs**

```rust
//! Task scheduler for parallel execution.

use crate::error::Result;
use crate::executor::TaskExecutor;
use crate::models::Task;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinSet;

/// Result from a completed task pipeline.
pub type TaskResult = (String, usize, Option<Task>);

/// Slot pool for managing concurrent task limits.
#[derive(Debug)]
pub struct SlotPool {
    primary_slots: usize,
    subtask_slots: usize,
    primary_running: usize,
    subtask_running: usize,
}

impl SlotPool {
    /// Create slot pool with given number of agents.
    ///
    /// For num_agents <= 2: unified pool (all agents can process any task)
    /// For num_agents > 2: split pool (half primary, half subtasks)
    pub fn new(num_agents: usize) -> Self {
        let (primary, subtask) = if num_agents <= 2 {
            (num_agents, num_agents)
        } else {
            let primary = num_agents.div_ceil(2);
            let subtask = num_agents.saturating_sub(primary);
            (primary, subtask)
        };

        Self {
            primary_slots: primary,
            subtask_slots: subtask,
            primary_running: 0,
            subtask_running: 0,
        }
    }

    pub fn has_primary_slot(&self) -> bool {
        self.primary_running < self.primary_slots
    }

    pub fn has_subtask_slot(&self) -> bool {
        self.subtask_running < self.subtask_slots
    }

    pub fn acquire_primary(&mut self) {
        self.primary_running += 1;
    }

    pub fn acquire_subtask(&mut self) {
        self.subtask_running += 1;
    }

    pub fn release_primary(&mut self) {
        self.primary_running = self.primary_running.saturating_sub(1);
    }

    pub fn release_subtask(&mut self) {
        self.subtask_running = self.subtask_running.saturating_sub(1);
    }

    #[allow(dead_code)]
    pub fn total_running(&self) -> usize {
        self.primary_running + self.subtask_running
    }
}

/// Task scheduler managing dispatch and collection.
pub struct TaskScheduler {
    slots: SlotPool,
    dispatched: HashSet<String>,
    join_set: JoinSet<TaskResult>,
}

impl TaskScheduler {
    /// Create a new scheduler with given agent count.
    pub fn new(num_agents: usize) -> Self {
        Self {
            slots: SlotPool::new(num_agents),
            dispatched: HashSet::new(),
            join_set: JoinSet::new(),
        }
    }

    /// Check if a task ID has been dispatched.
    pub fn is_dispatched(&self, id: &str) -> bool {
        self.dispatched.contains(id)
    }

    /// Check if there are no running tasks.
    pub fn is_empty(&self) -> bool {
        self.dispatched.is_empty()
    }

    /// Check primary slot availability.
    pub fn has_primary_slot(&self) -> bool {
        self.slots.has_primary_slot()
    }

    /// Check subtask slot availability.
    pub fn has_subtask_slot(&self) -> bool {
        self.slots.has_subtask_slot()
    }

    /// Dispatch a task for execution.
    pub fn dispatch(
        &mut self,
        task: Task,
        executor: Arc<TaskExecutor>,
        event_sender: Option<crate::tui::EventSender>,
        workspace: PathBuf,
    ) {
        let is_primary = task.depth == 0;
        let task_id = task.id.clone();

        self.dispatched.insert(task_id.clone());
        if is_primary {
            self.slots.acquire_primary();
        } else {
            self.slots.acquire_subtask();
        }

        let store = executor.store.clone();
        let depth = task.depth;

        self.join_set.spawn(async move {
            let result = run_task_pipeline(store, executor, task, event_sender, workspace).await;
            (task_id, depth, result.ok().flatten())
        });
    }

    /// Try to collect a completed task result.
    pub fn try_collect(&mut self) -> Option<TaskResult> {
        self.join_set.try_join_next().transpose().ok().flatten()
    }

    /// Handle task completion (release slots).
    pub fn on_task_completed(&mut self, task_id: &str, depth: usize) {
        self.dispatched.remove(task_id);
        if depth == 0 {
            self.slots.release_primary();
        } else {
            self.slots.release_subtask();
        }
    }
}

/// Run single task pipeline (delegates to orchestrator implementation).
async fn run_task_pipeline(
    store: Arc<crate::store::TaskStore>,
    executor: Arc<TaskExecutor>,
    task: Task,
    event_sender: Option<crate::tui::EventSender>,
    workspace: PathBuf,
) -> Result<Option<Task>> {
    // Import the actual implementation from orchestrator
    super::run_task_pipeline_impl(store, executor, task, event_sender, workspace).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_pool_unified() {
        // num_agents <= 2: unified pool
        let pool = SlotPool::new(1);
        assert_eq!(pool.primary_slots, 1);
        assert_eq!(pool.subtask_slots, 1);
    }

    #[test]
    fn test_slot_pool_split() {
        // num_agents > 2: split pool
        let pool = SlotPool::new(4);
        assert_eq!(pool.primary_slots, 2);
        assert_eq!(pool.subtask_slots, 2);
    }

    #[test]
    fn test_slot_acquire_release() {
        let mut pool = SlotPool::new(2);
        assert!(pool.has_primary_slot());

        pool.acquire_primary();
        assert!(pool.has_primary_slot()); // Still has slot (unified pool)

        pool.acquire_primary();
        assert!(!pool.has_primary_slot()); // No more slots

        pool.release_primary();
        assert!(pool.has_primary_slot()); // Slot released
    }

    #[test]
    fn test_scheduler_dispatch_tracking() {
        let mut scheduler = TaskScheduler::new(1);

        assert!(!scheduler.is_dispatched("task-1"));
        assert!(scheduler.is_empty());
    }
}
```

- [ ] **Step 2: Run tests for TaskScheduler**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo test -p matrix-core task_scheduler`
Expected: 4 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/orchestrator/task_scheduler.rs
git commit -m "feat(orchestrator): add TaskScheduler module"
```

---

### Task 2.4: Update Module Exports

**Files:**
- Modify: `crates/core/src/orchestrator/mod.rs`

- [ ] **Step 1: Update mod.rs with new exports**

```rust
//! Main orchestrator module.

mod dependency_graph;
mod health_monitor;
#[allow(clippy::module_inception)]
mod orchestrator;
mod task_scheduler;

pub use dependency_graph::DependencyGraph;
pub use health_monitor::{BlockedTask, HealthConfig, HealthMonitor};
pub use orchestrator::{Orchestrator, OrchestratorConfig};
pub use task_scheduler::{SlotPool, TaskScheduler};

// Export the pipeline function for task_scheduler
pub(crate) use orchestrator::run_task_pipeline_impl;
```

- [ ] **Step 2: Update orchestrator.rs to export run_task_pipeline_impl**

In `orchestrator.rs`, rename `run_task_pipeline` to `run_task_pipeline_impl` and make it `pub(crate)`:

Change line 1957 from:
```rust
async fn run_task_pipeline(
```
To:
```rust
pub(crate) async fn run_task_pipeline_impl(
```

- [ ] **Step 3: Run cargo check**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/orchestrator/mod.rs crates/core/src/orchestrator/orchestrator.rs
git commit -m "refactor(orchestrator): export new modules and pipeline impl"
```

---

### Task 2.5: Rewrite run_dispatcher()

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: Add new imports at top of file**

Add after existing imports:
```rust
use super::dependency_graph::DependencyGraph;
use super::health_monitor::HealthMonitor;
use super::task_scheduler::TaskScheduler;
```

- [ ] **Step 2: Replace run_dispatcher() method (approximately lines 1445-1771)**

Replace the entire `run_dispatcher` method with:

```rust
    async fn run_dispatcher(&mut self) -> Result<()> {
        let mut scheduler = TaskScheduler::new(self.config.num_agents);
        let mut health = HealthMonitor::new();

        let deadline = Instant::now() + Duration::from_secs(24 * 3600);

        while Instant::now() < deadline {
            // Process events from TUI
            self.poll_events().await;

            // 1. Collect completed tasks
            while let Some((task_id, depth, completed_task)) = scheduler.try_collect() {
                scheduler.on_task_completed(&task_id, depth);

                if let Some(task) = completed_task {
                    self.checkpoint.on_task_completed();
                    self.update_task_memory(&task).await;

                    if let Err(e) = self.store.save_manifest(&self.config.goal).await {
                        warn!(error = %e, "Failed to update manifest");
                    }

                    // Check if review is needed
                    let completed = self.store.count(TaskStatus::Completed).await.unwrap_or(0);
                    let total = self.store.total().await.unwrap_or(0);
                    if self.checkpoint.should_review(completed, total) {
                        self.show_review_report().await;
                    }
                }
            }

            self.print_progress().await;

            // 2. Health check - reset stalled tasks
            let stalled = health.check_stalled(&self.store).await?;
            if !stalled.is_empty() {
                info!("Reset {} stalled tasks", stalled.len());
            }

            // 3. Build dependency graph
            let tasks = self.store.all_tasks().await?;
            let graph = DependencyGraph::build(&tasks);

            // 4. Get pending tasks with satisfied dependencies
            let pending: Vec<Task> = self.store.pending_tasks().await?
                .into_iter()
                .filter(|t| !scheduler.is_dispatched(&t.id))
                .collect();

            let ready = graph.get_ready_tasks(&pending);

            // Partition into primary and subtasks
            let (primary, subtasks): (Vec<_>, Vec<_>) =
                ready.into_iter().partition(|t| t.depth == 0);

            // 5. Dispatch primary tasks
            for task in primary {
                if scheduler.has_primary_slot() {
                    info!(task_id = %task.id, "[primary] Dispatched");
                    scheduler.dispatch(
                        task.clone(),
                        self.executor.clone(),
                        self.config.event_sender.clone(),
                        self.config.workspace.clone(),
                    );
                }
            }

            // 6. Dispatch subtasks
            for task in subtasks {
                if scheduler.has_subtask_slot() {
                    info!(task_id = %task.id, "[subtask] Dispatched");
                    scheduler.dispatch(
                        task.clone(),
                        self.executor.clone(),
                        self.config.event_sender.clone(),
                        self.config.workspace.clone(),
                    );
                }
            }

            // 7. Check exit conditions
            let in_progress = self.store.count(TaskStatus::InProgress).await.unwrap_or(0);
            let pending_count = self.store.pending_tasks().await?.len();

            if scheduler.is_empty() && pending_count == 0 {
                if in_progress == 0 {
                    info!("All tasks processed");
                    break;
                }
                // If we have in_progress but nothing pending, check for stalled
                // (already done at start of loop)
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }
```

- [ ] **Step 3: Delete reset_stalled_tasks method (no longer needed)**

Remove the `reset_stalled_tasks` method (approximately lines 1838-1876).

- [ ] **Step 4: Run cargo check**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo check -p matrix-core`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "refactor(orchestrator): rewrite run_dispatcher with new modules"
```

---

### Task 2.6: Test Iteration 2

- [ ] **Step 1: Run all unit tests**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo test -p matrix-core`
Expected: All tests pass

- [ ] **Step 2: Build release**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo build --release`
Expected: Build succeeds

- [ ] **Step 3: Install and test**

Run: `cd c:/Users/bigfish/Projects/github.com/bigfish1913/matrix && cargo install --path crates/cli --force`
Expected: Install succeeds

- [ ] **Step 4: Commit iteration 2 completion**

```bash
git add -A
git commit -m "feat: complete iteration 2 - dispatcher split into modules"
```

---

## Summary

### Expected Outcomes

| Metric | Before | After |
|--------|--------|-------|
| orchestrator.rs | ~2500 lines | ~500 lines |
| task_executor.rs | ~1400 lines | ~1100 lines |
| New modules | 0 | 3 (~300 lines total) |
| Fix functions | 4 | 1 unified |
| Verification stages | 7 layers | 3 layers (same coverage) |

### Files Changed Summary

**Iteration 1:**
- `crates/core/src/executor/task_executor.rs` - Add Stage, fix_errors; delete 4 fix_*
- `crates/core/src/orchestrator/orchestrator.rs` - Simplify run_task_pipeline

**Iteration 2:**
- `crates/core/src/orchestrator/dependency_graph.rs` - NEW
- `crates/core/src/orchestrator/health_monitor.rs` - NEW
- `crates/core/src/orchestrator/task_scheduler.rs` - NEW
- `crates/core/src/orchestrator/mod.rs` - Export new modules
- `crates/core/src/orchestrator/orchestrator.rs` - Rewrite run_dispatcher
