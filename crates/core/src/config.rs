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