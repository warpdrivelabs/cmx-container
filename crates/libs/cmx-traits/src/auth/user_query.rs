//! 用户认证数据查询抽象。
//!
//! cmx-auth 不直接依赖 cmx-iam，通过此 trait 获取用户/角色/权限数据。
//! cmx-iam 负责实现此 trait。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::TraitError;

/// 用户认证数据（含密码哈希，仅认证服务可见）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAuthData {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 密码哈希（Argon2）。
    pub password_hash: Option<String>,
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
    /// 状态：0-禁用 1-启用。
    pub status: i64,
    /// 最后登录时间（Unix 时间戳）。
    pub last_login_at: Option<i64>,
    /// 最后登录 IP。
    pub last_login_ip: Option<String>,
    /// 描述。
    pub description: Option<String>,
}

/// API Key 数据（用于认证查询）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyData {
    /// Key 前缀（唯一标识）。
    pub key_prefix: String,
    /// Key 哈希（SHA256）。
    pub key_hash: String,
    /// 关联用户 ID。
    pub user_id: Option<String>,
    /// 关联服务名称。
    pub service_name: Option<String>,
    /// 允许的 scope（JSON 数组）。
    pub scopes: Vec<String>,
    /// 描述。
    pub description: Option<String>,
    /// 状态：0-禁用 1-启用。
    pub status: i64,
}

/// OAuth2 客户端数据（用于认证查询）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ClientData {
    /// 客户端标识。
    pub client_id: String,
    /// 客户端名称。
    pub client_name: String,
    /// 客户端密钥（confidential 类型使用，哈希存储）。
    pub client_secret: Option<String>,
    /// 回调地址列表（JSON 数组）。
    pub redirect_uris: Vec<String>,
    /// 允许的授权类型。
    pub grant_types: Vec<String>,
    /// 客户端类型：public / confidential。
    pub client_type: String,
    /// 是否强制 PKCE。
    pub pkce_required: bool,
    /// 允许的 scope。
    pub allowed_scopes: Vec<String>,
    /// 状态：0-禁用 1-启用。
    pub status: i64,
}

/// 第三方 OAuth2 用户信息（用于自动注册）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2UserInfo {
    /// Provider 标识。
    pub provider: String,
    /// Provider 侧用户唯一标识。
    pub provider_user_id: String,
    /// 邮箱。
    pub email: Option<String>,
    /// 用户名。
    pub username: Option<String>,
    /// 昵称/显示名。
    pub display_name: Option<String>,
    /// 头像 URL。
    pub avatar_url: Option<String>,
    /// 自动注册时的默认角色。
    pub default_role: Option<String>,
}

/// Provider 信息（供前端展示登录按钮）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider 名称。
    pub name: String,
    /// Provider 显示名称。
    pub display_name: String,
    /// 请求的 scope 列表。
    pub scopes: Vec<String>,
    /// Provider 图标 URL。
    pub icon_url: Option<String>,
    /// 品牌色（用于按钮样式）。
    pub brand_color: Option<String>,
}

/// 用户认证数据查询抽象。
///
/// cmx-auth 依赖此 trait 获取用户数据，cmx-iam 实现此 trait。
/// 不含 `db_id` 参数（认证固定使用 `default_db_id`）。
#[async_trait]
pub trait UserAuthQuery: Send + Sync {
    /// 根据用户名查询用户认证数据（含密码哈希）。
    ///
    /// # Arguments
    ///
    /// * `username` - 用户名。
    ///
    /// # Returns
    ///
    /// 用户存在时返回 `Ok(Some(user))`，不存在时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserAuthData>, TraitError>;

    /// 根据用户 ID 查询用户认证数据（refresh_token 场景使用）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 用户存在时返回 `Ok(Some(user))`，不存在时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<UserAuthData>, TraitError>;

    /// 获取用户角色编码列表。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 成功时返回角色编码列表，无角色时返回空 `Vec`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_role_codes(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, TraitError>;

    /// 获取用户权限编码列表。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 成功时返回权限编码列表，无权限时返回空 `Vec`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_permissions(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, TraitError>;

    /// 更新用户密码哈希（修改密码场景）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `new_hash` - 新密码哈希。
    ///
    /// # Errors
    ///
    /// 更新失败时返回 [`TraitError`]。
    async fn update_password_hash(
        &self,
        user_id: &str,
        new_hash: &str,
    ) -> Result<(), TraitError>;

    /// 更新最后登录信息（由 cmx-auth 登录流程调用）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `ip` - 登录 IP。
    ///
    /// # Errors
    ///
    /// 更新失败时返回 [`TraitError`]。
    async fn update_last_login(
        &self,
        user_id: &str,
        ip: &str,
    ) -> Result<(), TraitError>;

    /// 创建超管账号（含角色关联）。
    ///
    /// # Arguments
    ///
    /// * `username` - 超管用户名。
    /// * `password_hash` - 密码哈希。
    /// * `email` - 邮箱（可选）。
    /// * `roles` - 关联角色编码列表。
    ///
    /// # Errors
    ///
    /// 创建失败时返回 [`TraitError`]。
    async fn create_super_admin(
        &self,
        username: &str,
        password_hash: &str,
        email: Option<&str>,
        roles: &[String],
    ) -> Result<(), TraitError>;

    /// 根据邮箱查询用户认证数据（用于第三方 OAuth2 自动关联）。
    ///
    /// # Arguments
    ///
    /// * `email` - 邮箱。
    ///
    /// # Returns
    ///
    /// 用户存在时返回 `Ok(Some(user))`，不存在时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<UserAuthData>, TraitError>;

    /// 从第三方 OAuth2 信息自动注册用户（当 `auto_register=true` 时调用）。
    ///
    /// # Arguments
    ///
    /// * `provider` - Provider 名称。
    /// * `user_info` - 第三方用户信息。
    ///
    /// # Returns
    ///
    /// 成功时返回新创建的 user_id。
    ///
    /// # Errors
    ///
    /// 注册失败时返回 [`TraitError`]。
    async fn create_user_from_oauth2(
        &self,
        provider: &str,
        user_info: &OAuth2UserInfo,
    ) -> Result<String, TraitError>;
}
