# Matrix - Rust Agent Orchestrator 实现计划

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 使用 Rust 1:1 复刻 longtime.py，创建一个使用 Claude CLI 自主开发软件项目的 AI 代理编排系统。

**Architecture:** 采用 Rust workspace 架构，核心逻辑在 `matrix-core` crate 中，CLI 入口在 `matrix-cli` crate 中。使用 Tokio 异步运行时，spawn_blocking 处理阻塞操作。

**Tech Stack:** Rust 2021, Tokio, Clap, serde, thiserror/anyhow, tracing, chrono

---

## 文件结构

```
matrix/
├── Cargo.toml                    # Workspace 根配置
├── Taskfile.yml                  # 构建与部署自动化
├── crates/
│   ├── core/                     # 核心库
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs            # 公共 API 导出
│   │       ├── config.rs         # 配置常量
│   │       ├── error.rs          # 错误类型定义
│   │       ├── models/
│   │       │   ├── mod.rs
│   │       │   ├── task.rs       # Task, TaskStatus, Complexity
│   │       │   └── manifest.rs   # Manifest
│   │       ├── store/
│   │       │   ├── mod.rs
│   │       │   └── task_store.rs # TaskStore
│   │       ├── agent/
│   │       │   ├── mod.rs
│   │       │   ├── pool.rs       # AgentPool
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
│   │           └── orchestrator.rs
│   └── cli/                      # CLI 入口
│       ├── Cargo.toml
│       └── src/main.rs
└── tests/                        # 集成测试
    └── integration_test.rs
```

---

## Chunk 1: 项目初始化与基础配置

### Task 1.1: 创建 Workspace 根配置

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`

- [ ] **Step 1: 创建 Workspace Cargo.toml**

```toml
[workspace]
members = [
    "crates/core",
    "crates/cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/bigfish1913/matrix"
authors = ["bigfish1913"]
description = "Long-Running Agent Orchestrator using Claude CLI"
readme = "README.md"

[workspace.dependencies]
# 异步运行时
tokio = { version = "1", features = ["full"] }

# CLI
clap = { version = "4", features = ["derive"] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 错误处理
thiserror = "1"
anyhow = "1"

# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 时间
chrono = { version = "0.4", features = ["serde"] }

# 正则表达式
regex = "1"

# 文件遍历
walkdir = "2"

# 临时目录
tempfile = "3"

# 内部 crate
matrix-core = { path = "crates/core" }
```

- [ ] **Step 2: 创建 .gitignore**

```
# Rust
/target/
Cargo.lock

# IDE
.vscode/
.idea/
*.swp

# OS
.DS_Store
Thumbs.db

# Project specific
.tasks/
logs/
*.log
```

- [ ] **Step 3: 创建 crates 目录结构**

```bash
mkdir -p crates/core/src/models
mkdir -p crates/core/src/store
mkdir -p crates/core/src/agent
mkdir -p crates/core/src/detector
mkdir -p crates/core/src/executor
mkdir -p crates/core/src/orchestrator
mkdir -p crates/cli/src
mkdir -p tests
```

- [ ] **Step 4: 提交初始化**

```bash
git add Cargo.toml .gitignore
git commit -m "chore: initialize Rust workspace structure"
```

---

### Task 1.2: 创建 Taskfile

**Files:**
- Create: `Taskfile.yml`

- [ ] **Step 1: 创建 Taskfile.yml**

```yaml
version: "3"

vars:
  BINARY_NAME: matrix
  INSTALL_DIR: /usr/local/bin

tasks:
  default:
    desc: 显示帮助信息
    cmds:
      - task --list

  build:
    desc: 构建所有 workspace crates
    cmds:
      - cargo build --workspace --release

  dev:
    desc: 开发模式构建
    cmds:
      - cargo build --workspace

  test:
    desc: 运行所有测试
    cmds:
      - cargo test --workspace

  test-single:
    desc: 运行单个测试
    vars:
      TEST_NAME: ""
    cmds:
      - cargo test --workspace "{{.TEST_NAME}}"

  run:
    desc: 运行 CLI
    vars:
      GOAL: "测试项目"
    cmds:
      - cargo run -p matrix-cli -- "{{.GOAL}}"

  install:
    desc: 一键安装到全局
    cmds:
      - task: build
      - |
        if [ "$OSTYPE" = "msys" ] || [ "$OSTYPE" = "win32" ]; then
          cp target/release/matrix.exe ./matrix.exe 2>/dev/null || echo "Binary not found, run task build first"
          echo "Installed matrix.exe to current directory"
        else
          sudo cp target/release/matrix {{.INSTALL_DIR}}/matrix 2>/dev/null || cp target/release/matrix ./matrix
          sudo chmod +x {{.INSTALL_DIR}}/matrix 2>/dev/null || chmod +x ./matrix
          echo "Installed matrix"
        fi

  install-local:
    desc: 安装到用户目录
    cmds:
      - task: build
      - mkdir -p ~/.local/bin
      - cp target/release/matrix ~/.local/bin/matrix 2>/dev/null || cp target/release/matrix.exe ~/.local/bin/matrix.exe 2>/dev/null
      - chmod +x ~/.local/bin/matrix 2>/dev/null || true
      - echo "Installed to ~/.local/bin/"

  clean:
    desc: 清理构建产物
    cmds:
      - cargo clean

  fmt:
    desc: 格式化代码
    cmds:
      - cargo fmt --all

  lint:
    desc: 运行 clippy 检查
    cmds:
      - cargo clippy --workspace --all-targets -- -D warnings

  check:
    desc: 检查代码
    cmds:
      - cargo check --workspace

  watch:
    desc: 监听文件变化
    cmds:
      - cargo watch -x "build --workspace"
```

- [ ] **Step 2: 提交 Taskfile**

```bash
git add Taskfile.yml
git commit -m "chore: add Taskfile for build automation"
```

---

### Task 1.3: 创建 matrix-core crate

**Files:**
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`

- [ ] **Step 1: 创建 crates/core/Cargo.toml**

```toml
[package]
name = "matrix-core"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Core library for Matrix Agent Orchestrator"

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
chrono.workspace = true
regex.workspace = true
walkdir.workspace = true
tempfile.workspace = true

[dev-dependencies]
tokio-test = "0.4"
```

- [ ] **Step 2: 创建 crates/core/src/lib.rs**

```rust
//! Matrix Core - Long-Running Agent Orchestrator
//!
//! 一个使用 Claude CLI 自主开发软件项目的 AI 代理编排系统。

pub mod config;
pub mod error;
pub mod models;
pub mod store;
pub mod agent;
pub mod detector;
pub mod executor;
pub mod orchestrator;

pub use config::*;
pub use error::{Error, Result};
pub use models::{Task, TaskStatus, Complexity, Manifest};
pub use store::TaskStore;
pub use agent::{ClaudeRunner, ClaudeResult, AgentPool, SharedAgentPool};
pub use detector::{ProjectDetector, ProjectType, ProjectInfo, TestRunnerDetector, TestRunner};
pub use executor::{TaskExecutor, ExecutorConfig};
pub use orchestrator::{Orchestrator, OrchestratorConfig};
```

- [ ] **Step 3: 验证编译**

```bash
cargo check -p matrix-core
```

Expected: 编译失败（缺少模块文件，这是预期的）

- [ ] **Step 4: 提交**

```bash
git add crates/core/
git commit -m "feat(core): initialize matrix-core crate structure"
```

---

### Task 1.4: 创建 matrix-cli crate

**Files:**
- Create: `crates/cli/Cargo.toml`
- Create: `crates/cli/src/main.rs`

- [ ] **Step 1: 创建 crates/cli/Cargo.toml**

```toml
[package]
name = "matrix-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "CLI for Matrix Agent Orchestrator"

[[bin]]
name = "matrix"
path = "src/main.rs"

[dependencies]
matrix-core.workspace = true
tokio.workspace = true
clap.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
chrono.workspace = true
```

- [ ] **Step 2: 创建 crates/cli/src/main.rs (stub)**

```rust
//! Matrix CLI - Command-line interface for the Agent Orchestrator

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "matrix")]
#[command(author, version, about = "Long-Running Agent Orchestrator", long_about = None)]
struct Args {
    /// Project goal description
    goal: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _args = Args::parse();
    println!("Matrix CLI - stub");
    Ok(())
}
```

- [ ] **Step 3: 验证编译**

```bash
cargo check -p matrix-cli
```

Expected: 编译失败（缺少 matrix-core 模块，这是预期的）

- [ ] **Step 4: 提交**

```bash
git add crates/cli/
git commit -m "feat(cli): initialize matrix-cli crate stub"
```

---

## Chunk 2: Core 基础模块

### Task 2.1: 实现 config.rs

**Files:**
- Create: `crates/core/src/config.rs`
- Test: `crates/core/tests/config_test.rs`

- [ ] **Step 1: 编写测试**

创建 `crates/core/tests/config_test.rs`:

```rust
use matrix_core::{MAX_DEPTH, MAX_RETRIES, TIMEOUT_PLAN, TIMEOUT_EXEC, MAX_PROMPT_LENGTH};

#[test]
fn test_constants_have_expected_values() {
    assert_eq!(MAX_DEPTH, 3);
    assert_eq!(MAX_RETRIES, 3);
    assert_eq!(TIMEOUT_PLAN, 120);
    assert_eq!(TIMEOUT_EXEC, 3600);
    assert_eq!(MAX_PROMPT_LENGTH, 80000);
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p matrix-core config_test -- --nocapture
```

Expected: FAIL - 模块未找到

- [ ] **Step 3: 创建 config.rs**

```rust
//! Configuration constants for the Matrix orchestrator.

use std::fmt;

/// 可用的模型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Model {
    Fast,
    Smart,
}

impl Model {
    pub fn default_fast() -> Self {
        Self::Fast
    }

    pub fn default_smart() -> Self {
        Self::Smart
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fast => write!(f, "glm-5"),
            Self::Smart => write!(f, "glm-5"),
        }
    }
}

// ── 全局配置常量 ──────────────────────────────────────────────────

/// 最大任务拆分深度
pub const MAX_DEPTH: u32 = 3;

/// 最大重试次数
pub const MAX_RETRIES: u32 = 3;

/// 规划操作超时（秒）
pub const TIMEOUT_PLAN: u64 = 120;

/// 执行操作超时（秒）
pub const TIMEOUT_EXEC: u64 = 3600;

/// 最大 prompt 长度（字符）
pub const MAX_PROMPT_LENGTH: usize = 80000;

/// 最大工作区文件列表数量
pub const MAX_WORKSPACE_FILES: usize = 100;

/// 最大已完成上下文大小
pub const MAX_COMPLETED_CONTEXT: usize = 2000;

/// 最大内存文件大小
pub const MAX_MEMORY_SIZE: usize = 3000;

/// 最大文档内容大小
pub const MAX_DOC_SIZE: usize = 5000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_display() {
        assert_eq!(Model::Fast.to_string(), "glm-5");
        assert_eq!(Model::Smart.to_string(), "glm-5");
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_DEPTH, 3);
        assert_eq!(MAX_RETRIES, 3);
        assert_eq!(TIMEOUT_PLAN, 120);
        assert_eq!(TIMEOUT_EXEC, 3600);
        assert_eq!(MAX_PROMPT_LENGTH, 80000);
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p matrix-core config -- --nocapture
```

Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/config.rs crates/core/tests/config_test.rs
git commit -m "feat(core): add config module with constants"
```

---

### Task 2.2: 实现 error.rs

**Files:**
- Create: `crates/core/src/error.rs`

- [ ] **Step 1: 创建 error.rs**

```rust
//! Error types for the Matrix orchestrator.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Task generation failed: {0}")]
    TaskGeneration(String),

    #[error("Claude CLI error: {0}")]
    ClaudeCli(String),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Dependency error: {0}")]
    Dependency(String),

    #[error("Workspace error: {0}")]
    Workspace(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::ParseError("test error".to_string());
        assert_eq!(err.to_string(), "Parse error: test error");

        let err = Error::TaskNotFound("task-001".to_string());
        assert_eq!(err.to_string(), "Task not found: task-001");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Json(_)));
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test -p matrix-core error -- --nocapture
```

Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add crates/core/src/error.rs
git commit -m "feat(core): add error types with thiserror"
```

---

### Task 2.3: 实现 Task 模型

**Files:**
- Create: `crates/core/src/models/mod.rs`
- Create: `crates/core/src/models/task.rs`
- Create: `crates/core/src/models/manifest.rs`
- Test: `crates/core/tests/models_test.rs`

- [ ] **Step 1: 创建 models/mod.rs**

```rust
//! Data models for tasks and manifests.

mod task;
mod manifest;

pub use task::{Task, TaskStatus, Complexity};
pub use manifest::Manifest;
```

- [ ] **Step 2: 创建 models/task.rs**

```rust
//! Task model and related types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Task status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Task complexity enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Complexity {
    Unknown,
    Simple,
    Complex,
}

impl Default for Complexity {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Task model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task ID, e.g. "task-001"
    pub id: String,
    /// Short title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Current status
    #[serde(default)]
    pub status: TaskStatus,
    /// Parent task ID (when split)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Split depth (max 3)
    #[serde(default)]
    pub depth: u32,
    /// Assessed complexity
    #[serde(default)]
    pub complexity: Complexity,
    /// Number of retries
    #[serde(default)]
    pub retries: u32,
    /// Claude session ID for resumption
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Execution result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Test failure context for retry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_failure_context: Option<String>,
    /// Test output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_result: Option<String>,
    /// Dependencies (task IDs)
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Completion timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Verification result
    #[serde(default)]
    pub verification_result: HashMap<String, serde_json::Value>,
    /// Whether tests passed
    #[serde(default)]
    pub test_passed: bool,
    /// Files modified during execution
    #[serde(default)]
    pub modified_files: Vec<String>,
}

impl Task {
    /// Create a new task with the given ID, title, and description
    pub fn new(id: String, title: String, description: String) -> Self {
        Self {
            id,
            title,
            description,
            status: TaskStatus::default(),
            parent_id: None,
            depth: 0,
            complexity: Complexity::default(),
            retries: 0,
            session_id: None,
            result: None,
            error: None,
            test_failure_context: None,
            test_result: None,
            depends_on: Vec::new(),
            created_at: Utc::now(),
            completed_at: None,
            verification_result: HashMap::new(),
            test_passed: false,
            modified_files: Vec::new(),
        }
    }

    /// Create a subtask with parent reference
    pub fn subtask(id: String, title: String, description: String, parent_id: String, depth: u32) -> Self {
        let mut task = Self::new(id, title, description);
        task.parent_id = Some(parent_id);
        task.depth = depth;
        task
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_new() {
        let task = Task::new("task-001".to_string(), "Test".to_string(), "Description".to_string());
        assert_eq!(task.id, "task-001");
        assert_eq!(task.title, "Test");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.depth, 0);
        assert!(task.parent_id.is_none());
    }

    #[test]
    fn test_task_status_serde() {
        let status = TaskStatus::InProgress;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"in_progress\"");
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_task_serde() {
        let task = Task::new("task-001".to_string(), "Test".to_string(), "Description".to_string());
        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.title, task.title);
    }

    #[test]
    fn test_task_subtask() {
        let subtask = Task::subtask(
            "task-001-1".to_string(),
            "Subtask".to_string(),
            "Sub description".to_string(),
            "task-001".to_string(),
            1,
        );
        assert_eq!(subtask.parent_id, Some("task-001".to_string()));
        assert_eq!(subtask.depth, 1);
    }
}
```

- [ ] **Step 3: 创建 models/manifest.rs**

```rust
//! Manifest model for task tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Manifest for tracking overall progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Project goal
    pub goal: String,
    /// Total number of tasks
    pub total: usize,
    /// Number of completed tasks
    pub completed: usize,
    /// Number of failed tasks
    pub failed: usize,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// List of all task IDs
    pub tasks: Vec<String>,
}

impl Manifest {
    /// Create a new manifest with the given goal
    pub fn new(goal: String) -> Self {
        Self {
            goal,
            total: 0,
            completed: 0,
            failed: 0,
            updated_at: Utc::now(),
            tasks: Vec::new(),
        }
    }

    /// Update timestamp to now
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new("Test goal".to_string());
        assert_eq!(manifest.goal, "Test goal");
        assert_eq!(manifest.total, 0);
        assert!(manifest.tasks.is_empty());
    }

    #[test]
    fn test_manifest_serde() {
        let manifest = Manifest::new("Test goal".to_string());
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.goal, manifest.goal);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p matrix-core models -- --nocapture
```

Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/models/
git commit -m "feat(core): add Task and Manifest models with serde"
```

---

## Chunk 3: Store 模块

### Task 3.1: 实现 TaskStore

**Files:**
- Create: `crates/core/src/store/mod.rs`
- Create: `crates/core/src/store/task_store.rs`

- [ ] **Step 1: 创建 store/mod.rs**

```rust
//! Task storage layer.

mod task_store;

pub use task_store::TaskStore;
```

- [ ] **Step 2: 创建 store/task_store.rs**

```rust
//! TaskStore - persistent storage for tasks.

use crate::error::{Error, Result};
use crate::models::{Manifest, Task, TaskStatus};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, warn};

