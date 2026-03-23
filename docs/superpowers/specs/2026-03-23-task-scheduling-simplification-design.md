# Task Scheduling Simplification Design

**Date:** 2026-03-23
**Status:** Approved
**Author:** Claude + User

## Summary

Simplify the task scheduling system in two iterations:
1. Simplify verification pipeline: merge 4 fix functions into 1 unified `fix_errors()`
2. Split dispatcher: extract `DependencyGraph`, `HealthMonitor`, `TaskScheduler` modules

## Goals

- Reduce code complexity while maintaining quality
- Keep verification coverage unchanged (quality priority)
- Make each module have a single clear responsibility
- Reduce orchestrator.rs from ~2500 lines to ~400 lines

## Non-Goals

- Changing verification stages or coverage
- Modifying TUI or question/answer systems
- Changing task generation or clarification logic

---

## Design

### Part 1: Verification Pipeline Simplification

#### Current State (~300 lines in run_task_pipeline)

```
execute → test → fix_test → build → fix_build → func → fix_runtime → ai → fix_ai
```

4 separate fix functions:
- `fix_test_failure()` ~50 lines
- `fix_build_errors()` ~60 lines
- `fix_runtime_errors()` ~60 lines
- `fix_functionality_issues()` ~60 lines

All share the same pattern: send error to AI, get fix.

#### New Design

```rust
/// Verification stage
#[derive(Debug, Clone, Copy)]
pub enum Stage {
    Test,
    Build,
    AiReview,
}

/// Unified verification result
pub struct VerifyResult {
    pub passed: bool,
    pub stage: Stage,
    pub output: String,
}

impl TaskExecutor {
    /// Unified error fix entry point
    pub async fn fix_errors(
        &self,
        task: &mut Task,
        stage: Stage,
        error: &str,
    ) -> Result<bool> {
        let stage_context = match stage {
            Stage::Test => "test failures",
            Stage::Build => "compilation/build errors",
            Stage::AiReview => "functionality issues",
        };

        let prompt = format!(
            r#"You are fixing {} in a software project.

TASK: {}
DESCRIPTION: {}

ERRORS:
{}

Fix the issues. Make minimal, targeted changes.
Respond with a brief summary of what you fixed."#,
            stage_context, task.title, task.description, error
        );

        let result = self.runner.call(...).await?;
        Ok(!result.is_error)
    }
}
```

#### New Pipeline Flow

```rust
async fn run_task_pipeline(...) -> Result<Option<Task>> {
    // 1. Execute
    if !executor.execute(&mut task).await? {
        return handle_failure(&mut task, "Execution failed");
    }

    // 2. Verification loop
    let stages = [
        (Stage::Test, |t| executor.test(t)),
        (Stage::Build, |t| executor.verify_build(t)),
        (Stage::AiReview, |t| executor.ai_functionality_review(t)),
    ];

    for (stage, verify_fn) in stages {
        let (passed, output) = verify_fn(&mut task).await?;

        if !passed {
            let fixed = executor.fix_errors(&mut task, stage, &output).await?;

            if !fixed && task.retries < MAX_RETRIES {
                return retry_task(&mut task);
            } else if !fixed {
                return fail_task(&mut task, stage, &output);
            }
        }
    }

    // 3. Complete
    complete_task(&mut task);
    Ok(Some(task))
}
```

#### Code Change Summary

| Component | Current | After |
|-----------|---------|-------|
| fix_test_failure | ~50 lines | deleted |
| fix_build_errors | ~60 lines | deleted |
| fix_runtime_errors | ~60 lines | deleted |
| fix_functionality_issues | ~60 lines | deleted |
| fix_errors (new) | - | ~40 lines |
| run_task_pipeline | ~300 lines | ~80 lines |

**Net reduction: ~250 lines**

---

### Part 2: Dispatcher Split

#### Current State: `run_dispatcher()` (~300 lines)

All logic mixed together:
1. Collect completed tasks from JoinSet
2. Print progress
3. Checkpoint validation
4. Build dependency graph (with subtask mapping)
5. Get schedulable tasks
6. Dispatch tasks
7. Check exit conditions
8. Stalled task detection

#### New Module Structure

