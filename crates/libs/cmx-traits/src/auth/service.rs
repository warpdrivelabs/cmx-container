//! 认证服务统一接口
//!
//! 定义 AuthService trait，返回强类型 AuthError，
//! 消费方可直接 match 错误变体做 HTTP 映射。

use async_trait::async_trait;
use cmx_core::AuthContext;
use serde::{Deserialize, Serialize};

use super::error::AuthError;
use super::user_query::OAuth2ClientData;

/// 认证凭证（策略模式入口）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Credentials {
    /// 用户名密码认证
    Password {
        username: String,
        password: String,
    },
    /// Refresh Token 刷新
    RefreshToken {
        refresh_token: String,
    },
    /// OAuth2 授权码
    AuthorizationCode {
        code: String,
        code_verifier: String,
        client_id: String,
    },
    /// API Key 认证
    ApiKey {
        key: String,
    },
    /// 第三方 OAuth2 登录（Provider 已验证通过，直接签发本平台 Token）
    ThirdPartyOAuth2 {
        /// Provider 名称（如 "google", "github"）
        provider: String,
        /// Provider 侧用户唯一标识
        provider_user_id: String,
        /// 本平台用户 ID（已通过 AccountLinker 关联）
        user_id: String,
    },
}

/// Token 对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// Access Token
    pub access_token: String,
    /// Refresh Token
    pub refresh_token: String,
    /// Access Token 过期时间（Unix 时间戳）
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）
    pub refresh_expires_at: i64,
    /// Token 类型（固定 "Bearer"）
    pub token_type: String,
}

/// 第三方 OAuth2 回调结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2CallbackResult {
    /// 一次性回调授权码（前端用此码换 TokenPair）
    pub callback_code: String,
    /// 原始 state（用于前端校验）
    pub state: String,
    /// 是否为新注册用户
    pub is_new: bool,
    /// Provider 名称
    pub provider: String,
}

/// 第三方 OAuth2 回调授权码交换结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2CallbackExchangeResult {
    /// Access Token
    pub access_token: String,
    /// Refresh Token
    pub refresh_token: String,
    /// Token 类型
    pub token_type: String,
    /// Access Token 过期时间（Unix 时间戳）
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）
    pub refresh_expires_at: i64,
    /// 是否为新注册用户
    pub is_new: bool,
    /// Provider 名称
    pub provider: String,
    /// 原始 state（用于前端校验 CSRF）
    pub state: String,
}

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    /// 设备类型（web/mobile/desktop/api）
    pub device_type: String,
    /// 设备 ID
    pub device_id: String,
    /// 客户端 IP
    pub ip: Option<String>,
    /// User-Agent
    pub user_agent: Option<String>,
}

/// 认证服务统一接口
///
/// 返回强类型 AuthError，消费方可直接 match 错误变体做 HTTP 映射。
#[async_trait]
pub trait AuthService: Send + Sync {
    /// 认证（根据凭证类型分发到不同策略）
    async fn authenticate(
        &self,
        credentials: Credentials,
        device_info: Option<DeviceInfo>,
    ) -> Result<TokenPair, AuthError>;

    /// 校验 Token（返回 AuthContext）
    async fn validate_token(&self, token: &str) -> Result<AuthContext, AuthError>;

    /// 刷新 Token（Refresh Token Rotation）
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenPair, AuthError>;

    /// 撤销指定 Token
    async fn revoke_token(&self, token: &str) -> Result<(), AuthError>;

    /// 撤销用户所有 Token
    async fn revoke_all_tokens(&self, user_id: &str) -> Result<(), AuthError>;

    /// 密码哈希（供 cmx-iam 超管初始化等场景使用）
    async fn hash_password(&self, plain: &str) -> Result<String, AuthError>;

    /// 密码校验
    async fn verify_password(&self, plain: &str, hash: &str) -> Result<bool, AuthError>;

    /// 刷新会话心跳
    async fn heartbeat(&self, user_id: &str, device_type: &str) -> Result<bool, AuthError>;

    /// 本地缓存失效（由 Pub/Sub 回调触发）
    ///
    /// 收到其他实例发布的缓存失效消息后，清除本实例的本地缓存。
    /// message 格式: `blacklist:{jti}` 或 `revoke_all:{user_id}`
    async fn invalidate_local_cache(&self, message: &str);

    /// 修改密码（含策略和历史校验）
    async fn change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), AuthError>;

    /// 确保超管账号存在（启动时调用）
    async fn ensure_super_admin(&self) -> Result<(), AuthError>;

    /// 导入静态 API Key（启动时调用）
    async fn import_static_api_keys(&self) -> Result<(), AuthError>;

    /// 启动过期会话定时清理任务
    async fn start_cleanup_task(&self);

    /// 查询 OAuth2 客户端（供 handler 使用）
    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuth2ClientData>, AuthError>;

    /// 仅验证用户名密码，返回 user_id（不签发 Token）
    ///
    /// 适用于 OAuth2 login 等只需验证身份获取 user_id 的场景，
    /// 避免签发 Token 后又立即撤销的性能浪费。
    async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> Result<String, AuthError>;

    /// 验证 API Key 并返回 AuthContext（无状态，不创建会话）
    ///
    /// 适用于中间件 X-API-Key 头认证场景，直接验证 API Key
    /// 并构建 AuthContext，跳过 Token 签发和 Session 创建流程。
    /// API Key 认证是无状态的，不需要 Session 管理。
    async fn validate_api_key(&self, key: &str) -> Result<AuthContext, AuthError>;

    /// 列出已启用的第三方 OAuth2 Provider
    async fn list_oauth2_providers(
        &self,
    ) -> Result<Vec<super::user_query::ProviderInfo>, AuthError>;

    /// 处理第三方 OAuth2 回调（交换 Token + 获取用户信息 + 关联/注册 + 签发本平台 Token）
    async fn handle_oauth2_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> Result<OAuth2CallbackResult, AuthError>;

    /// 用回调授权码交换 TokenPair 及附加信息
    async fn exchange_oauth2_callback_code(
        &self,
        code: &str,
    ) -> Result<OAuth2CallbackExchangeResult, AuthError>;

    /// 绑定第三方 OAuth2 账号到已登录用户
    async fn link_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
        code: &str,
    ) -> Result<(), AuthError>;

    /// 解除第三方 OAuth2 账号绑定（含安全检查：最后一个绑定不可解绑）
    async fn unlink_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<(), AuthError>;

    /// 存储第三方 OAuth2 Provider state（用于 authorize 重定向）
    async fn store_oauth2_provider_state(
        &self,
        state: &str,
        provider: &str,
    ) -> Result<(), AuthError>;
}
