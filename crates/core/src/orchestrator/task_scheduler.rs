//! TaskScheduler - manages parallel task execution with slot-based concurrency.
//!
//! This module provides:
//! - `SlotPool`: Manages execution slots for primary tasks and subtasks
//! - `TaskScheduler`: Coordinates task dispatching and result collection

use crate::error::Result;
use crate::models::Task;
use std::collections::HashSet;
use tokio::task::JoinSet;

/// Result type returned from task execution
/// (task_id, depth, optional_completed_task)
pub type TaskResult = (String, usize, Option<Task>);

/// Slot pool for managing parallel execution slots.
///
/// When num_agents <= 2, uses a unified pool where all slots can process any task.
/// When num_agents > 2, splits the pool: half for primary tasks, half for subtasks.
#[derive(Debug)]
pub struct SlotPool {
    /// Maximum slots for primary tasks (depth == 0)
    primary_slots: usize,
    /// Maximum slots for subtasks (depth > 0)
    subtask_slots: usize,
    /// Currently running primary tasks
    primary_running: usize,
    /// Currently running subtasks
    subtask_running: usize,
}

impl SlotPool {
    /// Create a new slot pool.
    ///
    /// For num_agents <= 2: unified pool (all agents can process any task)
    /// For num_agents > 2: split pool (half primary, half subtask)
    pub fn new(num_agents: usize) -> Self {
        let (primary_slots, subtask_slots) = if num_agents <= 2 {
            // Unified pool: all agents can process any task
            (num_agents, num_agents)
        } else {
            // Split pool: half for primary, half for subtasks
            let primary = num_agents.div_ceil(2);
            let subtask = num_agents.saturating_sub(primary);
            (primary, subtask)
        };

        Self {
            primary_slots,
            subtask_slots,
            primary_running: 0,
            subtask_running: 0,
        }
    }

    /// Check if a primary task slot is available
    pub fn has_primary_slot(&self) -> bool {
        self.primary_running < self.primary_slots
    }

    /// Check if a subtask slot is available
    pub fn has_subtask_slot(&self) -> bool {
        self.subtask_running < self.subtask_slots
    }

    /// Acquire a primary task slot
    ///
    /// Returns true if slot was acquired, false if no slot available
    pub fn acquire_primary(&mut self) -> bool {
        if self.has_primary_slot() {
            self.primary_running += 1;
            true
        } else {
            false
        }
    }

    /// Acquire a subtask slot
    ///
    /// Returns true if slot was acquired, false if no slot available
    pub fn acquire_subtask(&mut self) -> bool {
        if self.has_subtask_slot() {
            self.subtask_running += 1;
            true
        } else {
            false
        }
    }

    /// Release a primary task slot
    pub fn release_primary(&mut self) {
        self.primary_running = self.primary_running.saturating_sub(1);
    }

    /// Release a subtask slot
    pub fn release_subtask(&mut self) {
        self.subtask_running = self.subtask_running.saturating_sub(1);
    }

    /// Get total number of currently running tasks
    pub fn total_running(&self) -> usize {
        self.primary_running + self.subtask_running
    }

    /// Get the maximum primary slots
    pub fn max_primary_slots(&self) -> usize {
        self.primary_slots
    }

    /// Get the maximum subtask slots
    pub fn max_subtask_slots(&self) -> usize {
        self.subtask_slots
    }

    /// Get the current primary running count
    pub fn primary_running(&self) -> usize {
        self.primary_running
    }

    /// Get the current subtask running count
    pub fn subtask_running(&self) -> usize {
        self.subtask_running
    }
}

/// Task scheduler for managing parallel task execution.
///
/// Coordinates:
/// - Slot management via `SlotPool`
/// - Dispatch tracking via `HashSet`
/// - Async task collection via `JoinSet`
pub struct TaskScheduler {
    /// Slot pool for managing execution slots
    slots: SlotPool,
    /// Set of currently dispatched task IDs
    dispatched: HashSet<String>,
    /// JoinSet for collecting task results
    join_set: JoinSet<TaskResult>,
}

