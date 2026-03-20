# 分组日志格式实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 优化 Logs 标签页日志输出，实现按任务/阶段分组显示的简洁格式

**Architecture:** 在现有 LogEntry 结构上添加 task_id/task_title/phase 字段，通过 tracing span 传递上下文，渲染层检测 task_id 变化自动插入分组头

**Tech Stack:** Rust, ratatui, tracing, chrono

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `crates/core/src/tui/mod.rs` | LogEntry 和 LogBuffer 数据结构 |
| `crates/core/src/tui/event.rs` | Event::Log 添加上下文字段 |
| `crates/core/src/tui/tracing_layer.rs` | 从 tracing span 提取上下文 |
| `crates/core/src/tui/components/logs.rs` | 分组渲染逻辑 |
| `crates/core/src/executor/task_executor.rs` | 创建 task span |
| `crates/core/src/orchestrator/orchestrator.rs` | 创建 phase span |

---

## Task 1: 更新数据结构

**Files:**
- Modify: `crates/core/src/tui/mod.rs:34-40`
- Modify: `crates/core/src/tui/event.rs`

- [ ] **Step 1: 添加 LogContext 结构体和更新 LogEntry**

在 `crates/core/src/tui/mod.rs` 中，找到 `LogEntry` 结构体定义，修改为：

```rust
/// Context for structured logging
#[derive(Debug, Clone, Default)]
pub struct LogContext {
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub phase: Option<String>,
}

/// A single log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub message: String,
    pub repeat_count: usize,
    // 新增字段
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub phase: Option<String>,
}
```

- [ ] **Step 2: 更新 LogBuffer::push 方法签名**

在 `crates/core/src/tui/mod.rs` 中，修改 `LogBuffer::push` 方法：

```rust
    /// Push a new log entry (deduplicates exact matches and pattern matches)
    pub fn push(&self, level: LogLevel, message: String, context: LogContext) {
        let mut entries = self.entries.lock().unwrap();

        // Skip empty messages
        if message.trim().is_empty() {
            return;
        }

        let pattern = Self::extract_pattern(&message);

        // Check if this matches the pattern of the last message (same context)
        if let Some(last) = entries.last_mut() {
            let last_pattern = Self::extract_pattern(&last.message);
            if last.level == level
                && last_pattern == pattern
                && last.task_id == context.task_id
                && last.phase == context.phase
            {
                // Same pattern and context, increment repeat count
                last.repeat_count += 1;
                return;
            }
        }

        // Add new entry
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level,
            message,
            repeat_count: 1,
            task_id: context.task_id,
            task_title: context.task_title,
            phase: context.phase,
        };
        entries.push(entry);
        if entries.len() > self.max_entries {
            entries.remove(0);
        }
    }
```

- [ ] **Step 3: 更新 Event::Log 变体**

在 `crates/core/src/tui/event.rs` 中，找到 `Event` 枚举，修改 `Log` 变体：

```rust
pub enum Event {
    // ... 其他变体保持不变
    Log {
        timestamp: chrono::DateTime<chrono::Utc>,
        level: LogLevel,
        message: String,
        // 新增字段
        task_id: Option<String>,
        task_title: Option<String>,
        phase: Option<String>,
    },
    // ...
}
```

- [ ] **Step 4: 导出 LogContext**

在 `crates/core/src/tui/mod.rs` 末尾的 pub use 中添加：

```rust
pub use app::TuiApp;
pub use event::{
    ClarificationQuestion, ClarificationSender, ConfirmSender, Event, ExecutionState, Key,
    LogLevel, LogContext, TuiEvent, VerbosityLevel,
};
```

- [ ] **Step 5: 验证编译**

Run: `cargo build -p matrix-core`

