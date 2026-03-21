//! Error types for the Matrix orchestrator.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Task generation failed: {0}")]
    TaskGeneration(String),

    #[error("Task execution failed: {0}")]
    TaskExecution(String),

    #[error("Claude CLI error: {0}")]
    ClaudeCli(String),

    #[error("Claude UI error: {0}")]
    ClaudeUi(String),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Dependency error: {0}")]
    Dependency(String),

    #[error("Workspace error: {0}")]
    Workspace(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::ParseError("test error".to_string());
        assert_eq!(err.to_string(), "Parse error: test error");

        let err = Error::TaskNotFound("task-001".to_string());
        assert_eq!(err.to_string(), "Task not found: task-001");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Json(_)));
    }
}
