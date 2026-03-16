//! Agent module - Claude CLI runner and session pool.

mod claude_runner;
mod pool;

pub use claude_runner::{ClaudeRunner, ClaudeResult};
pub use pool::{AgentPool, SharedAgentPool};