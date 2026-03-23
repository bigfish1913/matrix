# Project Roadmap

## 项目概述

本项目旨在使用 Rust 1:1 复刻 Python 脚本 `longtime.py` 的功能——一个长运行 Agent 编排系统。该系统通过调用 Claude CLI 实现软件项目的自主开发。

### 核心价值

- **自主开发**: 通过 AI Agent 自动完成软件开发任务
- **智能编排**: 多任务并行执行，依赖关系自动管理
- **上下文复用**: Agent 会话池管理，保持上下文连续性
- **质量保证**: 自动测试、自动修复、最终验证

---

## 架构决策

### 1. Workspace 架构

采用 Rust Workspace 多 Crate 架构，支持未来多客户端扩展：

```
matrix/
├── Cargo.toml              # Workspace 根配置
├── crates/
│   ├── matrix-core/        # 核心编排逻辑 (lib)
│   ├── matrix-cli/         # 命令行客户端 (bin)
│   ├── matrix-plugin/      # Claude Code 插件接口
│   ├── matrix-gui/         # GUI 客户端 (future)
│   └── matrix-web/         # Web 客户端 (future)
├── Taskfile.yml            # 构建与部署自动化
└── docs/
    └── longtime.py         # 源参考文件
```

**决策理由**:
- 核心逻辑与客户端分离，支持复用
- 各客户端独立开发、独立发布
- 符合 Rust 模块化最佳实践

### 2. 技术选型

| 领域 | 推荐方案 | 备选方案 |
|------|----------|----------|
| 异步运行时 | Tokio | async-std |
| CLI 解析 | clap + derive | argh |
| 错误处理 | thiserror + anyhow | eyre |
| 序列化 | serde + serde_json | - |
| Git 操作 | git2 crate | 命令行调用 |
| 日志 | tracing + tracing-subscriber | log + env_logger |
| 并发 | tokio::task + arc/mutex | rayon (CPU密集) |
| 终端输出 | termcolor / console | ansi_term |

**决策理由**:
- Tokio 是 Rust 异步生态的事实标准
- clap 功能全面，derive 宏使用简洁
- thiserror 定义错误类型，anyhow 处理错误传播
- git2 纯 Rust 实现，跨平台兼容性好
- tracing 支持结构化日志和 span 追踪

### 3. 命名规范

严格遵循 Rust 命名规范：
- **函数/变量**: `snake_case`
- **类型/Trait**: `PascalCase`
- **常量**: `SCREAMING_SNAKE_CASE`
- **模块**: `snake_case`
- **Crate**: `kebab-case` 或 `snake_case`

Python 到 Rust 命名映射示例：
| Python | Rust |
|--------|------|
| `call_claude` | `call_claude` |
| `TaskStore` | `TaskStore` |
| `MAX_RETRIES` | `MAX_RETRIES` |
| `detect_test_runner` | `detect_test_runner` |

---

## 实现阶段

### Phase 1: 项目骨架与基础设施

**目标**: 搭建可编译运行的 Workspace 项目结构

**任务清单**:
1. 创建 Workspace 根 `Cargo.toml`
2. 初始化 `matrix-core` crate (lib)
3. 初始化 `matrix-cli` crate (bin)
4. 配置基础依赖项
5. 创建 `Taskfile.yml` 基础任务
6. 设置 CI/CD 基础配置

**产出物**:
- 可编译的 Workspace 结构
- `task install` 可一键安装
- `task build` 可构建所有 crate
- `task test` 可运行测试

**验证标准**:
- [ ] `cargo build --workspace` 成功
- [ ] `cargo test --workspace` 成功
- [ ] `task build` 成功
- [ ] CLI 输出帮助信息

---

### Phase 2: 核心数据结构

**目标**: 实现 Task、TaskStore、AgentPool 等核心数据结构

**任务清单**:

#### 2.1 Task 结构
- [ ] 定义 `Task` 结构体及所有字段
- [ ] 实现 `TaskStatus` 枚举
- [ ] 实现 JSON 序列化/反序列化
- [ ] 实现文件持久化方法

