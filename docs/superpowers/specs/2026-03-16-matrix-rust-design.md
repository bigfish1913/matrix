# Matrix - Rust Agent Orchestrator 设计文档

**日期**: 2026-03-16
**版本**: 1.0
**状态**: 设计评审中

## 概述

Matrix 是 `longtime.py` 的 Rust 1:1 复刻版本 —— 一个使用 Claude CLI 自主开发软件项目的 AI 代理编排系统。

## 需求摘要

来源：`docs/require1.0.md`

1. 参数命名遵循 Rust 规范（snake_case）
2. 功能全面覆盖 longtime.py
3. 采用 Rust workspace 架构，支持未来的 cli、Claude Code 插件、gui、web 客户端
4. 使用 Taskfile 实现一键安装部署

## 技术选型

| 领域 | 选择 | 理由 |
|------|------|------|
| 异步运行时 | Tokio | Rust 生态最成熟，社区支持广泛 |
| CLI 解析 | Clap (derive) | 功能强大，自动生成帮助文档 |
| 错误处理 | thiserror + anyhow | thiserror 定义核心错误，anyhow 处理应用层 |
| 序列化 | serde + serde_json | Rust 生态标准 |
| 日志 | tracing | 现代框架，支持结构化日志和 span |
| 并发模型 | Tokio + spawn_blocking | 与 Python 线程池行为一致 |

## 项目结构

```
matrix/
├── Cargo.toml                    # Workspace 根配置
├── Taskfile.yml                  # 构建与部署自动化
├── crates/
│   ├── core/                     # 核心库（无 UI 依赖）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── error.rs
│   │       ├── models/
│   │       │   ├── mod.rs
│   │       │   ├── task.rs
│   │       │   └── manifest.rs
│   │       ├── store/
│   │       │   ├── mod.rs
│   │       │   └── task_store.rs
│   │       ├── agent/
│   │       │   ├── mod.rs
│   │       │   ├── pool.rs
│   │       │   └── claude_runner.rs
│   │       ├── detector/
│   │       │   ├── mod.rs
│   │       │   ├── project.rs
│   │       │   └── test_runner.rs
│   │       ├── executor/
│   │       │   ├── mod.rs
│   │       │   └── task_executor.rs
│   │       └── orchestrator/
│   │           ├── mod.rs
│   │           ├── orchestrator.rs
│   │           ├── dependency_graph.rs
│   │           ├── health_monitor.rs
│   │           ├── task_scheduler.rs
│   │           └── prompts.rs
│   ├── cli/                      # 命令行界面
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── plugin/                   # Claude Code 插件（预留）
│   ├── gui/                      # GUI 客户端（预留）
│   └── web/                      # Web 客户端（预留）
└── docs/
    └── longtime.py               # Python 源文件（参考）
```

## 核心数据结构

### Task

```rust
pub struct Task {
    pub id: String,                      // 任务ID，如 "task-001"
    pub title: String,                   // 简短标题
    pub description: String,             // 详细描述
    pub status: TaskStatus,              // pending|in_progress|completed|failed|skipped
    pub parent_id: Option<String>,       // 父任务ID（拆分时）
    pub depth: u32,                      // 拆分深度（最大5）
    pub complexity: Complexity,          // unknown|simple|complex
    pub retries: u32,                    // 重试次数
    pub session_id: Option<String>,      // Claude 会话ID
    pub result: Option<String>,          // 执行结果
    pub error: Option<String>,           // 错误信息
    pub test_failure_context: Option<String>,  // 测试失败上下文
    pub test_result: Option<String>,     // 测试输出
    pub depends_on: Vec<String>,         // 依赖的任务ID列表
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub verification_result: HashMap<String, Value>,
    pub test_passed: bool,
    pub modified_files: Vec<String>,     // 修改的文件列表
}
```

### TaskStore

负责任务的持久化存储和依赖图验证：

- 任务保存为 JSON 文件：`tasks/task-*.json`
- Manifest 文件：`tasks/manifest.json`
- 依赖图验证：检测缺失依赖和循环依赖（DFS 着色算法）

### AgentPool

会话池，管理 Claude 会话的复用策略：

1. **重试** → 恢复任务自己的会话（包含完整失败上下文）
2. **依赖链** → 继承最近完成的依赖任务的会话
3. **深度为 0 且无依赖** → 始终开始新会话
4. **其他** → 继续线程的滚动会话

### Orchestrator

主编排器，协调所有阶段：

- Phase 0: 交互式澄清（`--ask` 标志）
- Phase 1: 任务生成
- Phase 2: 评估复杂度和拆分
- Phase 3: 执行、测试、修复
- Phase 4: Git 提交
- Phase 5: 最终测试和总结

#### 编排器子模块

