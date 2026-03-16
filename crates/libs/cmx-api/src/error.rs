use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use tracing::debug;

/// 错误码枚举
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrCode {
    Success = 0,
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

impl Default for ErrCode {
    fn default() -> Self {
        Self::Success
    }
}

impl From<ErrCode> for u16 {
    fn from(code: ErrCode) -> Self {
        code as u16
    }
}

/// 结果类型
pub type Result<T> = core::result::Result<T, Error>;

/// Web 层错误类型
#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Error {
    SerdeJson(String),
    Validator(String),
    ReqStampNotInReqExt,
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    ValidationError(Vec<String>),
    BadRequest(String),
    InternalError(String),
    ServiceUnavailable(String),
    RateLimitExceeded {
        retry_after: u64,
        limit: u64,
        window: u64,
    },
    Timeout,
}

impl Error {
    pub fn code(&self) -> ErrCode {
        match self {
            Self::Unauthorized(_) => ErrCode::Unauthorized,
            Self::Forbidden(_) => ErrCode::Forbidden,
            Self::NotFound(_) => ErrCode::NotFound,
            Self::ValidationError(_) => ErrCode::ValidationError,
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
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::ValidationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unauthorized(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.clone(),
            Self::NotFound(msg) => msg.clone(),
            Self::ValidationError(errors) => errors.join(", "),
            Self::BadRequest(msg) => msg.clone(),
            Self::RateLimitExceeded { retry_after, limit, window } => {
                format!("请求过于频繁，请在 {} 秒后重试。限制: {} 请求/{} 秒", retry_after, limit, window)
            }
            Self::InternalError(msg) => msg.clone(),
            Self::ServiceUnavailable(msg) => msg.clone(),
            Self::Timeout => "请求超时".to_string(),
            Self::SerdeJson(e) => format!("JSON 解析错误: {}", e),
            Self::Validator(e) => format!("验证错误: {}", e),
            Self::ReqStampNotInReqExt => "请求标记不存在".to_string(),
        }
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
        Self::ValidationError(errors)
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }

    pub fn rate_limit_exceeded(retry_after: u64, limit: u64, window: u64) -> Self {
        Self::RateLimitExceeded { retry_after, limit, window }
    }

    /// 获取状态码
    pub fn get_status_code(&self) -> StatusCode {
        self.status_code()
    }

    /// 获取用于响应的 JSON 值
    pub fn to_response_body(&self) -> serde_json::Value {
        json!({
            "code": self.code() as u16,
            "msg": self.message(),
        })
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
        Self::ValidationError(error_messages)
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
    /// 用于限流错误的响应处理
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

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{}", self.message())
    }
}

impl std::error::Error for Error {}
