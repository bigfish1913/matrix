# Checkpoint 机制与两级记忆系统设计

## 概述

为 Matrix 任务编排系统添加：
1. **Checkpoint 机制** - 批次前验证依赖、检测问题
2. **进度汇报** - 可配置频率的进度 review
3. **两级记忆系统** - 全局 + 任务级记忆共享

## 架构

### 新增模块

```
crates/core/src/
├── checkpoint/
│   ├── mod.rs           # 模块导出
│   ├── manager.rs       # CheckpointManager - 检查点管理
│   └── review.rs        # ReviewReport - 进度汇报生成
├── memory/
│   ├── mod.rs           # 模块导出
│   ├── global.rs        # GlobalMemory - 全局记忆
│   └── task_memory.rs   # TaskMemory - 任务级记忆
└── models/
    └── task.rs          # 扩展 Task 结构
```

### 数据流

```
Task Batch Start
       │
       ▼
┌─────────────────┐
│ CheckpointManager│
│  - validate_deps │
│  - detect_cycles │
│  - check_blocked │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  ReviewReport   │──────► Log Panel / TUI
│  - progress     │
│  - upcoming     │
│  - problems     │
└────────┬────────┘
         │
         ▼
   Execute Tasks
         │
         ▼
┌─────────────────┐
│ Memory Update   │
│  - extract      │
│  - global.md    │
│  - task.memory  │
└─────────────────┘
```

## 数据结构

### Task 扩展

```rust
// models/task.rs
pub struct Task {
    // ... 现有字段 ...

    /// 任务级记忆
    #[serde(default)]
    pub memory: TaskMemory,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskMemory {
    /// 经验教训
    pub learnings: Vec<String>,

    /// 代码变更记录
    pub code_changes: Vec<CodeChange>,

    /// 问题解决方案
    pub solutions: Vec<ProblemSolution>,

    /// 关键信息 (API端点、配置路径等)
    pub key_info: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChange {
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemSolution {
    pub problem: String,
    pub solution: String,
}
```

### Checkpoint 配置

```rust
// config.rs
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// 汇报频率: 每 N 个任务 (None = 禁用)
    pub review_interval: Option<usize>,

    /// 汇报频率: 每 N% (如 20 表示 20%)
    pub review_percent: Option<usize>,

    /// 是否在每批任务前验证依赖
    pub validate_before_batch: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            review_interval: Some(5),
            review_percent: None,
            validate_before_batch: true,
        }
    }
}
```

## CheckpointManager

### 职责

1. 批次前验证依赖图
2. 检测阻塞任务
3. 检测卡住的任务
4. 判断是否需要汇报

### 实现

```rust
// checkpoint/manager.rs
pub struct CheckpointManager {
    store: Arc<TaskStore>,
    config: CheckpointConfig,
    /// 自上次汇报以来完成的任务数
    tasks_since_review: usize,
    /// 上次汇报的里程碑 (用于百分比模式)
    last_review_at: usize,
}

impl CheckpointManager {
    /// 每批任务调度前调用
    pub async fn pre_batch_checkpoint(&mut self) -> Result<CheckpointResult> {
        let mut result = CheckpointResult::default();

        // 1. 验证依赖图
        let warnings = self.store.validate_dependencies().await;
        result.warnings = warnings;

        // 2. 检查被阻塞的任务 (依赖失败)
        result.blocked = self.find_blocked_tasks().await?;

        // 3. 检查卡住的任务 (in_progress 时间过长)
        result.stalled = self.find_stalled_tasks().await?;

        result.can_proceed = result.blocked.is_empty() || !self.config.validate_before_batch;

        Ok(result)
    }

    /// 根据配置判断是否需要汇报
    pub fn should_review(&self, completed: usize, total: usize) -> bool {
        if let Some(interval) = self.config.review_interval {
            return self.tasks_since_review >= interval;
        }
        if let Some(percent) = self.config.review_percent {
            let threshold = (total as f64 * percent as f64 / 100.0) as usize;
            let milestone = completed / threshold.max(1);
            return milestone > self.last_review_at;
        }
        false
    }

    /// 生成进度汇报
    pub async fn generate_review(&mut self) -> Result<ReviewReport> {
        self.tasks_since_review = 0;
        // ... 生成汇报
    }

    /// 任务完成时调用
    pub fn on_task_completed(&mut self) {
        self.tasks_since_review += 1;
    }
}

#[derive(Debug, Default)]
pub struct CheckpointResult {
    /// 依赖/循环警告
    pub warnings: Vec<String>,
    /// 被失败依赖阻塞的任务
    pub blocked: Vec<BlockedTask>,
    /// 卡住太久的任务
    pub stalled: Vec<String>,
    /// 是否可以继续执行
    pub can_proceed: bool,
}

pub struct BlockedTask {
    pub task_id: String,
    pub blocked_by: Vec<String>,
}
```

