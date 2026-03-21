# Phase 1: GUI 设计文档

## 1. 需求范围

基于 CLAUDE.md 要求："Workspace architecture: Use Rust workspace structure for future cli, Claude Code plugin, gui, and web clients"

本设计实现 GUI 客户端（Phase 1）。

### 1.1 CLI 与 GUI 关系

**两者共存**：
- `matrix` - 现有 CLI 命令（不变）
- `matrix-gui` - 新增的 GUI 命令行入口

**区别**：
| 命令 | 模式 |
|------|------|
| `matrix "build app"` | CLI/TUI 模式（默认） |
| `matrix-gui` | 启动图形界面 |
| `matrix --gui "build app"` | 指定 GUI 模式启动任务 |

**使用场景**:
- 用户更喜欢图形界面而非 CLI/TUI
- 需要同时查看任务树、日志、Claude 输出
- 需要交互式回答 Clarification 问题

## 2. 整体架构

```
matrix/
├── crates/
│   ├── core/              # 现有核心逻辑（不变）
│   ├── cli/               # 现有 CLI（不变）
│   └── gui/               # 新增 GUI crate
│       ├── src-tauri/     # Rust 后端
│       │   ├── src/
│       │   │   ├── lib.rs
│       │   │   ├── commands/   # Tauri 命令
│       │   │   └── state.rs    # 应用状态
│       │   ├── Cargo.toml
│       │   └── tauri.conf.json
│       └── src/           # React 前端
│           ├── src/
│           │   ├── App.tsx
│           │   ├── views/
│           │   ├── components/
│           │   └── hooks/
│           ├── package.json
│           └── vite.config.ts
└── Taskfile.yml
```

### 2.1 核心通信机制

- **Tauri Commands**: 前端 → 后端（控制命令）
- **Tauri Events**: 后端 → 前端（实时更新）

## 3. 后端 Commands & State

### 3.1 Tauri Commands

文件位置: `crates/gui/src-tauri/src/commands/`

```rust
// orchestrator.rs
#[tauri::command]
async fn start_orchestrator(goal: String) -> Result<(), String>

#[tauri::command]
async fn pause_orchestrator() -> Result<(), String>

#[tauri::command]
async fn resume_orchestrator() -> Result<(), String>

#[tauri::command]
async fn get_tasks() -> Vec<Task>

#[tauri::command]
async fn answer_question(answer: String) -> Result<(), String>
```

### 3.2 State 管理

```rust
struct AppState {
    orchestrator: Option<Arc<Mutex<Orchestrator>>>,
    config: OrchestratorConfig,
    event_sender: EventSender,
}
```

## 4. 前端 UI 布局

### 4.1 主界面

```
+---------------------------------------------------------+
|  Matrix                                    ⚙️ 设置  ─ ✕  |
+---------------------------------------------------------+
|                                                         |
|  +-----------+  +------------------------------------+  |
|  | 侧边栏     |  | 主内容区                            |  |
|  |           |  |                                    |  |
|  | 📁 项目    |  |  +------------------------------+ |  |
|  | 📊 Token  |  |  | 任务树 / 进度视图             | |  |
|  | ⏱️ 耗时   |  |  +------------------------------+ |  |
|  |           |  |                                    |  |
|  |           |  |  +------------------------------+ |  |
|  |           |  |  | Claude 输出 / 日志           | |  |
|  |           |  |  +------------------------------+ |  |
|  +-----------+  +------------------------------------+  |
|                                                         |
+---------------------------------------------------------+
|  [▶ 开始]  [⏸ 暂停]  [⏹ 停止]          v0.1.6 | glm-5  |
+---------------------------------------------------------+
```

### 4.2 页面/视图

| 视图 | 组件 | 功能 |
|------|------|------|
| Dashboard | TaskTree, LogPanel, ClaudeOutput, StatusBar | 主监控视图 |
| Settings | - | 模型、语言、代理数配置 |
| QuestionDialog | QuestionDialog | Clarification 弹窗 |

### 4.3 状态栏格式

```
v0.1.6 Generating ⠋ | Task:00:05 | Total:02:15 | 3/8 | glm-5
```

## 5. 实时数据流

```
Orchestrator
    │
    emit TuiEvent (复用现有)
    ▼
EventSender (core channel)
    │
    ├──────────> CLI TUI (现有)
    │
    └──────────> GUI 后端 (监听 channel)
                      │
                      emit to Tauri
                      ▼
                  React 前端 (监听事件)
```

**复用现有 TuiEvent**: 无需修改 core crate

## 6. 技术栈

| 层级 | 选择 | 理由 |
|------|------|------|
| 框架 | Tauri 2.0 + React | Tauri 2.0 最新稳定版，React 生态丰富 |
| 构建 | Vite | 快速 HMR，与 Tauri 配合好 |
| UI | Shadcn/ui + Tailwind | Headless 组件，可高度定制 |
| 状态 | Zustand | 简洁，无样板代码 |
| 通信 | Tauri Events | 原生双向通信 |

**版本**: Tauri 2.0 (最新稳定版)

**排除**:
- Electron: 体积大 (100MB+)
- Flutter: Dart 引入额外语言栈

## 7. MVP 范围

**第一版功能** (P0): ✅
- 启动任务
- 任务监控
- 日志查看
- Claude 输出 (Markdown)
- 暂停/继续
- 提问交互 (Clarification)

**后续版本**:
- v0.2: Token 统计、历史项目
- v0.3: 设置面板、主题切换

## 8. 文件结构

```
crates/gui/
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs              # Tauri 应用入口
│   │   ├── main.rs             # 可执行文件入口
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── orchestrator.rs
│   │   │   ├── task.rs
│   │   │   └── config.rs
│   │   ├── state.rs            # 应用状态管理
│   │   └── events.rs           # 事件转发器
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                        # Vue 前端
│   ├── src/
│   │   ├── App.vue
│   │   ├── main.ts
│   │   ├── views/
│   │   │   ├── Dashboard.vue
│   │   │   └── Settings.vue
│   │   ├── components/
│   │   │   ├── TaskTree.vue
│   │   │   ├── LogPanel.vue
│   │   │   ├── ClaudeOutput.vue
│   │   │   ├── QuestionDialog.vue
│   │   │   └── StatusBar.vue
│   │   ├── stores/
│   │   │   └── orchestrator.ts
│   │   └── utils/
│   │       └── event.ts
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   └── index.html
└── Cargo.toml
```

## 9. 构建配置

### 9.1 Workspace 配置

根 Cargo.toml:
```toml
members = [
    "crates/core",
    "crates/cli",
    "crates/gui/src-tauri",
]
```

### 9.2 Taskfile 命令

```yaml
  gui:dev:
    desc: Start GUI dev server
    cmds:
      - cd crates/gui/src && npm run tauri dev

  gui:build:
    desc: Build GUI for production
    cmds:
      - cd crates/gui/src && npm run tauri build
```

## 10. 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Tauri 版本兼容 | 高 | 使用稳定版 1.6，锁定版本 |
| 事件性能 | 中 | 批量更新，频率限制 |
| 前端 bundle 大小 | 低 | tree-shaking，按需加载 |

## 11. 测试计划

- [ ] 启动/暂停/继续任务
- [ ] 任务树实时更新
- [ ] 日志实时显示
- [ ] Claude 输出 Markdown 渲染
- [ ] 提问弹窗交互
- [ ] 构建成功 (dev + release)

---
**Design Complete**: 2024-XX-XX
**Status**: 等待用户审阅
