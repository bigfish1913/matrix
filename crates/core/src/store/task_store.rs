//! TaskStore - persistent storage for tasks.

use crate::error::{Error, Result};
use crate::models::{Manifest, Task, TaskStatus};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// Task storage manager
pub struct TaskStore {
    tasks_dir: PathBuf,
    manifest_path: PathBuf,
}

impl TaskStore {
    /// Create a new TaskStore
    pub async fn new(tasks_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&tasks_dir).await?;
        let manifest_path = tasks_dir.join("manifest.json");
        Ok(Self {
            tasks_dir,
            manifest_path,
        })
    }

    /// Save a task to disk
    pub async fn save_task(&self, task: &Task) -> Result<()> {
        let path = self.tasks_dir.join(format!("{}.json", task.id));
        let content = serde_json::to_string_pretty(task)?;
        fs::write(&path, content).await?;
        debug!(task_id = %task.id, "Task saved");
        Ok(())
    }

    /// Load a task by ID
    pub async fn load_task(&self, id: &str) -> Result<Task> {
        let path = self.tasks_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| Error::TaskNotFound(format!("{}: {}", id, e)))?;
        let task: Task = serde_json::from_str(&content)?;
        Ok(task)
    }

    /// Get all tasks
    pub async fn all_tasks(&self) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        let mut entries: Vec<_> = Vec::new();

        let mut dir = fs::read_dir(&self.tasks_dir).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false)
                && path
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with("task-"))
                    .unwrap_or(false)
            {
                entries.push(path);
            }
        }

        entries.sort();

        for path in entries {
            let content = fs::read_to_string(&path).await?;
            if let Ok(task) = serde_json::from_str::<Task>(&content) {
                tasks.push(task);
            }
        }

        Ok(tasks)
    }

    /// Get pending tasks
    pub async fn pending_tasks(&self) -> Result<Vec<Task>> {
        let tasks = self.all_tasks().await?;
        Ok(tasks
            .into_iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect())
    }

    /// Count tasks by status
    pub async fn count(&self, status: TaskStatus) -> Result<usize> {
        let tasks = self.all_tasks().await?;
        Ok(tasks.iter().filter(|t| t.status == status).count())
    }

    /// Get total number of tasks
    pub async fn total(&self) -> Result<usize> {
        Ok(self.all_tasks().await?.len())
    }

    /// Validate dependency graph
    pub async fn validate_dependencies(&self) -> Vec<String> {
        let tasks = match self.all_tasks().await {
            Ok(t) => t,
            Err(_) => return vec!["Failed to load tasks".to_string()],
        };

        let mut warnings = Vec::new();
        let task_ids: HashSet<_> = tasks.iter().map(|t| t.id.clone()).collect();

        // Build a map of task_id -> subtask_ids (e.g., task-006 -> [task-006-1, task-006-2])
        let mut subtask_map: HashMap<String, Vec<String>> = HashMap::new();
        for task in &tasks {
            // Check if this is a subtask (contains a hyphen followed by a number)
            if let Some(pos) = task.id.rfind('-') {
                if pos > 0 {
                    let parent_id = &task.id[..pos];
                    // Check if the suffix after the last hyphen is a number
                    if task.id[pos + 1..].parse::<u32>().is_ok() {
                        subtask_map
                            .entry(parent_id.to_string())
                            .or_default()
                            .push(task.id.clone());
                    }
                }
            }
        }

        // Check for missing dependencies
        for task in &tasks {
            for dep in &task.depends_on {
                if !task_ids.contains(dep) {
                    // Check if this dependency has been split into subtasks
                    if let Some(subtasks) = subtask_map.get(dep) {
                        // Check if all subtasks exist (subtasks themselves are in task_ids)
                        let all_subtasks_exist = subtasks.iter().all(|s| task_ids.contains(s));
                        if !all_subtasks_exist {
                            warnings.push(format!("[{}] depends on [{}] which was split but some subtasks are missing", task.id, dep));
                        }
                        // If all subtasks exist, dependency is satisfied via subtasks - no warning needed
                    } else {
                        warnings.push(format!("[{}] depends on missing task [{}]", task.id, dep));
                    }
                }
            }
        }

        // Detect cycles using DFS with coloring
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Grey,
            Black,
        }

        let mut colors: HashMap<String, Color> =
            tasks.iter().map(|t| (t.id.clone(), Color::White)).collect();

        let dep_map: HashMap<String, Vec<String>> = tasks
            .iter()
            .map(|t| (t.id.clone(), t.depends_on.clone()))
            .collect();

        fn dfs(
            task_id: &str,
            path: &[String],
            colors: &mut HashMap<String, Color>,
            dep_map: &HashMap<String, Vec<String>>,
            warnings: &mut Vec<String>,
        ) {
            colors.insert(task_id.to_string(), Color::Grey);

            if let Some(deps) = dep_map.get(task_id) {
                for dep in deps {
                    if !colors.contains_key(dep) {
                        continue;
                    }
                    match colors.get(dep) {
                        Some(Color::Grey) => {
                            let mut cycle: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
                            cycle.push(dep);
                            warnings.push(format!(
                                "Circular dependency detected: {}",
                                cycle.join(" -> ")
                            ));
                        }
                        Some(Color::White) => {
                            let mut new_path = path.to_vec();
                            new_path.push(dep.clone());
                            dfs(dep, &new_path, colors, dep_map, warnings);
                        }
                        _ => {}
                    }
                }
            }

            colors.insert(task_id.to_string(), Color::Black);
        }

        for task in &tasks {
            if colors.get(&task.id) == Some(&Color::White) {
                dfs(
                    &task.id,
                    std::slice::from_ref(&task.id),
                    &mut colors,
                    &dep_map,
                    &mut warnings,
                );
            }
        }

        warnings
    }

    /// Save manifest
    pub async fn save_manifest(&self, goal: &str) -> Result<()> {
        let tasks = self.all_tasks().await?;
        let completed = self.count(TaskStatus::Completed).await?;
        let failed = self.count(TaskStatus::Failed).await?;

        let manifest = Manifest {
            goal: goal.to_string(),
            total: tasks.len(),
            completed,
            failed,
            updated_at: chrono::Utc::now(),
            tasks: tasks.iter().map(|t| t.id.clone()).collect(),
        };

        let content = serde_json::to_string_pretty(&manifest)?;
        fs::write(&self.manifest_path, content).await?;
        Ok(())
    }

    /// Load manifest
    pub async fn load_manifest(&self) -> Result<Option<Manifest>> {
        if !self.manifest_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&self.manifest_path).await?;
        let manifest: Manifest = serde_json::from_str(&content)?;
        Ok(Some(manifest))
    }

    /// Clear all tasks (for fresh start)
    pub async fn clear(&self) -> Result<()> {
        let mut entries: Vec<_> = Vec::new();

        let mut dir = fs::read_dir(&self.tasks_dir).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                entries.push(path);
            }
        }

        for path in entries {
            fs::remove_file(&path).await?;
        }

        if self.manifest_path.exists() {
            fs::remove_file(&self.manifest_path).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_task_store_save_and_load() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let task = Task::new(
            "task-001".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );
        store.save_task(&task).await.unwrap();

        let loaded = store.load_task("task-001").await.unwrap();
        assert_eq!(loaded.id, "task-001");
        assert_eq!(loaded.title, "Test");
    }

    #[tokio::test]
    async fn test_task_store_all_tasks() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let task1 = Task::new(
            "task-001".to_string(),
            "Task 1".to_string(),
            "D1".to_string(),
        );
        let task2 = Task::new(
            "task-002".to_string(),
            "Task 2".to_string(),
            "D2".to_string(),
        );
        store.save_task(&task1).await.unwrap();
        store.save_task(&task2).await.unwrap();

        let all = store.all_tasks().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_task_store_validate_dependencies() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let mut task1 = Task::new("task-001".to_string(), "T1".to_string(), "D1".to_string());
        task1.depends_on = vec!["task-002".to_string()]; // Missing dependency

        store.save_task(&task1).await.unwrap();

        let warnings = store.validate_dependencies().await;
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("missing task"));
    }

    #[tokio::test]
    async fn test_task_store_circular_dependency() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let mut task1 = Task::new("task-001".to_string(), "T1".to_string(), "D1".to_string());
        task1.depends_on = vec!["task-002".to_string()];

        let mut task2 = Task::new("task-002".to_string(), "T2".to_string(), "D2".to_string());
        task2.depends_on = vec!["task-001".to_string()]; // Circular!

        store.save_task(&task1).await.unwrap();
        store.save_task(&task2).await.unwrap();

        let warnings = store.validate_dependencies().await;
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("Circular dependency")));
    }

    #[tokio::test]
    async fn test_task_store_manifest() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let task = Task::new("task-001".to_string(), "T1".to_string(), "D1".to_string());
        store.save_task(&task).await.unwrap();
        store.save_manifest("Test goal").await.unwrap();

        let manifest = store.load_manifest().await.unwrap().unwrap();
        assert_eq!(manifest.goal, "Test goal");
        assert_eq!(manifest.total, 1);
    }
}
