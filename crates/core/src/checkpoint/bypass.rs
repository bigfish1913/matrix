//! Bypass strategies for blocked tasks.

use serde::{Deserialize, Serialize};

/// Bypass strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy")]
pub enum BypassStrategy {
    /// Remove failed dependency, try to execute independently
    #[serde(rename = "remove_dependency")]
    RemoveDependency {
        task_id: String,
        remove_deps: Vec<String>,
    },
    /// Replace blocked task with new task
    #[serde(rename = "replace_task")]
    ReplaceTask {
        original_id: String,
        replacement: ReplacementTask,
    },
    /// Split task, skip failed dependency parts
    #[serde(rename = "split_and_skip")]
    SplitAndSkip {
        task_id: String,
        keep_parts: Vec<String>,
        skip_reason: String,
    },
    /// Mark as skipped, continue with subsequent tasks
    #[serde(rename = "mark_skipped")]
    MarkSkipped { task_id: String, reason: String },
}

/// Replacement task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementTask {
    pub title: String,
    pub description: String,
    pub depends_on: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bypass_strategy_serde() {
        let strategy = BypassStrategy::MarkSkipped {
            task_id: "task-001".to_string(),
            reason: "Cannot bypass".to_string(),
        };

        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("mark_skipped"));

        let parsed: BypassStrategy = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, BypassStrategy::MarkSkipped { .. }));
    }
}
