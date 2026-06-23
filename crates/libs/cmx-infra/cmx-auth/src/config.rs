//! 认证配置。
//!
//! 定义 `AuthConfig` 及其子配置，支持从 TOML 文件加载。

use serde::{Deserialize, Serialize};

/// 认证配置根结构体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT 配置。
    pub jwt: JwtConfig,
    /// Token 过期配置。
    pub token: TokenExpiryConfig,
    /// Argon2 配置。
    pub argon2: Argon2Config,
    /// 会话配置。
    pub session: SessionConfig,
    /// 缓存配置。
    pub cache: CacheConfig,
    /// OAuth2 配置（可选）。
    pub oauth2: Option<OAuth2Config>,
    /// 静态 API Key 列表（从配置文件加载，启动时自动导入）。
    #[serde(default)]
    pub static_api_keys: Vec<StaticApiKeyConfig>,
    /// 超管配置（可选，启动时自动创建/更新超管账号）。
    #[serde(default)]
    pub super_admin: Option<SuperAdminConfig>,
    /// 认证白名单（无需认证的路径前缀列表）。
    ///
    /// 启动时与内置白名单合并，作为认证中间件的免认证路径集合。
    /// 默认为空数组（仅使用内置白名单），用户可在 TOML 中追加自定义路径。
    ///
    /// # Examples
    ///
    /// ```toml
    /// [auth]
    /// whitelist = ["/api/public", "/api/v1/webhook"]
    /// ```
    #[serde(default)]
    pub whitelist: Vec<String>,
}

/// 认证白名单内置默认值。
///
/// 包含无需认证的基础路径（登录、刷新、OAuth2 流程、文档、健康检查等）。
/// 用户可在 TOML 中通过 `[auth] whitelist = [...]]` 追加自定义路径。
pub const BUILTIN_WHITELIST: &[&str] = &[
    "/api/auth/login",
    "/api/auth/refresh",
    "/api/auth/validate",
    "/api/auth/logout",
    "/api/auth/health",
    "/api/auth/oauth2/authorize",
    "/api/auth/oauth2/login",
    "/api/auth/oauth2/token",
    "/api/auth/oauth2/providers",
    "/api/auth/oauth2/provider",
    "/swagger",
    "/api-docs",
    "/health",
];

/// JWT 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// 签名算法：`RS256` / `HS256`。
    pub algorithm: String,
    /// 私钥（支持文件路径或 PEM 内容，生产建议文件路径 + `chmod 600`）。
    pub private_key: Option<String>,
    /// 公钥（支持文件路径或 PEM 内容）。
    pub public_key: Option<String>,
    /// HMAC 密钥（`HS256` 模式使用）。
    pub secret: Option<String>,
    /// 签发者。
    pub issuer: String,
    /// 受众。
    pub audience: String,
    /// 当前签发使用的 `kid`（密钥轮换）。
    pub current_kid: Option<String>,
    /// 历史公钥列表（验签用，宽限期内的旧密钥）。
    #[serde(default)]
    pub legacy_public_keys: Vec<(String, String)>,
}

/// Token 过期配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExpiryConfig {
    /// Access Token TTL（秒），默认 1800（30 分钟）。
    pub access_ttl_secs: u64,
    /// Refresh Token TTL（秒），默认 604800（7 天）。
    pub refresh_ttl_secs: u64,
}

/// Argon2 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argon2Config {
    /// 内存开销，默认 65536（64MB）。
    pub memory_cost: u32,
    /// 时间开销，默认 3。
    pub time_cost: u32,
    /// 并行度，默认 4。
    pub parallelism: u32,
}

/// 会话配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// 同一设备类型是否只允许一个会话。
    pub single_session_per_device_type: bool,
    /// 最大会话数（0 = 不限制）。
    pub max_sessions: usize,
    /// 空闲超时（秒），默认 86400（24h）。
    pub idle_timeout_secs: u64,
    /// 心跳间隔（秒），默认 300（5 分钟）。
    pub heartbeat_interval_secs: u64,
}

