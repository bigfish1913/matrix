//! HealthMonitor - detects stalled/blocked tasks and manages health warnings.
//!
//! This module provides health monitoring for the task orchestration system,
//! detecting tasks that are stalled (no activity for too long) or blocked
//! (waiting on failed dependencies).

use crate::error::Result;
use crate::models::TaskStatus;
use crate::store::TaskStore;
use chrono::Utc;
use std::time::Instant;
use tracing::warn;

/// Configuration for the health monitor.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Time in minutes after which a task with no activity is considered stalled.
    pub stall_threshold_minutes: i64,
    /// Minimum time in seconds between repeated warnings for the same issue.
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

/// Information about a blocked task.
#[derive(Debug, Clone)]
pub struct BlockedTask {
    /// The task ID that is blocked.
    pub task_id: String,
    /// List of task IDs that are blocking this task.
    pub blocked_by: Vec<String>,
}

/// Health monitor for detecting stalled and blocked tasks.
#[derive(Debug)]
pub struct HealthMonitor {
    /// Configuration for health checks.
    config: HealthConfig,
    /// Last time a warning was emitted (for throttling).
    last_warning: Option<Instant>,
}

impl HealthMonitor {
    /// Create a new HealthMonitor with default configuration.
    pub fn new() -> Self {
        Self {
            config: HealthConfig::default(),
            last_warning: None,
        }
    }

    /// Create a new HealthMonitor with custom configuration.
    pub fn with_config(config: HealthConfig) -> Self {
        Self {
            config,
            last_warning: None,
        }
    }

    /// Check for stalled tasks and reset them to Pending.
    ///
    /// A task is considered stalled if:
    /// - It has status `InProgress`
    /// - It has had no activity for `stall_threshold_minutes` minutes
    ///
    /// Returns a list of task IDs that were reset.
    pub async fn check_stalled(&self, store: &TaskStore) -> Result<Vec<String>> {
        let all_tasks = store.all_tasks().await?;
        let mut stalled_ids = Vec::new();
        let now = Utc::now();

        for task in &all_tasks {
            if task.status != TaskStatus::InProgress {
                continue;
            }

            if Self::is_stalled(task, &self.config, now) {
                stalled_ids.push(task.id.clone());
            }
        }

        // Reset stalled tasks
        for task_id in &stalled_ids {
            let mut task = store.load_task(task_id).await?;
            task.status = TaskStatus::Pending;
            task.started_at = None;
            task.last_activity_at = None;
            store.save_task(&task).await?;
            warn!(task_id = %task_id, "Reset stalled task to Pending");
        }

        Ok(stalled_ids)
    }

    /// Check and warn about blocked tasks.
    ///
    /// A task is blocked if it depends on tasks that have failed.
    /// Warnings are throttled to avoid spam.
    pub fn check_blocked(&mut self, blocked: &[BlockedTask]) {
        if blocked.is_empty() {
            return;
        }

        if self.should_warn() {
            for blocked_task in blocked {
                warn!(
                    task_id = %blocked_task.task_id,
                    blocked_by = ?blocked_task.blocked_by,
                    "Task blocked by failed dependencies"
                );
            }
            self.last_warning = Some(Instant::now());
        }
    }

    /// Check if a task is stalled based on its activity timestamps.
    fn is_stalled(
        task: &crate::models::Task,
        config: &HealthConfig,
        now: chrono::DateTime<Utc>,
    ) -> bool {
        // Use last_activity_at if available, otherwise fall back to started_at
        let activity_time = match task.last_activity_at.or(task.started_at) {
            Some(t) => t,
            None => {
                // Task is InProgress but has no activity time - definitely stalled
                return true;
            }
        };

        let elapsed = now.signed_duration_since(activity_time);
        elapsed.num_minutes() > config.stall_threshold_minutes
    }

    /// Check if a task is stalled (public helper for external use).
    pub fn is_task_stalled(&self, task: &crate::models::Task) -> bool {
        Self::is_stalled(task, &self.config, Utc::now())
    }

