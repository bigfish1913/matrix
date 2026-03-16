//! Matrix Core - Long-Running Agent Orchestrator
//!
//! 一个使用 Claude CLI 自主开发软件项目的 AI 代理编排系统。

pub mod config;
pub mod error;
pub mod models;
pub mod store;
pub mod agent;
pub mod detector;
pub mod executor;
pub mod orchestrator;

pub use config::*;
pub use error::{Error, Result};
pub use models::{Task, TaskStatus, Complexity, Manifest};
pub use store::TaskStore;
pub use agent::{ClaudeRunner, ClaudeResult, AgentPool, SharedAgentPool};
pub use detector::{ProjectDetector, ProjectType, ProjectInfo, TestRunnerDetector, TestRunner};
pub use executor::{TaskExecutor, ExecutorConfig};
pub use orchestrator::{Orchestrator, OrchestratorConfig};