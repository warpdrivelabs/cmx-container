use thiserror::Error;

/// 模型结果类型
pub type Result<T> = core::result::Result<T, ServiceError>;

/// 模型错误类型
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("服务内部错误{0}")]
    InternalError(String),
    #[error("请求参数错误{0}")]
    BadRequest(String),
    /// SeaQuery 错误
    #[error("SeaQuery异常{0}")]
    SeaQuery(#[from] sea_query::error::Error),

    /// Modql 转换为 SeaQuery 错误
    #[error(" Modql 转换为 SeaQuery 错误{0}")]
    ModqlIntoSea(#[from] modql::filter::IntoSeaError),
}

impl ServiceError {
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
}
