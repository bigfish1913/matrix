//! Checkpoint mechanism for task orchestration.

mod manager;
mod review;
mod bypass;

pub use manager::{CheckpointManager, CheckpointResult, BlockedTask};
pub use review::{ReviewReport, ProgressStats, UpcomingTask, Issue};
pub use bypass::{BypassStrategy, ReplacementTask};