## ReviewReport

### 数据结构

```rust
// checkpoint/review.rs
pub struct ReviewReport {
    pub timestamp: DateTime<Utc>,
    pub progress: ProgressStats,
    pub upcoming_tasks: Vec<UpcomingTask>,
    pub issues: Vec<Issue>,
    pub eta: Option<Duration>,
}

pub struct ProgressStats {
    pub total: usize,
    pub completed: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub failed: usize,
    pub skipped: usize,
    pub completion_percent: f64,
}

pub struct UpcomingTask {
    pub id: String,
    pub title: String,
    pub depth: u32,
    pub depends_on: Vec<String>,
}

pub enum Issue {
    CircularDependency { cycle: Vec<String> },
    MissingDependency { task_id: String, missing: String },
    Blocked { task_id: String, blocked_by: Vec<String> },
    Stalled { task_id: String, duration: Duration },
}
```

### 输出格式

```
══════════════════════════════════════════════════════
  📊 进度汇报 (每 5 个任务)
══════════════════════════════════════════════════════

📈 统计: 12/30 完成 (40%) | 15 待处理 | 2 进行中 | 1 失败
⏱️  已用: 15m30s | 预估剩余: ~23m

📋 即将执行:
  • [task-013] 实现用户认证 (等待: task-012)
  • [task-014] 添加日志系统
  • [task-015] 配置数据库连接

⚠️  问题检测:
  • [task-008] 被阻塞: 依赖 task-007 失败

══════════════════════════════════════════════════════
```

## 两级记忆系统

### GlobalMemory

```rust
// memory/global.rs
pub struct GlobalMemory {
    path: PathBuf,
    content: String,
    cache: Option<String>,
}

impl GlobalMemory {
    pub fn new(workspace: &Path) -> Self {
        let path = workspace.join(".claude").join("memory.md");
        Self { path, content: String::new(), cache: None }
    }

    /// 读取 (带缓存)
    pub fn read(&mut self) -> &str {
        if self.cache.is_none() {
            self.cache = std::fs::read_to_string(&self.path).ok();
        }
        self.cache.as_deref().unwrap_or("")
    }

    /// 追加内容
    pub async fn append(&mut self, section: &str, content: &str) -> Result<()> {
        let new_content = format!(
            "\n\n---\n## {}\n\n{}\n",
            section, content
        );
        self.content.push_str(&new_content);

        // 确保目录存在
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 追加到文件
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?
            .write_all(new_content.as_bytes())
            .await?;

        // 清除缓存
        self.cache = None;

        Ok(())
    }

    /// 用于 prompt (截断到 MAX_MEMORY_SIZE)
    pub fn for_prompt(&self, max_size: usize) -> String {
        if self.content.len() > max_size {
            format!("{}...\n[已截断]", &self.content[..max_size])
        } else {
            self.content.clone()
        }
    }
}
```

### TaskMemory

```rust
// memory/task_memory.rs
impl TaskMemory {
    /// 从任务执行结果提取记忆
    pub async fn extract_from_result(
        runner: &ClaudeRunner,
        workspace: &Path,
        task: &Task,
    ) -> Result<Self> {
        let prompt = format!(
            r#"你是一个技术文档编写者，正在更新项目记忆。

当前任务:
- 标题: {}
- 描述: {}
- 执行结果: {}

请提取以下信息 (JSON 格式):
{{
  "learnings": ["学到的经验1", "经验2"],
  "code_changes": [
    {{"path": "src/auth.rs", "description": "添加了用户认证"}}
  ],
  "solutions": [
    {{"problem": "编译错误", "solution": "添加了缺失的 trait"}}
  ],
  "key_info": {{
    "api_endpoint": "/api/v1/auth"
  }}
}}

如果没有重要信息，返回空对象 {{}}。"#,
            task.title, task.description, task.result
        );

        let result = runner.call(&prompt, workspace, Some(60), None, None).await?;

        if result.is_error {
            return Ok(Self::default());
        }

        // 解析 JSON
        let memory: Self = match serde_json::from_str(&result.text) {
            Ok(m) => m,
            Err(_) => Self::default(),
        };

        Ok(memory)
    }

    /// 合并到全局记忆
    pub async fn merge_to_global(&self, global: &mut GlobalMemory, task: &Task) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        let mut content = String::new();

        if !self.learnings.is_empty() {
            content.push_str("### 经验教训\n");
            for l in &self.learnings {
                content.push_str(&format!("- {}\n", l));
            }
        }

        if !self.code_changes.is_empty() {
            content.push_str("### 代码变更\n");
            for c in &self.code_changes {
                content.push_str(&format!("- `{}`: {}\n", c.path, c.description));
            }
        }

        if !self.solutions.is_empty() {
            content.push_str("### 问题解决\n");
            for s in &self.solutions {
                content.push_str(&format!("- 问题: {}\n  解决: {}\n", s.problem, s.solution));
            }
        }

        if !self.key_info.is_empty() {
            content.push_str("### 关键信息\n");
            for (k, v) in &self.key_info {
                content.push_str(&format!("- {}: {}\n", k, v));
            }
        }

        if !content.is_empty() {
            global.append(&format!("[{}] {}", task.id, task.title), &content).await?;
        }

        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.learnings.is_empty()
            && self.code_changes.is_empty()
            && self.solutions.is_empty()
            && self.key_info.is_empty()
    }

    /// 供依赖任务读取的上下文
    pub fn for_dependency_context(&self) -> String {
        let mut parts = Vec::new();

        if !self.key_info.is_empty() {
            parts.push("关键信息:".to_string());
            for (k, v) in &self.key_info {
                parts.push(format!("  {}: {}", k, v));
            }
        }

        if !self.solutions.is_empty() {
            parts.push("注意事项:".to_string());
            for s in &self.solutions {
                parts.push(format!("  - {}", s.problem));
            }
        }

        parts.join("\n")
    }
}
```

