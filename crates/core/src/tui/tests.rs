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
            parent_id: None,
            depth: 0,
            depends_on: vec![],
        })
        .unwrap();

    let event = receiver.try_recv().unwrap();
    match event {
        Event::TaskCreated {
            id,
            title,
            parent_id,
            depth,
            depends_on,
        } => {
            assert_eq!(id, "task-001");
            assert_eq!(title, "Test");
            assert_eq!(parent_id, None);
            assert_eq!(depth, 0);
            assert!(depends_on.is_empty());
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_log_buffer() {
    let buffer = LogBuffer::new(3);
    let ctx = LogContext::default();

    buffer.push(LogLevel::Info, "msg1".to_string(), ctx.clone());
    buffer.push(LogLevel::Warn, "msg2".to_string(), ctx.clone());
    buffer.push(LogLevel::Error, "msg3".to_string(), ctx.clone());
    buffer.push(LogLevel::Debug, "msg4".to_string(), ctx); // Should push out msg1

    let entries = buffer.get_entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message, "msg2");
    assert_eq!(entries[2].message, "msg4");
}

#[test]
fn test_log_buffer_dedup() {
    let buffer = LogBuffer::new(10);
    let ctx = LogContext::default();

    // Push same message multiple times - should dedupe
    buffer.push(LogLevel::Info, "test message".to_string(), ctx.clone());
    buffer.push(LogLevel::Info, "test message".to_string(), ctx.clone());
    buffer.push(LogLevel::Info, "test message".to_string(), ctx.clone());

    let entries = buffer.get_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].repeat_count, 3);

    // Different level should not be deduped
    buffer.push(LogLevel::Warn, "test message".to_string(), ctx.clone());
    let entries = buffer.get_entries();
    assert_eq!(entries.len(), 2);

    // Empty message should be skipped
    buffer.push(LogLevel::Info, "".to_string(), ctx.clone());
    buffer.push(LogLevel::Info, "   ".to_string(), ctx);
    let entries = buffer.get_entries();
    assert_eq!(entries.len(), 2); // No new entries added
}

#[test]
fn test_log_buffer_pattern_dedup() {
    let buffer = LogBuffer::new(10);
    let ctx = LogContext::default();

    // Messages with different numbers but same pattern should be deduped
    buffer.push(LogLevel::Info, "Progress: 1 completed".to_string(), ctx.clone());
    buffer.push(LogLevel::Info, "Progress: 2 completed".to_string(), ctx.clone());
    buffer.push(LogLevel::Info, "Progress: 10 completed".to_string(), ctx);

    let entries = buffer.get_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].repeat_count, 3);
    // Should keep the first message
    assert_eq!(entries[0].message, "Progress: 1 completed");
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
