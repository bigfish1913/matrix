# Matrix

基于 Rust 实现的长时运行 Agent 编排器，使用 Claude CLI 自主开发软件项目。

## 功能特性

- **自主任务编排**：自动生成、拆分、执行和验证开发任务
- **多 Agent 并行执行**：支持多个 Claude Agent 并行工作，加速开发
- **交互式 TUI**：实时终端界面，包含日志、任务状态和 Claude 输出面板
- **智能项目检测**：自动检测项目类型（Rust、Node.js、Python、Go）和测试运行器
- **Git 集成**：自动创建分支隔离和提交
- **上下文连续性**：共享内存系统实现跨 Agent 知识传递
- **断点续传**：支持从中断处恢复运行

## 安装

### 前置条件

- [Rust](https://rustup.rs/) 1.70+
- [Claude CLI](https://claude.ai/code) 已安装并认证
- [Task](https://taskfile.dev/)（可选，用于构建自动化）

### 快速安装

```bash
# 克隆仓库
git clone https://github.com/bigfish1913/matrix.git
cd matrix

# 全局安装
task install
```

或直接使用 cargo：

```bash
cargo build --release
cp target/release/matrix ~/.local/bin/
```

## 使用方法

```
matrix <目标> [路径] [选项]
```

### 参数

| 参数 | 说明 |
|------|------|
| `<目标>` | 项目目标描述 |
| `[路径]` | 输出路径（父目录或新目录） |

### 选项

| 选项 | 说明 |
|------|------|
| `--doc <文件>` | 规格/需求文档 |
| `-d, --workspace` | 指定工作区目录 |
| `--mcp-config <文件>` | MCP 配置 JSON（用于端到端测试） |
| `--resume` | 恢复上次运行 |
| `-n, --agents <N>` | 并行 Agent 数量（默认：1） |
| `--debug` | 实时输出 Claude 原始输出 |
| `--ask, -q` | 规划前先提出澄清问题 |

### 示例

```bash
# 创建新项目并进行交互式澄清
matrix "构建一个带用户认证的 REST API" ./my-api --ask

# 恢复中断的运行
matrix --resume

# 使用多个 Agent 运行
matrix "创建一个待办事项应用" ./todo -n 3

# 使用规格文档
matrix "实现这些功能" ./project --doc specs.md
```

## TUI 快捷键

| 按键 | 操作 |
|------|------|
| `Tab` / `Shift+Tab` | 切换标签页 |
| `↑` / `↓` | 滚动内容 |
| `v` / `V` | 切换详细程度 |
| `q` | 退出 |

### 标签页

1. **Logs** - 实时追踪日志
2. **Tasks** - 任务列表及状态
3. **Claude Output** - Claude 原始输出

## 架构

```
matrix/
├── Cargo.toml          # Workspace 根配置
├── crates/
│   ├── core/           # 共享编排逻辑
│   │   ├── agent/      # Claude 运行器
│   │   ├── executor/   # 任务执行器
│   │   ├── models/     # 数据结构
│   │   ├── orchestrator/ # 主编排器
│   │   ├── store/      # 任务持久化
│   │   └── tui/        # 终端 UI
│   └── cli/            # 命令行接口
├── Taskfile.yml        # 构建自动化
└── docs/               # 文档
```

### 执行流程

1. **阶段 0**（可选）：交互式澄清（`--ask`）
2. **阶段 1**：通过 Claude 生成任务
3. **阶段 2**：评估复杂度并拆分复杂任务
4. **阶段 3**：执行、运行测试、自动修复失败
5. **阶段 4**：Git 提交并隔离分支
6. **阶段 5**：最终测试和总结

## 开发

```bash
# 构建
task build

# 运行测试
task test

# 运行 CLI
task run

# 格式化代码
task fmt

# 代码检查
task lint
```

## 配置

| 常量 | 默认值 | 说明 |
|------|--------|------|
| `MAX_DEPTH` | 3 | 最大任务拆分深度 |
| `MAX_RETRIES` | 3 | 每个任务的重试次数 |
| `TIMEOUT_PLAN` | 120s | 规划/评估/验证超时 |
| `TIMEOUT_EXEC` | 3600s | 代码执行超时 |

## 支持的项目类型

| 项目 | 标识文件 | 测试命令 |
|------|----------|----------|
| Rust | `Cargo.toml` | `cargo test` |
| Node.js | `package.json` | `npm test` |
| Python | `pytest.ini`, `pyproject.toml` | `pytest -v` |
| Go | `go.mod` | `go test ./...` |
| Makefile | `Makefile` | `make test` |

## 许可证

MIT