#### 2.2 TaskStore
- [ ] 任务目录管理
- [ ] 任务 CRUD 操作
- [ ] 依赖关系验证 (缺失检测)
- [ ] 循环依赖检测 (DFS/Kahn算法)
- [ ] Manifest 文件管理

#### 2.3 AgentPool
- [ ] 会话 ID 存储 (task_id → session_id)
- [ ] 线程会话管理 (thread → session_id)
- [ ] 会话获取策略实现
- [ ] 线程安全 (Arc<Mutex<>>)

#### 2.4 配置常量
- [ ] 模型配置 (MODEL_FAST, MODEL_SMART)
- [ ] 超时配置 (TIMEOUT_PLAN, TIMEOUT_EXEC)
- [ ] 限制配置 (MAX_DEPTH, MAX_RETRIES, MAX_PROMPT_LENGTH)

**产出物**:
- `matrix-core/src/task.rs`
- `matrix-core/src/store.rs`
- `matrix-core/src/agent_pool.rs`
- `matrix-core/src/config.rs`

**验证标准**:
- [ ] 单元测试覆盖所有数据结构
- [ ] 依赖图循环检测测试通过
- [ ] JSON 序列化与 Python 版本兼容

---

### Phase 3: Claude CLI 集成

**目标**: 实现与 Claude CLI 的子进程通信

**任务清单**:

#### 3.1 子进程调用
- [ ] 实现 `call_claude` 函数
- [ ] stdin 管道传递 prompt
- [ ] stdout/stderr 流式读取
- [ ] 超时控制

#### 3.2 输出解析
- [ ] JSON 输出解析 (NDJSON)
- [ ] 从文本提取 JSON (正则)
- [ ] 错误处理和重试

#### 3.3 Debug 模式
- [ ] 实时输出流 (stream-json + verbose)
- [ ] 进度显示

#### 3.4 Prompt 管理
- [ ] 智能截断 (truncate_prompt_safely)
- [ ] 多策略优先级截断

**产出物**:
- `matrix-core/src/claude.rs`
- `matrix-core/src/prompt.rs`

**验证标准**:
- [ ] 可成功调用 Claude CLI
- [ ] 正确解析 JSON 响应
- [ ] 超时机制工作正常
- [ ] 长 prompt 可安全截断

---

### Phase 4: 项目检测与设置

**目标**: 自动检测项目类型和依赖安装

**任务清单**:

#### 4.1 项目类型检测
- [ ] Go 项目检测 (go.mod)
- [ ] Rust 项目检测 (Cargo.toml)
- [ ] Node.js 项目检测 (package.json)
- [ ] Python 项目检测 (pytest.ini, pyproject.toml 等)
- [ ] Makefile 检测

#### 4.2 测试运行器检测
- [ ] `detect_test_runner` 函数
- [ ] 返回测试命令

#### 4.3 工作区设置
- [ ] 依赖安装 (npm install, pip install, cargo fetch 等)
- [ ] 多目录支持 (backend/, frontend/, 等)

**产出物**:
- `matrix-core/src/detect.rs`
- `matrix-core/src/setup.rs`

**验证标准**:
- [ ] 正确检测各类型项目
- [ ] 自动运行依赖安装
- [ ] 测试命令正确返回

---

### Phase 5: Orchestrator 核心逻辑

**目标**: 实现主编排引擎

**任务清单**:

#### 5.1 Orchestrator 结构
- [ ] 定义 `Orchestrator` 结构体
- [ ] 初始化方法
- [ ] 配置管理

#### 5.2 Phase 0: 交互式澄清
- [ ] `clarify_goal` 方法
- [ ] 生成澄清问题
- [ ] 用户交互输入
- [ ] Q&A 格式化

#### 5.3 Phase 1: 任务生成
- [ ] `generate_tasks` 方法
- [ ] 调用 Claude 生成任务列表
- [ ] 任务验证和保存
- [ ] 拓扑打印

#### 5.4 Phase 2: 复杂度评估
- [ ] `assess_complexity` 方法
- [ ] 复杂任务拆分
- [ ] 递归拆分 (最大深度限制)

#### 5.5 辅助方法
- [ ] 工作区文件列表 (_workspace_files)
- [ ] 已完成任务上下文 (_completed_context)
- [ ] 依赖上下文 (_dependency_context)
- [ ] 工作区快照 (_ws_snapshot)

