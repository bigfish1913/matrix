//! Agent module - Claude CLI runner and session pool.

mod claude_runner;
mod pool;

pub use claude_runner::{ClaudeResult, ClaudeRunner};
pub use pool::{AgentPool, SharedAgentPool};
