//! Checkpoint manager for task orchestration.

use crate::checkpoint::bypass::BypassStrategy;
use crate::checkpoint::review::{Issue, ProgressStats, ReviewReport, UpcomingTask};
use crate::config::CheckpointConfig;
use crate::error::Result;
use crate::models::{Task, TaskStatus};
use crate::store::TaskStore;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// Blocked task
#[derive(Debug, Clone)]
pub struct BlockedTask {
    pub task_id: String,
    pub blocked_by: Vec<String>,
}

/// Checkpoint check result
#[derive(Debug, Default)]
pub struct CheckpointResult {
    /// Dependency/cycle warnings
    pub warnings: Vec<String>,
    /// Tasks blocked by failed dependencies
    pub blocked: Vec<BlockedTask>,
    /// Tasks stalled for too long
    pub stalled: Vec<String>,
    /// Whether execution can proceed
    pub can_proceed: bool,
}

/// Checkpoint manager
pub struct CheckpointManager {
    store: Arc<TaskStore>,
    config: CheckpointConfig,
    /// Tasks completed since last review
    tasks_since_review: usize,
    /// Last review milestone (for percentage mode)
    last_review_at: usize,
    /// Last review time
    last_review_time: Option<Instant>,
    /// Project start time (for ETA calculation)
    start_time: Option<Instant>,
}

impl CheckpointManager {
    pub fn new(store: Arc<TaskStore>, config: CheckpointConfig) -> Self {
        Self {
            store,
            config,
            tasks_since_review: 0,
            last_review_at: 0,
            last_review_time: None,
            start_time: None,
        }
    }

    /// Set start time
    pub fn set_start_time(&mut self) {
        self.start_time = Some(Instant::now());
        // Also initialize last_review_time so timeout trigger works from the start
        self.last_review_time = Some(Instant::now());
    }

    /// Called when a task completes
    pub fn on_task_completed(&mut self) {
        self.tasks_since_review += 1;
    }

    /// Determine if review is needed based on config
    pub fn should_review(&self, completed: usize, total: usize) -> bool {
        // Condition 1: Task count trigger
        if let Some(interval) = self.config.review_interval {
            if self.tasks_since_review >= interval {
                return true;
            }
        }

        // Condition 2: Percentage trigger
        if let Some(percent) = self.config.review_percent {
            let threshold = (total as f64 * percent as f64 / 100.0) as usize;
            let milestone = completed / threshold.max(1);
            if milestone > self.last_review_at {
                return true;
            }
        }

        // Condition 3: Time timeout trigger
        if let Some(timeout_mins) = self.config.review_timeout_mins {
            if let Some(last_time) = self.last_review_time {
                let elapsed = last_time.elapsed();
                if elapsed >= Duration::from_secs(timeout_mins * 60) {
                    return true;
                }
            }
        }

        false
    }

    /// Called before each batch of task scheduling
    pub async fn pre_batch_checkpoint(&mut self) -> Result<CheckpointResult> {
        let mut result = CheckpointResult::default();

        // 1. Validate dependency graph
        let warnings = self.store.validate_dependencies().await;
        result.warnings = warnings;

        // 2. Check for blocked tasks (dependencies failed)
        result.blocked = self.find_blocked_tasks().await?;

        // 3. Check for stalled tasks (in_progress too long)
        result.stalled = self.find_stalled_tasks().await?;

        result.can_proceed = result.blocked.is_empty() || !self.config.validate_before_batch;

        Ok(result)
    }

    /// Find blocked tasks (tasks depending on failed tasks)
    async fn find_blocked_tasks(&self) -> Result<Vec<BlockedTask>> {
        let tasks = self.store.all_tasks().await?;

        // Collect all failed task IDs
        let failed_ids: HashSet<_> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Failed)
            .map(|t| t.id.clone())
            .collect();

        // Find pending tasks that depend on failed tasks
        let blocked: Vec<BlockedTask> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .filter(|t| t.depends_on.iter().any(|d| failed_ids.contains(d.as_str())))
            .map(|t| BlockedTask {
                task_id: t.id.clone(),
                blocked_by: t
                    .depends_on
                    .iter()
                    .filter(|d| failed_ids.contains(d.as_str()))
                    .cloned()
                    .collect(),
            })
            .collect();

