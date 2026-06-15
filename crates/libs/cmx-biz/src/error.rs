//! cmx-biz 错误类型定义

use thiserror::Error;

/// cmx-biz 统一错误类型
#[derive(Debug, Error)]
pub enum BizError {
    /// 数据库 CRUD 操作错误
    #[error("数据库操作错误: {0}")]
    Crud(#[from] cmx_database::crud::ServiceError),

    /// 数据库管理错误
    #[error("数据库管理错误: {0}")]
    Database(String),

    /// 业务逻辑错误
    #[error("业务错误: {0}")]
    Business(String),

    /// 数据未找到
    #[error("数据未找到: {0}")]
    NotFound(String),

    /// JSON 序列化/反序列化错误
    #[error("JSON 解析错误: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// 插件函数调用错误
    #[error("插件函数调用错误: {0}")]
    PluginInvoke(String),

    /// 服务编排错误
    #[error("服务编排错误: {0}")]
    Orchestration(String),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

/// cmx-biz 统一结果类型别名
pub type Result<T> = core::result::Result<T, BizError>;

impl BizError {
    /// 创建业务错误
    pub fn business(msg: impl Into<String>) -> Self {
        Self::Business(msg.into())
    }

    /// 创建未找到错误
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 创建内部错误
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// 支持 BizError 到 cmx_api_types::Error 的转换，
/// 使 cmx-api handler 中可以使用 `?` 操作符传播业务层错误。
impl From<BizError> for cmx_api_types::Error {
    fn from(e: BizError) -> Self {
        match e {
            BizError::Crud(e) => cmx_api_types::Error::from(e),
            BizError::Business(msg) => cmx_api_types::Error::business_error(msg),
            BizError::NotFound(msg) => cmx_api_types::Error::not_found(msg),
            BizError::SerdeJson(e) => cmx_api_types::Error::from(e),
            BizError::Database(msg)
            | BizError::PluginInvoke(msg)
            | BizError::Orchestration(msg)
            | BizError::Internal(msg) => cmx_api_types::Error::internal_error(msg),
        }
    }
}
