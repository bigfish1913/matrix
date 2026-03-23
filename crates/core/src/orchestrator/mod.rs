//! Main orchestrator module.

mod dependency_graph;
#[allow(clippy::module_inception)]
mod orchestrator;

pub use dependency_graph::DependencyGraph;
pub use orchestrator::{Orchestrator, OrchestratorConfig};