| 模块 | 功能 |
|------|------|
| `orchestrator.rs` | 主调度器，协调所有阶段执行 |
| `dependency_graph.rs` | 任务依赖关系管理，循环依赖检测（DFS 着色算法） |
| `health_monitor.rs` | 监控阻塞任务，生成会议清单（Meeting 清单） |
| `task_scheduler.rs` | 槽位池管理，任务并行调度（SlotPool + TaskScheduler） |
| `prompts.rs` | 集中管理 AI 提示词模板（语言配置、JSON 格式约束） |

## 执行流水线

```
┌─────────────────────────────────────────────────────────────────┐
│                     Task Execution Pipeline                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │  Assess  │───▶│ Execute  │───▶│   Test   │───▶│  Verify  │  │
│  │  & Split │    │          │    │          │    │          │  │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘  │
│       │               │               │               │         │
│       ▼               ▼               ▼               ▼         │
│   [complex?]     [claude CLI]    [test runner]   [acceptance]   │
│       │               │               │               │         │
│       ▼               ▼               ▼               ▼         │
│   [split]         [retry]         [fix/retry]     [commit]      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## 并发调度模型

采用分层槽位分配：

- `primary_slots = ceil(num_agents / 2)` — 主任务槽位
- `subtask_slots = num_agents - primary_slots` — 子任务槽位

主任务（depth=0）和子任务（depth>0）分开调度，避免子任务饿死主任务。

## 项目检测

支持的项目类型和测试运行器：

| 项目类型 | 标记文件 | 测试命令 |
|----------|----------|----------|
| Rust | Cargo.toml | `cargo test` |
| Node.js | package.json | `npm test` |
| Python | pytest.ini, pyproject.toml, test_*.py | `pytest -v` |
| Go | go.mod | `go test ./...` |
| Ruby | Gemfile | `bundle exec rspec` |
| PHP | composer.json | `phpunit` |
| Makefile | Makefile (test目标) | `make test` |

## CLI 接口

```
Usage: matrix <goal> [path] [OPTIONS]

Arguments:
  <goal>              项目目标描述
  [path]              输出路径（父目录或新目录）

Options:
  --doc <FILE>        规格/需求文档
  -d, --workspace     显式指定工作区目录
  --mcp-config <FILE> MCP 配置 JSON（用于 e2e 测试）
  --resume            恢复之前的运行
  -n, --agents <N>    并行代理数量（默认：1）
  --debug             流式输出 Claude 的实时输出
  --ask, -q           在规划前询问澄清问题
```

## 配置常量

```rust
pub const MAX_DEPTH: u32 = 5;              // 最大拆分深度
pub const MAX_RETRIES: u32 = 3;            // 最大重试次数
pub const TIMEOUT_PLAN: u64 = 120;         // 规划超时（秒）
pub const TIMEOUT_EXEC: u64 = 3600;        // 执行超时（秒）
pub const MAX_PROMPT_LENGTH: usize = 80000; // 最大 prompt 长度
pub const MAX_WORKSPACE_FILES: usize = 100; // 最大文件列表数量
```

## Taskfile 命令

| 命令 | 描述 |
|------|------|
| `task build` | 构建所有 workspace crates |
| `task test` | 运行所有测试 |
| `task install` | 一键安装到全局 |
| `task install-local` | 安装到用户目录 |
| `task run` | 运行 CLI |
| `task lint` | 运行 clippy 检查 |
| `task fmt` | 格式化代码 |

## 测试策略

- **单元测试**：每个模块有详细的单元测试
- **集成测试**：关键流程有端到端测试
- 测试重点：
  - 依赖图验证（循环检测）
  - JSON 解析（从 Claude 输出提取）
  - 项目类型检测
  - 任务状态流转

## 与 Python 版本的差异

| 方面 | Python 版本 | Rust 版本 |
|------|-------------|-----------|
| 并发 | ThreadPoolExecutor | Tokio + spawn_blocking |
| 类型系统 | 动态类型 | 静态类型 + serde |
| 错误处理 | 异常 | Result<T, Error> |
| 日志 | print + 文件 | tracing 结构化日志 |
| 数据格式 | 兼容 | 独立（不与 Python 版互操作） |

## 未来扩展

1. **Claude Code 插件**：通过 IPC 或 FFI 与核心库通信
2. **GUI 客户端**：使用 egui 或 Tauri
3. **Web 客户端**：通过 HTTP API 暴露功能

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| Claude CLI 输出解析失败 | 多层 JSON 提取策略 |
| 并发竞态条件 | 使用 Arc<Mutex> 保护共享状态 |
| 跨平台兼容性 | 使用标准库和跨平台 crate |
| 大型项目性能 | 限制文件列表和 prompt 大小 |