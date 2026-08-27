//! 统一错误码与错误类型定义。
//!
//! 该模块提供了 REST API 中标准化的错误码 [`ErrCode`]、错误类型 [`Error`] 和
//! 结果类型别名 [`Result`]，支持错误码与 HTTP 状态码的自动映射，
//! 并实现了 axum 的 `IntoResponse` trait 以便直接作为 handler 返回值。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

/// 业务错误码枚举。
///
/// 定义了所有标准业务错误码，与 HTTP 状态码对齐但独立于 HTTP 层，
/// 用于 API 响应体中的 `code` 字段。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrCode {
    /// 成功。
    Success = 0,
    /// 业务逻辑错误。
    BusinessError = 1,
    /// 未授权（未登录或令牌无效）。
    Unauthorized = 401,
    /// 禁止访问（权限不足）。
    Forbidden = 403,
    /// 资源不存在。
    NotFound = 404,
    /// 请求参数错误。
    BadRequest = 400,
    /// 数据验证失败。
    ValidationError = 422,
    /// 资源冲突（乐观锁 / 并发修改）。
    Conflict = 409,
    /// 请求频率超限。
    RateLimitExceeded = 429,
    /// 服务器内部错误。
    InternalError = 500,
    /// 服务不可用。
    ServiceUnavailable = 503,
    /// 请求超时。
    Timeout = 504,
}

impl From<ErrCode> for u16 {
    fn from(code: ErrCode) -> Self {
        code as u16
    }
}

/// 统一结果类型别名。
///
/// 所有返回错误的函数应使用 `Result<T>` 代替 `core::result::Result<T, Error>`。
pub type Result<T> = core::result::Result<T, Error>;

/// 统一错误类型。
///
/// 涵盖 REST API 中所有可能的错误场景，实现了 `IntoResponse` trait，
/// 可直接作为 axum handler 的返回值。
#[derive(Debug, Error)]
pub enum Error {
    /// JSON 序列化/反序列化错误。
    #[error("JSON 解析错误: {0}")]
    SerdeJson(String),

    /// 数据验证错误。
    #[error("验证错误: {0}")]
    Validator(String),

    /// 请求扩展中未找到 svrContext。
    #[error("未获取到svrContext")]
    SvrContextNotInReqExt,

    /// 业务逻辑错误。
    #[error("{0}")]
    BusinessError(String),

    /// 未授权错误。
    #[error("未授权: {0}")]
    Unauthorized(String),

    /// 禁止访问错误。
    #[error("禁止访问: {0}")]
    Forbidden(String),

    /// 资源不存在错误。
    #[error("资源不存在: {0}")]
    NotFound(String),

    /// 数据验证失败，包含多个错误消息。
    #[error("验证失败")]
    ValidationError {
        /// 验证错误消息列表。
        errors: Vec<String>,
    },

    /// 请求参数错误。
    #[error("请求错误: {0}")]
    BadRequest(String),

    /// 资源冲突（乐观锁：单据已被他人修改）。映射 HTTP 409。
    #[error("{0}")]
    Conflict(String),

    /// 服务器内部错误。
    #[error("{0}")]
    InternalError(String),

    /// 服务不可用错误。
    #[error("服务不可用: {0}")]
    ServiceUnavailable(String),

    /// 服务调用错误。
    #[error("服务错误: {0}")]
    ServiceError(String),

    /// 请求频率超限错误。
    #[error("请求过于频繁，请在 {retry_after} 秒后重试。限制: {limit} 请求/{window} 秒")]
    RateLimitExceeded {
        /// 建议重试等待时间（秒）。
        retry_after: u64,
        /// 请求限制数量。
        limit: u64,
        /// 限制时间窗口（秒）。
        window: u64,
    },

    /// 请求超时。
    #[error("请求超时")]
    Timeout,
}

impl Error {
    /// 返回错误对应的业务错误码。
    ///
    /// # Returns
    ///
    /// 与错误类型匹配的 [`ErrCode`] 枚举值。
    pub fn code(&self) -> ErrCode {
        match self {
            Self::BusinessError(_) => ErrCode::BusinessError,
            Self::Unauthorized(_) => ErrCode::Unauthorized,
            Self::Forbidden(_) => ErrCode::Forbidden,
            Self::NotFound(_) => ErrCode::NotFound,
            Self::ValidationError { .. } => ErrCode::ValidationError,
            Self::Conflict(_) => ErrCode::Conflict,
            Self::BadRequest(_) => ErrCode::BadRequest,
            Self::RateLimitExceeded { .. } => ErrCode::RateLimitExceeded,
            Self::InternalError(_) => ErrCode::InternalError,
            Self::ServiceUnavailable(_) => ErrCode::ServiceUnavailable,
            Self::Timeout => ErrCode::Timeout,
            _ => ErrCode::InternalError,
        }
    }

