//! Global memory management.

use crate::error::Result;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Global memory manager
pub struct GlobalMemory {
    path: std::path::PathBuf,
    cache: Option<String>,
}

impl GlobalMemory {
    pub fn new(workspace: &Path) -> Self {
        let path = workspace.join(".claude").join("memory.md");
        Self { path, cache: None }
    }

    /// Read global memory (cached)
    pub fn read(&mut self) -> &str {
        if self.cache.is_none() {
            self.cache = std::fs::read_to_string(&self.path).ok();
        }
        self.cache.as_deref().unwrap_or("# Project Memory\n\n")
    }

    /// Append content to global memory
    pub async fn append(&mut self, section: &str, content: &str) -> Result<()> {
        let new_content = format!(
            "\n\n---\n## {}\n\n{}\n",
            section, content
        );

        // Ensure directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Append to file
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(new_content.as_bytes()).await?;
        file.flush().await?;

        // Clear cache
        self.cache = None;

        Ok(())
    }

    /// For prompt (truncate to max size)
    pub fn for_prompt(&mut self, max_size: usize) -> String {
        let content = self.read();
        if content.len() > max_size {
            format!("{}...\n[truncated]", &content[..max_size])
        } else {
            content.to_string()
        }
    }

    /// Get memory file path
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_global_memory_append() {
        let dir = tempdir().unwrap();
        let mut memory = GlobalMemory::new(dir.path());

        memory.append("Test Section", "Test content").await.unwrap();

        let content = memory.read();
        assert!(content.contains("Test Section"));
        assert!(content.contains("Test content"));
    }

    #[tokio::test]
    async fn test_global_memory_for_prompt() {
        let dir = tempdir().unwrap();
        let mut memory = GlobalMemory::new(dir.path());

        memory.append("Section", "Content").await.unwrap();

        let truncated = memory.for_prompt(10);
        assert!(truncated.contains("[truncated]") || truncated.len() <= 20);
    }
}