```
crates/core/src/
├── orchestrator/
│   ├── mod.rs              # exports
│   ├── orchestrator.rs     # main coordinator (~150 lines)
│   ├── task_scheduler.rs   # task scheduling (~120 lines)
│   ├── dependency_graph.rs # dependency management (~100 lines)
│   └── health_monitor.rs   # health monitoring (~80 lines)
└── executor/
    └── task_executor.rs    # simplified
```

#### Module 1: DependencyGraph

```rust
// dependency_graph.rs

pub struct DependencyGraph {
    /// parent task -> subtask list
    subtask_map: HashMap<String, Vec<String>>,
    /// completed task IDs
    completed_ids: HashSet<String>,
}

impl DependencyGraph {
    /// Build from task list
    pub fn build(tasks: &[Task]) -> Self { ... }

    /// Check if dependencies are satisfied (considering subtask splits)
    pub fn is_satisfied(&self, task: &Task) -> bool {
        task.depends_on.iter().all(|dep| self.is_dep_completed(dep))
    }

    /// Get all ready-to-run tasks
    pub fn get_ready_tasks(&self, pending: &[Task]) -> Vec<Task> {
        pending.iter()
            .filter(|t| self.is_satisfied(t))
            .cloned()
            .collect()
    }

    fn is_dep_completed(&self, dep: &str) -> bool {
        self.completed_ids.contains(dep)
            || self.subtask_children_all_completed(dep)
    }
}
```

#### Module 2: HealthMonitor

```rust
// health_monitor.rs

pub struct HealthConfig {
    pub stall_threshold_minutes: i64,
    pub check_interval_ms: u64,
}

pub struct HealthMonitor {
    config: HealthConfig,
    last_warning: Option<Instant>,
}

impl HealthMonitor {
    /// Detect and reset stalled tasks
    pub async fn check_stalled(&self, store: &TaskStore) -> Result<Vec<String>> {
        let now = Utc::now();
        let mut reset_ids = vec![];

        for task in store.all_tasks().await? {
            if task.status != TaskStatus::InProgress {
                continue;
            }
            if self.is_stalled(&task, now) {
                store.reset_to_pending(&task.id).await?;
                reset_ids.push(task.id);
            }
        }
        Ok(reset_ids)
    }

    /// Check blocked tasks and warn
    pub fn check_blocked(&mut self, blocked: &[BlockedTask]) {
        if self.should_warn() && !blocked.is_empty() {
            warn!("{} tasks blocked by failed dependencies", blocked.len());
            self.last_warning = Some(Instant::now());
        }
    }

    fn is_stalled(&self, task: &Task, now: DateTime<Utc>) -> bool { ... }
    fn should_warn(&self) -> bool { ... }
}
```

#### Module 3: TaskScheduler

```rust
// task_scheduler.rs

pub struct SlotPool {
    primary_slots: usize,
    subtask_slots: usize,
    primary_running: usize,
    subtask_running: usize,
}

impl SlotPool {
    pub fn new(num_agents: usize) -> Self {
        let (primary, subtask) = if num_agents <= 2 {
            (num_agents, num_agents)
        } else {
            (num_agents.div_ceil(2), num_agents - num_agents.div_ceil(2))
        };
        Self { primary_slots: primary, subtask_slots: subtask, primary_running: 0, subtask_running: 0 }
    }

    pub fn has_primary_slot(&self) -> bool { self.primary_running < self.primary_slots }
    pub fn has_subtask_slot(&self) -> bool { self.subtask_running < self.subtask_slots }
    pub fn acquire_primary(&mut self) { self.primary_running += 1; }
    pub fn acquire_subtask(&mut self) { self.subtask_running += 1; }
    pub fn release_primary(&mut self) { self.primary_running = self.primary_running.saturating_sub(1); }
    pub fn release_subtask(&mut self) { self.subtask_running = self.subtask_running.saturating_sub(1); }
}

pub struct TaskScheduler {
    slots: SlotPool,
    dispatched: HashSet<String>,
    join_set: JoinSet<TaskResult>,
}

impl TaskScheduler {
    pub fn dispatch(&mut self, task: Task, executor: Arc<TaskExecutor>) { ... }
    pub fn try_collect(&mut self) -> Option<TaskResult> { ... }
    pub fn is_empty(&self) -> bool { self.dispatched.is_empty() }
    pub fn is_dispatched(&self, id: &str) -> bool { self.dispatched.contains(id) }
}
```

