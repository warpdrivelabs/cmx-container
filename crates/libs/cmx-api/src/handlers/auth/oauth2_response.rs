//! OAuth2 API 响应结构体

use serde::Serialize;
use utoipa::ToSchema;

/// OAuth2 authorize 响应
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuth2AuthorizeResponse {
    /// CSRF state（原样返回）
    pub state: String,
}

/// OAuth2 login 响应（返回授权码 + state）
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuth2LoginResponse {
    /// 授权码
    pub code: String,
    /// CSRF state（原样返回）
    pub state: String,
}

/// OAuth2 token 响应
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuth2TokenResponse {
    /// Access Token
    pub access_token: String,
    /// Refresh Token
    pub refresh_token: String,
    /// Token 类型
    pub token_type: String,
    /// Access Token 有效期（秒）
    pub expires_in: i64,
    /// Refresh Token 有效期（秒）
    pub refresh_expires_in: i64,
}