    /// 返回错误对应的 HTTP 状态码。
    ///
    /// # Returns
    ///
    /// 与错误类型匹配的 [`StatusCode`]。
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BusinessError(_) => StatusCode::OK,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::ValidationError { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 返回错误的描述消息。
    ///
    /// # Returns
    ///
    /// 错误的字符串描述。
    pub fn message(&self) -> String {
        self.to_string()
    }

    /// 创建未授权错误。
    ///
    /// # Arguments
    ///
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// `Error::Unauthorized` 变体。
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    /// 创建禁止访问错误。
    ///
    /// # Arguments
    ///
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// `Error::Forbidden` 变体。
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    /// 创建资源不存在错误。
    ///
    /// # Arguments
    ///
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// `Error::NotFound` 变体。
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 创建资源冲突错误（乐观锁，HTTP 409）。
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    /// 创建数据验证失败错误。
    ///
    /// # Arguments
    ///
    /// * `errors` - 验证错误消息列表。
    ///
    /// # Returns
    ///
    /// `Error::ValidationError` 变体。
    pub fn validation_error(errors: Vec<String>) -> Self {
        Self::ValidationError { errors }
    }

    /// 创建请求参数错误。
    ///
    /// # Arguments
    ///
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// `Error::BadRequest` 变体。
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    /// 创建业务逻辑错误。
    ///
    /// # Arguments
    ///
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// `Error::BusinessError` 变体。
    pub fn business_error(msg: impl Into<String>) -> Self {
        Self::BusinessError(msg.into())
    }

    /// 创建服务器内部错误。
    ///
    /// # Arguments
    ///
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// `Error::InternalError` 变体。
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }

    /// 创建请求频率超限错误。
    ///
    /// # Arguments
    ///
    /// * `retry_after` - 建议重试等待时间（秒）。
    /// * `limit` - 请求限制数量。
    /// * `window` - 限制时间窗口（秒）。
    ///
    /// # Returns
    ///
    /// `Error::RateLimitExceeded` 变体。
    pub fn rate_limit_exceeded(retry_after: u64, limit: u64, window: u64) -> Self {
        Self::RateLimitExceeded {
            retry_after,
            limit,
            window,
        }
    }

    /// 获取错误对应的 HTTP 状态码。
    ///
    /// 这是 [`status_code`](Error::status_code) 方法的别名。
    ///
    /// # Returns
    ///
    /// 与错误类型匹配的 [`StatusCode`]。
    pub fn get_status_code(&self) -> StatusCode {
        self.status_code()
    }

    /// 将错误转换为 API 响应体 JSON。
    ///
    /// # Returns
    ///
    /// 包含 `code` 和 `msg` 字段的 `serde_json::Value`。
    pub fn to_response_body(&self) -> serde_json::Value {
        json!({
            "code": self.code() as u16,
            "msg": self.message(),
        })
    }
}

/// sqlx 链错误转换桥（`db-error` feature 门控，默认开——平台侧零感知；引擎树经
/// `default-features = false` 消费本 crate 时不拉 sqlx 链）。
#[cfg(feature = "db-error")]
impl From<cmx_database::crud::ServiceError> for Error {
    fn from(e: cmx_database::crud::ServiceError) -> Self {
        match e {
            cmx_database::crud::ServiceError::BusinessError(s) => Self::business_error(s),
            _ => Self::ServiceError(e.to_string()),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::SerdeJson(e.to_string())
    }
}

impl From<validator::ValidationErrors> for Error {
    fn from(errors: validator::ValidationErrors) -> Self {
        let error_messages: Vec<String> = errors
            .field_errors()
            .into_iter()
            .map(|(field, errs)| {
                let msgs: Vec<String> = errs
                    .iter()
                    .map(|err| {
                        err.message
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                format!("{}: {}", field, msgs.join(", "))
            })
            .collect();
        Self::ValidationError {
            errors: error_messages,
        }
    }
}

impl From<cmx_core::PermissionDeniedError> for Error {
    fn from(e: cmx_core::PermissionDeniedError) -> Self {
        if e.is_unauthenticated() {
            Self::Unauthorized(e.to_string())
        } else {
            Self::Forbidden(e.to_string())
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::error!("{:<12} - Error {self:?}", "ERROR");
        let status_code = self.get_status_code();
        let body = self.to_response_body();

        let body = axum::Json(body);
        (status_code, body).into_response()
    }
}

impl Error {
    /// 将错误转换为 HTTP 响应，对频率超限错误添加 `retry-after` 响应头。
    ///
    /// 当错误为 `RateLimitExceeded` 时，自动在响应头中添加 `retry-after` 字段，
    /// 其他错误类型直接调用 [`into_response`](IntoResponse::into_response)。
    ///
    /// # Returns
    ///
    /// 包含适当状态码、响应体和可选 `retry-after` 头的 HTTP 响应。
    pub fn into_rate_limit_response(self) -> Response {
        if let Self::RateLimitExceeded { retry_after, .. } = self {
            let mut response = self.into_response();
            response
                .headers_mut()
                .insert("retry-after", retry_after.to_string().parse().unwrap());
            response
        } else {
            self.into_response()
        }
    }
}
