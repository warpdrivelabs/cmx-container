//! 认证服务统一接口。
//!
//! 定义 AuthService trait，返回强类型 AuthError，
//! 消费方可直接 match 错误变体做 HTTP 映射。

use async_trait::async_trait;
use cmx_core::AuthContext;
use serde::{Deserialize, Serialize};

use super::error::AuthError;
use super::user_query::OAuth2ClientData;

/// 当前登录用户完整信息（用于 /api/auth/me 接口）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 昵称。
    pub nickname: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 手机号。
    pub phone: Option<String>,
    /// 头像 URL。
    pub avatar: Option<String>,
    /// 组织 ID。
    pub org_id: Option<String>,
    /// 性别：0-未知，1-男，2-女。
    pub gender: i64,
    /// 最后登录时间（Unix 时间戳）。
    pub last_login_at: Option<i64>,
    /// 最后登录 IP。
    pub last_login_ip: Option<String>,
    /// 描述。
    pub description: Option<String>,
    /// 角色列表。
    pub roles: Vec<String>,
    /// 权限列表。
    pub permissions: Vec<String>,
    /// 会话 ID。
    pub session_id: Option<String>,
    /// 设备类型。
    pub device_type: Option<String>,
    /// 认证方式。
    pub auth_method: Option<String>,
}

/// 认证凭证（策略模式入口）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Credentials {
    /// 用户名密码认证。
    Password {
        /// 用户名。
        username: String,
        /// 密码明文。
        password: String,
    },
    /// Refresh Token 刷新。
    RefreshToken {
        /// 已签发的 Refresh Token。
        refresh_token: String,
    },
    /// OAuth2 授权码。
    AuthorizationCode {
        /// 授权码。
        code: String,
        /// PKCE code_verifier。
        code_verifier: String,
        /// 客户端 ID。
        client_id: String,
    },
    /// API Key 认证。
    ApiKey {
        /// API Key 字符串。
        key: String,
    },
    /// 第三方 OAuth2 登录（Provider 已验证通过，直接签发本平台 Token）。
    ThirdPartyOAuth2 {
        /// Provider 名称（如 "google", "github"）。
        provider: String,
        /// Provider 侧用户唯一标识。
        provider_user_id: String,
        /// 本平台用户 ID（已通过 AccountLinker 关联）。
        user_id: String,
    },
}

/// Token 对。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// Access Token。
    pub access_token: String,
    /// Refresh Token。
    pub refresh_token: String,
    /// Access Token 过期时间（Unix 时间戳）。
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）。
    pub refresh_expires_at: i64,
    /// Token 类型（固定 "Bearer"）。
    pub token_type: String,
}

/// 第三方 OAuth2 回调结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2CallbackResult {
    /// 一次性回调授权码（前端用此码换 TokenPair）。
    pub callback_code: String,
    /// 原始 state（用于前端校验）。
    pub state: String,
    /// 是否为新注册用户。
    pub is_new: bool,
    /// Provider 名称。
    pub provider: String,
}

/// 第三方 OAuth2 回调授权码交换结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2CallbackExchangeResult {
    /// Access Token。
    pub access_token: String,
    /// Refresh Token。
    pub refresh_token: String,
    /// Token 类型。
    pub token_type: String,
    /// Access Token 过期时间（Unix 时间戳）。
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）。
    pub refresh_expires_at: i64,
    /// 是否为新注册用户。
    pub is_new: bool,
    /// Provider 名称。
    pub provider: String,
    /// 原始 state（用于前端校验 CSRF）。
    pub state: String,
}

/// 设备信息。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    /// 设备类型（web/mobile/desktop/api）。
    pub device_type: String,
    /// 设备 ID。
    pub device_id: String,
    /// 客户端 IP。
    pub ip: Option<String>,
    /// User-Agent。
    pub user_agent: Option<String>,
}

/// 认证服务统一接口。
///
/// 返回强类型 AuthError，消费方可直接 match 错误变体做 HTTP 映射。
#[async_trait]
pub trait AuthService: Send + Sync {
    /// 根据凭证类型分发到不同策略进行认证。
    ///
    /// # Arguments
    ///
    /// * `credentials` - 认证凭证，决定使用哪种认证策略。
    /// * `device_info` - 设备信息，用于会话追踪和审计。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`TokenPair`]。
    ///
    /// # Errors
    ///
    /// 认证失败时返回对应的 [`AuthError`]，如 [`AuthError::InvalidCredentials`]、[`AuthError::UserDisabled`]。
    async fn authenticate(
        &self,
        credentials: Credentials,
        device_info: Option<DeviceInfo>,
    ) -> Result<TokenPair, AuthError>;

    /// 校验 Token 并返回认证上下文。
    ///
    /// # Arguments
    ///
    /// * `token` - 待校验的 Access Token。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`AuthContext`]。
    ///
    /// # Errors
    ///
    /// * [`AuthError::InvalidToken`] - Token 无效。
    /// * [`AuthError::TokenExpired`] - Token 已过期。
    /// * [`AuthError::TokenRevoked`] - Token 已被撤销。
    async fn validate_token(&self, token: &str) -> Result<AuthContext, AuthError>;