/// Task storage manager
pub struct TaskStore {
    tasks_dir: PathBuf,
    manifest_path: PathBuf,
}

impl TaskStore {
    /// Create a new TaskStore
    pub async fn new(tasks_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&tasks_dir).await?;
        let manifest_path = tasks_dir.join("manifest.json");
        Ok(Self {
            tasks_dir,
            manifest_path,
        })
    }

    /// Save a task to disk
    pub async fn save_task(&self, task: &Task) -> Result<()> {
        let path = self.tasks_dir.join(format!("{}.json", task.id));
        let content = serde_json::to_string_pretty(task)?;
        fs::write(&path, content).await?;
        debug!(task_id = %task.id, "Task saved");
        Ok(())
    }

    /// Load a task by ID
    pub async fn load_task(&self, id: &str) -> Result<Task> {
        let path = self.tasks_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&path).await
            .map_err(|e| Error::TaskNotFound(format!("{}: {}", id, e)))?;
        let task: Task = serde_json::from_str(&content)?;
        Ok(task)
    }

    /// Get all tasks
    pub async fn all_tasks(&self) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(&self.tasks_dir)
            .await?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|e| e == "json").unwrap_or(false)
                    && p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("task-"))
                        .unwrap_or(false)
            })
            .collect();

        entries.sort();

        for path in entries {
            let content = fs::read_to_string(&path).await?;
            if let Ok(task) = serde_json::from_str::<Task>(&content) {
                tasks.push(task);
            }
        }

        Ok(tasks)
    }

    /// Get pending tasks
    pub async fn pending_tasks(&self) -> Result<Vec<Task>> {
        let tasks = self.all_tasks().await?;
        Ok(tasks.into_iter().filter(|t| t.status == TaskStatus::Pending).collect())
    }

    /// Count tasks by status
    pub async fn count(&self, status: TaskStatus) -> Result<usize> {
        let tasks = self.all_tasks().await?;
        Ok(tasks.iter().filter(|t| t.status == status).count())
    }

    /// Get total number of tasks
    pub async fn total(&self) -> Result<usize> {
        Ok(self.all_tasks().await?.len())
    }

    /// Validate dependency graph
    pub async fn validate_dependencies(&self) -> Vec<String> {
        let tasks = match self.all_tasks().await {
            Ok(t) => t,
            Err(_) => return vec!["Failed to load tasks".to_string()],
        };

        let mut warnings = Vec::new();
        let task_ids: HashSet<_> = tasks.iter().map(|t| t.id.clone()).collect();

        // Check for missing dependencies
        for task in &tasks {
            for dep in &task.depends_on {
                if !task_ids.contains(dep) {
                    warnings.push(format!("[{}] depends on missing task [{}]", task.id, dep));
                }
            }
        }

        // Detect cycles using DFS with coloring
        #[derive(Clone, Copy, PartialEq)]
        enum Color { White, Grey, Black }

        let mut colors: HashMap<String, Color> = tasks.iter()
            .map(|t| (t.id.clone(), Color::White))
            .collect();

        let dep_map: HashMap<String, Vec<String>> = tasks.iter()
            .map(|t| (t.id.clone(), t.depends_on.clone()))
            .collect();

        fn dfs(
            task_id: &str,
            path: &[String],
            colors: &mut HashMap<String, Color>,
            dep_map: &HashMap<String, Vec<String>>,
            warnings: &mut Vec<String>,
        ) {
            colors.insert(task_id.to_string(), Color::Grey);

            if let Some(deps) = dep_map.get(task_id) {
                for dep in deps {
                    if !colors.contains_key(dep) {
                        continue;
                    }
                    match colors.get(dep) {
                        Some(Color::Grey) => {
                            let mut cycle: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
                            cycle.push(dep);
                            warnings.push(format!("Circular dependency detected: {}", cycle.join(" → ")));
                        }
                        Some(Color::White) => {
                            let mut new_path = path.to_vec();
                            new_path.push(dep.clone());
                            dfs(dep, &new_path, colors, dep_map, warnings);
                        }
                        _ => {}
                    }
                }
            }

            colors.insert(task_id.to_string(), Color::Black);
        }

        for task in &tasks {
            if colors.get(&task.id) == Some(&Color::White) {
                dfs(&task.id, &[task.id.clone()], &mut colors, &dep_map, &mut warnings);
            }
        }

        warnings
    }

    /// Save manifest
    pub async fn save_manifest(&self, goal: &str) -> Result<()> {
        let tasks = self.all_tasks().await?;
        let completed = self.count(TaskStatus::Completed).await?;
        let failed = self.count(TaskStatus::Failed).await?;

        let manifest = Manifest {
            goal: goal.to_string(),
            total: tasks.len(),
            completed,
            failed,
            updated_at: chrono::Utc::now(),
            tasks: tasks.iter().map(|t| t.id.clone()).collect(),
        };

        let content = serde_json::to_string_pretty(&manifest)?;
        fs::write(&self.manifest_path, content).await?;
        Ok(())
    }

    /// Load manifest
    pub async fn load_manifest(&self) -> Result<Option<Manifest>> {
        if !self.manifest_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&self.manifest_path).await?;
        let manifest: Manifest = serde_json::from_str(&content)?;
        Ok(Some(manifest))
    }

    /// Clear all tasks (for fresh start)
    pub async fn clear(&self) -> Result<()> {
        let entries: Vec<_> = fs::read_dir(&self.tasks_dir)
            .await?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .collect();

        for path in entries {
            fs::remove_file(&path).await?;
        }

        if self.manifest_path.exists() {
            fs::remove_file(&self.manifest_path).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_task_store_save_and_load() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let task = Task::new("task-001".to_string(), "Test".to_string(), "Description".to_string());
        store.save_task(&task).await.unwrap();

        let loaded = store.load_task("task-001").await.unwrap();
        assert_eq!(loaded.id, "task-001");
        assert_eq!(loaded.title, "Test");
    }

    #[tokio::test]
    async fn test_task_store_all_tasks() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let task1 = Task::new("task-001".to_string(), "Task 1".to_string(), "D1".to_string());
        let task2 = Task::new("task-002".to_string(), "Task 2".to_string(), "D2".to_string());
        store.save_task(&task1).await.unwrap();
        store.save_task(&task2).await.unwrap();

        let all = store.all_tasks().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_task_store_validate_dependencies() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let mut task1 = Task::new("task-001".to_string(), "T1".to_string(), "D1".to_string());
        task1.depends_on = vec!["task-002".to_string()]; // Missing dependency

        store.save_task(&task1).await.unwrap();

        let warnings = store.validate_dependencies().await;
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("missing task"));
    }

    #[tokio::test]
    async fn test_task_store_circular_dependency() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let mut task1 = Task::new("task-001".to_string(), "T1".to_string(), "D1".to_string());
        task1.depends_on = vec!["task-002".to_string()];

        let mut task2 = Task::new("task-002".to_string(), "T2".to_string(), "D2".to_string());
        task2.depends_on = vec!["task-001".to_string()]; // Circular!

        store.save_task(&task1).await.unwrap();
        store.save_task(&task2).await.unwrap();

        let warnings = store.validate_dependencies().await;
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("Circular dependency")));
    }

    #[tokio::test]
    async fn test_task_store_manifest() {
        let dir = tempdir().unwrap();
        let store = TaskStore::new(dir.path().to_path_buf()).await.unwrap();

        let task = Task::new("task-001".to_string(), "T1".to_string(), "D1".to_string());
        store.save_task(&task).await.unwrap();
        store.save_manifest("Test goal").await.unwrap();

        let manifest = store.load_manifest().await.unwrap().unwrap();
        assert_eq!(manifest.goal, "Test goal");
        assert_eq!(manifest.total, 1);
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core store -- --nocapture
```

Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/store/
git commit -m "feat(core): add TaskStore with persistence and dependency validation"
```

---

## Chunk 4: Agent 模块

### Task 4.1: 实现 ClaudeRunner

**Files:**
- Create: `crates/core/src/agent/mod.rs`
- Create: `crates/core/src/agent/claude_runner.rs`

- [ ] **Step 1: 创建 agent/mod.rs**

```rust
//! Agent module - Claude CLI runner and session pool.

mod claude_runner;
mod pool;

pub use claude_runner::{ClaudeRunner, ClaudeResult};
pub use pool::{AgentPool, SharedAgentPool};
```

- [ ] **Step 2: 创建 agent/claude_runner.rs**

```rust
//! Claude CLI runner - handles subprocess calls to claude.

use crate::config::MAX_PROMPT_LENGTH;
use crate::error::{Error, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, warn};

/// Result from a Claude CLI call
#[derive(Debug, Clone)]
pub struct ClaudeResult {
    pub text: String,
    pub is_error: bool,
    pub session_id: Option<String>,
}

/// Claude CLI runner
#[derive(Debug, Clone)]
pub struct ClaudeRunner {
    model: String,
    debug_mode: bool,
}

impl ClaudeRunner {
    /// Create a new ClaudeRunner with default model
    pub fn new() -> Self {
        Self {
            model: "glm-5".to_string(),
            debug_mode: false,
        }
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Enable debug mode
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug_mode = debug;
        self
    }

    /// Call Claude CLI with a prompt
    pub async fn call(
        &self,
        prompt: &str,
        workdir: &Path,
        timeout_secs: Option<u64>,
        mcp_config: Option<&Path>,
        resume_session_id: Option<&str>,
    ) -> Result<ClaudeResult> {
        let timeout_duration = Duration::from_secs(timeout_secs.unwrap_or(120));

        // Truncate prompt if too long
        let prompt = if prompt.len() > MAX_PROMPT_LENGTH {
            warn!(len = prompt.len(), max = MAX_PROMPT_LENGTH, "Prompt truncated");
            truncate_prompt_safely(prompt, MAX_PROMPT_LENGTH)
        } else {
            prompt.to_string()
        };

        // Build command
        let mut cmd = Command::new("claude");
        cmd.args(["--model", &self.model])
            .args(["--output-format", if self.debug_mode { "stream-json" } else { "json" }])
            .arg("--dangerously-skip-permissions")
            .arg("-p")
            .current_dir(workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(mcp) = mcp_config {
            if let Some(mcp_str) = mcp.to_str() {
                cmd.args(["--mcp-config", mcp_str]);
            }
        }

        if let Some(sid) = resume_session_id {
            cmd.args(["--resume", sid]);
        }

        debug!(model = %self.model, "Calling Claude CLI");

        let result = timeout(timeout_duration, async {
            let mut child = cmd.spawn()
                .map_err(|e| Error::ClaudeCli(format!("Failed to spawn: {}", e)))?;

            // Write prompt to stdin
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(prompt.as_bytes()).await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to write stdin: {}", e)))?;
                stdin.shutdown().await
                    .map_err(|e| Error::ClaudeCli(format!("Failed to close stdin: {}", e)))?;
            }

            // Collect output
            let output = child.wait_with_output().await
                .map_err(|e| Error::ClaudeCli(format!("Failed to wait: {}", e)))?;

            Ok::<_, Error>(output)
        })
        .await
        .map_err(|_| Error::Timeout("Claude call timed out".to_string()))??;

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // Parse JSON result
        parse_claude_result(&combined)
    }
}

impl Default for ClaudeRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate prompt safely, preserving structure
fn truncate_prompt_safely(prompt: &str, max_length: usize) -> String {
    if prompt.len() <= max_length {
        return prompt.to_string();
    }

    let keep_len = (max_length - 100) / 2;
    format!(
        "{}\n\n... (content truncated) ...\n\n{}",
        &prompt[..keep_len],
        &prompt[prompt.len().saturating_sub(keep_len)..]
    )
}

/// Parse Claude CLI output to extract result
fn parse_claude_result(output: &str) -> Result<ClaudeResult> {
    // Try to extract JSON from code block
    if let Some(json) = extract_json_from_code_block(output) {
        if let Ok(result) = parse_result_json(&json) {
            return Ok(result);
        }
    }

    // Try to parse each line as JSON
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(result) = parse_result_json(line) {
            return Ok(result);
        }
    }

    // Try to parse entire output as JSON
    if let Ok(result) = parse_result_json(output) {
        return Ok(result);
    }

    Err(Error::ParseError(format!(
        "No valid JSON result from Claude. Output: {}",
        &output[..output.len().min(500)]
    )))
}

/// Extract JSON from ```json code block
fn extract_json_from_code_block(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"```json\s*(\{.*\})\s*```").ok()?;
    let caps = re.captures(text)?;
    Some(caps[1].to_string())
}

