# Enhanced Status Display Design

## Problem

Users cannot tell if the program is executing or frozen because:
1. Long operations (Claude API calls) have no visual feedback for tens of seconds
2. "Running" state is too generic - doesn't show what operation is happening
3. No periodic visual indicator (heartbeat) to show the program is still alive

## Solution

Enhance the status bar with granular activity states and an activity pulse indicator.

## Design

### 1. Granular Activity States

Extend `ExecutionState::Running` with activity sub-types:

```rust
// crates/core/src/tui/event.rs

pub enum ExecutionState {
    Idle,
    Clarifying,
    Generating,
    Running { activity: Activity },  // Enhanced with activity type
    Completed,
    Failed,
}

pub enum Activity {
    ApiCall,      // Claude API call
    FileWrite,    // File write operation
    Test,         // Running tests
    Planning,     // Task planning
    Assessing,    // Complexity assessment
    Git,          // Git operations
    Other(String),// Other operations
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiCall => write!(f, "api"),
            Self::FileWrite => write!(f, "file"),
            Self::Test => write!(f, "test"),
            Self::Planning => write!(f, "plan"),
            Self::Assessing => write!(f, "assess"),
            Self::Git => write!(f, "git"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}
```

**Status bar display:**
- `Running:api` - Yellow
- `Running:test` - Blue
- `Running:git` - Green
- `Running:file` - Cyan

### 2. Activity Pulse Mechanism

Add heartbeat indicator for long-running operations:

```rust
// crates/core/src/tui/event.rs - Add to Event enum

ActivityPulse { task_id: String, activity: Activity }
```

```rust
// crates/core/src/tui/app.rs - Add to TuiApp

pub last_pulse_time: Option<Instant>,
pub current_activity: Option<Activity>,
```

**Visual behavior:**
- Within 2 seconds of last pulse: Show spinner `⠋⠙⠚⠞⠟`
- After 2 seconds, still receiving pulses: Show pulsing `●` indicator
- After 30 seconds without pulse: Show `⚠` warning (potentially frozen)

### 3. Status Bar Rendering

Updated status bar format:

```
v0.1.6 Running:api ● | Task:00:05 | Total:02:15 | 3/10 | glm-5 | N | ?:Help q:Quit
              ↑activity  ↑pulse
```

### 4. Executor Integration

`TaskExecutor` sends activity updates at key operation points:

```rust
// Before API call
self.emit_event(Event::ExecutionStateChanged {
    state: ExecutionState::Running { activity: Activity::ApiCall }
});

// Periodic pulses during long operations (every 5 seconds)
self.emit_event(Event::ActivityPulse {
    task_id: task_id.clone(),
    activity: Activity::ApiCall,
});

// After operation completes
self.emit_event(Event::ExecutionStateChanged {
    state: ExecutionState::Running { activity: Activity::FileWrite }
});
```

## Implementation Scope

### Files to Modify

1. `crates/core/src/tui/event.rs`
   - Add `Activity` enum
   - Modify `ExecutionState::Running` to include activity
   - Add `ActivityPulse` event variant

2. `crates/core/src/tui/app.rs`
   - Add `last_pulse_time` and `current_activity` fields
   - Handle `ActivityPulse` event in `process_event()`
   - Update pulse indicator logic in tick handler

3. `crates/core/src/tui/components/status.rs`
   - Update `render()` to show activity and pulse indicator
   - Add pulse timing logic

4. `crates/core/src/executor/task_executor.rs`
   - Emit activity state changes at operation boundaries
   - Implement periodic pulse sending during long operations

5. `crates/core/src/orchestrator/orchestrator.rs`
   - Emit activity states during phase transitions

## Testing

1. Unit tests for `Activity` display formatting
2. Unit tests for pulse timing logic
3. Manual testing: verify status bar shows correct activity during:
   - Claude API calls
   - File write operations
   - Test execution
   - Git operations

## Success Criteria

- Status bar shows current activity type (api/test/git/file/etc.)
- Pulse indicator visible during long operations
- Warning indicator appears if no pulse for 30+ seconds
- No performance impact from pulse mechanism