    /// 刷新 Token（Refresh Token Rotation）。
    ///
    /// # Arguments
    ///
    /// * `refresh_token` - 已签发的 Refresh Token。
    ///
    /// # Returns
    ///
    /// 成功时返回新的 [`TokenPair`]，旧 Refresh Token 失效。
    ///
    /// # Errors
    ///
    /// * [`AuthError::InvalidToken`] - Refresh Token 无效。
    /// * [`AuthError::TokenExpired`] - Refresh Token 已过期。
    /// * [`AuthError::ReplayDetected`] - 检测到重放攻击。
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenPair, AuthError>;

    /// 撤销指定 Token。
    ///
    /// # Arguments
    ///
    /// * `token` - 待撤销的 Token（Access 或 Refresh）。
    ///
    /// # Errors
    ///
    /// Token 无效或撤销失败时返回错误。
    async fn revoke_token(&self, token: &str) -> Result<(), AuthError>;

    /// 撤销用户所有 Token。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Errors
    ///
    /// 撤销失败时返回错误。
    async fn revoke_all_tokens(&self, user_id: &str) -> Result<(), AuthError>;

    /// 对明文密码进行哈希（供 cmx-iam 超管初始化等场景使用）。
    ///
    /// # Arguments
    ///
    /// * `plain` - 明文密码。
    ///
    /// # Returns
    ///
    /// 成功时返回 Argon2 哈希字符串。
    ///
    /// # Errors
    ///
    /// 哈希计算失败时返回 [`AuthError::PasswordHashError`]。
    async fn hash_password(&self, plain: &str) -> Result<String, AuthError>;

    /// 校验明文密码与哈希是否匹配。
    ///
    /// # Arguments
    ///
    /// * `plain` - 明文密码。
    /// * `hash` - 已存储的密码哈希。
    ///
    /// # Returns
    ///
    /// 匹配返回 `Ok(true)`，不匹配返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 哈希计算异常时返回 [`AuthError::PasswordVerifyFailed`]。
    async fn verify_password(&self, plain: &str, hash: &str) -> Result<bool, AuthError>;

    /// 刷新会话心跳。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `device_type` - 设备类型。
    ///
    /// # Returns
    ///
    /// 会话有效返回 `Ok(true)`，会话不存在或已过期返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 更新失败时返回错误。
    async fn heartbeat(&self, user_id: &str, device_type: &str) -> Result<bool, AuthError>;

    /// 本地缓存失效（由 Pub/Sub 回调触发）。
    ///
    /// 收到其他实例发布的缓存失效消息后，清除本实例的本地缓存。
    /// message 格式：`blacklist:{jti}` 或 `revoke_all:{user_id}`。
    ///
    /// # Arguments
    ///
    /// * `message` - 缓存失效消息。
    async fn invalidate_local_cache(&self, message: &str);