/// 缓存配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// 是否启用本地缓存。
    pub enable_local_cache: bool,
    /// 本地缓存 TTL（秒），默认 30。
    pub local_ttl_secs: u64,
    /// 本地缓存最大容量，默认 10000。
    pub local_cache_max_entries: u64,
    /// 登录失败锁定阈值，默认 5。
    pub max_login_attempts: u32,
    /// 锁定时长（秒），默认 900（15 分钟）。
    pub lock_duration_secs: u64,
}

/// 第三方 OAuth2 Provider 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ProviderConfig {
    /// Provider 唯一标识（如 `google`、`github`）。
    pub name: String,
    /// Provider 显示名称（如 `Google`、`GitHub`）。
    #[serde(default)]
    pub display_name: String,
    /// Provider 类型（内置实现名称如 `google`/`github`，或 `generic` 使用通用实现）。
    #[serde(default = "default_provider_type")]
    pub provider_type: String,
    /// OAuth2 Client ID。
    pub client_id: String,
    /// OAuth2 Client Secret。
    pub client_secret: String,
    /// 回调地址（服务端生成，前端不可覆盖，防止 Open Redirect）。
    #[serde(default)]
    pub redirect_uri: String,
    /// 授权端点 URL（`generic` 类型必需，内置类型可省略）。
    #[serde(default)]
    pub authorize_url: String,
    /// Token 端点 URL（`generic` 类型必需，内置类型可省略）。
    #[serde(default)]
    pub token_url: String,
    /// 用户信息端点 URL（`generic` 类型必需，内置类型可省略）。
    #[serde(default)]
    pub userinfo_url: String,
    /// 请求的 scope 列表。
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 用户信息字段映射（标准字段名 → JSON 字段名，支持 number→string 自动转换）。
    #[serde(default)]
    pub field_mapping: std::collections::HashMap<String, String>,
    /// Token 端点认证方式：`client_secret_post`（默认）或 `client_secret_basic`。
    #[serde(default = "default_auth_method")]
    pub token_endpoint_auth_method: String,
    /// Provider 图标 URL。
    #[serde(default)]
    pub icon_url: Option<String>,
    /// 品牌色（用于前端按钮样式）。
    #[serde(default)]
    pub brand_color: Option<String>,
    /// 是否启用。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_provider_type() -> String {
    "generic".to_string()
}

fn default_enabled() -> bool {
    true
}

fn default_auth_method() -> String {
    "client_secret_post".to_string()
}

/// 第三方账号关联配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountLinkConfig {
    /// 是否根据邮箱自动关联已有本地用户。
    #[serde(default = "default_auto_link_by_email")]
    pub auto_link_by_email: bool,
    /// 是否自动注册新用户（当无匹配的本地用户时）。
    #[serde(default = "default_auto_register")]
    pub auto_register: bool,
    /// 自动注册时的默认角色（`role_code`）。
    #[serde(default)]
    pub default_role: Option<String>,
    /// 用户名生成策略：`provider_prefix` / `email_prefix` / `display_name`。
    #[serde(default = "default_username_strategy")]
    pub username_strategy: String,
}

impl Default for AccountLinkConfig {
    fn default() -> Self {
        Self {
            auto_link_by_email: true,
            auto_register: false,
            default_role: None,
            username_strategy: "provider_prefix".to_string(),
        }
    }
}

fn default_auto_link_by_email() -> bool {
    true
}

fn default_auto_register() -> bool {
    false
}

fn default_username_strategy() -> String {
    "provider_prefix".to_string()
}

fn default_state_ttl() -> u64 {
    600
}

fn default_callback_code_ttl() -> u64 {
    30
}

/// OAuth2 配置（扩展）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuth2Config {
    // === Authorization Server 配置（原有） ===
    /// 授权码有效期（秒），默认 600（10 分钟）。
    #[serde(default = "default_auth_code_ttl")]
    pub auth_code_ttl_secs: u64,
    /// 是否强制 PKCE。
    #[serde(default = "default_pkce_required")]
    pub pkce_required: bool,

    // === OAuth2 Client 配置（第三方 Provider 对接） ===
    /// 第三方 OAuth2 Provider 列表。
    #[serde(default)]
    pub providers: Vec<OAuth2ProviderConfig>,
    /// 第三方账号关联配置。
    #[serde(default)]
    pub account_link: AccountLinkConfig,
    /// Provider state 有效期（秒），默认 600。
    #[serde(default = "default_state_ttl")]
    pub state_ttl_secs: u64,
    /// 回调授权码有效期（秒），默认 30。
    #[serde(default = "default_callback_code_ttl")]
    pub callback_code_ttl_secs: u64,
    /// 第三方 OAuth2 登录成功后重定向到前端的 URL。
    #[serde(default)]
    pub frontend_callback_url: String,
}

