# AGENTS.md

指导代理在 Matrix 代码库中工作的指南。

## 项目概述

Matrix 是一个使用 Claude CLI 自主开发软件项目的长运行 Agent 编排器。Rust 工作区架构，包含核心库和 CLI 客户端。

## 构建与测试命令

### 使用 Taskfile（推荐）

```bash
# 构建所有 crate（发布模式）
task build

# 开发模式构建
task dev

# 运行所有测试
task test

# 运行单个测试（将 TEST_NAME 替换为测试名称）
task test-single TEST_NAME=test_name

# 格式化代码
task fmt

# 运行 clippy 检查
task lint

# 检查代码
task check

# 安装到全局
task install
```

### 直接使用 Cargo

```bash
# 构建所有 crate
cargo build --workspace

# 运行所有测试
cargo test --workspace

# 运行单个测试
cargo test -p matrix-core test_name

# 运行特定模块的测试
cargo test -p matrix-core models::

# 运行 CLI
cargo run -p matrix-cli -- "项目目标"

# 格式化代码
cargo fmt --all

# 运行 clippy
cargo clippy --workspace --all-targets -- -D warnings

# 检查代码
cargo check --workspace
```

## 代码风格指南

### 导入顺序

1. 标准库导入
2. 外部 crate 导入
3. 本地模块导入
4. 使用 `use` 语句分组，每组之间空一行

```rust
use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::MAX_DEPTH;
use crate::models::Task;
```

### 命名约定

- **变量和函数**：`snake_case`
- **类型和结构体**：`PascalCase`
- **常量**：`SCREAMING_SNAKE_CASE`
- **枚举变体**：`PascalCase`
- **模块**：`snake_case`

### 类型定义

- 使用 `#[derive(...)]` 宏：`Debug`, `Clone`, `Serialize`, `Deserialize`
- 枚举使用 `#[serde(rename_all = "snake_case")]` 或 `#[serde(rename_all = "lowercase")]`
- 实现 `Display` trait 用于用户友好输出

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            // ...
        }
    }
}
```

### 错误处理

- 使用 `thiserror` 定义错误枚举
- 使用 `#[error("...")]` 宏提供错误消息
- 使用 `#[from]` 自动转换标准错误类型
- 高层函数使用 `anyhow::Result<T>`

```rust
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Task not found: {0}")]
    TaskNotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

### 文档注释

- 模块级文档：`//!` 注释
- 函数和类型文档：`///` 注释
- 使用中文注释（项目主要语言）

```rust
//! 模块描述

/// 函数描述
/// 
/// # 参数
/// * `param1` - 参数说明
/// 
/// # 返回值
/// 返回值说明
pub fn function_name(param1: Type) -> Result<()> {
    // 实现
}
```

### 测试

- 测试模块使用 `#[cfg(test)]`
- 测试函数使用 `#[test]` 宏
- 使用 `assert_eq!` 和 `assert!` 进行断言
- 测试模块放在文件末尾

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        let result = function_name();
        assert_eq!(result, expected);
    }
}
```

### 代码组织

- 每个模块一个文件
- 使用 `mod.rs` 或 `module_name.rs` 文件
- 公共 API 使用 `pub` 关键字
- 内部实现保持私有

### 异步编程

- 使用 `tokio` 作为异步运行时
- 使用 `async`/`await` 语法
- 使用 `tokio::spawn` 创建并发任务
- 使用 `tokio::select!` 处理多个异步操作

### 常量定义

- 在 `config.rs` 中定义全局常量
- 使用 `pub const` 导出
- 添加文档注释说明用途

```rust
/// 最大任务拆分深度
pub const MAX_DEPTH: u32 = 5;

/// 最大重试次数
pub const MAX_RETRIES: u32 = 3;
```

## 工作区结构

```
crates/
├── core/           # 共享编排逻辑
│   ├── agent/      # Claude 运行器
│   ├── detector/   # 项目类型检测
│   ├── executor/   # 任务执行器
│   ├── models/     # 数据结构
│   ├── orchestrator/ # 主编排器
│   ├── store/      # 任务持久化
│   └── tui/        # 终端 UI
└── cli/            # 命令行接口
```

## 关键依赖

- `tokio`：异步运行时
- `clap`：CLI 参数解析
- `serde`/`serde_json`：序列化
- `thiserror`/`anyhow`：错误处理
- `tracing`：日志记录
- `chrono`：时间处理
- `ratatui`/`crossterm`：TUI 界面
