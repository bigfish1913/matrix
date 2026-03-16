//! TUI UI Components.

pub mod logs;
pub mod output;
pub mod status;
pub mod tabs;
pub mod tasks;

pub use logs::LogsPanel;
pub use output::OutputPanel;
pub use status::StatusBar;
pub use tabs::TabSwitcher;
pub use tasks::TasksPanel;