impl TaskScheduler {
    /// Create a new task scheduler with the given number of agents
    pub fn new(num_agents: usize) -> Self {
        Self {
            slots: SlotPool::new(num_agents),
            dispatched: HashSet::new(),
            join_set: JoinSet::new(),
        }
    }

    /// Check if a task ID has been dispatched
    pub fn is_dispatched(&self, id: &str) -> bool {
        self.dispatched.contains(id)
    }

    /// Check if the scheduler is empty (no running tasks)
    pub fn is_empty(&self) -> bool {
        self.dispatched.is_empty() && self.join_set.is_empty()
    }

    /// Check if a primary task slot is available
    pub fn has_primary_slot(&self) -> bool {
        self.slots.has_primary_slot()
    }

    /// Check if a subtask slot is available
    pub fn has_subtask_slot(&self) -> bool {
        self.slots.has_subtask_slot()
    }

    /// Dispatch a task for execution.
    ///
    /// The task will be spawned into the JoinSet and tracked in the dispatched set.
    /// Returns true if dispatch was successful, false if no slot available or already dispatched.
    pub fn dispatch<F, Fut>(&mut self, task_id: String, depth: usize, f: F) -> bool
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = TaskResult> + Send + 'static,
    {
        // Check if already dispatched
        if self.dispatched.contains(&task_id) {
            return false;
        }

        // Try to acquire appropriate slot
        let acquired = if depth == 0 {
            self.slots.acquire_primary()
        } else {
            self.slots.acquire_subtask()
        };

        if !acquired {
            return false;
        }

        // Track dispatched task
        self.dispatched.insert(task_id.clone());

        // Spawn the task
        self.join_set.spawn(async move {
            let result = f().await;
            result
        });

        true
    }

    /// Try to collect a completed task result.
    ///
    /// Returns:
    /// - `Some((task_id, depth, completed_task))` if a task completed
    /// - `None` if no tasks have completed
    pub fn try_collect(&mut self) -> Option<TaskResult> {
        self.join_set.try_join_next().transpose().ok().flatten()
    }

    /// Called when a task completes to update slot counts.
    ///
    /// This should be called after collecting a result from `try_collect`.
    pub fn on_task_completed(&mut self, task_id: &str, depth: usize) {
        self.dispatched.remove(task_id);
        if depth == 0 {
            self.slots.release_primary();
        } else {
            self.slots.release_subtask();
        }
    }

    /// Get the underlying slot pool (read-only)
    pub fn slots(&self) -> &SlotPool {
        &self.slots
    }

    /// Get the underlying slot pool (mutable)
    pub fn slots_mut(&mut self) -> &mut SlotPool {
        &mut self.slots
    }

    /// Get the number of dispatched tasks
    pub fn dispatched_count(&self) -> usize {
        self.dispatched.len()
    }

    /// Get a reference to the dispatched set
    pub fn dispatched_set(&self) -> &HashSet<String> {
        &self.dispatched
    }

    /// Remove a task from dispatched set without releasing slots.
    /// Use with caution - typically only for cleanup of stale entries.
    pub fn remove_dispatched(&mut self, task_id: &str) -> bool {
        self.dispatched.remove(task_id)
    }

    /// Clear all dispatched tasks and reset slot counts.
    /// Use only when aborting or resetting the scheduler.
    pub fn clear(&mut self) {
        self.dispatched.clear();
        self.join_set.shutdown();
        self.slots.primary_running = 0;
        self.slots.subtask_running = 0;
    }
}