/// Parse a JSON string into ClaudeResult
fn parse_result_json(json: &str) -> Result<ClaudeResult> {
    let obj: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| Error::ParseError(format!("JSON parse error: {}", e)))?;

    let is_error = obj.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
    let text = obj.get("result").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let session_id = obj.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    Ok(ClaudeResult {
        text,
        is_error,
        session_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_prompt_safely() {
        let long_prompt = "x".repeat(100000);
        let truncated = truncate_prompt_safely(&long_prompt, 80000);
        assert!(truncated.len() <= 80000);
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn test_parse_result_json() {
        let json = r#"{"result": "Hello", "is_error": false, "session_id": "abc123"}"#;
        let result = parse_result_json(json).unwrap();
        assert_eq!(result.text, "Hello");
        assert!(!result.is_error);
        assert_eq!(result.session_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_parse_result_json_error() {
        let json = r#"{"result": "Error message", "is_error": true}"#;
        let result = parse_result_json(json).unwrap();
        assert!(result.is_error);
        assert_eq!(result.text, "Error message");
    }

    #[test]
    fn test_extract_json_from_code_block() {
        let text = r#"```json
{"result": "test", "is_error": false}
```"#;
        let json = extract_json_from_code_block(text).unwrap();
        assert!(json.contains("result"));
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core claude_runner -- --nocapture
```

Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/agent/claude_runner.rs crates/core/src/agent/mod.rs
git commit -m "feat(core): add ClaudeRunner for CLI subprocess calls"
```

---

### Task 4.2: 实现 AgentPool

**Files:**
- Create: `crates/core/src/agent/pool.rs`

- [ ] **Step 1: 创建 agent/pool.rs**

```rust
//! AgentPool - session pool for Claude session reuse.

use crate::models::Task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// Session pool for Claude session reuse
///
/// Session reuse policy (in priority order):
/// 1. Retry → resume the task's own session (full failure context)
/// 2. Dependency chain → inherit the most-recently-completed dependency's session
/// 3. Depth-0 task with no dependencies → always start fresh
/// 4. Anything else → continue the thread's rolling session
#[derive(Debug, Default)]
pub struct AgentPool {
    /// task_id → session_id (recorded when execution finishes successfully)
    task_sessions: HashMap<String, String>,
    /// thread_name → session_id (rolling "last session" per worker thread)
    thread_sessions: HashMap<String, String>,
}

impl AgentPool {
    /// Create a new AgentPool
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the best session_id to resume for a task, or None for fresh
    pub fn get_session(&self, task: &Task, thread_name: &str) -> Option<String> {
        // 1. Retry: own session has the full failure context
        if task.retries > 0 {
            if let Some(ref sid) = task.session_id {
                if !sid.is_empty() {
                    debug!(task_id = %task.id, session_id = %sid, "Resuming own session for retry");
                    return Some(sid.clone());
                }
            }
        }

        // 2. Dependency chain: inherit last dependency's session
        for dep_id in task.depends_on.iter().rev() {
            if let Some(sid) = self.task_sessions.get(dep_id) {
                if !sid.is_empty() {
                    debug!(task_id = %task.id, dep_id = %dep_id, "Inheriting dependency session");
                    return Some(sid.clone());
                }
            }
        }

        // 3. Depth-0 + no deps → fresh (prevent unrelated context bleed)
        if task.depth == 0 && task.depends_on.is_empty() {
            debug!(task_id = %task.id, "Starting fresh session for depth-0 task");
            return None;
        }

        // 4. Thread's rolling session
        if let Some(sid) = self.thread_sessions.get(thread_name) {
            if !sid.is_empty() {
                debug!(task_id = %task.id, thread = %thread_name, "Using thread rolling session");
                return Some(sid.clone());
            }
        }

        None
    }

    /// Record a session after successful execution
    pub fn record(&mut self, task: &Task, session_id: &str, thread_name: &str) {
        if session_id.is_empty() {
            return;
        }
        self.task_sessions.insert(task.id.clone(), session_id.to_string());
        self.thread_sessions.insert(thread_name.to_string(), session_id.to_string());
        debug!(task_id = %task.id, thread = %thread_name, "Session recorded");
    }

    /// Clear a thread's rolling session
    pub fn clear_thread(&mut self, thread_name: &str) {
        self.thread_sessions.remove(thread_name);
        debug!(thread = %thread_name, "Thread session cleared");
    }

    /// Get statistics
    pub fn stats(&self) -> String {
        format!(
            "pool: {} task sessions | {} active thread sessions",
            self.task_sessions.len(),
            self.thread_sessions.len()
        )
    }
}

/// Thread-safe wrapper for AgentPool
#[derive(Debug, Clone)]
pub struct SharedAgentPool {
    inner: Arc<Mutex<AgentPool>>,
}

impl SharedAgentPool {
    /// Create a new shared AgentPool
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AgentPool::new())),
        }
    }

    /// Get session for a task
    pub async fn get_session(&self, task: &Task, thread_name: &str) -> Option<String> {
        self.inner.lock().await.get_session(task, thread_name)
    }

    /// Record a session
    pub async fn record(&self, task: &Task, session_id: &str, thread_name: &str) {
        self.inner.lock().await.record(task, session_id, thread_name);
    }

    /// Clear a thread's session
    pub async fn clear_thread(&self, thread_name: &str) {
        self.inner.lock().await.clear_thread(thread_name);
    }

    /// Get stats
    pub async fn stats(&self) -> String {
        self.inner.lock().await.stats()
    }
}

impl Default for SharedAgentPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaskStatus;

    #[test]
    fn test_agent_pool_retry_session() {
        let mut pool = AgentPool::new();
        let mut task = Task::new("task-001".to_string(), "Test".to_string(), "D".to_string());
        task.retries = 1;
        task.session_id = Some("session-123".to_string());

        pool.record(&Task::new("other".into(), "O".into(), "D".into()), "session-456", "thread-1");

        let session = pool.get_session(&task, "thread-1");
        assert_eq!(session, Some("session-123".to_string()));
    }

    #[test]
    fn test_agent_pool_dependency_session() {
        let mut pool = AgentPool::new();
        let dep_task = Task::new("task-001".to_string(), "Dep".to_string(), "D".to_string());
        pool.record(&dep_task, "session-abc", "thread-1");

        let mut task = Task::new("task-002".to_string(), "Test".to_string(), "D".to_string());
        task.depends_on = vec!["task-001".to_string()];

        let session = pool.get_session(&task, "thread-1");
        assert_eq!(session, Some("session-abc".to_string()));
    }

    #[test]
    fn test_agent_pool_fresh_for_depth_zero() {
        let pool = AgentPool::new();
        let task = Task::new("task-001".to_string(), "Test".to_string(), "D".to_string());

        let session = pool.get_session(&task, "thread-1");
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_shared_agent_pool() {
        let pool = SharedAgentPool::new();
        let task = Task::new("task-001".to_string(), "Test".to_string(), "D".to_string());

        pool.record(&task, "session-xyz", "thread-1").await;

        let stats = pool.stats().await;
        assert!(stats.contains("1 task sessions"));
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test -p matrix-core pool -- --nocapture
```

Expected: PASS

---

## Chunk 5: Detector 模块

### Task 5.1: 实现 ProjectDetector

**Files:**
- Create: `crates/core/src/detector/mod.rs`
- Create: `crates/core/src/detector/project.rs`

- [ ] **Step 1: 创建 detector/mod.rs**

```rust
//! Project and test runner detection.

mod project;
mod test_runner;

pub use project::{ProjectDetector, ProjectType, ProjectInfo};
pub use test_runner::{TestRunnerDetector, TestRunner};
```

- [ ] **Step 2: 创建 detector/project.rs**

```rust
//! Project type detection.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported project types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    NodeJs,
    Python,
    Go,
    Ruby,
    Php,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::NodeJs => write!(f, "Node.js"),
            Self::Python => write!(f, "Python"),
            Self::Go => write!(f, "Go"),
            Self::Ruby => write!(f, "Ruby"),
            Self::Php => write!(f, "PHP"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Project information
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub project_type: ProjectType,
    pub package_manager: Option<String>,
    pub install_command: Option<Vec<String>>,
}

/// Project type detector
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect project type in workspace
    pub fn detect(workspace: &Path) -> ProjectInfo {
        // Rust
        if workspace.join("Cargo.toml").exists() {
            return ProjectInfo {
                project_type: ProjectType::Rust,
                package_manager: Some("cargo".to_string()),
                install_command: Some(vec!["cargo".to_string(), "fetch".to_string()]),
            };
        }

        // Node.js
        if workspace.join("package.json").exists() {
            let pm = Self::detect_node_package_manager(workspace);
            return ProjectInfo {
                project_type: ProjectType::NodeJs,
                package_manager: Some(pm.clone()),
                install_command: Some(vec![pm, "install".to_string()]),
            };
        }

        // Python
        if Self::is_python_project(workspace) {
            return ProjectInfo {
                project_type: ProjectType::Python,
                package_manager: Some("pip".to_string()),
                install_command: Self::detect_python_install_command(workspace),
            };
        }

        // Go
        if workspace.join("go.mod").exists() {
            return ProjectInfo {
                project_type: ProjectType::Go,
                package_manager: Some("go".to_string()),
                install_command: Some(vec!["go".to_string(), "mod".to_string(), "download".to_string()]),
            };
        }

        // Ruby
        if workspace.join("Gemfile").exists() {
            return ProjectInfo {
                project_type: ProjectType::Ruby,
                package_manager: Some("bundler".to_string()),
                install_command: Some(vec!["bundle".to_string(), "install".to_string()]),
            };
        }

        // PHP
        if workspace.join("composer.json").exists() {
            return ProjectInfo {
                project_type: ProjectType::Php,
                package_manager: Some("composer".to_string()),
                install_command: Some(vec!["composer".to_string(), "install".to_string()]),
            };
        }

        ProjectInfo {
            project_type: ProjectType::Unknown,
            package_manager: None,
            install_command: None,
        }
    }

    fn detect_node_package_manager(workspace: &Path) -> String {
        if workspace.join("bun.lockb").exists() {
            "bun".to_string()
        } else if workspace.join("pnpm-lock.yaml").exists() {
            "pnpm".to_string()
        } else if workspace.join("yarn.lock").exists() {
            "yarn".to_string()
        } else {
            "npm".to_string()
        }
    }

    fn is_python_project(workspace: &Path) -> bool {
        workspace.join("requirements.txt").exists()
            || workspace.join("pyproject.toml").exists()
            || workspace.join("setup.py").exists()
            || workspace.join("setup.cfg").exists()
            || has_files_matching(workspace, "test_*.py")
            || has_files_matching(workspace, "*_test.py")
    }

    fn detect_python_install_command(workspace: &Path) -> Option<Vec<String>> {
        if workspace.join("requirements.txt").exists() {
            Some(vec!["pip".to_string(), "install".to_string(), "-r".to_string(), "requirements.txt".to_string()])
        } else if workspace.join("pyproject.toml").exists() {
            Some(vec!["pip".to_string(), "install".to_string(), "-e".to_string(), ".".to_string()])
        } else {
            None
        }
    }
}

/// Check if directory has files matching pattern
fn has_files_matching(dir: &Path, pattern: &str) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            if let Some(name) = e.file_name().to_str() {
                if pattern.starts_with('*') {
                    name.ends_with(&pattern[1..])
                } else if pattern.ends_with('*') {
                    name.starts_with(&pattern[..pattern.len() - 1])
                } else {
                    name == pattern
                }
            } else {
                false
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_rust_project() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.project_type, ProjectType::Rust);
        assert_eq!(info.package_manager, Some("cargo".to_string()));
    }

    #[test]
    fn test_detect_node_project() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.project_type, ProjectType::NodeJs);
    }

    #[test]
    fn test_detect_unknown_project() {
        let dir = tempdir().unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.project_type, ProjectType::Unknown);
    }

    #[test]
    fn test_detect_node_package_manager() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let info = ProjectDetector::detect(dir.path());
        assert_eq!(info.package_manager, Some("yarn".to_string()));
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core project -- --nocapture
```

Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/detector/
git commit -m "feat(core): add ProjectDetector for project type detection"
```

