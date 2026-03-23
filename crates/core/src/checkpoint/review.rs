//! Review report generation for progress tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Progress statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressStats {
    pub total: usize,
    pub completed: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub failed: usize,
    pub skipped: usize,
    pub completion_percent: f64,
}

/// Upcoming task for review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingTask {
    pub id: String,
    pub title: String,
    pub depth: u32,
    pub depends_on: Vec<String>,
}

/// Detected issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Issue {
    CircularDependency {
        cycle: Vec<String>,
    },
    MissingDependency {
        task_id: String,
        missing: String,
    },
    Blocked {
        task_id: String,
        blocked_by: Vec<String>,
    },
    Stalled {
        task_id: String,
        duration_secs: u64,
    },
}

/// Progress review report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    pub timestamp: DateTime<Utc>,
    pub progress: ProgressStats,
    pub upcoming_tasks: Vec<UpcomingTask>,
    pub issues: Vec<Issue>,
    pub eta: Option<Duration>,
}

impl ReviewReport {
    /// Format to readable text
    pub fn format(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str("══════════════════════════════════════════════════════\n");
        output.push_str("  📊 进度汇报\n");
        output.push_str("══════════════════════════════════════════════════════\n\n");

        // Statistics
        let p = &self.progress;
        output.push_str(&format!(
            "📈 统计: {}/{} 已完成 ({:.0}%) | {} 待处理 | {} 执行中 | {} 失败\n",
            p.completed, p.total, p.completion_percent, p.pending, p.in_progress, p.failed
        ));

        // Time
        if let Some(eta) = self.eta {
            output.push_str(&format!("⏱️  预计剩余: {}\n", format_duration(eta)));
        }
        output.push('\n');

        // Upcoming tasks
        if !self.upcoming_tasks.is_empty() {
            output.push_str("📋 即将执行:\n");
            for task in self.upcoming_tasks.iter().take(10) {
                let deps = if task.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" (等待: {})", task.depends_on.join(", "))
                };
                output.push_str(&format!("  • [{}] {}{}\n", task.id, task.title, deps));
            }
            output.push('\n');
        }

        // Issues
        if !self.issues.is_empty() {
            output.push_str("⚠️  问题:\n");
            for issue in &self.issues {
                match issue {
                    Issue::CircularDependency { cycle } => {
                        output.push_str(&format!("  • 循环依赖: {}\n", cycle.join(" -> ")));
                    }
                    Issue::MissingDependency { task_id, missing } => {
                        output.push_str(&format!("  • [{}] 缺少依赖: {}\n", task_id, missing));
                    }
                    Issue::Blocked {
                        task_id,
                        blocked_by,
                    } => {
                        output.push_str(&format!(
                            "  • [{}] 被阻塞: {}\n",
                            task_id,
                            blocked_by.join(", ")
                        ));
                    }
                    Issue::Stalled {
                        task_id,
                        duration_secs,
                    } => {
                        output.push_str(&format!("  • [{}] 超时 {}秒\n", task_id, duration_secs));
                    }
                }
            }
            output.push('\n');
        }

        output.push_str("══════════════════════════════════════════════════════\n");

        output
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let m = secs / 60;
    let s = secs % 60;
    if m > 60 {
        let h = m / 60;
        let m = m % 60;
        format!("{}h{}m", h, m)
    } else {
        format!("{}m{}s", m, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_report_format() {
        let report = ReviewReport {
            timestamp: Utc::now(),
            progress: ProgressStats {
                total: 10,
                completed: 5,
                pending: 3,
                in_progress: 1,
                failed: 1,
                skipped: 0,
                completion_percent: 50.0,
            },
            upcoming_tasks: vec![UpcomingTask {
                id: "task-006".to_string(),
                title: "Test task".to_string(),
                depth: 0,
                depends_on: vec![],
            }],
            issues: vec![],
            eta: Some(Duration::from_secs(300)),
        };

        let output = report.format();
        assert!(output.contains("5/10"));
        assert!(output.contains("50%"));
        assert!(output.contains("task-006"));
    }
}