/// Run the task pipeline.
///
/// This is a helper function that delegates to `super::run_task_pipeline_impl()`.
/// The actual implementation is in the orchestrator module.
pub async fn run_task_pipeline(
    _task_id: String,
    _depth: usize,
) -> Result<Option<Task>> {
    // This is a placeholder - the actual implementation is in orchestrator.rs
    // The TaskScheduler will use a closure that captures the necessary context
    // to call the actual run_task_pipeline function from orchestrator.rs
    unimplemented!(
        "run_task_pipeline should be called via dispatch closure with proper context"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_pool_unified() {
        // num_agents = 1 should give unified pool
        let pool = SlotPool::new(1);
        assert_eq!(pool.max_primary_slots(), 1);
        assert_eq!(pool.max_subtask_slots(), 1);

        // num_agents = 2 should also give unified pool
        let pool = SlotPool::new(2);
        assert_eq!(pool.max_primary_slots(), 2);
        assert_eq!(pool.max_subtask_slots(), 2);
    }

    #[test]
    fn test_slot_pool_split() {
        // num_agents = 4 should give split pool
        let pool = SlotPool::new(4);
        // div_ceil(4 / 2) = 2 primary, 4 - 2 = 2 subtask
        assert_eq!(pool.max_primary_slots(), 2);
        assert_eq!(pool.max_subtask_slots(), 2);

        // num_agents = 3 should give split pool
        let pool = SlotPool::new(3);
        // div_ceil(3 / 2) = 2 primary, 3 - 2 = 1 subtask
        assert_eq!(pool.max_primary_slots(), 2);
        assert_eq!(pool.max_subtask_slots(), 1);

        // num_agents = 5
        let pool = SlotPool::new(5);
        // div_ceil(5 / 2) = 3 primary, 5 - 3 = 2 subtask
        assert_eq!(pool.max_primary_slots(), 3);
        assert_eq!(pool.max_subtask_slots(), 2);
    }

    #[test]
    fn test_slot_acquire_release() {
        let mut pool = SlotPool::new(4);
        assert_eq!(pool.max_primary_slots(), 2);
        assert_eq!(pool.max_subtask_slots(), 2);

        // Acquire primary slots
        assert!(pool.acquire_primary());
        assert_eq!(pool.primary_running(), 1);
        assert!(pool.acquire_primary());
        assert_eq!(pool.primary_running(), 2);
        // No more primary slots
        assert!(!pool.acquire_primary());
        assert_eq!(pool.primary_running(), 2);

        // Acquire subtask slots
        assert!(pool.acquire_subtask());
        assert_eq!(pool.subtask_running(), 1);
        assert!(pool.acquire_subtask());
        assert_eq!(pool.subtask_running(), 2);
        // No more subtask slots
        assert!(!pool.acquire_subtask());
        assert_eq!(pool.subtask_running(), 2);

        // Total running
        assert_eq!(pool.total_running(), 4);

        // Release slots
        pool.release_primary();
        assert_eq!(pool.primary_running(), 1);
        pool.release_subtask();
        assert_eq!(pool.subtask_running(), 1);

        // Now we can acquire again
        assert!(pool.acquire_primary());
        assert!(pool.acquire_subtask());
    }

    #[test]
    fn test_slot_pool_edge_cases() {
        // num_agents = 0
        let pool = SlotPool::new(0);
        assert_eq!(pool.max_primary_slots(), 0);
        assert_eq!(pool.max_subtask_slots(), 0);
        assert!(!pool.has_primary_slot());
        assert!(!pool.has_subtask_slot());

        // Release on empty should not underflow
        let mut pool = SlotPool::new(1);
        pool.release_primary();
        assert_eq!(pool.primary_running(), 0);
        pool.release_subtask();
        assert_eq!(pool.subtask_running(), 0);
    }

    #[tokio::test]
    async fn test_scheduler_dispatch_tracking() {
        let mut scheduler = TaskScheduler::new(4);

        // Initially not dispatched
        assert!(!scheduler.is_dispatched("task-001"));

        // Dispatch a primary task
        let dispatched = scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });
        assert!(dispatched);
        assert!(scheduler.is_dispatched("task-001"));
        assert_eq!(scheduler.dispatched_count(), 1);
        assert_eq!(scheduler.slots().primary_running(), 1);

        // Dispatch a subtask
        let dispatched = scheduler.dispatch("task-001-1".to_string(), 1, || async {
            ("task-001-1".to_string(), 1usize, None)
        });
        assert!(dispatched);
        assert!(scheduler.is_dispatched("task-001-1"));
        assert_eq!(scheduler.dispatched_count(), 2);
        assert_eq!(scheduler.slots().subtask_running(), 1);

        // Try to dispatch same task again - should fail
        let dispatched = scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });
        assert!(!dispatched);
        assert_eq!(scheduler.dispatched_count(), 2);
    }

    #[tokio::test]
    async fn test_scheduler_slot_limits() {
        let mut scheduler = TaskScheduler::new(2);
        // With num_agents=2, we have unified pool: 2 primary, 2 subtask

        // Dispatch two primary tasks
        assert!(scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        }));
        assert!(scheduler.dispatch("task-002".to_string(), 0, || async {
            ("task-002".to_string(), 0usize, None)
        }));
        assert_eq!(scheduler.slots().primary_running(), 2);

        // Third primary should fail (no slots)
        assert!(!scheduler.has_primary_slot());
        assert!(!scheduler.dispatch("task-003".to_string(), 0, || async {
            ("task-003".to_string(), 0usize, None)
        }));

        // But subtask slots should still be available in unified pool
        assert!(scheduler.has_subtask_slot());
    }

    #[tokio::test]
    async fn test_scheduler_collect_and_complete() {
        let mut scheduler = TaskScheduler::new(4);

        // Dispatch a task
        scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });
        assert_eq!(scheduler.slots().primary_running(), 1);

        // Collect the result (it completes immediately since it's just async {})
        // Note: In practice, we need to poll the runtime
        // For unit testing purposes, we test the on_task_completed method directly
        scheduler.on_task_completed("task-001", 0);
        assert!(!scheduler.is_dispatched("task-001"));
        assert_eq!(scheduler.slots().primary_running(), 0);
    }

    #[tokio::test]
    async fn test_scheduler_remove_dispatched() {
        let mut scheduler = TaskScheduler::new(4);

        scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });
        assert!(scheduler.is_dispatched("task-001"));

        // Remove without releasing slot (for stale cleanup)
        assert!(scheduler.remove_dispatched("task-001"));
        assert!(!scheduler.is_dispatched("task-001"));
        // Slot count unchanged
        assert_eq!(scheduler.slots().primary_running(), 1);

        // Already removed, should return false
        assert!(!scheduler.remove_dispatched("task-001"));
    }

    #[tokio::test]
    async fn test_scheduler_is_empty() {
        let mut scheduler = TaskScheduler::new(4);

        // Initially empty
        assert!(scheduler.is_empty());

        // Dispatch a task
        scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });
        assert!(!scheduler.is_empty());

        // After completing
        scheduler.on_task_completed("task-001", 0);
        // dispatched is empty but join_set might still have the task
        // is_empty checks both dispatched and join_set
    }

    #[tokio::test]
    async fn test_scheduler_async_collect() {
        let mut scheduler = TaskScheduler::new(4);

        // Dispatch a task that completes immediately
        scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });

        // Give the runtime a chance to complete the task
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Try to collect
        if let Some(result) = scheduler.try_collect() {
            let (task_id, depth, _task) = result;
            assert_eq!(task_id, "task-001");
            assert_eq!(depth, 0);
            scheduler.on_task_completed(&task_id, depth);
        }

        assert!(scheduler.is_empty());
    }

    #[tokio::test]
    async fn test_scheduler_clear() {
        let mut scheduler = TaskScheduler::new(4);

        // Dispatch multiple tasks
        scheduler.dispatch("task-001".to_string(), 0, || async {
            ("task-001".to_string(), 0usize, None)
        });
        scheduler.dispatch("task-002".to_string(), 1, || async {
            ("task-002".to_string(), 1usize, None)
        });

        assert_eq!(scheduler.dispatched_count(), 2);
        assert_eq!(scheduler.slots().total_running(), 2);

        // Clear everything
        scheduler.clear();

        assert_eq!(scheduler.dispatched_count(), 0);
        assert_eq!(scheduler.slots().total_running(), 0);
    }
}
