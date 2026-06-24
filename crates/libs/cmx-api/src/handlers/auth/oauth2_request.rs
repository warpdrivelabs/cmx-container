//! OAuth2 API 请求结构体

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// OAuth2 authorize 请求参数（GET 查询串）。
///
/// Authorization Code Flow 第一步：客户端引导用户访问此接口，cmx-auth 校验
/// 客户端注册信息后将 CSRF `state` 透传返回，由前端携带 `state` 跳转到登录页。
#[derive(Debug, Deserialize, IntoParams)]
pub struct OAuth2AuthorizeRequest {
    /// 客户端 ID。
    pub client_id: String,
    /// 回调地址，必须与客户端注册时填写的 redirect_uri 一致。
    pub redirect_uri: String,
    /// 响应类型，固定为 "code"。
    #[serde(default = "default_response_type")]
    pub response_type: String,
    /// PKCE code_challenge（S256 方法）。
    pub code_challenge: Option<String>,
    /// PKCE code_challenge_method（S256 / plain）。
    pub code_challenge_method: Option<String>,
    /// 请求的 scope（空格分隔）。
    pub scope: Option<String>,
    /// CSRF state，由客户端生成并在回调时回传校验。
    pub state: String,
}

fn default_response_type() -> String {
    "code".to_string()
}

/// OAuth2 login 请求载荷（用户认证 + 授权确认）。
///
/// Authorization Code Flow 第二步：用户提交用户名/密码后，cmx-auth 校验凭据
/// 并基于此前 authorize 阶段存储的 state 上下文签发授权码。
#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuth2LoginRequest {
    /// CSRF state（来自 authorize 阶段）。
    pub state: String,
    /// 用户名。
    pub username: String,
    /// 密码。
    pub password: String,
    /// 客户端 ID。
    pub client_id: String,
    /// 回调地址。
    pub redirect_uri: String,
    /// PKCE code_challenge。
    pub code_challenge: Option<String>,
    /// PKCE code_challenge_method。
    pub code_challenge_method: Option<String>,
    /// 请求的 scope（空格或逗号分隔）。
    pub scope: Option<String>,
}

/// OAuth2 token 交换请求载荷。
///
/// Authorization Code Flow 第三步：客户端使用授权码换发 Access/Refresh Token。
/// 若客户端在 authorize/login 阶段启用了 PKCE，必须同时提供 `code_verifier`。
#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuth2TokenRequest {
    /// 授权类型，固定为 "authorization_code"。
    #[serde(default = "default_grant_type")]
    pub grant_type: String,
    /// 授权码。
    pub code: String,
    /// PKCE code_verifier。
    pub code_verifier: Option<String>,
    /// 客户端 ID。
    pub client_id: String,
    /// 回调地址。
    pub redirect_uri: String,
}

fn default_grant_type() -> String {
    "authorization_code".to_string()
}