        Ok(blocked)
    }

    /// Find stalled tasks (no activity for too long)
    async fn find_stalled_tasks(&self) -> Result<Vec<String>> {
        let tasks = self.store.all_tasks().await?;
        let threshold = Duration::from_secs(self.config.stalled_threshold_secs);
        let now = Utc::now();

        let stalled: Vec<String> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .filter_map(|t| {
                // Use last_activity_at if available, otherwise fall back to started_at
                let activity_time = t.last_activity_at.or(t.started_at)?;
                let elapsed = now.signed_duration_since(activity_time).to_std().ok()?;
                if elapsed > threshold {
                    Some(t.id.clone())
                } else {
                    None
                }
            })
            .collect();

        Ok(stalled)
    }

    /// Generate progress review
    pub async fn generate_review(&mut self) -> Result<ReviewReport> {
        self.tasks_since_review = 0;
        self.last_review_time = Some(Instant::now());

        let tasks = self.store.all_tasks().await?;

        // Statistics
        let total = tasks.len();
        let completed = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        let pending = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count();
        let in_progress = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .count();
        let failed = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Failed)
            .count();
        let skipped = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Skipped)
            .count();

        let completion_percent = if total > 0 {
            (completed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Collect completed task IDs
        let completed_ids: HashSet<_> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped)
            .map(|t| t.id.clone())
            .collect();

        // Upcoming tasks (pending with dependencies satisfied)
        let upcoming: Vec<UpcomingTask> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .filter(|t| t.depends_on.iter().all(|d| completed_ids.contains(d)))
            .take(10)
            .map(|t| UpcomingTask {
                id: t.id.clone(),
                title: t.title.clone(),
                depth: t.depth,
                depends_on: t.depends_on.clone(),
            })
            .collect();

        // Issue detection
        let issues = self.collect_issues(&tasks).await?;

        // ETA calculation
        let eta = self.calculate_eta(completed, total);

        Ok(ReviewReport {
            timestamp: Utc::now(),
            progress: ProgressStats {
                total,
                completed,
                pending,
                in_progress,
                failed,
                skipped,
                completion_percent,
            },
            upcoming_tasks: upcoming,
            issues,
            eta,
        })
    }

    async fn collect_issues(&self, tasks: &[Task]) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();

        // Dependency warnings
        for warning in &self.store.validate_dependencies().await {
            if warning.contains("Circular") {
                issues.push(Issue::CircularDependency {
                    cycle: warning.split(" -> ").map(String::from).collect(),
                });
            } else if warning.contains("missing") {
                issues.push(Issue::MissingDependency {
                    task_id: "unknown".to_string(),
                    missing: warning.clone(),
                });
            }
        }

        // Blocked tasks
        for blocked in self.find_blocked_tasks().await? {
            issues.push(Issue::Blocked {
                task_id: blocked.task_id,
                blocked_by: blocked.blocked_by,
            });
        }

        // Stalled tasks
        for task_id in self.find_stalled_tasks().await? {
            issues.push(Issue::Stalled {
                task_id,
                duration_secs: self.config.stalled_threshold_secs,
            });
        }

        Ok(issues)
    }

    fn calculate_eta(&self, completed: usize, total: usize) -> Option<Duration> {
        if completed == 0 || total == 0 || completed >= total {
            return None;
        }

        let start = self.start_time?;
        let elapsed = start.elapsed();
        let avg_time_per_task = elapsed / completed as u32;
        let remaining = total - completed;

        Some(avg_time_per_task * remaining as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_should_review_interval() {
        let dir = tempdir().unwrap();
        let store = Arc::new(TaskStore::new(dir.path().to_path_buf()).await.unwrap());
        let mut config = CheckpointConfig::default();
        config.review_interval = Some(3);
        config.review_timeout_mins = None;
        config.review_percent = None;

        let mut manager = CheckpointManager::new(store, config);

        // 2 tasks completed, should not review
        manager.on_task_completed();
        manager.on_task_completed();
        assert!(!manager.should_review(2, 10));

        // 3rd task completed, should review
        manager.on_task_completed();
        assert!(manager.should_review(3, 10));
    }

    #[tokio::test]
    async fn test_should_review_timeout() {
        let dir = tempdir().unwrap();
        let store = Arc::new(TaskStore::new(dir.path().to_path_buf()).await.unwrap());
        let mut config = CheckpointConfig::default();
        config.review_interval = Some(1000); // Large, won't trigger
        config.review_timeout_mins = Some(0); // Immediate timeout
        config.review_percent = None;

        let mut manager = CheckpointManager::new(store, config);
        manager.last_review_time = Some(Instant::now() - Duration::from_secs(1));

        // Timeout condition met
        assert!(manager.should_review(1, 100));
    }
}
