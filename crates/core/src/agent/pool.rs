//! AgentPool - session pool for Claude session reuse.

use crate::models::Task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// Session pool for Claude session reuse
///
/// Session reuse policy (in priority order):
/// 1. Retry → resume the task's own session (full failure context)
/// 2. Dependency chain → inherit the most-recently-completed dependency's session
/// 3. Depth-0 task with no dependencies → always start fresh
/// 4. Anything else → continue the thread's rolling session
#[derive(Debug, Default)]
pub struct AgentPool {
    /// task_id → session_id (recorded when execution finishes successfully)
    task_sessions: HashMap<String, String>,
    /// thread_name → session_id (rolling "last session" per worker thread)
    thread_sessions: HashMap<String, String>,
}

impl AgentPool {
    /// Create a new AgentPool
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the best session_id to resume for a task, or None for fresh
    pub fn get_session(&self, task: &Task, thread_name: &str) -> Option<String> {
        // 1. Retry: own session has the full failure context
        if task.retries > 0 {
            if let Some(ref sid) = task.session_id {
                if !sid.is_empty() {
                    debug!(task_id = %task.id, session_id = %sid, "Resuming own session for retry");
                    return Some(sid.clone());
                }
            }
        }

        // 2. Dependency chain: inherit last dependency's session
        for dep_id in task.depends_on.iter().rev() {
            if let Some(sid) = self.task_sessions.get(dep_id) {
                if !sid.is_empty() {
                    debug!(task_id = %task.id, dep_id = %dep_id, "Inheriting dependency session");
                    return Some(sid.clone());
                }
            }
        }

        // 3. Depth-0 + no deps → fresh (prevent unrelated context bleed)
        if task.depth == 0 && task.depends_on.is_empty() {
            debug!(task_id = %task.id, "Starting fresh session for depth-0 task");
            return None;
        }

        // 4. Thread's rolling session
        if let Some(sid) = self.thread_sessions.get(thread_name) {
            if !sid.is_empty() {
                debug!(task_id = %task.id, thread = %thread_name, "Using thread rolling session");
                return Some(sid.clone());
            }
        }

        None
    }

    /// Record a session after successful execution
    pub fn record(&mut self, task: &Task, session_id: &str, thread_name: &str) {
        if session_id.is_empty() {
            return;
        }
        self.task_sessions
            .insert(task.id.clone(), session_id.to_string());
        self.thread_sessions
            .insert(thread_name.to_string(), session_id.to_string());
        debug!(task_id = %task.id, thread = %thread_name, "Session recorded");
    }

    /// Clear a thread's rolling session
    pub fn clear_thread(&mut self, thread_name: &str) {
        self.thread_sessions.remove(thread_name);
        debug!(thread = %thread_name, "Thread session cleared");
    }

    /// Get statistics
    pub fn stats(&self) -> String {
        format!(
            "pool: {} task sessions | {} active thread sessions",
            self.task_sessions.len(),
            self.thread_sessions.len()
        )
    }
}

/// Thread-safe wrapper for AgentPool
#[derive(Debug, Clone)]
pub struct SharedAgentPool {
    inner: Arc<Mutex<AgentPool>>,
}

impl SharedAgentPool {
    /// Create a new shared AgentPool
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AgentPool::new())),
        }
    }

    /// Get session for a task
    pub async fn get_session(&self, task: &Task, thread_name: &str) -> Option<String> {
        self.inner.lock().await.get_session(task, thread_name)
    }

    /// Record a session
    pub async fn record(&self, task: &Task, session_id: &str, thread_name: &str) {
        self.inner
            .lock()
            .await
            .record(task, session_id, thread_name);
    }

    /// Clear a thread's session
    pub async fn clear_thread(&self, thread_name: &str) {
        self.inner.lock().await.clear_thread(thread_name);
    }

    /// Get stats
    pub async fn stats(&self) -> String {
        self.inner.lock().await.stats()
    }
}

impl Default for SharedAgentPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_pool_retry_session() {
        let mut pool = AgentPool::new();
        let mut task = Task::new("task-001".to_string(), "Test".to_string(), "D".to_string());
        task.retries = 1;
        task.session_id = Some("session-123".to_string());

        pool.record(
            &Task::new("other".into(), "O".into(), "D".into()),
            "session-456",
            "thread-1",
        );

        let session = pool.get_session(&task, "thread-1");
        assert_eq!(session, Some("session-123".to_string()));
    }

    #[test]
    fn test_agent_pool_dependency_session() {
        let mut pool = AgentPool::new();
        let dep_task = Task::new("task-001".to_string(), "Dep".to_string(), "D".to_string());
        pool.record(&dep_task, "session-abc", "thread-1");

        let mut task = Task::new("task-002".to_string(), "Test".to_string(), "D".to_string());
        task.depends_on = vec!["task-001".to_string()];

        let session = pool.get_session(&task, "thread-1");
        assert_eq!(session, Some("session-abc".to_string()));
    }

    #[test]
    fn test_agent_pool_fresh_for_depth_zero() {
        let pool = AgentPool::new();
        let task = Task::new("task-001".to_string(), "Test".to_string(), "D".to_string());

        let session = pool.get_session(&task, "thread-1");
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_shared_agent_pool() {
        let pool = SharedAgentPool::new();
        let task = Task::new("task-001".to_string(), "Test".to_string(), "D".to_string());

        pool.record(&task, "session-xyz", "thread-1").await;

        let stats = pool.stats().await;
        assert!(stats.contains("1 task sessions"));
    }
}
