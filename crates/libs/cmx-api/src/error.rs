use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use tracing::debug;

/// 错误码枚举
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrCode {
    Success = 0,
    /// 业务错误（后台主动抛出的已知错误，如参数校验失败），HTTP 200，json code 1
    BusinessError = 1,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    BadRequest = 400,
    ValidationError = 422,
    RateLimitExceeded = 429,
    InternalError = 500,
    ServiceUnavailable = 503,
    Timeout = 504,
}

impl From<ErrCode> for u16 {
    fn from(code: ErrCode) -> Self {
        code as u16
    }
}

/// 结果类型
pub type Result<T> = core::result::Result<T, Error>;

/// Web 层错误类型
#[derive(Debug, Error)]
pub enum Error {
    #[error("JSON 解析错误: {0}")]
    SerdeJson(String),

    #[error("验证错误: {0}")]
    Validator(String),

    #[error("未获取到svrContext")]
    SvrContextNotInReqExt,

    /// 业务错误（HTTP 200，json code -1），用于后台主动抛出的已知错误，如参数校验失败
    #[error("{0}")]
    BusinessError(String),

    #[error("未授权: {0}")]
    Unauthorized(String),

    #[error("禁止访问: {0}")]
    Forbidden(String),

    #[error("资源不存在: {0}")]
    NotFound(String),

    #[error("验证失败")]
    ValidationError {
        errors: Vec<String>,
    },

    #[error("请求错误: {0}")]
    BadRequest(String),

    #[error("{0}")]
    InternalError(String),

    #[error("服务不可用: {0}")]
    ServiceUnavailable(String),

    #[error("服务错误: {0}")]
    ServiceError(String),

    #[error("请求过于频繁，请在 {retry_after} 秒后重试。限制: {limit} 请求/{window} 秒")]
    RateLimitExceeded {
        retry_after: u64,
        limit: u64,
        window: u64,
    },

    #[error("请求超时")]
    Timeout,
}



impl Error {
    pub fn code(&self) -> ErrCode {
        match self {
            Self::BusinessError(_) => ErrCode::BusinessError,
            Self::Unauthorized(_) => ErrCode::Unauthorized,
            Self::Forbidden(_) => ErrCode::Forbidden,
            Self::NotFound(_) => ErrCode::NotFound,
            Self::ValidationError { .. } => ErrCode::ValidationError,
            Self::BadRequest(_) => ErrCode::BadRequest,
            Self::RateLimitExceeded { .. } => ErrCode::RateLimitExceeded,
            Self::InternalError(_) => ErrCode::InternalError,
            Self::ServiceUnavailable(_) => ErrCode::ServiceUnavailable,
            Self::Timeout => ErrCode::Timeout,
            _ => ErrCode::InternalError,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BusinessError(_) => StatusCode::OK,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::ValidationError { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> String {
        self.to_string()
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn validation_error(errors: Vec<String>) -> Self {
        Self::ValidationError { errors }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn business_error(msg: impl Into<String>) -> Self {
        Self::BusinessError(msg.into())
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }

    pub fn rate_limit_exceeded(retry_after: u64, limit: u64, window: u64) -> Self {
        Self::RateLimitExceeded { retry_after, limit, window }
    }

    pub fn get_status_code(&self) -> StatusCode {
        self.status_code()
    }

    pub fn to_response_body(&self) -> serde_json::Value {
        json!({
            "code": self.code() as u16,
            "msg": self.message(),
        })
    }
}

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
                let msgs: Vec<String> = errs.iter()
                    .map(|err| err.message.as_ref().map(|s| s.to_string()).unwrap_or_default())
                    .collect();
                format!("{}: {}", field, msgs.join(", "))
            })
            .collect();
        Self::ValidationError { errors: error_messages }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        debug!("{:<12} - Error {self:?}", "ERROR");
        let status_code = self.get_status_code();
        let body = self.to_response_body();

        let body = axum::Json(body);
        (status_code, body).into_response()
    }
}

impl Error {
    pub fn into_rate_limit_response(self) -> Response {
        if let Self::RateLimitExceeded { retry_after, .. } = self {
            let mut response = self.into_response();
            response.headers_mut().insert(
                "retry-after",
                retry_after.to_string().parse().unwrap(),
            );
            response
        } else {
            self.into_response()
        }
    }
}