Expected: 编译通过（可能有未使用警告）

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/tui/mod.rs crates/core/src/tui/event.rs
git commit -m "feat(core): add LogContext and update LogEntry with task/phase fields"
```

---

## Task 2: 更新 tracing_layer 提取 span 上下文

**Files:**
- Modify: `crates/core/src/tui/tracing_layer.rs`

- [ ] **Step 1: 添加 span 上下文提取逻辑**

在 `crates/core/src/tui/tracing_layer.rs` 中，修改 `on_event` 方法：

```rust
impl<S> Layer<S> for TuiLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &TracingEvent, ctx: Context<'_, S>) {
        // Skip logs from certain noisy modules
        let module = event.metadata().module_path().unwrap_or("");
        if module.contains("tokio")
            || module.contains("hyper")
            || module.contains("mio")
            || module.contains("reqwest")
            || module.contains("h2")
        {
            return;
        }

        // Skip DEBUG and TRACE logs to reduce noise
        let level = *event.metadata().level();
        if level == tracing::Level::TRACE || level == tracing::Level::DEBUG {
            return;
        }

        // Get the log level
        let log_level = match level {
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
            _ => return, // Already filtered above
        };

        // Extract context from span
        let (task_id, task_title, phase) = ctx.lookup_current()
            .map(|span| {
                let mut task_id = None;
                let mut task_title = None;
                let mut phase = None;

                // Extract fields from span
                span.metadata().fields().for_each(|field| {
                    // Check span name for phase info
                    let name = span.metadata().name();
                    if name.starts_with("task_") || name == "task" {
                        if field.name() == "task_id" || field.name() == "id" {
                            // Will be extracted below
                        }
                    }
                });

                // Extract from span extensions or use name-based detection
                let name = span.metadata().name();
                if name.starts_with("task_") {
                    phase = Some("task".to_string());
                } else if name.contains("clarif") {
                    phase = Some("clarification".to_string());
                } else if name.contains("generate") {
                    phase = Some("generating".to_string());
                }

                (task_id, task_title, phase)
            })
            .unwrap_or((None, None, None));

        // Format the message
        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        // Send to TUI with context
        let timestamp = chrono::Utc::now();
        let _ = self.sender.send(Event::Log {
            timestamp,
            level: log_level,
            message,
            task_id,
            task_title,
            phase,
        });
    }
}
```

- [ ] **Step 2: 简化方案 - 使用事件字段传递上下文**

由于 tracing span 字段提取复杂，采用更简单的方案：从消息中解析 task_id。

修改 `on_event` 方法：

```rust
impl<S> Layer<S> for TuiLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &TracingEvent, _ctx: Context<'_, S>) {
        // Skip logs from certain noisy modules
        let module = event.metadata().module_path().unwrap_or("");
        if module.contains("tokio")
            || module.contains("hyper")
            || module.contains("mio")
            || module.contains("reqwest")
            || module.contains("h2")
        {
            return;
        }

        // Skip DEBUG and TRACE logs to reduce noise
        let level = *event.metadata().level();
        if level == tracing::Level::TRACE || level == tracing::Level::DEBUG {
            return;
        }

        // Get the log level
        let log_level = match level {
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
            _ => return, // Already filtered above
        };

        // Format the message
        let mut message = String::new();
        let mut task_id = None;
        let mut task_title = None;
        let mut phase = None;

        let mut visitor = ContextAwareVisitor {
            message: &mut message,
            task_id: &mut task_id,
            task_title: &mut task_title,
            phase: &mut phase,
        };
        event.record(&mut visitor);

        // Send to TUI with context
        let timestamp = chrono::Utc::now();
        let _ = self.sender.send(Event::Log {
            timestamp,
            level: log_level,
            message,
            task_id,
            task_title,
            phase,
        });
    }
}
```

- [ ] **Step 3: 添加 ContextAwareVisitor**

替换原有的 `MessageVisitor`：

```rust
/// A visitor to extract the message and context from tracing fields
struct ContextAwareVisitor<'a> {
    message: &'a mut String,
    task_id: &'a mut Option<String>,
    task_title: &'a mut Option<String>,
    phase: &'a mut Option<String>,
}