    /// 修改密码（含策略和历史校验）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `old_password` - 旧密码明文。
    /// * `new_password` - 新密码明文。
    ///
    /// # Errors
    ///
    /// * [`AuthError::PasswordVerifyFailed`] - 旧密码校验失败。
    /// * [`AuthError::PasswordPolicyViolated`] - 新密码不符合策略。
    /// * [`AuthError::PasswordReused`] - 新密码与历史密码重复。
    async fn change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), AuthError>;

    /// 确保超管账号存在（启动时调用）。
    ///
    /// # Errors
    ///
    /// 创建超管账号失败时返回错误。
    async fn ensure_super_admin(&self) -> Result<(), AuthError>;

    /// 导入静态 API Key（启动时调用）。
    ///
    /// # Errors
    ///
    /// 导入失败时返回错误。
    async fn import_static_api_keys(&self) -> Result<(), AuthError>;

    /// 查询 OAuth2 客户端（供 handler 使用）。
    ///
    /// # Arguments
    ///
    /// * `client_id` - 客户端 ID。
    ///
    /// # Returns
    ///
    /// 客户端存在时返回 `Ok(Some(client))`，不存在时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回错误。
    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuth2ClientData>, AuthError>;

    /// 仅验证用户名密码，返回 user_id（不签发 Token）。
    ///
    /// 适用于 OAuth2 login 等只需验证身份获取 user_id 的场景，
    /// 避免签发 Token 后又立即撤销的性能浪费。
    ///
    /// # Arguments
    ///
    /// * `username` - 用户名。
    /// * `password` - 明文密码。
    ///
    /// # Returns
    ///
    /// 成功时返回 user_id。
    ///
    /// # Errors
    ///
    /// * [`AuthError::InvalidCredentials`] - 用户名或密码错误。
    /// * [`AuthError::UserDisabled`] - 用户已被禁用。
    async fn verify_credentials(&self, username: &str, password: &str)
    -> Result<String, AuthError>;

    /// 验证 API Key 并返回 AuthContext（无状态，不创建会话）。
    ///
    /// 适用于中间件 X-API-Key 头认证场景，直接验证 API Key
    /// 并构建 AuthContext，跳过 Token 签发和 Session 创建流程。
    /// API Key 认证是无状态的，不需要 Session 管理。
    ///
    /// # Arguments
    ///
    /// * `key` - API Key 字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`AuthContext`]。
    ///
    /// # Errors
    ///
    /// * [`AuthError::InvalidApiKey`] - API Key 无效。
    /// * [`AuthError::ApiKeyExpired`] - API Key 已过期。
    async fn validate_api_key(&self, key: &str) -> Result<AuthContext, AuthError>;

    /// 获取当前登录用户的完整信息（含 nickname/email/roles/permissions）。
    ///
    /// 用于 `/api/auth/me` 接口，从 cmx_user 表查询用户基本信息，
    /// 并附加角色、权限列表。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`UserInfo`]。
    ///
    /// # Errors
    ///
    /// 用户不存在或查询失败时返回错误。
    async fn get_user_info(&self, user_id: &str) -> Result<UserInfo, AuthError>;

    /// 列出已启用的第三方 OAuth2 Provider。
    ///
    /// # Returns
    ///
    /// 成功时返回 Provider 信息列表。
    ///
    /// # Errors
    ///
    /// 查询失败时返回错误。
    async fn list_oauth2_providers(
        &self,
    ) -> Result<Vec<super::user_query::ProviderInfo>, AuthError>;

    /// 处理第三方 OAuth2 回调（交换 Token + 获取用户信息 + 关联/注册 + 签发本平台 Token）。
    ///
    /// # Arguments
    ///
    /// * `provider` - Provider 名称。
    /// * `code` - 授权码。
    /// * `state` - state 参数（用于 CSRF 校验）。
    /// * `device_info` - 设备信息。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`OAuth2CallbackResult`]，包含一次性回调授权码。
    ///
    /// # Errors
    ///
    /// * [`AuthError::OAuth2CallbackCodeInvalid`] - 授权码无效或已过期。
    /// * [`AuthError::OAuth2ProviderNotFound`] - Provider 不存在。
    /// * [`AuthError::OAuth2ProviderUnavailable`] - Provider 服务不可达。
    async fn handle_oauth2_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> Result<OAuth2CallbackResult, AuthError>;

    /// 用授权码交换 TokenPair 及附加信息（统一接口，支持两种模式）。
    ///
    /// **后端回调模式**：`code` 为 [`Self::handle_oauth2_callback`] 签发的一次性回调码，
    /// 后端从 Redis 取回已签发的 TokenPair 返回。
    ///
    /// **前端直调模式**：`code` 为 Provider 返回的原始授权码，后端消费 `state`
    /// 获取 provider，执行完整的换 token + 用户关联 + 签发 TokenPair 流程。
    ///
    /// 后端自动判断模式，前端无感知。
    ///
    /// # Arguments
    ///
    /// * `code` - 一次性回调授权码（后端回调模式）或 Provider 原始授权码（前端直调模式）。
    /// * `state` - 原始 state（用于 CSRF 校验 + 前端直调模式下消费获取 provider）。
    /// * `device_info` - 设备信息（前端直调模式签发 TokenPair 时使用）。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`OAuth2CallbackExchangeResult`]。
    ///
    /// # Errors
    ///
    /// * [`AuthError::OAuth2CallbackCodeInvalid`] - 授权码和 state 均无效或已过期。
    /// * [`AuthError::OAuth2`] - state 不匹配 / 账号未注册 / Provider 错误等。
    async fn exchange_oauth2_callback_code(
        &self,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> Result<OAuth2CallbackExchangeResult, AuthError>;

    /// 绑定第三方 OAuth2 账号到已登录用户。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 已登录用户 ID。
    /// * `provider` - Provider 名称。
    /// * `code` - 授权码。
    ///
    /// # Errors
    ///
    /// 绑定失败时返回错误。
    async fn link_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
        code: &str,
    ) -> Result<(), AuthError>;

    /// 解除第三方 OAuth2 账号绑定（含安全检查：最后一个绑定不可解绑）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `provider` - Provider 名称。
    ///
    /// # Errors
    ///
    /// * [`AuthError::OAuth2LastBindingCannotRemove`] - 无法解除最后一个登录绑定。
    async fn unlink_oauth2_account(&self, user_id: &str, provider: &str) -> Result<(), AuthError>;

    /// 存储第三方 OAuth2 Provider state（用于 authorize 重定向）。
    ///
    /// # Arguments
    ///
    /// * `state` - state 字符串（用于 CSRF 校验）。
    /// * `provider` - Provider 名称。
    ///
    /// # Errors
    ///
    /// 存储失败时返回错误。
    async fn store_oauth2_provider_state(
        &self,
        state: &str,
        provider: &str,
    ) -> Result<(), AuthError>;
}