**产出物**:
- `matrix-core/src/orchestrator.rs`
- `matrix-core/src/orchestrator/` (子模块)

**验证标准**:
- [ ] 可生成有效任务列表
- [ ] 复杂任务正确拆分
- [ ] 依赖关系正确处理

---

### Phase 6: 任务执行管道

**目标**: 实现任务执行、测试、修复的完整管道

**任务清单**:

#### 6.1 Phase 3: 任务执行
- [ ] `execute_task` 方法
- [ ] Prompt 构建
- [ ] 会话复用
- [ ] 文件变更追踪

#### 6.2 Phase 3b: 测试
- [ ] `test_task` 方法
- [ ] 测试文件生成
- [ ] 测试运行
- [ ] 跳过不可测试任务

#### 6.3 Phase 3c: 自动修复
- [ ] `auto_fix` 方法
- [ ] 失败分析
- [ ] 修复尝试
- [ ] 最大重试限制

#### 6.4 Phase 3d: 验证
- [ ] `verify_task` 方法
- [ ] 验证结果记录

#### 6.5 Phase 4: Git 提交
- [ ] `commit_task` 方法
- [ ] 分支创建
- [ ] 文件暂存
- [ ] 提交消息生成

**产出物**:
- `matrix-core/src/pipeline/execute.rs`
- `matrix-core/src/pipeline/test.rs`
- `matrix-core/src/pipeline/fix.rs`
- `matrix-core/src/pipeline/verify.rs`
- `matrix-core/src/pipeline/commit.rs`

**验证标准**:
- [ ] 任务执行成功
- [ ] 测试自动运行
- [ ] 失败自动修复
- [ ] Git 提交正确

---

### Phase 7: 并行执行引擎

**目标**: 实现多 Agent 并行任务执行

**任务清单**:

#### 7.1 线程池管理
- [ ] ThreadPoolExecutor 模式 (使用 tokio::task)
- [ ] 分层调度 (primary vs subtask)
- [ ] 插槽管理

#### 7.2 任务调度
- [ ] 依赖满足检测
- [ ] 优先级队列
- [ ] 死锁预防

#### 7.3 状态同步
- [ ] 任务状态更新
- [ ] 进度追踪
- [ ] 错误传播

**产出物**:
- `matrix-core/src/scheduler.rs`
- `matrix-core/src/executor.rs`

**验证标准**:
- [ ] 多任务并行执行
- [ ] 依赖关系正确处理
- [ ] 无死锁
- [ ] 进度正确显示

---

### Phase 8: 共享内存与上下文

**目标**: 实现 Agent 间知识共享机制

**任务清单**:

#### 8.1 内存文件管理
- [ ] `.claude/memory.md` 读写
- [ ] 内容更新策略

#### 8.2 记忆更新
- [ ] `update_memory` 方法
- [ ] Claude 提取关键学习
- [ ] 增量追加

**产出物**:
- `matrix-core/src/memory.rs`

**验证标准**:
- [ ] 内存文件正确读写
- [ ] 知识正确提取和追加

---

### Phase 9: 拓扑可视化

**目标**: 实现任务依赖关系可视化

**任务清单**:

#### 9.1 ASCII 拓扑图
- [ ] `print_topology` 方法
- [ ] 依赖层级计算
- [ ] 状态着色

#### 9.2 Mermaid 图导出
- [ ] 生成 Mermaid 格式
- [ ] 保存到文件

**产出物**:
- `matrix-core/src/topology.rs`

**验证标准**:
- [ ] ASCII 图正确显示
- [ ] Mermaid 文件可渲染

---

### Phase 10: CLI 接口

**目标**: 实现完整的命令行接口

**任务清单**:

#### 10.1 参数解析
- [ ] clap derive 宏定义
- [ ] 所有参数支持:
  - `<goal>` 项目目标
  - `[path]` 输出路径
  - `--doc <FILE>` 规格文档
  - `-d, --workspace` 工作区目录
  - `--mcp-config <FILE>` MCP 配置
  - `--resume` 恢复运行
  - `-n, --agents <N>` 并行 Agent 数量
  - `--debug` 实时输出
  - `--ask, -q` 交互式澄清

