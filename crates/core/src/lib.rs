//! Matrix Core - Long-Running Agent Orchestrator
//!
//! 一个使用 Claude CLI 自主开发软件项目的 AI 代理编排系统。

pub mod agent;
pub mod config;
pub mod detector;
pub mod error;
pub mod executor;
pub mod models;
pub mod orchestrator;
pub mod store;
pub mod tui;

pub use agent::{AgentPool, ClaudeResult, ClaudeRunner, SharedAgentPool};
pub use config::*;
pub use detector::{ProjectDetector, ProjectInfo, ProjectType, TestRunner, TestRunnerDetector};
pub use error::{Error, Result};
pub use executor::{ExecutorConfig, TaskExecutor};
pub use models::{Complexity, Manifest, Task, TaskStatus};
pub use orchestrator::{Orchestrator, OrchestratorConfig};
pub use store::TaskStore;
pub use tui::{
    render_app, Event, EventSender, LogBuffer, MatrixTerminal, TuiApp, TuiEvent, VerbosityLevel,
};
