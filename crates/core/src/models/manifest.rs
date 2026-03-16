//! Manifest model for task tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Manifest for tracking overall progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Project goal
    pub goal: String,
    /// Total number of tasks
    pub total: usize,
    /// Number of completed tasks
    pub completed: usize,
    /// Number of failed tasks
    pub failed: usize,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// List of all task IDs
    pub tasks: Vec<String>,
}

impl Manifest {
    /// Create a new manifest with the given goal
    pub fn new(goal: String) -> Self {
        Self {
            goal,
            total: 0,
            completed: 0,
            failed: 0,
            updated_at: Utc::now(),
            tasks: Vec::new(),
        }
    }

    /// Update timestamp to now
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new("Test goal".to_string());
        assert_eq!(manifest.goal, "Test goal");
        assert_eq!(manifest.total, 0);
        assert!(manifest.tasks.is_empty());
    }

    #[test]
    fn test_manifest_serde() {
        let manifest = Manifest::new("Test goal".to_string());
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.goal, manifest.goal);
    }
}