---

### Task 5.2: 实现 TestRunnerDetector

**Files:**
- Create: `crates/core/src/detector/test_runner.rs`

- [ ] **Step 1: 创建 detector/test_runner.rs**

```rust
//! Test runner detection.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Test runner configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestRunner {
    pub name: String,
    pub command: Vec<String>,
}

/// Test runner detector
pub struct TestRunnerDetector;

impl TestRunnerDetector {
    /// Detect test runner in workspace
    pub fn detect(workspace: &Path) -> Option<TestRunner> {
        // Go
        if workspace.join("go.mod").exists() {
            return Some(TestRunner {
                name: "go".to_string(),
                command: vec!["go".to_string(), "test".to_string(), "./...".to_string()],
            });
        }

        // Rust
        if workspace.join("Cargo.toml").exists() {
            return Some(TestRunner {
                name: "cargo".to_string(),
                command: vec!["cargo".to_string(), "test".to_string()],
            });
        }

        // Node.js
        if workspace.join("package.json").exists() {
            if let Ok(content) = std::fs::read_to_string(workspace.join("package.json")) {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                    if pkg.get("scripts").and_then(|s| s.get("test")).is_some() {
                        return Some(TestRunner {
                            name: "npm".to_string(),
                            command: vec![
                                "npm".to_string(),
                                "test".to_string(),
                                "--".to_string(),
                                "--passWithNoTests".to_string(),
                            ],
                        });
                    }
                }
            }
        }

        // Python
        for marker in ["pytest.ini", "setup.cfg", "pyproject.toml", "setup.py"] {
            if workspace.join(marker).exists() {
                return Some(TestRunner {
                    name: "pytest".to_string(),
                    command: vec![
                        "python".to_string(),
                        "-m".to_string(),
                        "pytest".to_string(),
                        "-v".to_string(),
                        "--tb=short".to_string(),
                    ],
                });
            }
        }

        // Python test files
        if has_python_test_files(workspace) {
            return Some(TestRunner {
                name: "pytest".to_string(),
                command: vec![
                    "python".to_string(),
                    "-m".to_string(),
                    "pytest".to_string(),
                    "-v".to_string(),
                    "--tb=short".to_string(),
                ],
            });
        }

        // Makefile
        if workspace.join("Makefile").exists() {
            if let Ok(content) = std::fs::read_to_string(workspace.join("Makefile")) {
                if content.lines().any(|line| line.starts_with("test:")) {
                    return Some(TestRunner {
                        name: "make".to_string(),
                        command: vec!["make".to_string(), "test".to_string()],
                    });
                }
            }
        }

        None
    }

    /// Detect test runners in workspace and subdirectories
    pub fn detect_with_subdirs(workspace: &Path) -> Vec<(String, TestRunner)> {
        let mut runners = Vec::new();

        if let Some(runner) = Self::detect(workspace) {
            runners.push((".".to_string(), runner));
        }

        for subdir in ["backend", "frontend", "server", "client", "api"] {
            let sub = workspace.join(subdir);
            if sub.exists() {
                if let Some(runner) = Self::detect(&sub) {
                    runners.push((subdir.to_string(), runner));
                }
            }
        }

        runners
    }
}

fn has_python_test_files(workspace: &Path) -> bool {
    walkdir::WalkDir::new(workspace)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            if let Some(name) = e.file_name().to_str() {
                name.starts_with("test_") || name.ends_with("_test.py")
            } else {
                false
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_cargo_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "cargo");
        assert_eq!(runner.command, vec!["cargo", "test"]);
    }

    #[test]
    fn test_detect_go_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "").unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "go");
    }

    #[test]
    fn test_detect_npm_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"scripts": {"test": "jest"}}"#).unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "npm");
    }

    #[test]
    fn test_detect_makefile_test() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Makefile"), "test:\n\techo test\n").unwrap();

        let runner = TestRunnerDetector::detect(dir.path()).unwrap();
        assert_eq!(runner.name, "make");
    }

    #[test]
    fn test_no_test_runner() {
        let dir = tempdir().unwrap();
        let runner = TestRunnerDetector::detect(dir.path());
        assert!(runner.is_none());
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test -p matrix-core test_runner -- --nocapture
```

