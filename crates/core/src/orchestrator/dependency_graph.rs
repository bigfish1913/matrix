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
        // First check if this dep was split into subtasks
        // If split, only subtasks count - parent status is ignored
        if let Some(subtasks) = self.subtask_map.get(dep) {
            // All subtasks must be completed
            return subtasks.iter().all(|s| self.completed_ids.contains(s));
        }

        // Not split - check direct completion
        self.completed_ids.contains(dep)
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
        assert!(!graph.is_satisfied(&tasks[3]));
    }
}