#### Simplified Orchestrator

```rust
// orchestrator.rs

impl Orchestrator {
    async fn run_dispatcher(&mut self) -> Result<()> {
        let mut scheduler = TaskScheduler::new(self.config.num_agents);
        let mut health = HealthMonitor::default();

        loop {
            // 1. Collect completed tasks
            while let Some(result) = scheduler.try_collect() {
                self.handle_task_result(result);
            }

            // 2. Build dependency graph
            let tasks = self.store.all_tasks().await?;
            let graph = DependencyGraph::build(&tasks);

            // 3. Health check
            let stalled = health.check_stalled(&self.store).await?;
            if !stalled.is_empty() {
                info!("Reset {} stalled tasks", stalled.len());
            }

            // 4. Get ready tasks
            let pending: Vec<_> = self.store.pending_tasks().await?
                .into_iter()
                .filter(|t| !scheduler.is_dispatched(&t.id))
                .collect();

            let ready = graph.get_ready_tasks(&pending);
            let (primary, subtasks): (Vec<_>, Vec<_>) =
                ready.into_iter().partition(|t| t.depth == 0);

            // 5. Dispatch
            for task in primary {
                if scheduler.has_primary_slot() {
                    scheduler.dispatch(task, self.executor.clone());
                }
            }
            for task in subtasks {
                if scheduler.has_subtask_slot() {
                    scheduler.dispatch(task, self.executor.clone());
                }
            }

            // 6. Check exit
            if scheduler.is_empty() && pending.is_empty() {
                break;
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }
}
```

---

## Implementation Plan

### Iteration 1: Simplify Verification Pipeline

**Files changed:**
- `crates/core/src/executor/task_executor.rs` - add `fix_errors()`, delete 4 separate fix functions
- `crates/core/src/orchestrator/orchestrator.rs` - simplify `run_task_pipeline()`

**Steps:**
1. Add `Stage` enum and `fix_errors()` method
2. Modify `run_task_pipeline()` to use new unified flow
3. Delete old `fix_test_failure`, `fix_build_errors`, `fix_runtime_errors`, `fix_functionality_issues`
4. Test: run a simple project, verify validation flow works

**Risk:** Low - only merging identical logic

### Iteration 2: Split Dispatcher

**New files:**
- `crates/core/src/orchestrator/task_scheduler.rs`
- `crates/core/src/orchestrator/dependency_graph.rs`
- `crates/core/src/orchestrator/health_monitor.rs`

**Files changed:**
- `crates/core/src/orchestrator/mod.rs` - export new modules
- `crates/core/src/orchestrator/orchestrator.rs` - rewrite `run_dispatcher()`

**Steps:**
1. Create `DependencyGraph` and test
2. Create `HealthMonitor` and test
3. Create `TaskScheduler` and test
4. Rewrite `run_dispatcher()` using new modules
5. Delete old methods like `reset_stalled_tasks()`
6. Integration test

**Risk:** Medium - dispatcher is core logic

---

## Testing Strategy

### Unit Tests
- `DependencyGraph::is_satisfied()` - dependency satisfaction logic
- `SlotPool` - slot management
- `fix_errors()` - fix invocation

### Integration Tests
- Run a simple project (e.g., calculator), verify complete flow
- Intentionally introduce errors, verify fix mechanism

### Regression Tests
- Ensure existing features (questions, resume, git commit) work

---

## Unchanged Components

- `clarify_goal()` - clarification questions
- `generate_tasks()` - task generation
- `assess_and_split()` - complexity assessment
- `git_commit_task()` - git commits
- TUI event system
- Question/Answer system

---

## Expected Outcomes

| Metric | Current | After |
|--------|---------|-------|
| orchestrator.rs lines | ~2500 | ~400 |
| task_executor.rs lines | ~1400 | ~1100 |
| Largest function | run_dispatcher ~300 | run_dispatcher ~80 |
| Verification stages | 7 layers | 3 layers (coverage unchanged) |
| Fix functions | 4 | 1 |