fn default_auth_code_ttl() -> u64 {
    600
}

fn default_pkce_required() -> bool {
    true
}

/// 静态 API Key 配置（从配置文件加载）。
///
/// 简化用法：只填 `key` 字段，`key_prefix` 自动从 key 前 8 位提取。
///
/// # Examples
///
/// ```toml
/// [[auth.static_api_keys]]
/// key = "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456"
/// service_name = "billing-service"
/// ```
///
/// 高级用法：显式指定 `key_prefix`（用于迁移或自定义前缀）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticApiKeyConfig {
    /// 明文 Key（启动时自动 SHA256 哈希后存储；`key_prefix` 未填时从 key 前 8 位提取）。
    pub key: String,
    /// API Key 前缀（唯一标识）。
    ///
    /// 可选：未填时自动从 key 前 8 位提取。
    #[serde(default)]
    pub key_prefix: Option<String>,
    /// 关联用户 ID。
    pub user_id: Option<String>,
    /// 关联服务名称。
    pub service_name: Option<String>,
    /// 允许的 scope。
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 描述。
    pub description: Option<String>,
}

impl StaticApiKeyConfig {
    /// 解析 `key_prefix`：优先使用显式配置，否则从 key 前 8 位提取。
    ///
    /// # Returns
    ///
    /// 返回解析后的 `key_prefix` 字符串。当显式配置非空时直接返回；
    /// 否则从 `key` 前 8 位提取（key 长度不足 8 时取全部）。
    pub fn resolve_key_prefix(&self) -> String {
        if let Some(prefix) = &self.key_prefix
            && !prefix.is_empty() {
                return prefix.clone();
            }
        // 从 key 前 8 位提取（key 长度不足 8 时取全部）
        if self.key.len() >= 8 {
            self.key[..8].to_string()
        } else {
            self.key.clone()
        }
    }
}

/// 超管配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperAdminConfig {
    /// 超管用户名。
    pub username: String,
    /// 超管初始密码。
    pub password: String,
    /// 超管邮箱。
    pub email: Option<String>,
    /// 超管角色编码列表。
    #[serde(default = "default_super_admin_roles")]
    pub roles: Vec<String>,
}

fn default_super_admin_roles() -> Vec<String> {
    vec!["admin".to_string()]
}

impl Default for SuperAdminConfig {
    fn default() -> Self {
        Self {
            username: "admin".to_string(),
            password: "cmxadmin".to_string(),
            email: None,
            roles: default_super_admin_roles(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt: JwtConfig {
                algorithm: "HS256".into(),
                private_key: None,
                public_key: None,
                secret: Some("a7k9m2p4x8q1w5e3r6t0y7u2i9o4p1".into()),
                issuer: "cmx-auth".into(),
                audience: "cmx-platform".into(),
                current_kid: None,
                legacy_public_keys: vec![],
            },
            token: TokenExpiryConfig {
                access_ttl_secs: 1800,
                refresh_ttl_secs: 604800,
            },
            argon2: Argon2Config {
                memory_cost: 65536,
                time_cost: 3,
                parallelism: 4,
            },
            session: SessionConfig {
                single_session_per_device_type: false,
                max_sessions: 0,
                idle_timeout_secs: 86400,
                heartbeat_interval_secs: 300,
            },
            cache: CacheConfig {
                enable_local_cache: true,
                local_ttl_secs: 30,
                local_cache_max_entries: 10000,
                max_login_attempts: 5,
                lock_duration_secs: 900,
            },
            oauth2: None,
            static_api_keys: vec![],
            super_admin: Some(SuperAdminConfig::default()),
            whitelist: vec![],
        }
    }
}
