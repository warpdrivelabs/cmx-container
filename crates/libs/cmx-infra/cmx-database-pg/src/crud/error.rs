//! CRUD 服务层错误类型。

use thiserror::Error;

/// 模型结果类型
pub type Result<T> = core::result::Result<T, ServiceError>;

/// 模型错误类型
#[derive(Debug, Error)]
pub enum ServiceError {
    /// 服务内部错误（映射 500）。
    #[error("服务内部错误{0}")]
    InternalError(String),
    /// 请求参数错误（映射 400）。
    #[error("请求参数错误{0}")]
    BadRequest(String),
    /// SeaQuery 错误
    #[error("SeaQuery异常{0}")]
    SeaQuery(#[from] sea_query::error::Error),

    /// Modql 转换为 SeaQuery 错误
    #[error(" Modql 转换为 SeaQuery 错误{0}")]
    ModqlIntoSea(#[from] modql::filter::IntoSeaError),

    /// 业务数据异常。
    #[error("{0}")]
    BusinessError(String),
}

impl ServiceError {
    /// 构造服务内部错误。
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }

    /// 构造请求参数错误。
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    /// 构造业务数据异常。
    pub fn business_error(msg: impl Into<String>) -> Self {
        Self::BusinessError(msg.into())
    }
}
