//! Project and test runner detection.

mod project;
mod test_runner;

pub use project::{ProjectDetector, ProjectType, ProjectInfo};
pub use test_runner::{TestRunnerDetector, TestRunner};