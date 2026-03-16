//! Project and test runner detection.

mod project;
mod test_runner;

pub use project::{ProjectDetector, ProjectInfo, ProjectType};
pub use test_runner::{TestRunner, TestRunnerDetector};
