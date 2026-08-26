//! 门户业务错误类型。
//!
//! 遵循项目规范：统一用 `thiserror` 派生，禁止手写 `impl Display/Error`。
//! 通过 [`From<PortalError> for cmx_api_types::Error`] 把业务错误映射为 HTTP 响应错误，
//! 使 handler 层可直接用 `?` 传播。

use thiserror::Error;

/// 门户业务层统一错误。
#[derive(Debug, Error)]
pub enum PortalError {
    /// 资源文件不存在（映射 404）。
    #[error("资源不存在: {0}")]
    NotFound(String),

    /// 请求参数非法（映射 400）。
    #[error("请求参数错误: {0}")]
    BadRequest(String),

    /// 无权执行该操作（映射 403）。
    #[error("无权执行: {0}")]
    Forbidden(String),

    /// 触发限流（映射 429）。
    #[error("请求过于频繁: {0}")]
    TooManyRequests(String),

    /// JSON 解析失败（映射 500）。
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 文件 I/O 错误（映射 500）。
    #[error("文件读写失败: {0}")]
    Io(#[from] std::io::Error),

    /// 其它业务错误（映射 500）。
    #[error("{0}")]
    Business(String),
}

impl PortalError {
    /// 构造资源不存在错误。
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 构造请求参数错误。
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    /// 构造无权执行错误。
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    /// 构造限流错误。
    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        Self::TooManyRequests(msg.into())
    }

    /// 构造通用业务错误。
    pub fn business(msg: impl Into<String>) -> Self {
        Self::Business(msg.into())
    }
}

/// 门户业务层 Result 别名。
pub type PortalResult<T> = Result<T, PortalError>;

/// 把门户业务错误映射为 API 层统一错误，使 handler 可直接 `?` 传播。
impl From<PortalError> for cmx_api_types::Error {
    fn from(err: PortalError) -> Self {
        match err {
            PortalError::NotFound(msg) => cmx_api_types::Error::not_found(msg),
            PortalError::BadRequest(msg) => cmx_api_types::Error::bad_request(msg),
            PortalError::Forbidden(msg) => cmx_api_types::Error::forbidden(msg),
            PortalError::TooManyRequests(_) => cmx_api_types::Error::rate_limit_exceeded(60, 0, 60),
            other => cmx_api_types::Error::internal_error(other.to_string()),
        }
    }
}
