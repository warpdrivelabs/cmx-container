//! 核心错误类型定义

use thiserror::Error;

/// 核心模块错误
#[derive(Debug, Error)]
pub enum CoreError {
    /// 已初始化
    #[error("{0}")]
    AlreadyInitialized(String),
}