#### 10.2 输出格式化
- [ ] ANSI 颜色输出
- [ ] 进度显示
- [ ] 日志格式化

#### 10.3 交互模式
- [ ] 恢复确认
- [ ] 失败任务重试确认

**产出物**:
- `matrix-cli/src/main.rs`
- `matrix-cli/src/args.rs`
- `matrix-cli/src/output.rs`

**验证标准**:
- [ ] 所有参数正确解析
- [ ] 输出格式美观
- [ ] 交互流畅

---

### Phase 11: Taskfile 自动化

**目标**: 实现一键安装和全局部署

**任务清单**:

#### 11.1 构建任务
- [ ] `task build` - 构建所有 crate
- [ ] `task test` - 运行所有测试
- [ ] `task run` - 运行 CLI

#### 11.2 安装任务
- [ ] `task install` - 一键安装
- [ ] 全局命令部署 (复制到 /usr/local/bin 或用户目录)
- [ ] 跨平台支持 (Linux, macOS, Windows)

#### 11.3 开发任务
- [ ] `task dev` - 开发模式运行
- [ ] `task lint` - 代码检查
- [ ] `task fmt` - 格式化

**产出物**:
- `Taskfile.yml`

**验证标准**:
- [ ] `task install` 成功安装
- [ ] `longtime` 命令全局可用
- [ ] 跨平台兼容

---

### Phase 12: 最终测试与文档

**目标**: 完善测试覆盖和文档

**任务清单**:

#### 12.1 集成测试
- [ ] 端到端测试
- [ ] 与 Python 版本输出对比

#### 12.2 文档
- [ ] README.md
- [ ] API 文档 (rustdoc)
- [ ] 使用示例

#### 12.3 性能优化
- [ ] 基准测试
- [ ] 内存优化
- [ ] 并行效率优化

**产出物**:
- `tests/` 集成测试
- `README.md`
- `docs/` 文档目录

**验证标准**:
- [ ] 测试覆盖率 > 80%
- [ ] 文档完整
- [ ] 性能与 Python 版本相当或更好

---

## 技术需求

### Rust 版本
- Rust 1.70+ (推荐最新 stable)

### 核心依赖

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
git2 = "0.18"
regex = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
dirs = "5"
```

### 外部依赖
- Claude CLI (claude) 已安装并配置

---

## 成功标准

### 功能完整性
- [ ] 所有 CLI 参数与 Python 版本一致
- [ ] 所有执行阶段正确实现
- [ ] 输出格式与 Python 版本兼容

### 质量标准
- [ ] 单元测试覆盖率 > 80%
- [ ] 集成测试覆盖主要流程
- [ ] 无 clippy 警告
- [ ] 代码格式化通过

### 性能标准
- [ ] 启动时间 < 100ms
- [ ] 内存占用 < Python 版本的 50%
- [ ] 并行效率 >= Python 版本

### 用户体验
- [ ] 一键安装成功
- [ ] 错误信息清晰
- [ ] 进度显示友好

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| git2 crate 跨平台兼容性 | 中 | 提供命令行回退方案 |
| Claude CLI 输出格式变化 | 高 | 多层解析策略 |
| Windows 终端颜色支持 | 低 | 使用 crossbeam-term |
| 异步复杂度 | 中 | 充分单元测试 |

---

## 时间估算

| 阶段 | 估算时间 |
|------|----------|
| Phase 1-2 | 2-3 天 |
| Phase 3-4 | 2-3 天 |
| Phase 5-6 | 3-4 天 |
| Phase 7-8 | 2-3 天 |
| Phase 9-10 | 2-3 天 |
| Phase 11-12 | 2-3 天 |
| **总计** | **13-19 天** |

---

## 里程碑

1. **M1 - 骨架完成**: Phase 1-2 完成，可编译运行
2. **M2 - 核心功能**: Phase 3-6 完成，可执行单个任务
3. **M3 - 并行执行**: Phase 7-8 完成，支持多 Agent
4. **M4 - CLI 就绪**: Phase 9-10 完成，可命令行使用
5. **M5 - 发布就绪**: Phase 11-12 完成，可正式发布
