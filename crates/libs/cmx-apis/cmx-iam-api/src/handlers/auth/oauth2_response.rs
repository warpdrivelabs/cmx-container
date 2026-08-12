//! OAuth2 API 响应结构体

use serde::Serialize;
use utoipa::ToSchema;

/// OAuth2 authorize 响应载荷。
///
/// 校验通过后原样回传 `state`，由前端跳转登录页时携带。
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuth2AuthorizeResponse {
    /// CSRF state（原样返回）。
    pub state: String,
}

/// OAuth2 login 响应载荷（返回授权码 + state）。
///
/// 授权码有效期通常为 5-10 分钟，仅可使用一次。客户端拿到后立即调用 token 端点换发 Token。
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuth2LoginResponse {
    /// 授权码。
    pub code: String,
    /// CSRF state（原样返回，用于客户端最终校验）。
    pub state: String,
}

/// OAuth2 token 响应载荷。
///
/// 遵循 RFC 6749 标准的 token 响应字段。`expires_in` / `refresh_expires_in`
/// 为相对秒数（自当前时刻起），便于客户端无需做时区换算。
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuth2TokenResponse {
    /// Access Token。
    pub access_token: String,
    /// Refresh Token。
    pub refresh_token: String,
    /// Token 类型，固定为 "Bearer"。
    pub token_type: String,
    /// Access Token 有效期（秒）。
    pub expires_in: i64,
    /// Refresh Token 有效期（秒）。
    pub refresh_expires_in: i64,
}