Expected: PASS

- [ ] **Step 3: 提交**

```bash
git add crates/core/src/detector/test_runner.rs
git commit -m "feat(core): add TestRunnerDetector for test framework detection"
```

---

## Chunk 6: Executor 模块

### Task 6.1: 实现 TaskExecutor

**Files:**
- Create: `crates/core/src/executor/mod.rs`
- Create: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: 创建 executor/mod.rs**

```rust
//! Task execution module.

mod task_executor;

pub use task_executor::{TaskExecutor, ExecutorConfig};
```

- [ ] **Step 2: 创建 executor/task_executor.rs**

```rust
//! TaskExecutor - handles task execution, testing, and fixing.

use crate::agent::{ClaudeResult, ClaudeRunner, SharedAgentPool};
use crate::config::{MAX_PROMPT_LENGTH, MAX_WORKSPACE_FILES, TIMEOUT_EXEC, TIMEOUT_PLAN};
use crate::detector::{ProjectDetector, ProjectInfo, TestRunnerDetector};
use crate::error::Result;
use crate::models::{Task, TaskStatus};
use crate::store::TaskStore;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, info, warn, step};

/// Executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub model_fast: String,
    pub model_smart: String,
    pub doc_content: Option<String>,
    pub mcp_config: Option<PathBuf>,
    pub debug_mode: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            model_fast: "glm-5".to_string(),
            model_smart: "glm-5".to_string(),
            doc_content: None,
            mcp_config: None,
            debug_mode: false,
        }
    }
}

/// Task executor
pub struct TaskExecutor {
    workspace: PathBuf,
    store: Arc<TaskStore>,
    pub runner: ClaudeRunner,
    agent_pool: SharedAgentPool,
    config: ExecutorConfig,
    setup_done: bool,
}

impl TaskExecutor {
    /// Create a new TaskExecutor
    pub fn new(
        workspace: PathBuf,
        store: Arc<TaskStore>,
        agent_pool: SharedAgentPool,
        config: ExecutorConfig,
    ) -> Self {
        let runner = ClaudeRunner::new()
            .with_debug(config.debug_mode);

        Self {
            workspace,
            store,
            runner,
            agent_pool,
            config,
            setup_done: false,
        }
    }

    /// Setup workspace (install dependencies)
    pub async fn setup_workspace(&mut self) -> Result<()> {
        if self.setup_done {
            return Ok(());
        }
        self.setup_done = true;

        let files: Vec<_> = fs::read_dir(&self.workspace)
            .await?
            .filter_map(|e| e.ok())
            .collect();

        if files.is_empty() {
            info!("Empty workspace, skipping setup");
            return Ok(());
        }

        step!("Setting up workspace environment...");

        let info = ProjectDetector::detect(&self.workspace);
        if let Some(cmd) = &info.install_command {
            self.run_install_command(cmd).await?;
        }

        // Check subdirectories
        for subdir in ["backend", "frontend", "server", "client", "api"] {
            let sub = self.workspace.join(subdir);
            if sub.exists() {
                let sub_info = ProjectDetector::detect(&sub);
                if let Some(cmd) = &sub_info.install_command {
                    info!(subdir = subdir, "Running install in subdirectory");
                    self.run_install_command_in_subdir(cmd, subdir).await?;
                }
            }
        }

        Ok(())
    }

    async fn run_install_command(&self, cmd: &[String]) -> Result<()> {
        info!(command = ?cmd, "Running install");

        let result = Command::new(&cmd[0])
            .args(&cmd[1..])
            .current_dir(&self.workspace)
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                info!("Install succeeded");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(stderr = %stderr, "Install had warnings");
            }
            Err(e) => {
                warn!(error = %e, "Install command failed");
            }
        }

        Ok(())
    }

    async fn run_install_command_in_subdir(&self, cmd: &[String], subdir: &str) -> Result<()> {
        // For subdirs, we need to cd first
        let shell_cmd = if cfg!(windows) {
            format!("cd {} && {}", subdir, cmd.join(" "))
        } else {
            format!("cd {} && {}", subdir, cmd.join(" "))
        };

        self.run_install_command(&["sh".to_string(), "-c".to_string(), shell_cmd]).await
    }

    /// Execute a task
    pub async fn execute(&self, task: &mut Task, thread_name: &str) -> Result<bool> {
        let model = if task.complexity == crate::models::Complexity::Complex {
            &self.config.model_smart
        } else {
            &self.config.model_fast
        };

        step!(task_id = %task.id, title = %task.title, model = %model, "Executing task");

        // Build prompt
        let prompt = self.build_execution_prompt(task);

        // Get session
        let resume_sid = self.agent_pool.get_session(task, thread_name).await;

        // Snapshot workspace before
        let pre_snapshot = self.snapshot_workspace().await?;

        // Update task status
        task.status = TaskStatus::InProgress;
        self.store.save_task(task).await?;

        // Call Claude
        let result = self.runner.call(
            &prompt,
            &self.workspace,
            Some(TIMEOUT_EXEC),
            self.config.mcp_config.as_deref(),
            resume_sid.as_deref(),
        ).await;

        match result {
            Ok(claude_result) if claude_result.is_error => {
                warn!(task_id = %task.id, error = %claude_result.text, "Execution failed");
                task.error = Some(claude_result.text);
                Ok(false)
            }
            Ok(claude_result) => {
                // Record modified files
                let post_snapshot = self.snapshot_workspace().await?;
                task.modified_files = self.snapshot_diff(&pre_snapshot, &post_snapshot);

                task.result = Some(claude_result.text);
                if let Some(sid) = &claude_result.session_id {
                    task.session_id = Some(sid.clone());
                    self.agent_pool.record(task, sid, thread_name).await;
                }

                info!(task_id = %task.id, stats = %self.agent_pool.stats().await, "Task executed");
                Ok(true)
            }
            Err(e) => {
                warn!(task_id = %task.id, error = %e, "Execution error");
                task.error = Some(e.to_string());
                Ok(false)
            }
        }
    }

    /// Run tests for a task
    pub async fn test(&self, task: &mut Task) -> Result<(bool, String)> {
        info!(task_id = %task.id, "Running tests");

        let runner = TestRunnerDetector::detect(&self.workspace);

        if runner.is_none() {
            info!("No test runner detected, skipping tests");
            return Ok((true, "No test runner detected".to_string()));
        }

        let runner = runner.unwrap();

        // Run tests
        let result = Command::new(&runner.command[0])
            .args(&runner.command[1..])
            .current_dir(&self.workspace)
            .output()
            .await;

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);

                if output.status.success() {
                    info!(task_id = %task.id, "Tests passed");
                    Ok((true, combined))
                } else {
                    warn!(task_id = %task.id, "Tests failed");
                    Ok((false, combined))
                }
            }
            Err(e) => {
                warn!(task_id = %task.id, error = %e, "Test command failed");
                Ok((false, e.to_string()))
            }
        }
    }

    /// Attempt to fix test failures
    pub async fn fix_test_failure(&self, task: &mut Task, test_output: &str) -> Result<bool> {
        info!(task_id = %task.id, "Attempting to fix test failures");

        let prompt = format!(
            r#"You are a senior developer fixing test failures.

TASK: {}
DESCRIPTION: {}

TEST FAILURES:
{}

Fix the failing tests. Make minimal, targeted changes.
Respond with a brief summary of what you fixed."#,
            task.title,
            task.description,
            test_output
        );

        let result = self.runner.call(&prompt, &self.workspace, Some(TIMEOUT_EXEC), None, None).await?;

        if result.is_error {
            warn!(error = %result.text, "Fix attempt failed");
            return Ok(false);
        }

        info!(task_id = %task.id, summary = %result.text, "Fix applied");
        Ok(true)
    }

    // Helper methods

    fn build_execution_prompt(&self, task: &Task) -> String {
        let doc_section = self.doc_section();
        let memory_section = self.memory_section();
        let dep_section = self.dependency_section(task);

        let prompt = format!(
            r#"You are an expert software developer implementing a specific task.

TASK: {}
DESCRIPTION: {}
{}
{}
WORKSPACE: {}
CURRENT FILES:
{}

ALREADY COMPLETED:
{}

INSTRUCTIONS:
1. Implement the task completely according to the description
2. Create all necessary files in the workspace directory
3. Production-quality code, no TODOs or placeholders
4. Ensure compatibility with previously completed work
5. Follow patterns and decisions from PROJECT MEMORY

Implement the task now. Work directly in the workspace directory."#,
            task.title,
            task.description,
            doc_section,
            memory_section,
            self.workspace.display(),
            self.workspace_files(Some(MAX_WORKSPACE_FILES)),
            self.completed_context()
        );

        if prompt.len() > MAX_PROMPT_LENGTH {
            self.truncate_prompt(&prompt, MAX_PROMPT_LENGTH)
        } else {
            prompt
        }
    }

    fn doc_section(&self) -> String {
        if let Some(doc) = &self.config.doc_content {
            format!("\nREFERENCE DOCUMENT:\n{}\n", doc.chars().take(5000).collect::<String>())
        } else {
            String::new()
        }
    }

    fn memory_section(&self) -> String {
        let memory_path = self.workspace.join(".claude").join("memory.md");
        if let Ok(content) = std::fs::read_to_string(&memory_path) {
            if content.len() > 100 {
                return format!("\nPROJECT MEMORY:\n{}\n", content.chars().take(3000).collect::<String>());
            }
        }
        String::new()
    }

    fn dependency_section(&self, task: &Task) -> String {
        if task.depends_on.is_empty() {
            return String::new();
        }
        format!("\nDEPENDS ON: {:?}\n", task.depends_on)
    }

    fn workspace_files(&self, limit: Option<usize>) -> String {
        let limit = limit.unwrap_or(100);
        let mut files: Vec<String> = Vec::new();

        for entry in walkdir::WalkDir::new(&self.workspace)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(&self.workspace) {
                files.push(relative.to_string_lossy().to_string());
            }
        }

        files.sort();

        if files.len() > limit {
            format!("{}\n... ({} more files)", files[..limit].join("\n"), files.len() - limit)
        } else if files.is_empty() {
            "(empty)".to_string()
        } else {
            files.join("\n")
        }
    }

    fn completed_context(&self) -> String {
        "(none yet)".to_string()
    }

    fn truncate_prompt(&self, prompt: &str, max_length: usize) -> String {
        let keep_len = (max_length - 100) / 2;
        format!(
            "{}\n\n... (content truncated) ...\n\n{}",
            &prompt[..keep_len],
            &prompt[prompt.len().saturating_sub(keep_len)..]
        )
    }

    async fn snapshot_workspace(&self) -> Result<HashMap<String, SystemTime>> {
        let mut snapshot = HashMap::new();

        for entry in walkdir::WalkDir::new(&self.workspace)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(&self.workspace) {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        snapshot.insert(relative.to_string_lossy().to_string(), modified);
                    }
                }
            }
        }

        Ok(snapshot)
    }

    fn snapshot_diff(
        &self,
        before: &HashMap<String, SystemTime>,
        after: &HashMap<String, SystemTime>,
    ) -> Vec<String> {
        after.iter()
            .filter(|(path, mtime)| {
                before.get(*path).map_or(true, |old_mtime| old_mtime < *mtime)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Note: Full integration tests require mocking Claude CLI
    // Unit tests here focus on helper methods

    #[test]
    fn test_executor_config_default() {
        let config = ExecutorConfig::default();
        assert_eq!(config.model_fast, "glm-5");
        assert_eq!(config.model_smart, "glm-5");
        assert!(config.doc_content.is_none());
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core executor -- --nocapture
```

