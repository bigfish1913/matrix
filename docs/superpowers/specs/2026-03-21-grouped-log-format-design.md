# 分组日志格式设计

## 概述

优化 Logs 标签页的日志输出格式，采用按任务/阶段分组显示的方式，参考 Claude Code terminal 的简洁风格。

## 目标

1. 更紧凑的时间戳 - 去掉秒，只显示 `HH:MM`
2. 更清晰的级别标识 - 用图标替代文字（✓ ⚠ ✗）
3. 结构化信息 - 任务ID、阶段等分组显示
4. 更简洁的格式 - 完整重新设计

## 数据结构

### LogEntry 扩展

```rust
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub message: String,
    pub repeat_count: usize,
    // 新增字段
    pub task_id: Option<String>,      // 任务ID（如 "task-001"）
    pub task_title: Option<String>,   // 任务标题（用于分组头）
    pub phase: Option<String>,        // 阶段（如 "clarification", "generating"）
}
```

### LogBuffer 修改

`push` 方法签名更新：
```rust
pub fn push(&self, level: LogLevel, message: String, context: LogContext)
```

其中 `LogContext`：
```rust
pub struct LogContext {
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub phase: Option<String>,
}
```

## 日志格式设计

### 分组头格式

```
━━━ task-001: 实现用户登录 ━━━
```

- 使用 `━` 字符作为分隔线
- 显示 task_id 和 task_title
- 颜色：青色 (Cyan)

### 日志行格式（有 task_id）

```
  14:32 ✓ 开始执行
  14:33 ⚠ 测试失败 · 重试 2/3
  14:35 ✓ 完成 · 1.2k tokens
```

- 缩进 2 空格
- 时间戳：`HH:MM`
- 级别图标
- 消息内容
- 附加信息用 `·` 分隔

### 系统日志（无 task_id）

```
14:32 ℹ Phase 0: 开始提问...
14:33 ℹ 生成 5 个任务
```

- 无缩进
- 使用 `ℹ` 图标表示系统信息

## 级别图标映射

| 级别 | 图标 | 颜色 |
|------|------|------|
| INFO | ✓ | 绿色 (Green) |
| WARN | ⚠ | 黄色 (Yellow) |
| ERROR | ✗ | 红色 (Red) |
| DEBUG | ○ | 灰色 (Gray) |
| TRACE | · | 暗灰 (DarkGray) |

## 文件修改清单

### 1. `crates/core/src/tui/mod.rs`

- LogEntry 添加 task_id, task_title, phase 字段
- 新增 LogContext 结构体
- LogBuffer::push 方法更新

### 2. `crates/core/src/tui/tracing_layer.rs`

- 从 tracing span 提取 task_id 信息
- 构建 LogContext 传递给 LogBuffer

### 3. `crates/core/src/tui/components/logs.rs`

- 重写渲染逻辑
- 检测 task_id 变化插入分组头
- 应用新格式：时间戳、图标、缩进

### 4. `crates/core/src/executor/task_executor.rs`

- 在 task 执行时创建 tracing span 包含 task_id
- 确保 info!/warn!/error! 宏在 span context 中执行

### 5. `crates/core/src/orchestrator/orchestrator.rs`

- 为阶段日志添加 phase 信息
- 提交任务时包含 task_title

## 效果预览

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

## 实现顺序

1. 修改 LogEntry 和 LogBuffer 数据结构
2. 更新 tracing_layer 提取 span 信息
3. 重写 logs.rs 渲染逻辑
4. 在 executor 和 orchestrator 中添加 span context
5. 测试并调整样式
