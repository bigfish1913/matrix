//! Task model and related types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Task status enum
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Task complexity enum
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Complexity {
    #[default]
    Unknown,
    Simple,
    Complex,
}

/// Task model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task ID, e.g. "task-001"
    pub id: String,
    /// Short title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Current status
    #[serde(default)]
    pub status: TaskStatus,
    /// Parent task ID (when split)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Split depth (max 3)
    #[serde(default)]
    pub depth: u32,
    /// Assessed complexity
    #[serde(default)]
    pub complexity: Complexity,
    /// Number of retries
    #[serde(default)]
    pub retries: u32,
    /// Claude session ID for resumption
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Execution result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Test failure context for retry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_failure_context: Option<String>,
    /// Test output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_result: Option<String>,
    /// Dependencies (task IDs)
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Completion timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Verification result
    #[serde(default)]
    pub verification_result: HashMap<String, serde_json::Value>,
    /// Whether tests passed
    #[serde(default)]
    pub test_passed: bool,
    /// Files modified during execution
    #[serde(default)]
    pub modified_files: Vec<String>,
    /// Whether this is a clarification question task
    #[serde(default)]
    pub is_clarification: bool,
    /// Task-level memory
    #[serde(default)]
    pub memory: TaskMemory,
    /// Task start time (for detecting stalled tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
}

impl Task {
    /// Create a new task with the given ID, title, and description
    pub fn new(id: String, title: String, description: String) -> Self {
        Self {
            id,
            title,
            description,
            status: TaskStatus::default(),
            parent_id: None,
            depth: 0,
            complexity: Complexity::default(),
            retries: 0,
            session_id: None,
            result: None,
            error: None,
            test_failure_context: None,
            test_result: None,
            depends_on: Vec::new(),
            created_at: Utc::now(),
            completed_at: None,
            verification_result: HashMap::new(),
            test_passed: false,
            modified_files: Vec::new(),
            is_clarification: false,
            memory: TaskMemory::default(),
            started_at: None,
        }
    }

    /// Create a subtask with parent reference
    pub fn subtask(
        id: String,
        title: String,
        description: String,
        parent_id: String,
        depth: u32,
    ) -> Self {
        let mut task = Self::new(id, title, description);
        task.parent_id = Some(parent_id);
        task.depth = depth;
        task.memory = TaskMemory::default();
        task.started_at = None;
        task
    }
}

/// Task-level memory
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TaskMemory {
    /// Lessons learned
    #[serde(default)]
    pub learnings: Vec<String>,
    /// Code change records
    #[serde(default)]
    pub code_changes: Vec<CodeChange>,
    /// Problem solutions
    #[serde(default)]
    pub solutions: Vec<ProblemSolution>,
    /// Key information (API endpoints, config paths, etc.)
    #[serde(default)]
    pub key_info: HashMap<String, String>,
}

impl TaskMemory {
    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.learnings.is_empty()
            && self.code_changes.is_empty()
            && self.solutions.is_empty()
            && self.key_info.is_empty()
    }

    /// Merge this task memory to global memory
    pub async fn merge_to_global(
        &self,
        global: &mut crate::GlobalMemory,
        task: &Task,
    ) -> crate::error::Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        let mut content = String::new();

        if !self.learnings.is_empty() {
            content.push_str("### 经验教训\n");
            for l in &self.learnings {
                content.push_str(&format!("- {}\n", l));
            }
        }

        if !self.code_changes.is_empty() {
            content.push_str("### 代码变更\n");
            for c in &self.code_changes {
                content.push_str(&format!("- `{}`: {}\n", c.path, c.description));
            }
        }

        if !self.solutions.is_empty() {
            content.push_str("### 问题解决\n");
            for s in &self.solutions {
                content.push_str(&format!("- 问题: {}\n  解决: {}\n", s.problem, s.solution));
            }
        }

        if !self.key_info.is_empty() {
            content.push_str("### 关键信息\n");
            for (k, v) in &self.key_info {
                content.push_str(&format!("- {}: {}\n", k, v));
            }
        }

        if !content.is_empty() {
            global
                .append(&format!("[{}] {}", task.id, task.title), &content)
                .await?;
        }

        Ok(())
    }
}

/// Code change record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeChange {
    pub path: String,
    pub description: String,
}

/// Problem solution record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProblemSolution {
    pub problem: String,
    pub solution: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_new() {
        let task = Task::new(
            "task-001".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );
        assert_eq!(task.id, "task-001");
        assert_eq!(task.title, "Test");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.depth, 0);
        assert!(task.parent_id.is_none());
    }

    #[test]
    fn test_task_status_serde() {
        let status = TaskStatus::InProgress;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"in_progress\"");
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_task_serde() {
        let task = Task::new(
            "task-001".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );
        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.title, task.title);
    }

    #[test]
    fn test_task_subtask() {
        let subtask = Task::subtask(
            "task-001-1".to_string(),
            "Subtask".to_string(),
            "Sub description".to_string(),
            "task-001".to_string(),
            1,
        );
        assert_eq!(subtask.parent_id, Some("task-001".to_string()));
        assert_eq!(subtask.depth, 1);
    }
}
