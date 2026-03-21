//! Task-level memory extraction.

use crate::agent::ClaudeRunner;
use crate::error::Result;
use crate::models::{Task, TaskMemory};
use crate::memory::GlobalMemory;
use std::path::Path;

/// Extension trait for TaskMemory operations
pub trait TaskMemoryExt {
    /// Extract memory from task execution result
    async fn extract_from_result(
        runner: &ClaudeRunner,
        workspace: &Path,
        task: &Task,
    ) -> Result<TaskMemory>;

    /// Merge to global memory
    async fn merge_to_global(&self, global: &mut GlobalMemory, task: &Task) -> Result<()>;

    /// Context for dependent tasks
    fn for_dependency_context(&self) -> String;
}

impl TaskMemoryExt for TaskMemory {
    async fn extract_from_result(
        runner: &ClaudeRunner,
        workspace: &Path,
        task: &Task,
    ) -> Result<Self> {
        let result_text = task.result.as_deref().unwrap_or("(no execution result)");

        let prompt = format!(
            r#"You are a technical documentation writer updating project memory.

Current task:
- Title: {}
- Description: {}
- Execution result: {}

Please extract the following information (JSON format):
{{
  "learnings": ["lesson1", "lesson2"],
  "code_changes": [
    {{"path": "src/auth.rs", "description": "Added user auth"}}
  ],
  "solutions": [
    {{"problem": "Compile error", "solution": "Added missing trait"}}
  ],
  "key_info": {{
    "api_endpoint": "/api/v1/auth"
  }}
}}

Return empty object {{}} if no important info. Output JSON only, no other content."#,
            task.title, task.description, result_text
        );

        let result = runner.call(&prompt, workspace, Some(60), None, None).await?;

        if result.is_error {
            return Ok(Self::default());
        }

        // Try to parse JSON
        let memory: Self = match serde_json::from_str(&result.text) {
            Ok(m) => m,
            Err(_) => Self::default(),
        };

        Ok(memory)
    }

    async fn merge_to_global(&self, global: &mut GlobalMemory, task: &Task) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        let mut content = String::new();

        if !self.learnings.is_empty() {
            content.push_str("### Learnings\n");
            for l in &self.learnings {
                content.push_str(&format!("- {}\n", l));
            }
        }

        if !self.code_changes.is_empty() {
            content.push_str("### Code Changes\n");
            for c in &self.code_changes {
                content.push_str(&format!("- `{}`: {}\n", c.path, c.description));
            }
        }

        if !self.solutions.is_empty() {
            content.push_str("### Solutions\n");
            for s in &self.solutions {
                content.push_str(&format!("- Problem: {}\n  Solution: {}\n", s.problem, s.solution));
            }
        }

        if !self.key_info.is_empty() {
            content.push_str("### Key Info\n");
            for (k, v) in &self.key_info {
                content.push_str(&format!("- {}: {}\n", k, v));
            }
        }

        if !content.is_empty() {
            global.append(&format!("[{}] {}", task.id, task.title), &content).await?;
        }

        Ok(())
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
        memory.key_info.insert("api".to_string(), "/api/v1".to_string());

        let ctx = memory.for_dependency_context();
        assert!(ctx.contains("Key Info"));
        assert!(ctx.contains("api: /api/v1"));
    }
}