Expected: PASS

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/executor/
git commit -m "feat(core): add TaskExecutor for task execution and testing"
```

---

## Chunk 7: Orchestrator 模块

### Task 7.1: 实现 Orchestrator

**Files:**
- Create: `crates/core/src/orchestrator/mod.rs`
- Create: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: 创建 orchestrator/mod.rs**

```rust
//! Main orchestrator module.

mod orchestrator;

pub use orchestrator::{Orchestrator, OrchestratorConfig};
```

- [ ] **Step 2: 创建 orchestrator/orchestrator.rs (Part 1 - 结构和配置)**

```rust
//! Orchestrator - main coordination engine.

use crate::agent::SharedAgentPool;
use crate::config::{MAX_DEPTH, MAX_RETRIES};
use crate::error::Result;
use crate::executor::{ExecutorConfig, TaskExecutor};
use crate::models::{Complexity, Task, TaskStatus};
use crate::store::TaskStore;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::task::JoinSet;
use tracing::{debug, error, info, step, warn};

/// Orchestrator configuration
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub goal: String,
    pub workspace: PathBuf,
    pub tasks_dir: PathBuf,
    pub doc_content: Option<String>,
    pub mcp_config: Option<PathBuf>,
    pub num_agents: usize,
    pub debug_mode: bool,
    pub ask_mode: bool,
    pub resume: bool,
}

impl OrchestratorConfig {
    pub fn new(goal: String, workspace: PathBuf, tasks_dir: PathBuf) -> Self {
        Self {
            goal,
            workspace,
            tasks_dir,
            doc_content: None,
            mcp_config: None,
            num_agents: 1,
            debug_mode: false,
            ask_mode: false,
            resume: false,
        }
    }
}

/// Main orchestrator
pub struct Orchestrator {
    config: OrchestratorConfig,
    store: Arc<TaskStore>,
    agent_pool: SharedAgentPool,
    executor: Arc<TaskExecutor>,
    start_time: Option<Instant>,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub async fn new(config: OrchestratorConfig) -> Result<Self> {
        // Create directories
        fs::create_dir_all(&config.workspace).await?;
        fs::create_dir_all(&config.tasks_dir).await?;

        // Initialize components
        let store = Arc::new(TaskStore::new(config.tasks_dir.clone()).await?);
        let agent_pool = SharedAgentPool::new();

        let executor_config = ExecutorConfig {
            doc_content: config.doc_content.clone(),
            mcp_config: config.mcp_config.clone(),
            debug_mode: config.debug_mode,
            ..Default::default()
        };

        let executor = Arc::new(TaskExecutor::new(
            config.workspace.clone(),
            store.clone(),
            agent_pool.clone(),
            executor_config,
        ));

        Ok(Self {
            config,
            store,
            agent_pool,
            executor,
            start_time: None,
        })
    }
}
```

- [ ] **Step 3: 添加运行方法 (Part 2)**

继续编辑 `orchestrator.rs`，添加主运行逻辑：

```rust
impl Orchestrator {
    // ... 继续上面的 impl 块

    /// Run the orchestrator
    pub async fn run(&mut self) -> Result<()> {
        self.start_time = Some(Instant::now());
        self.print_header();

        // Ensure .gitignore exists
        self.ensure_gitignore().await?;

        // Phase 0: Interactive clarification (optional)
        let clarification = if self.config.ask_mode && !self.config.resume {
            self.clarify_goal().await?
        } else {
            String::new()
        };

        // Phase 1: Generate or resume tasks
        if self.config.resume && self.store.total().await? > 0 {
            self.resume_tasks().await?;
        } else if !self.config.resume && self.store.total().await? > 0 {
            // Found existing tasks, ask user (simplified: always resume)
            self.resume_tasks().await?;
        } else {
            self.generate_tasks(&clarification).await?;
        }

        // Show progress
        self.print_progress().await;

        // Run dispatcher
        self.run_dispatcher().await?;

        // Final summary
        self.print_summary().await?;

        Ok(())
    }

    async fn ensure_gitignore(&self) -> Result<()> {
        let gitignore = self.config.workspace.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, DEFAULT_GITIGNORE).await?;
        }
        Ok(())
    }

    async fn clarify_goal(&self) -> Result<String> {
        step!("Generating clarifying questions...");
        // Simplified: return empty string
        Ok(String::new())
    }

    async fn resume_tasks(&self) -> Result<()> {
        let total = self.store.total().await?;
        let completed = self.store.count(TaskStatus::Completed).await?;
        let failed = self.store.count(TaskStatus::Failed).await?;
        let pending = self.store.count(TaskStatus::Pending).await?;

        info!(total, completed, failed, pending, "Resuming tasks");

        // Reset stuck in_progress tasks
        let tasks = self.store.all_tasks().await?;
        for mut task in tasks {
            if task.status == TaskStatus::InProgress {
                task.status = TaskStatus::Pending;
                self.store.save_task(&task).await?;
                info!(task_id = %task.id, "Reset stuck task");
            }
        }

        Ok(())
    }

    async fn generate_tasks(&self, _clarification: &str) -> Result<()> {
        step!(goal = %self.config.goal, "Generating task list");

        let prompt = format!(
            r#"You are a software project planner. Break down the following goal into tasks.

PROJECT GOAL: {}

Generate 3-10 tasks. Respond ONLY with JSON:
{{"tasks": [{{"id": "task-001", "title": "...", "description": "...", "depends_on": []}}]}}"#,
            self.config.goal
        );

        let result = self.executor.runner.call(
            &prompt,
            &self.config.workspace,
            Some(120),
            None,
            None,
        ).await?;

        if result.is_error {
            error!(error = %result.text, "Failed to generate tasks");
            return Ok(());
        }

        // Parse tasks
        if let Ok(tasks_response) = serde_json::from_str::<TasksResponse>(&result.text) {
            info!(count = tasks_response.tasks.len(), "Tasks generated");

            for t in tasks_response.tasks {
                let mut task = Task::new(t.id, t.title, t.description);
                task.depends_on = t.depends_on;
                self.store.save_task(&task).await?;
            }

            self.store.save_manifest(&self.config.goal).await?;
        }

        Ok(())
    }
}
```

- [ ] **Step 4: 添加调度器 (Part 3)**

继续编辑 `orchestrator.rs`，添加调度逻辑：

```rust
impl Orchestrator {
    // ... 继续上面的 impl 块