impl<'a> tracing::field::Visit for ContextAwareVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        match field.name() {
            "message" => self.message.push_str(&format!("{:?}", value)),
            "task_id" | "id" => *self.task_id = Some(format!("{:?}", value).trim_matches('"').to_string()),
            "title" | "task_title" => *self.task_title = Some(format!("{:?}", value).trim_matches('"').to_string()),
            "phase" => *self.phase = Some(format!("{:?}", value).trim_matches('"').to_string()),
            _ => {
                // Include other fields in message
                if !self.message.is_empty() {
                    self.message.push_str(", ");
                }
                self.message.push_str(&format!("{}={:?}", field.name(), value));
            }
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "message" => self.message.push_str(value),
            "task_id" | "id" => *self.task_id = Some(value.to_string()),
            "title" | "task_title" => *self.task_title = Some(value.to_string()),
            "phase" => *self.phase = Some(value.to_string()),
            _ => {
                if !self.message.is_empty() {
                    self.message.push_str(", ");
                }
                self.message.push_str(&format!("{}={}", field.name(), value));
            }
        }
    }
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo build -p matrix-core`

Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/tui/tracing_layer.rs
git commit -m "feat(core): extract task context from tracing events"
```

---

## Task 3: 更新 app.rs 处理新 Event

**Files:**
- Modify: `crates/core/src/tui/app.rs`

- [ ] **Step 1: 更新 Event::Log 处理**

找到处理 `Event::Log` 的代码，修改为使用 `LogContext`：

```rust
Event::Log { timestamp, level, message, task_id, task_title, phase } => {
    let context = LogContext {
        task_id,
        task_title,
        phase,
    };
    self.log_buffer.push(level, message, context);
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p matrix-core`

Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/tui/app.rs
git commit -m "feat(core): handle Log event with context in TuiApp"
```

---

## Task 4: 重写 logs.rs 渲染逻辑

**Files:**
- Modify: `crates/core/src/tui/components/logs.rs`

- [ ] **Step 1: 添加级别图标映射**

在 `LogsPanel` 中添加：

```rust
impl LogsPanel {
    /// Get icon and color for log level
    fn level_style(level: LogLevel) -> (&'static str, Color) {
        match level {
            LogLevel::Trace => ("·", Color::DarkGray),
            LogLevel::Debug => ("○", Color::Gray),
            LogLevel::Info => ("✓", Color::Green),
            LogLevel::Warn => ("⚠", Color::Yellow),
            LogLevel::Error => ("✗", Color::Red),
        }
    }
```

- [ ] **Step 2: 重写 render 方法实现分组**

```rust
    /// Render logs panel with grouped display
    pub fn render(
        entries: &[LogEntry],
        scroll_offset: u16,
        _viewport_height: u16,
    ) -> Paragraph<'static> {
        let mut lines: Vec<Line> = Vec::new();
        let mut current_task_id: Option<&str> = None;
        let mut current_phase: Option<&str> = None;

