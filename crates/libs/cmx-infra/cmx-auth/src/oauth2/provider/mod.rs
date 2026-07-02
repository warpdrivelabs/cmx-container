//! 第三方 OAuth2 Provider 抽象。
//!
//! 定义 `OAuth2Provider` trait，提供统一的第三方 Provider 接口。
//! 内置 Provider（Google/GitHub）和通用 Provider 均实现此 trait。

pub mod account_linker;
pub mod generic;
pub mod github;
pub mod google;
pub mod registry;

use async_trait::async_trait;
use cmx_traits::auth::AuthError;
use serde::{Deserialize, Serialize};

/// 第三方 OAuth2 Provider 统一接口。
#[async_trait]
pub trait OAuth2Provider: Send + Sync {
    /// 返回 Provider 唯一标识（如 `google`、`github`）。
    fn name(&self) -> &str;

    /// 返回 Provider 显示名称（如 `Google`、`GitHub`）。
    fn display_name(&self) -> &str;

    /// 返回 Provider 图标 URL（内置 Provider 提供默认值）。
    fn icon_url(&self) -> Option<&str> {
        None
    }

    /// 返回品牌色（用于前端按钮样式，如 `#4285F4`）。
    fn brand_color(&self) -> Option<&str> {
        None
    }

    /// 构建授权 URL（第一步：重定向用户到 Provider 授权页面）。
    ///
    /// # Arguments
    ///
    /// * `state` - CSRF state 字符串。
    /// * `redirect_uri` - 回调地址。
    /// * `scopes` - 请求的 scope 列表。
    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String;

    /// 用授权码交换 Token（第二步：POST 到 Provider token endpoint）。
    ///
    /// # Arguments
    ///
    /// * `code` - 第三方 Provider 返回的授权码。
    /// * `redirect_uri` - 回调地址。
    ///
    /// # Returns
    ///
    /// 成功时返回 `ProviderTokenResponse`。
    ///
    /// # Errors
    ///
    /// 当第三方服务不可达或返回错误时返回 `AuthError`。
    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<ProviderTokenResponse, AuthError>;

    /// 获取用户信息（第三步：用 access_token/id_token 获取用户信息）。
    ///
    /// # Arguments
    ///
    /// * `token_response` - `exchange_code` 返回的 Token 响应。
    ///
    /// # Returns
    ///
    /// 成功时返回 `ProviderUserInfo`。
    ///
    /// # Errors
    ///
    /// 当第三方服务不可达或返回错误时返回 `AuthError`。
    async fn get_user_info(
        &self,
        token_response: &ProviderTokenResponse,
    ) -> Result<ProviderUserInfo, AuthError>;

    /// 返回 Provider 特有的 scope 列表（默认值）。
    fn default_scopes(&self) -> Vec<String>;

    /// 返回 Provider 配置的 `redirect_uri`。
    fn redirect_uri(&self) -> &str;
}

/// 第三方 Provider Token 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTokenResponse {
    /// 访问令牌。
    pub access_token: String,

    /// 令牌类型（通常为 `bearer`）。
    pub token_type: String,

    /// 过期时间（秒，相对当前时间）。
    pub expires_in: Option<u64>,

    /// 刷新令牌（OIDC Provider 通常返回）。
    pub refresh_token: Option<String>,

    /// 实际授权的 scope 列表（空格分隔字符串）。
    pub scope: Option<String>,

    /// ID Token（OIDC Provider 如 Google 会返回，可用于无 userinfo 调用）。
    pub id_token: Option<String>,
}

/// 第三方 Provider 用户信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUserInfo {
    /// Provider 侧的用户唯一标识。
    pub provider_user_id: String,

    /// 邮箱（可能为空，取决于请求的 scope）。
    pub email: Option<String>,

    /// 邮箱是否已验证。
    pub email_verified: Option<bool>,

    /// 用户名（部分 Provider 如 GitHub 返回）。
    pub username: Option<String>,

    /// 昵称/显示名。
    pub display_name: Option<String>,

    /// 头像 URL。
    pub avatar_url: Option<String>,
}

pub use account_linker::AccountLinker;
pub use account_linker::LinkResult;
pub use registry::OAuth2ProviderRegistry;
