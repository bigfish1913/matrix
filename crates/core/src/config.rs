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
            Self::Fast => write!(f, "haiku"),
            Self::Smart => write!(f, "sonnet"),
        }
    }
}

// ── 全局配置常量 ──────────────────────────────────────────────────

/// 最大任务拆分深度
pub const MAX_DEPTH: u32 = 2;

/// 最大重试次数
pub const MAX_RETRIES: u32 = 3;

/// 规划操作超时（秒）
pub const TIMEOUT_PLAN: u64 = 300;

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

/// 进度汇报间隔（秒）
pub const REPORT_INTERVAL_SECS: u64 = 300; // 每5分钟汇报一次

/// Checkpoint配置
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// 汇报频率: 每 N 个任务 (None = 禁用)
    pub review_interval: Option<usize>,
    /// 汇报频率: 每 N% (如 20 表示 20%)
    pub review_percent: Option<usize>,
    /// 汇报频率: 距上次汇报超过 N 分钟 (默认 30 分钟)
    pub review_timeout_mins: Option<u64>,
    /// 是否在每批任务前验证依赖
    pub validate_before_batch: bool,
    /// 任务卡住阈值 (秒), 超过此时间被视为卡住
    pub stalled_threshold_secs: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            review_interval: Some(5),
            review_percent: None,
            review_timeout_mins: Some(30),
            validate_before_batch: true,
            stalled_threshold_secs: 600, // 10 分钟
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_display() {
        assert_eq!(Model::Fast.to_string(), "haiku");
        assert_eq!(Model::Smart.to_string(), "sonnet");
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_DEPTH, 2);
        assert_eq!(MAX_RETRIES, 3);
        assert_eq!(TIMEOUT_PLAN, 300);
        assert_eq!(TIMEOUT_EXEC, 3600);
        assert_eq!(MAX_PROMPT_LENGTH, 80000);
    }

    #[test]
    fn test_checkpoint_config_default() {
        let config = CheckpointConfig::default();
        assert_eq!(config.review_interval, Some(5));
        assert_eq!(config.review_timeout_mins, Some(30));
        assert!(config.validate_before_batch);
        assert_eq!(config.stalled_threshold_secs, 600);
    }
}