## Orchestrator 集成

### 修改

```rust
// orchestrator/orchestrator.rs
pub struct Orchestrator {
    // ... 现有字段 ...

    checkpoint: CheckpointManager,
    global_memory: GlobalMemory,
}

impl Orchestrator {
    async fn run_dispatcher(&mut self) -> Result<()> {
        // ... 现有代码 ...

        while Instant::now() < deadline {
            // 收集完成的任务
            while let Some(res) = join_set.try_join_next() {
                // ... 处理完成 ...

                // ✨ 更新记忆
                if let Some(ref task) = completed_task {
                    self.update_task_memory(task).await?;
                    self.checkpoint.on_task_completed();
                }

                // ✨ 检查是否需要汇报
                let completed = self.store.count(TaskStatus::Completed).await?;
                let total = self.store.total().await?;
                if self.checkpoint.should_review(completed, total) {
                    self.show_review_report().await?;
                }
            }

            // ✨ 批次前检查点
            let checkpoint_result = self.checkpoint.pre_batch_checkpoint().await?;
            if !checkpoint_result.can_proceed {
                self.handle_checkpoint_issues(&checkpoint_result).await?;
            }

            // ... 调度任务 ...
        }
    }

    async fn update_task_memory(&mut self, task: &Task) -> Result<()> {
        // 1. 提取记忆
        let memory = TaskMemory::extract_from_result(
            &self.executor.runner,
            &self.config.workspace,
            task,
        ).await?;

        // 2. 保存到任务
        let mut task = task.clone();
        task.memory = memory.clone();
        self.store.save_task(&task).await?;

        // 3. 合并到全局
        memory.merge_to_global(&mut self.global_memory, &task).await?;

        info!(task_id = %task.id, "Memory updated");
        Ok(())
    }

    async fn show_review_report(&mut self) -> Result<()> {
        let report = self.checkpoint.generate_review().await?;
        info!("{}", report.format());
        self.emit_event(Event::ProgressReview { report });
        Ok(())
    }
}
```

## 执行流程

```
1. 批次开始
   └─► CheckpointManager.pre_batch_checkpoint()
       ├─► 验证依赖图
       ├─► 检测循环依赖
       └─► 查找阻塞/卡住的任务

2. 执行任务批次
   └─► 每个任务完成后:
       ├─► TaskMemory.extract_from_result()
       ├─► 保存到 Task.memory
       └─► 合并到 GlobalMemory

3. 检查汇报条件
   └─► CheckpointManager.should_review()
       └─► 生成 ReviewReport
           └─► 输出到日志/TUI

4. 重复步骤 1-3
```

## 配置

### 默认值

```rust
CheckpointConfig {
    review_interval: Some(5),      // 每 5 个任务汇报
    review_percent: None,          // 不使用百分比
    validate_before_batch: true,   // 批次前验证
}
```

### CLI 参数 (未来)

```bash
matrix "goal" --review-interval 10    # 每 10 个任务汇报
matrix "goal" --review-percent 20     # 每 20% 汇报
matrix "goal" --no-checkpoint         # 禁用检查点
```

## 测试计划

1. **单元测试**
   - `CheckpointManager::should_review()` 逻辑
   - `TaskMemory::extract_from_result()` JSON 解析
   - `GlobalMemory::append()` 文件操作

2. **集成测试**
   - 完整流程: 执行 5 个任务 → 触发汇报
   - 依赖失败: 阻塞检测
   - 记忆持久化: 重启后读取

3. **E2E 测试**
   - 运行真实项目 → 验证记忆文件生成
