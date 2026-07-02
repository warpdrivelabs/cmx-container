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
    "/api/auth/oauth2/*/callback",
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

    // === Token 响应解析配置 ===
    /// Token 响应嵌套路径（点分 JSON 路径，如 `data` 或 `result.data`）。
    ///
    /// 部分厂商将 Token 响应包装在 `{"code":0,"data":{...}}` 中，
    /// 配置此字段后先导航到指定路径再提取 Token 字段。
    /// 空字符串表示无包装（直接从根对象提取）。
    #[serde(default)]
    pub token_response_path: String,

    /// Token 响应字段映射（标准字段名 → 厂商实际字段名）。
    ///
    /// 如 `access_token` → `accessToken`、`expires_in` → `expire`。
    #[serde(default)]
    pub token_field_mapping: std::collections::HashMap<String, String>,

    // === 用户信息端点配置 ===
    /// 用户信息端点请求方法：`GET`（默认）或 `POST`。
    #[serde(default = "default_userinfo_method")]
    pub userinfo_method: String,

    /// 用户信息端点 token 传递方式：`bearer`（默认）/ `query` / `form`。
    #[serde(default = "default_userinfo_token_param")]
    pub userinfo_token_param: String,

    /// 用户信息端点额外请求参数（始终作为 query 参数附加）。
    #[serde(default)]
    pub userinfo_extra_params: std::collections::HashMap<String, String>,

    /// 用户信息响应嵌套路径（点分 JSON 路径，如 `data`）。
    ///
    /// 空字符串表示无包装。
    #[serde(default)]
    pub userinfo_response_path: String,

    // === 授权 URL 配置 ===
    /// 授权 URL 额外参数（如 Azure AD `resource` 参数）。
    #[serde(default)]
    pub authorize_extra_params: std::collections::HashMap<String, String>,

    // === 网络配置 ===
    /// 是否跳过 SSL 证书验证（仅内网自签名证书场景使用，生产环境慎用）。
    #[serde(default)]
    pub skip_ssl_verification: bool,
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

fn default_userinfo_method() -> String {
    "GET".to_string()
}

fn default_userinfo_token_param() -> String {
    "bearer".to_string()
}

/// 第三方账号关联配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountLinkConfig {
    /// 是否根据邮箱自动关联已有本地用户。
    #[serde(default = "default_auto_link_by_email")]
    pub auto_link_by_email: bool,
    /// 是否根据用户名自动关联已有本地用户（企业场景常用）。
    ///
    /// 仅应对可信 Provider 启用：与邮箱关联不同，username 无"已验证"概念，
    /// 恶意 Provider 可通过返回目标用户名关联到任意本地账号。
    #[serde(default = "default_auto_link_by_username")]
    pub auto_link_by_username: bool,
    /// 是否自动注册新用户（当无匹配的本地用户时）。
    #[serde(default = "default_auto_register")]
    pub auto_register: bool,
    /// 自动注册时的默认角色（`role_code`）。
    #[serde(default)]
    pub default_role: Option<String>,
    /// 用户名生成策略：`provider_prefix` / `provider_user_id` / `username` / `email_prefix` / `display_name`。
    #[serde(default = "default_username_strategy")]
    pub username_strategy: String,
}

impl Default for AccountLinkConfig {
    fn default() -> Self {
        Self {
            auto_link_by_email: false,
            auto_link_by_username: true,
            auto_register: false,
            default_role: None,
            username_strategy: "username".to_string(),
        }
    }
}

fn default_auto_link_by_email() -> bool {
    false
}

fn default_auto_link_by_username() -> bool {
    true
}

fn default_auto_register() -> bool {
    true
}

fn default_username_strategy() -> String {
    "username".to_string()
}

fn default_state_ttl() -> u64 {
    600
}

fn default_callback_code_ttl() -> u64 {
    30
}

