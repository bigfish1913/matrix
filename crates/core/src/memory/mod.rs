//! Memory system for task orchestration.

mod global;
mod task_memory;

pub use global::GlobalMemory;
pub use task_memory::TaskMemoryOps;