    /// Check if enough time has passed since the last warning to emit a new one.
    pub fn should_warn(&self) -> bool {
        match self.last_warning {
            None => true,
            Some(last) => {
                let elapsed = last.elapsed().as_secs();
                elapsed >= self.config.warning_throttle_secs
            }
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &HealthConfig {
        &self.config
    }

    /// Reset the warning throttle (useful for testing or forced warnings).
    pub fn reset_warning_throttle(&mut self) {
        self.last_warning = None;
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
    use crate::models::Task;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_health_config_default() {
        let config = HealthConfig::default();
        assert_eq!(config.stall_threshold_minutes, 10);
        assert_eq!(config.warning_throttle_secs, 30);
    }

    #[test]
    fn test_health_monitor_new() {
        let monitor = HealthMonitor::new();
        assert_eq!(monitor.config().stall_threshold_minutes, 10);
        assert_eq!(monitor.config().warning_throttle_secs, 30);
        assert!(monitor.last_warning.is_none());
    }

    #[test]
    fn test_health_monitor_with_config() {
        let config = HealthConfig {
            stall_threshold_minutes: 5,
            warning_throttle_secs: 15,
        };
        let monitor = HealthMonitor::with_config(config);
        assert_eq!(monitor.config().stall_threshold_minutes, 5);
        assert_eq!(monitor.config().warning_throttle_secs, 15);
    }

    #[test]
    fn test_should_warn_throttling() {
        let mut monitor = HealthMonitor::new();

        // Initially should warn
        assert!(monitor.should_warn());

        // Simulate a warning being issued
        monitor.last_warning = Some(Instant::now());

        // Should not warn immediately after
        assert!(!monitor.should_warn());

        // After resetting throttle, should warn again
        monitor.reset_warning_throttle();
        assert!(monitor.should_warn());
    }

    #[tokio::test]
    async fn test_should_warn_after_throttle_period() {
        let mut monitor = HealthMonitor::with_config(HealthConfig {
            stall_threshold_minutes: 10,
            warning_throttle_secs: 0, // 0 seconds for immediate test
        });

        // Initially should warn
        assert!(monitor.should_warn());

        // Set last warning
        monitor.last_warning = Some(Instant::now());

        // With 0 second throttle, need a tiny delay
        sleep(Duration::from_millis(10)).await;

        // Should warn after throttle period
        assert!(monitor.should_warn());
    }

    #[test]
    fn test_is_stalled_no_activity_time() {
        let config = HealthConfig::default();
        let mut task = Task::new(
            "task-001".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );
        task.status = TaskStatus::InProgress;
        // No started_at or last_activity_at

        assert!(HealthMonitor::is_stalled(&task, &config, Utc::now()));
    }

    #[test]
    fn test_is_stalled_recent_activity() {
        let config = HealthConfig::default();
        let mut task = Task::new(
            "task-001".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );
        task.status = TaskStatus::InProgress;
        task.last_activity_at = Some(Utc::now());

        // Should not be stalled with recent activity
        assert!(!HealthMonitor::is_stalled(&task, &config, Utc::now()));
    }

    #[test]
    fn test_is_stalled_old_activity() {
        let config = HealthConfig {
            stall_threshold_minutes: 5,
            warning_throttle_secs: 30,
        };
        let mut task = Task::new(
            "task-001".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );
        task.status = TaskStatus::InProgress;

        // Set activity time to 10 minutes ago
        let old_time = Utc::now() - chrono::Duration::minutes(10);
        task.last_activity_at = Some(old_time);

        // Should be stalled
        assert!(HealthMonitor::is_stalled(&task, &config, Utc::now()));
    }

    #[test]
    fn test_blocked_task_struct() {
        let blocked = BlockedTask {
            task_id: "task-001".to_string(),
            blocked_by: vec!["task-002".to_string(), "task-003".to_string()],
        };

        assert_eq!(blocked.task_id, "task-001");
        assert_eq!(blocked.blocked_by.len(), 2);
    }

    #[test]
    fn test_check_blocked_empty_list() {
        let mut monitor = HealthMonitor::new();
        // Should not panic with empty list
        monitor.check_blocked(&[]);
        assert!(monitor.last_warning.is_none());
    }

    #[test]
    fn test_check_blocked_issues_warning() {
        let mut monitor = HealthMonitor::new();
        let blocked = vec![BlockedTask {
            task_id: "task-001".to_string(),
            blocked_by: vec!["task-002".to_string()],
        }];

        monitor.check_blocked(&blocked);
        assert!(monitor.last_warning.is_some());
    }

    #[test]
    fn test_check_blocked_throttled() {
        let mut monitor = HealthMonitor::new();

        // First call should issue warning
        let blocked = vec![BlockedTask {
            task_id: "task-001".to_string(),
            blocked_by: vec!["task-002".to_string()],
        }];
        monitor.check_blocked(&blocked);
        assert!(monitor.last_warning.is_some());

        // Second call immediately should be throttled (no new warning time)
        let first_warning = monitor.last_warning;
        monitor.check_blocked(&blocked);
        assert_eq!(monitor.last_warning, first_warning);
    }
}
