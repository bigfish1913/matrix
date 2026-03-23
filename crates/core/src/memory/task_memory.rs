//! Task-level memory extraction.

use crate::memory::GlobalMemory;
use crate::models::{Task, TaskMemory};
use std::path::Path;

/// Extension trait for TaskMemory operations
pub trait TaskMemoryExt {
    /// Check if memory is empty
    fn is_empty(&self) -> bool;

    /// Format for dependent task context
    fn for_dependency_context(&self) -> String;
}

impl TaskMemoryExt for TaskMemory {
    fn is_empty(&self) -> bool {
        self.learnings.is_empty()
            && self.code_changes.is_empty()
            && self.solutions.is_empty()
            && self.key_info.is_empty()
    }

    fn for_dependency_context(&self) -> String {
        let mut parts = Vec::new();

        if !self.key_info.is_empty() {
            parts.push("Key Info:".to_string());
            for (k, v) in &self.key_info {
                parts.push(format!("  {}: {}", k, v));
            }
        }

        if !self.solutions.is_empty() {
            parts.push("Notes:".to_string());
            for s in &self.solutions {
                parts.push(format!("  - {}", s.problem));
            }
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_task_memory_is_empty() {
        let empty = TaskMemory::default();
        assert!(empty.is_empty());

        let mut non_empty = TaskMemory::default();
        non_empty.learnings.push("learning".to_string());
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_for_dependency_context() {
        let mut memory = TaskMemory::default();
        memory
            .key_info
            .insert("api".to_string(), "/api/v1".to_string());

        let ctx = memory.for_dependency_context();
        assert!(ctx.contains("Key Info"));
        assert!(ctx.contains("api: /api/v1"));
    }
}
