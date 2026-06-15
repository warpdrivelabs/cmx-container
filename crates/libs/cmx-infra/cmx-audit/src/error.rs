//! cmx-audit 错误类型定义

use thiserror::Error;

/// cmx-audit 统一错误类型
#[derive(Debug, Error)]
pub enum AuditError {
    /// 数据库操作错误
    #[error("数据库操作错误: {0}")]
    Database(String),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

/// cmx-audit 统一结果类型别名
pub type Result<T> = core::result::Result<T, AuditError>;
