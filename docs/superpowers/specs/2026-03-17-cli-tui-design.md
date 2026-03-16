# CLI TUI Output Design

**Date:** 2026-03-17
**Status:** Draft

## Overview

Optimize Matrix CLI output with a terminal user interface (TUI) that provides real-time visibility into orchestrator execution, Claude activity, and task progress.

## Goals

- Provide clear, real-time visibility into orchestrator execution
- Display Claude's thinking process and tool calls
- Support different verbosity levels for different use cases
- Enable tab-based navigation between different views

## Non-Goals

- Mouse interaction support
- Complex keyboard shortcuts beyond tab switching
- Configuration file for TUI settings

## Architecture

### TUI Framework

Use **ratatui** library for terminal UI rendering:
- Async-friendly, integrates with existing tokio runtime
- Active community, good documentation
- Flexible layout control for tab-based design

### Layout Structure

```
┌─────────────────────────────────────────────────────────┐
│  matrix TUI                                             │
├─────────────────────────────────────────────────────────┤
│  [Tasks] [Claude Output] [Logs]     <- Tab switcher     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│                     Main Panel                          │
│                                                         │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  Status: Running task-001 | 3/10 | 02:34 | haiku       │
└─────────────────────────────────────────────────────────┘
```

### Components

1. **Tab Switcher** - Top navigation bar with available views
2. **Main Panel** - Content area showing current tab's content
3. **Status Bar** - Bottom bar with execution summary

## Tab Pages

### 1. Tasks Panel

Display all tasks with their status:

```
┌─ Tasks ─────────────────────────────────────────────────┐
│ ✓ task-001  Initialize game project          [02:34]   │
│ ● task-002  Create game state management      Running   │
│ ○ task-003  Build map and terrain system      Pending   │
│ ○ task-004  Implement enemy tank system       Pending   │
│ ...                                                     │
└─────────────────────────────────────────────────────────┘
```

**Status Icons:**
- `✓` Completed
- `●` Running/In Progress
- `○` Pending
- `✗` Failed
- `⊘` Skipped

**Columns:**
- Status icon
- Task ID
- Task title
- Duration (for completed) or status text

### 2. Claude Output Panel

Display real-time Claude activity based on verbosity level.

**Quiet Mode (`-q, --quiet`):**
```
┌─ Claude Output ────────────────────────────────────────┐
│ Running task-002...                                     │
│                                                         │
│ ✓ Completed                                             │
└─────────────────────────────────────────────────────────┘
```

**Normal Mode (default):**
```
┌─ Claude Output (task-002) ──────────────────────────────┐
│ [Read] src/main.rs                                      │
│ [Write] src/states.rs                                   │
│ [Bash] cargo check ✓                                    │
│                                                         │
│ Result: Created GameState enum...                       │
└─────────────────────────────────────────────────────────┘
```

**Verbose Mode (`-v, --verbose`):**
```
┌─ Claude Output (task-002) ──────────────────────────────┐
│ [Thinking] Analyzing the game state requirements...     │
│                                                         │
│ [Tool:Read] src/main.rs                                 │
│   → Reading file content...                             │
│                                                         │
│ [Tool:Write] src/states.rs (125 lines)                  │
│   → Writing: pub enum GameState { Menu, Playing...      │
│                                                         │
│ [Bash] cargo check                                       │
│   → Compiling game...                                    │
│   → Finished in 3.2s                                     │
│                                                         │
│ ── Result ──                                            │
│ Created GameState enum with Menu, Playing, Paused...    │
└─────────────────────────────────────────────────────────┘
```

### 3. Logs Panel

Scrollable log viewer with timestamp and level:

```
┌─ Logs ─────────────────────────────────────────────────┐
│ 14:32:01 INFO  Task task-001 completed                  │
│ 14:32:05 INFO  Dispatching task-002                     │
│ 14:32:05 INFO  [Tool] Read src/main.rs                  │
│ 14:32:08 WARN  cargo check had warnings                 │
│ 14:32:10 ERROR Task task-003 failed: timeout            │
└─────────────────────────────────────────────────────────┘
```

## Status Bar

Always visible at the bottom:

```
Status: <state> | <current_task> | <progress> | <elapsed> | <model>
```

**States:**
- `Idle` - No active execution
- `Generating` - Creating task list
- `Running` - Executing tasks
- `Completed` - All tasks done
- `Failed` - Execution stopped due to errors

**Example:**
```
Status: Running | task-002 | 3/10 completed | 02:34 | haiku
```

## Verbosity Levels

| Level | Flag | Claude Output | Progress Updates |
|-------|------|---------------|------------------|
| Quiet | `-q, --quiet` | Final result only | Minimal |
| Normal | (default) | Tool names + brief result | Standard |
| Verbose | `-v, --verbose` | Full thinking + tool details | Detailed |

## Keyboard Controls

| Key | Action |
|-----|--------|
| `Tab` / `←` `→` | Switch between tabs |
| `↑` `↓` | Scroll within current panel |
| `q` / `Esc` | Exit TUI mode (with confirmation if running) |
| `?` | Show help overlay |

## Implementation Approach

### Phase 1: TUI Infrastructure
- Add ratatui dependency
- Create `crates/core/src/tui/` module
- Implement basic terminal setup/teardown
- Create event loop for keyboard input

### Phase 2: Layout & Tabs
- Implement three-panel layout
- Build tab switcher component
- Create placeholder panels

### Phase 3: Task Panel
- Connect to TaskStore for real-time updates
- Implement task list rendering
- Add status icons and formatting

### Phase 4: Claude Output Panel
- Extend ClaudeRunner to emit events
- Create event channel for UI updates
- Implement output formatting for each verbosity level

### Phase 5: Logs Panel
- Integrate with tracing subscriber
- Buffer recent log entries
- Render with timestamps and levels

### Phase 6: Status Bar & Polish
- Implement status bar rendering
- Add keyboard handling
- Handle terminal resize
- Graceful shutdown

## File Structure

```
crates/core/src/
├── tui/
│   ├── mod.rs           # Module exports
│   ├── app.rs           # Main TUI application state
│   ├── event.rs         # Event handling (keyboard, resize)
│   ├── render.rs        # Rendering logic
│   └── components/
│       ├── mod.rs
│       ├── tabs.rs      # Tab switcher
│       ├── tasks.rs     # Task list panel
│       ├── output.rs    # Claude output panel
│       ├── logs.rs      # Log viewer panel
│       └── status.rs    # Status bar
```

## Dependencies

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"  # Terminal backend for ratatui
tokio = { version = "1", features = ["sync"] }
```

## Fallback Behavior

If terminal doesn't support TUI (non-interactive, CI environment):
- Automatically fall back to simple line-by-line output
- Detect with `std::io::stdout().is_terminal()`
- User can force simple mode with `--no-tui` flag

## Open Questions

1. **Color scheme** - Use terminal default colors or custom palette?
   - Recommendation: Use terminal defaults for better compatibility

2. **Task detail view** - Should pressing Enter on a task show more details?
   - Recommendation: Out of scope for initial implementation

3. **Log filtering** - Should logs panel support filtering by level?
   - Recommendation: Out of scope, can be added later