#[allow(clippy::module_inception)]
mod global;
mod task_memory;

pub use global::GlobalMemory;
pub use task_memory::TaskMemoryExt;
