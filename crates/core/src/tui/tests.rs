//! Unit tests for TUI module.

use super::*;
use crate::tui::components::LogsPanel;

#[test]
fn test_verbosity_level_ordering() {
    use VerbosityLevel::*;
    assert!(Verbose >= Normal);
    assert!(Normal >= Quiet);
    assert!(Verbose >= Quiet);
}

#[test]
fn test_event_channel() {
    let (sender, mut receiver) = create_event_channel();

    sender
        .send(Event::TaskCreated {
            id: "task-001".to_string(),
            title: "Test".to_string(),
        })
        .unwrap();

    let event = receiver.try_recv().unwrap();
    match event {
        Event::TaskCreated { id, title } => {
            assert_eq!(id, "task-001");
            assert_eq!(title, "Test");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_log_buffer() {
    let buffer = LogBuffer::new(3);

    buffer.push(LogLevel::Info, "msg1".to_string());
    buffer.push(LogLevel::Warn, "msg2".to_string());
    buffer.push(LogLevel::Error, "msg3".to_string());
    buffer.push(LogLevel::Debug, "msg4".to_string()); // Should push out msg1

    let entries = buffer.get_entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message, "msg2");
    assert_eq!(entries[2].message, "msg4");
}

#[test]
fn test_logs_panel_auto_scroll() {
    // Test when entries fit within viewport (no scroll needed)
    assert_eq!(LogsPanel::calculate_auto_scroll(5, 10), 0);

    // Test when entries exceed viewport (scroll to show latest)
    // 20 entries, viewport height 10 = scroll 10 to show entries 10-19
    assert_eq!(LogsPanel::calculate_auto_scroll(20, 10), 10);

    // Test edge case: exactly fits
    assert_eq!(LogsPanel::calculate_auto_scroll(10, 10), 0);

    // Test edge case: one more than fits
    assert_eq!(LogsPanel::calculate_auto_scroll(11, 10), 1);

    // Test edge case: empty entries
    assert_eq!(LogsPanel::calculate_auto_scroll(0, 10), 0);

    // Test edge case: zero viewport height
    assert_eq!(LogsPanel::calculate_auto_scroll(5, 0), 0);
}