        for entry in entries {
            // Check if we need a group header
            let needs_header = entry.task_id.as_deref() != current_task_id
                || (entry.task_id.is_none() && entry.phase.as_deref() != current_phase);

            if needs_header {
                if let Some(ref task_id) = entry.task_id {
                    // Task group header
                    let title = entry.task_title.as_deref().unwrap_or("");
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("━━━ {}: {} ━━━", task_id, title),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]));
                } else if let Some(ref phase) = entry.phase {
                    // Phase group header
                    let phase_display = match phase.as_str() {
                        "clarification" => "Clarification",
                        "generating" => "Generating",
                        "running" => "Running",
                        _ => phase,
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("━━━ {} ━━━", phase_display),
                            Style::default().fg(Color::Magenta),
                        ),
                    ]));
                }

                current_task_id = entry.task_id.as_deref();
                current_phase = entry.phase.as_deref();
            }

            // Format time as HH:MM
            let time = entry.timestamp.format("%H:%M");

            // Get level icon and color
            let (icon, color) = Self::level_style(entry.level);

            // Show repeat count if message was duplicated
            let message = if entry.repeat_count > 1 {
                format!("{} · x{}", entry.message, entry.repeat_count)
            } else {
                entry.message.clone()
            };

            // Build log line with appropriate indentation
            if entry.task_id.is_some() {
                // Task logs: indented
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(message, Style::default().fg(Color::White)),
                ]));
            } else {
                // System logs: no indent, use info icon
                lines.push(Line::from(vec![
                    Span::styled(time.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled("ℹ", Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(message, Style::default().fg(Color::White)),
                ]));
            }
        }

        Paragraph::new(lines)
            .block(Block::default().title(" Logs ").borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0))
    }
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p matrix-core`

Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/tui/components/logs.rs
git commit -m "feat(core): implement grouped log display with task/phase headers"
```

---

## Task 5: 在 executor 中添加 task context

**Files:**
- Modify: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: 在 execute 方法中添加 task_id 和 task_title 字段**

找到 `info!(task_id = %task.id, ...)` 这类日志调用，确保它们包含 task_title：

```rust
info!(
    task_id = %task.id,
    title = %task.title,
    model = %model,
    "Executing task"
);
```

- [ ] **Step 2: 更新其他日志调用**

搜索所有 `info!(task_id = %task.id,` 的地方，添加 title 字段：

```rust
info!(task_id = %task.id, title = %task.title, "Running tests");
info!(task_id = %task.id, title = %task.title, "Tests passed");
info!(task_id = %task.id, title = %task.title, tokens = usage.total_tokens, "Token usage");
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p matrix-core`

Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/executor/task_executor.rs
git commit -m "feat(core): add task title to executor logs for grouping"
```

---

## Task 6: 在 orchestrator 中添加 phase context

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: 为阶段日志添加 phase 字段**

在 `clarify_goal`、`generate_tasks`、`run_final_tests` 等方法中添加 phase：

```rust
info!(phase = "clarification", "Phase 0: Starting clarification...");
info!(phase = "clarification", "Parsed {} clarification questions", q.len());
info!(phase = "generating", "Generating task list");
info!(phase = "testing", "Running final tests...");
info!(phase = "summary", "All tasks processed: {}/{} completed", completed, total);
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p matrix-core`

Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "feat(core): add phase context to orchestrator logs"
```

---

## Task 7: 集成测试

**Files:**
- None (手动测试)

- [ ] **Step 1: 构建并安装**

Run: `cargo build --release && cargo install --path crates/cli --force`

Expected: 编译成功

- [ ] **Step 2: 运行测试**

Run: `matrix "build a hello world app" -w ./test-project`

Expected: Logs 标签页显示分组格式

- [ ] **Step 3: 验证格式**

检查日志显示：
- [ ] 任务日志有分组头 `━━━ task-001: xxx ━━━`
- [ ] 任务日志有缩进
- [ ] 时间戳为 HH:MM 格式
- [ ] 级别使用图标（✓ ⚠ ✗）

- [ ] **Step 4: Final Commit**

```bash
git add -A
git commit -m "feat: implement grouped log format with task/phase display"
```

---

## 预期效果

```
━━━ Clarification ━━━
  14:30 ✓ 生成 3 个问题
  14:31 ✓ 用户回答完成

━━━ task-001: 项目初始化 ━━━
  14:32 ✓ 开始执行
  14:32 ✓ 安装依赖
  14:33 ✓ 完成 · 800 tokens

━━━ task-002: 实现核心模块 ━━━
  14:34 ✓ 开始执行
  14:35 ⚠ 测试失败
  14:36 ✓ 修复后通过 · 1.5k tokens

━━━ Summary ━━━
  14:40 ✓ 全部完成 · 5/5 任务
```