    async fn run_dispatcher(&self) -> Result<()> {
        let primary_slots = (self.config.num_agents + 1) / 2;
        let subtask_slots = self.config.num_agents.saturating_sub(primary_slots);
        let deadline = Instant::now() + Duration::from_secs(24 * 3600);

        let mut join_set = JoinSet::new();
        let mut dispatched_ids: HashSet<String> = HashSet::new();
        let mut primary_running = 0usize;
        let mut subtask_running = 0usize;

        while Instant::now() < deadline {
            // Collect completed tasks
            while let Some(res) = join_set.try_join_next() {
                match res {
                    Ok((task_id, depth)) => {
                        dispatched_ids.remove(&task_id);
                        if depth == 0 {
                            primary_running = primary_running.saturating_sub(1);
                        } else {
                            subtask_running = subtask_running.saturating_sub(1);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Task pipeline error");
                    }
                }
            }

            self.print_progress().await;

            // Get schedulable tasks
            let completed_ids: HashSet<String> = self.store.all_tasks().await?
                .into_iter()
                .filter(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped)
                .map(|t| t.id)
                .collect();

            let pending: Vec<Task> = self.store.pending_tasks().await?
                .into_iter()
                .filter(|t| !dispatched_ids.contains(&t.id))
                .filter(|t| t.depends_on.iter().all(|dep| completed_ids.contains(dep)))
                .collect();

            let primary_pending: Vec<_> = pending.iter().filter(|t| t.depth == 0).collect();
            let subtask_pending: Vec<_> = pending.iter().filter(|t| t.depth > 0).collect();

            // Dispatch primary tasks
            for task in primary_pending {
                if primary_running >= primary_slots {
                    break;
                }

                let task_id = task.id.clone();
                dispatched_ids.insert(task_id.clone());
                primary_running += 1;

                info!(task_id = %task.id, "[primary] Dispatched");

                let store = self.store.clone();
                let executor = self.executor.clone();
                let agent_pool = self.agent_pool.clone();
                let task = task.clone();

                join_set.spawn(async move {
                    let result = run_task_pipeline(store, executor, agent_pool, task).await;
                    (task_id, 0usize) // depth 0
                });
            }

            // Dispatch subtasks
            for task in subtask_pending {
                if subtask_running >= subtask_slots {
                    break;
                }

                let task_id = task.id.clone();
                dispatched_ids.insert(task_id.clone());
                subtask_running += 1;

                info!(task_id = %task.id, "[subtask] Dispatched");

                let store = self.store.clone();
                let executor = self.executor.clone();
                let agent_pool = self.agent_pool.clone();
                let task = task.clone();

                join_set.spawn(async move {
                    let result = run_task_pipeline(store, executor, agent_pool, task).await;
                    (task_id, 1usize) // depth > 0
                });
            }

            // Check exit conditions
            if dispatched_ids.is_empty() && self.store.pending_tasks().await?.is_empty() {
                info!("All tasks completed");
                break;
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Ok(())
    }

    fn print_header(&self) {
        println!();
        println!("╔══════════════════════════════════════════╗");
        println!("║     MATRIX - Agent Orchestrator          ║");
        println!("╚══════════════════════════════════════════╝");
        println!();
        println!("Goal:      {}", self.config.goal);
        println!("Workspace: {}", self.config.workspace.display());
        println!("Agents:    {}", self.config.num_agents);
        println!();
    }

    async fn print_progress(&self) {
        let total = self.store.total().await.unwrap_or(0);
        let completed = self.store.count(TaskStatus::Completed).await.unwrap_or(0);
        let pending = self.store.count(TaskStatus::Pending).await.unwrap_or(0);
        let failed = self.store.count(TaskStatus::Failed).await.unwrap_or(0);

        let elapsed = self.start_time.map(|t| t.elapsed()).unwrap_or_default();
        let elapsed_str = format_duration(elapsed);

        println!();
        println!(
            "Progress: {} completed | {} pending | {} failed / {} total [elapsed {}]",
            completed, pending, failed, total, elapsed_str
        );
        println!();
    }

    async fn print_summary(&self) -> Result<()> {
        let completed = self.store.count(TaskStatus::Completed).await?;
        let failed = self.store.count(TaskStatus::Failed).await?;
        let total = self.store.total().await?;

        println!();
        println!("{}", "═".repeat(45));
        println!("All tasks processed: {}/{} completed, {} failed", completed, total, failed);
        println!("{}", "═".repeat(45));
        println!();

        Ok(())
    }
}

/// Run single task pipeline
async fn run_task_pipeline(
    store: Arc<TaskStore>,
    executor: Arc<TaskExecutor>,
    _agent_pool: SharedAgentPool,
    mut task: Task,
) -> Result<()> {
    let thread_name = format!("thread-{}", task.id);

    // Setup workspace
    // Note: In production, this should only run once

    // Execute
    let success = executor.execute(&mut task, &thread_name).await?;

    if !success {
        if task.retries < MAX_RETRIES {
            task.retries += 1;
            task.status = TaskStatus::Pending;
            store.save_task(&task).await?;
            warn!(task_id = %task.id, attempt = task.retries, "Retrying");
        } else {
            task.status = TaskStatus::Failed;
            store.save_task(&task).await?;
            error!(task_id = %task.id, "Permanently failed");
        }
        return Ok(());
    }

    // Test
    let (tests_passed, test_output) = executor.test(&mut task).await?;

    if !tests_passed {
        // Try to fix
        let fixed = executor.fix_test_failure(&mut task, &test_output).await?;

        if !fixed && task.retries < MAX_RETRIES {
            task.retries += 1;
            task.status = TaskStatus::Pending;
            task.test_failure_context = Some(test_output);
            store.save_task(&task).await?;
            warn!(task_id = %task.id, attempt = task.retries, "Tests failed, retrying");
            return Ok(());
        } else if !fixed {
            task.status = TaskStatus::Failed;
            task.error = Some(format!("Tests failed: {}", test_output));
            store.save_task(&task).await?;
            error!(task_id = %task.id, "Tests failed permanently");
            return Ok(());
        }
    }

    // Mark completed
    task.status = TaskStatus::Completed;
    store.save_task(&task).await?;
    info!(task_id = %task.id, "Task completed successfully");

    Ok(())
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}h{:02}m{:02}s", h, m, s)
    } else {
        format!("{}m{:02}s", m, s)
    }
}

const DEFAULT_GITIGNORE: &str = r#"
# Dependencies
node_modules/
target/
__pycache__/
*.pyc
.env

# IDE
.vscode/
.idea/

# Build
dist/
build/

# Logs
*.log
logs/
"#;

// Helper types

#[derive(Debug, serde::Deserialize)]
struct TasksResponse {
    tasks: Vec<TaskDefinition>,
}

#[derive(Debug, serde::Deserialize)]
struct TaskDefinition {
    id: String,
    title: String,
    description: String,
    #[serde(default)]
    depends_on: Vec<String>,
}
```

- [ ] **Step 5: 运行测试**

```bash
cargo test -p matrix-core orchestrator -- --nocapture
```

Expected: PASS

- [ ] **Step 6: 提交**

```bash
git add crates/core/src/orchestrator/
git commit -m "feat(core): add Orchestrator with task dispatch pipeline"
```

---

## Chunk 7.5: 补全核心功能

### Task 7.5.1: 实现 Phase 0 - 交互式澄清

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: 实现 clarify_goal 方法**

```rust
async fn clarify_goal(&self) -> Result<String> {
    use std::io::{self, BufRead, Write};

    step!("Generating clarifying questions...");

    let prompt = format!(
        r#"You are helping plan a software development project.

GOAL: {}
{}

Generate 3-5 concise, targeted clarifying questions.

Respond ONLY with JSON:
{{"questions": ["Question 1?", "Question 2?", "Question 3?"]}}"#,
        self.config.goal,
        self.config.doc_content.as_ref().map(|d| format!("DOCUMENT:\n{}", d)).unwrap_or_default()
    );

    let result = self.executor.runner.call(
        &prompt,
        &self.config.workspace,
        Some(120),
        None,
        None,
    ).await?;

    if result.is_error {
        warn!("Could not generate clarifying questions");
        return Ok(String::new());
    }

    // Parse questions
    let questions: ClarificationResponse = match serde_json::from_str(&result.text) {
        Ok(q) => q,
        Err(_) => return Ok(String::new()),
    };

    // Interactive Q&A
    println!("\n╔══════════════════════════════════════════════╗");
    println!("║  [?] Clarifying Questions                     ║");
    println!("╚══════════════════════════════════════════════╝");
    println!("  Answer these to help plan your project.");
    println!("  Press Enter to skip any question.\n");

    let mut answers: Vec<String> = Vec::new();
    let stdin = io::stdin();

    for (i, question) in questions.questions.iter().enumerate() {
        println!("  [{}] {}", i + 1, question);
        print!("  ▶ ");
        io::stdout().flush().ok();

        let mut answer = String::new();
        if let Ok(()) = stdin.lock().read_line(&mut answer) {
            let answer = answer.trim();
            if !answer.is_empty() {
                answers.push(format!("Q{}: {}\nA{}: {}", i + 1, question, i + 1, answer));
            }
        }
        println!();
    }

    if answers.is_empty() {
        println!("  (no answers provided, proceeding with goal as-is)\n");
        return Ok(String::new());
    }

    println!("  [+] Clarifications captured\n");
    Ok(answers.join("\n\n"))
}
```

- [ ] **Step 2: 添加辅助结构体**

```rust
#[derive(Debug, serde::Deserialize)]
struct ClarificationResponse {
    questions: Vec<String>,
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core orchestrator -- --nocapture
```

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "feat(core): implement Phase 0 interactive clarification"
```

---

### Task 7.5.2: 实现 Phase 2 - 复杂度评估与任务拆分

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: 实现 assess_and_split 方法**

```rust
async fn assess_and_split(&self, task: &mut Task) -> Result<bool> {
    use crate::config::MAX_DEPTH;

    if task.depth >= MAX_DEPTH {
        task.complexity = Complexity::Simple;
        self.store.save_task(task).await?;
        return Ok(true);
    }

    info!(task_id = %task.id, "Assessing complexity");

    let prompt = format!(
        r#"You are a senior software engineer evaluating a development task.

TASK: {}
DESCRIPTION: {}
CURRENT DEPTH: {} / {}

Is this task SIMPLE (doable in one claude session) or COMPLEX (needs splitting)?

Respond ONLY with JSON:
{{"split": false, "reason": "..."}}
OR if splitting needed:
{{"split": true, "reason": "...", "subtasks": [{{"title": "...", "description": "..."}}]}}"#,
        task.title,
        task.description,
        task.depth,
        MAX_DEPTH
    );

    let result = self.executor.runner.call(
        &prompt,
        &self.config.workspace,
        Some(120),
        None,
        None,
    ).await?;

    let data: AssessResponse = match serde_json::from_str(&result.text) {
        Ok(d) => d,
        Err(_) => {
            task.complexity = Complexity::Simple;
            self.store.save_task(task).await?;
            return Ok(true);
        }
    };

    if data.split {
        let subtasks = data.subtasks.unwrap_or_default();
        info!(task_id = %task.id, count = subtasks.len(), "Splitting into subtasks");

        for (i, sub) in subtasks.into_iter().enumerate() {
            let sub_id = format!("{}-{}", task.id, i + 1);
            let mut subtask = Task::subtask(
                sub_id,
                sub.title,
                sub.description,
                task.id.clone(),
                task.depth + 1,
            );
            self.store.save_task(&subtask).await?;
            info!(subtask_id = %subtask.id, "Subtask created");
        }

        task.status = TaskStatus::Skipped;
        task.complexity = Complexity::Complex;
        self.store.save_task(task).await?;
        self.store.save_manifest(&self.config.goal).await?;

        return Ok(false);
    }

    task.complexity = Complexity::Simple;
    self.store.save_task(task).await?;
    Ok(true)
}

#[derive(Debug, serde::Deserialize)]
struct AssessResponse {
    split: bool,
    reason: String,
    #[serde(default)]
    subtasks: Option<Vec<SubtaskDef>>,
}

#[derive(Debug, serde::Deserialize)]
struct SubtaskDef {
    title: String,
    description: String,
}
```

- [ ] **Step 2: 在 run_task_pipeline 中调用 assess_and_split**

更新 `run_task_pipeline` 函数：

```rust
async fn run_task_pipeline(
    store: Arc<TaskStore>,
    executor: Arc<TaskExecutor>,
    orchestrator: Option<Arc<tokio::sync::Mutex<Orchestrator>>>,
    mut task: Task,
) -> Result<()> {
    let thread_name = format!("thread-{}", task.id);

    // Phase 2: Assess and split
    if let Some(orch) = &orchestrator {
        let should_proceed = orch.lock().await.assess_and_split(&mut task).await?;
        if !should_proceed {
            return Ok(()); // Task was split, subtasks will be dispatched
        }
    }

    // ... rest of pipeline
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core orchestrator -- --nocapture
```

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "feat(core): implement Phase 2 complexity assessment and task splitting"
```

---

### Task 7.5.3: 实现 Phase 4 - Git 提交

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`
- Modify: `crates/core/src/error.rs`

- [ ] **Step 1: 添加 Git 相关错误类型**

在 `error.rs` 中添加：

```rust
#[derive(Debug, Error)]
pub enum Error {
    // ... existing variants ...

    #[error("Git error: {0}")]
    Git(String),
}
```

- [ ] **Step 2: 实现 git_commit_task 方法**

```rust
impl Orchestrator {
    // ... existing methods ...

    /// Commit changes for a completed task
    async fn git_commit_task(&self, task: &Task) -> Result<()> {
        if task.modified_files.is_empty() {
            debug!(task_id = %task.id, "No files to commit");
            return Ok(());
        }

        info!(task_id = %task.id, files = ?task.modified_files, "Committing changes");

        // Check if git is initialized
        let git_dir = self.config.workspace.join(".git");
        if !git_dir.exists() {
            // Initialize git repo
            let output = tokio::process::Command::new("git")
                .args(["init"])
                .current_dir(&self.config.workspace)
                .output()
                .await
                .map_err(|e| Error::Git(format!("Failed to init git: {}", e)))?;

            if !output.status.success() {
                warn!("Git init failed, skipping commit");
                return Ok(());
            }
        }

        // Stage files
        for file in &task.modified_files {
            let _ = tokio::process::Command::new("git")
                .args(["add", file])
                .current_dir(&self.config.workspace)
                .output()
                .await;
        }

        // Create commit
        let message = format!("[{}] {}", task.id, task.title);
        let output = tokio::process::Command::new("git")
            .args(["commit", "-m", &message])
            .current_dir(&self.config.workspace)
            .output()
            .await
            .map_err(|e| Error::Git(format!("Failed to commit: {}", e)))?;

        if output.status.success() {
            info!(task_id = %task.id, "Changes committed");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(stderr = %stderr, "Commit had issues");
        }

        Ok(())
    }
}
```

- [ ] **Step 3: 在 run_task_pipeline 中调用 git_commit_task**

```rust
// In run_task_pipeline, after tests pass:

    // Phase 4: Git commit
    if let Some(orch) = &orchestrator {
        orch.lock().await.git_commit_task(&task).await?;
    }

    // Mark completed
    task.status = TaskStatus::Completed;
    store.save_task(&task).await?;
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p matrix-core -- --nocapture
```

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/orchestrator/orchestrator.rs crates/core/src/error.rs
git commit -m "feat(core): implement Phase 4 git commit for completed tasks"
```

---

### Task 7.5.4: 实现 Phase 5 - 最终测试

**Files:**
- Modify: `crates/core/src/orchestrator/orchestrator.rs`

- [ ] **Step 1: 实现 run_final_tests 方法**

```rust
impl Orchestrator {
    /// Run final project-wide tests
    async fn run_final_tests(&self) -> Result<()> {
        use crate::detector::TestRunnerDetector;

        step!("Running final tests...");

        let runner = match TestRunnerDetector::detect(&self.config.workspace) {
            Some(r) => r,
            None => {
                info!("No test runner detected, skipping final tests");
                return Ok(());
            }
        };

        let output = tokio::process::Command::new(&runner.command[0])
            .args(&runner.command[1..])
            .current_dir(&self.config.workspace)
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    info!("Final tests passed");
                    println!("\n✓ Final tests passed\n");
                } else {
                    warn!("Final tests failed");
                    println!("\n✗ Final tests failed:\n{}\n", stdout);
                    println!("{}\n", stderr);
                }
            }
            Err(e) => {
                warn!(error = %e, "Final tests could not run");
            }
        }