/// OAuth2 配置（扩展）。
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for OAuth2Config {
    fn default() -> Self {
        Self {
            auth_code_ttl_secs: default_auth_code_ttl(),
            pkce_required: default_pkce_required(),
            providers: Vec::new(),
            account_link: AccountLinkConfig::default(),
            state_ttl_secs: default_state_ttl(),
            callback_code_ttl_secs: default_callback_code_ttl(),
            frontend_callback_url: String::new(),
        }
    }
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
            && !prefix.is_empty()
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个 `StaticApiKeyConfig`，便于测试。
    fn make_config(key: &str, key_prefix: Option<&str>) -> StaticApiKeyConfig {
        StaticApiKeyConfig {
            key: key.to_string(),
            key_prefix: key_prefix.map(|s| s.to_string()),
            user_id: None,
            service_name: None,
            scopes: vec![],
            description: None,
        }
    }

    #[test]
    fn test_resolve_key_prefix_from_key_default() {
        // 未显式配置 key_prefix，应从 key 前 8 位提取
        let config = make_config("cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456", None);

        let prefix = config.resolve_key_prefix();
        assert_eq!(
            prefix, "cmx_sk_A",
            "未显式配置时应取 key 前 8 位作为 prefix"
        );
    }

    #[test]
    fn test_resolve_key_prefix_explicit_value_used() {
        // 显式配置的 key_prefix 应优先使用
        let config = make_config(
            "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456",
            Some("custom_prefix"),
        );

        let prefix = config.resolve_key_prefix();
        assert_eq!(prefix, "custom_prefix", "显式配置的 key_prefix 应被使用");
    }

    #[test]
    fn test_resolve_key_prefix_explicit_empty_falls_back_to_key() {
        // 显式配置空字符串时，应回退到从 key 提取
        let config = make_config("cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456", Some(""));

        let prefix = config.resolve_key_prefix();
        assert_eq!(prefix, "cmx_sk_A", "显式空配置应回退到 key 前 8 位");
    }

    #[test]
    fn test_resolve_key_prefix_short_key_uses_whole_key() {
        // key 长度不足 8 位时，应使用整个 key
        let config = make_config("short", None);

        let prefix = config.resolve_key_prefix();
        assert_eq!(prefix, "short", "短 key 应直接使用整个 key 作为 prefix");
    }

    #[test]
    fn test_resolve_key_prefix_exactly_8_chars() {
        // key 恰好 8 位，应使用整个 key
        let config = make_config("12345678", None);

        let prefix = config.resolve_key_prefix();
        assert_eq!(prefix, "12345678", "8 位 key 应使用整个 key 作为 prefix");
    }

    #[test]
    fn test_resolve_key_prefix_with_dev_key_pattern() {
        // 模拟开发环境常用的 API Key 格式
        let dev_key = "cmx_sk_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6";
        let config = make_config(dev_key, None);

        let prefix = config.resolve_key_prefix();
        assert_eq!(prefix, "cmx_sk_d");
        assert_eq!(prefix.len(), 8);
    }

    #[test]
    fn test_default_auth_config() {
        // 验证 Default 实现符合预期
        let config = AuthConfig::default();
        assert_eq!(config.jwt.algorithm, "HS256");
        assert_eq!(config.jwt.issuer, "cmx-auth");
        assert_eq!(config.jwt.audience, "cmx-platform");
        assert_eq!(config.token.access_ttl_secs, 1800);
        assert_eq!(config.token.refresh_ttl_secs, 604800);
        assert_eq!(config.argon2.memory_cost, 65536);
        assert_eq!(config.argon2.time_cost, 3);
        assert_eq!(config.argon2.parallelism, 4);
        assert!(config.oauth2.is_none());
        assert!(config.super_admin.is_some());
    }

    #[test]
    fn test_super_admin_config_default() {
        let sa = SuperAdminConfig::default();
        assert_eq!(sa.username, "admin");
        assert_eq!(sa.password, "cmxadmin");
        assert!(sa.email.is_none());
        assert_eq!(sa.roles, vec!["admin".to_string()]);
    }

    #[test]
    fn test_builtin_whitelist_contains_auth_paths() {
        // 验证内置白名单包含关键认证路径
        assert!(BUILTIN_WHITELIST.contains(&"/api/auth/login"));
        assert!(BUILTIN_WHITELIST.contains(&"/api/auth/refresh"));
        assert!(BUILTIN_WHITELIST.contains(&"/health"));
        assert!(BUILTIN_WHITELIST.contains(&"/swagger"));
    }
}
