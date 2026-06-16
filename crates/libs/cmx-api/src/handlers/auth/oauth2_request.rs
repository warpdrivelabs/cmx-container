//! OAuth2 API 请求结构体

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// OAuth2 authorize 请求（GET 参数）
#[derive(Debug, Deserialize, IntoParams)]
pub struct OAuth2AuthorizeRequest {
    /// 客户端 ID
    pub client_id: String,
    /// 回调地址
    pub redirect_uri: String,
    /// 响应类型（固定 "code"）
    #[serde(default = "default_response_type")]
    pub response_type: String,
    /// PKCE code_challenge（S256 方法）
    pub code_challenge: Option<String>,
    /// PKCE code_challenge_method（S256 / plain）
    pub code_challenge_method: Option<String>,
    /// 请求的 scope（空格分隔）
    pub scope: Option<String>,
    /// CSRF state
    pub state: String,
}

fn default_response_type() -> String {
    "code".to_string()
}

/// OAuth2 login 请求（用户认证 + 授权确认）
#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuth2LoginRequest {
    /// CSRF state（从 authorize 返回）
    pub state: String,
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
    /// 客户端 ID
    pub client_id: String,
    /// 回调地址
    pub redirect_uri: String,
    /// PKCE code_challenge
    pub code_challenge: Option<String>,
    /// PKCE code_challenge_method
    pub code_challenge_method: Option<String>,
    /// 请求的 scope（逗号或空格分隔）
    pub scope: Option<String>,
}

/// OAuth2 token 交换请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuth2TokenRequest {
    /// 授权类型（固定 "authorization_code"）
    #[serde(default = "default_grant_type")]
    pub grant_type: String,
    /// 授权码
    pub code: String,
    /// PKCE code_verifier
    pub code_verifier: Option<String>,
    /// 客户端 ID
    pub client_id: String,
    /// 回调地址
    pub redirect_uri: String,
}

fn default_grant_type() -> String {
    "authorization_code".to_string()
}