        Ok(())
    }
}
```

- [ ] **Step 2: 在 run 方法中调用 final tests**

```rust
// In run(), after dispatcher completes:

    // Phase 5: Final tests
    self.run_final_tests().await?;

    // Final summary
    self.print_summary().await?;
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p matrix-core -- --nocapture
```

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/orchestrator/orchestrator.rs
git commit -m "feat(core): implement Phase 5 final tests execution"
```

---

### Task 7.5.5: 实现 completed_context 辅助方法

**Files:**
- Modify: `crates/core/src/executor/task_executor.rs`

- [ ] **Step 1: 实现 completed_context 方法**

```rust
impl TaskExecutor {
    // ... existing methods ...

    fn completed_context(&self) -> String {
        // Note: In a real implementation, this would query the TaskStore
        // For now, return a placeholder that indicates the method exists
        let completed_path = self.workspace.join(".matrix").join("completed.txt");
        if let Ok(content) = std::fs::read_to_string(&completed_path) {
            if !content.is_empty() {
                return content;
            }
        }
        "(none yet)".to_string()
    }

    /// Update completed context file after a task completes
    pub async fn update_completed_context(&self, task: &Task) -> Result<()> {
        let completed_path = self.workspace.join(".matrix").join("completed.txt");
        if let Some(parent) = completed_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let entry = format!("- [{}] {}\n", task.id, task.title);

        let existing = std::fs::read_to_string(&completed_path).unwrap_or_default();
        let updated = existing + &entry;

        tokio::fs::write(&completed_path, updated).await?;
        Ok(())
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test -p matrix-core executor -- --nocapture
```

- [ ] **Step 3: 提交**

```bash
git add crates/core/src/executor/task_executor.rs
git commit -m "feat(core): implement completed_context helper for execution prompts"
```

---

## Chunk 8: CLI 模块

### Task 8.1: 实现 CLI 入口

**Files:**
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/cli/Cargo.toml`

- [ ] **Step 1: 更新 crates/cli/Cargo.toml**

```toml
[package]
name = "matrix-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "CLI for Matrix Agent Orchestrator"

[[bin]]
name = "matrix"
path = "src/main.rs"

[dependencies]
matrix-core.workspace = true
tokio.workspace = true
clap.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
chrono.workspace = true
which = "6"
```

- [ ] **Step 2: 更新 crates/cli/src/main.rs**

```rust
//! Matrix CLI - Command-line interface for the Agent Orchestrator

use clap::Parser;
use matrix_core::{Orchestrator, OrchestratorConfig};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "matrix")]
#[command(author, version, about = "Long-Running Agent Orchestrator using Claude CLI", long_about = None)]
struct Args {
    /// Project goal description
    goal: String,

    /// Output path (parent dir or new dir)
    #[arg(name = "PATH")]
    path: Option<PathBuf>,

    /// Specification/requirements document
    #[arg(short, long = "doc")]
    doc: Option<PathBuf>,

    /// Explicit workspace directory
    #[arg(short = 'd', long = "workspace")]
    workspace: Option<PathBuf>,

    /// MCP config JSON for e2e tests
    #[arg(long = "mcp-config")]
    mcp_config: Option<PathBuf>,

    /// Resume previous run
    #[arg(short, long)]
    resume: bool,

    /// Number of parallel agent workers
    #[arg(short = 'n', long, default_value = "1")]
    agents: usize,

    /// Stream Claude's live output (verbose)
    #[arg(long)]
    debug: bool,

    /// Ask clarifying questions before planning
    #[arg(short, long)]
    ask: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "matrix=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Check runtime dependencies
    check_dependencies()?;

    // Resolve workspace path
    let workspace = resolve_workspace(&args)?;
    let tasks_dir = workspace.join(".matrix").join("tasks");

    // Load document content
    let doc_content = if let Some(doc_path) = &args.doc {
        if !doc_path.exists() {
            anyhow::bail!("Document not found: {}", doc_path.display());
        }
        let content = std::fs::read_to_string(doc_path)?;
        info!(lines = content.lines().count(), "Loaded document");
        Some(content)
    } else {
        None
    };

    // Create config
    let config = OrchestratorConfig {
        goal: args.goal.clone(),
        workspace,
        tasks_dir,
        doc_content,
        mcp_config: args.mcp_config,
        num_agents: args.agents,
        debug_mode: args.debug,
        ask_mode: args.ask,
        resume: args.resume,
    };

    // Run orchestrator
    let mut orchestrator = Orchestrator::new(config).await?;
    orchestrator.run().await?;

    Ok(())
}

/// Resolve workspace path
fn resolve_workspace(args: &Args) -> anyhow::Result<PathBuf> {
    if let Some(ws) = &args.workspace {
        return Ok(ws.clone());
    }

    if let Some(path) = &args.path {
        if path.is_dir() {
            // Create named subdirectory
            let slug = slugify(&args.goal);
            return Ok(path.join(slug));
        }
        return Ok(path.clone());
    }

    Ok(std::env::current_dir()?.join("workspace"))
}

/// Generate URL-friendly slug
fn slugify(s: &str) -> String {
    let slug: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    let slug: String = slug.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");

    if slug.is_empty() {
        chrono::Local::now().format("project-%Y%m%d-%H%M%S").to_string()
    } else {
        slug.chars().take(40).collect()
    }
}

/// Check required dependencies
fn check_dependencies() -> anyhow::Result<()> {
    let hard_deps = [
        ("claude", "npm i -g @anthropic-ai/claude-code"),
        ("git", "https://git-scm.com/downloads"),
    ];

    let soft_deps = [
        ("node", "https://nodejs.org (needed for JS/TS projects)"),
        ("npm", "https://nodejs.org (needed for JS/TS projects)"),
        ("python", "https://python.org (needed for Python projects)"),
        ("cargo", "https://rustup.rs (needed for Rust projects)"),
    ];

    // Check hard dependencies
    for (cmd, install) in &hard_deps {
        if which::which(cmd).is_err() {
            eprintln!("\x1b[31mError: '{}' not found. Install: {}\x1b[0m", cmd, install);
            anyhow::bail!("Missing required dependency: {}", cmd);
        }
    }

    // Check soft dependencies
    let missing_soft: Vec<_> = soft_deps
        .iter()
        .filter(|(cmd, _)| which::which(cmd).is_err())
        .collect();

    if !missing_soft.is_empty() {
        eprintln!("\x1b[33mWarning: some optional tools are missing:\x1b[0m");
        for (cmd, install) in missing_soft {
            eprintln!("  \x1b[33m·\x1b[0m {:10}  →  {}", cmd, install);
        }
    }

    Ok(())
}
```

- [ ] **Step 3: 构建并测试**

```bash
cargo build --workspace
cargo run -p matrix-cli -- --help
```

Expected: 显示帮助信息

- [ ] **Step 4: 提交**

```bash
git add crates/cli/
git commit -m "feat(cli): implement full CLI with clap"
```

---

## 最终步骤

### Task Final: 集成测试和清理

- [ ] **Step 1: 运行所有测试**

```bash
cargo test --workspace
```

Expected: 所有测试通过

- [ ] **Step 2: 运行 lint**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 3: 格式化代码**

```bash
cargo fmt --all
```

- [ ] **Step 4: 构建发布版本**

```bash
cargo build --workspace --release
```

- [ ] **Step 5: 最终提交**

```bash
git add .
git commit -m "chore: final integration and cleanup"
```

---

## 执行完成

计划完成！项目现已准备好实现。

**实现顺序：**
1. Chunk 1: 项目初始化
2. Chunk 2: Core 基础模块
3. Chunk 3: Store 模块
4. Chunk 4: Agent 模块
5. Chunk 5: Detector 模块
6. Chunk 6: Executor 模块
7. Chunk 7: Orchestrator 模块
8. Chunk 8: CLI 模块
9. 最终集成