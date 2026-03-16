//! Claude agent integration for the Matrix orchestrator.

mod pool;

pub use pool::{AgentPool, SharedAgentPool};

pub struct ClaudeRunner;
pub struct ClaudeResult;
