//! OAuth2 实体与存储。
//!
//! `OAuth2Client` 和 `AuthorizationCode` 的 Redis 存储管理。

use std::time::Duration;

use cmx_buffer::CacheManager;
use cmx_traits::auth::OAuth2ClientData;
use serde::{Deserialize, Serialize};

use crate::config::AuthConfig;
use crate::error::Result;

/// OAuth2 授权码消费 Lua 脚本。
///
/// 原子操作：检查授权码是否已使用 → 获取数据 → 标记已使用 → 删除原始码。
///
/// # KEYS
///
/// - `KEYS[1]` = `auth:oauth2:authcode:{code}`
/// - `KEYS[2]` = `auth:oauth2:authcode:{code}:used`
///
/// # ARGV
///
/// - `ARGV[1]` = `auth_code_ttl_secs`
///
/// # 返回值
///
/// - `0`: 授权码已使用
/// - `nil`: 授权码不存在
/// - `string`: 授权码 JSON 数据（成功）
pub const CONSUME_AUTH_CODE_LUA_SCRIPT: &str = r#"
-- 1. 检查是否已使用
if redis.call('EXISTS', KEYS[2]) == 1 then
    return 0
end

-- 2. 获取授权码数据
local data = redis.call('GET', KEYS[1])
if not data then
    return nil
end

-- 3. 标记为已使用
redis.call('SET', KEYS[2], '1', 'EX', ARGV[1])

-- 4. 删除原始授权码
redis.call('DEL', KEYS[1])

return data
"#;

/// 第三方 OAuth2 Provider State 消费 Lua 脚本。
///
/// 原子操作：读取并删除 state，防止并发重放。
///
/// # KEYS
///
/// - `KEYS[1]` = `auth:oauth2:provider:state:{state}`
///
/// # 返回值
///
/// - `nil`: state 不存在
/// - `string`: provider 名称（成功）
pub const CONSUME_PROVIDER_STATE_LUA_SCRIPT: &str = r#"
local value = redis.call('GET', KEYS[1])
if not value then
    return nil
end
redis.call('DEL', KEYS[1])
return value
"#;

/// 第三方 OAuth2 回调授权码消费 Lua 脚本。
///
/// 原子操作：读取并删除回调授权码，防止并发重放。
///
/// # KEYS
///
/// - `KEYS[1]` = `auth:oauth2:provider:callback:{code}`
///
/// # 返回值
///
/// - `nil`: 授权码不存在
/// - `string`: TokenPair JSON（成功）
pub const CONSUME_CALLBACK_CODE_LUA_SCRIPT: &str = r#"
local value = redis.call('GET', KEYS[1])
if not value then
    return nil
end
redis.call('DEL', KEYS[1])
return value
"#;

/// OAuth2 客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Client {
    /// 客户端标识。
    pub client_id: String,
    /// 客户端名称。
    pub client_name: String,
    /// 客户端密钥（`confidential` 类型使用，哈希存储）。
    pub client_secret: Option<String>,
    /// 回调地址列表（JSON 数组）。
    pub redirect_uris: Vec<String>,
    /// 允许的授权类型。
    pub grant_types: Vec<String>,
    /// 客户端类型：`public` / `confidential`。
    pub client_type: String,
    /// 是否强制 PKCE。
    pub pkce_required: bool,
    /// 允许的 scope。
    pub allowed_scopes: Vec<String>,
    /// 状态：`0` 禁用，`1` 启用。
    pub status: i64,
}

impl From<OAuth2ClientData> for OAuth2Client {
    fn from(data: OAuth2ClientData) -> Self {
        Self {
            client_id: data.client_id,
            client_name: data.client_name,
            client_secret: data.client_secret,
            redirect_uris: data.redirect_uris,
            grant_types: data.grant_types,
            client_type: data.client_type,
            pkce_required: data.pkce_required,
            allowed_scopes: data.allowed_scopes,
            status: data.status,
        }
    }
}

/// OAuth2 授权码。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationCode {
    /// 授权码。
    pub code: String,
    /// 绑定的客户端 ID。
    pub client_id: String,
    /// 授权用户 ID。
    pub user_id: Option<String>,
    /// 绑定的回调地址。
    pub redirect_uri: String,
    /// PKCE `code_challenge`。
    pub code_challenge: Option<String>,
    /// PKCE `code_challenge_method`。
    pub code_challenge_method: Option<String>,
    /// 请求的 scope。
    pub scope: Vec<String>,
    /// CSRF state。
    pub state: String,
    /// 是否已授权。
    pub approved: bool,
    /// 创建时间戳。
    pub created_at: i64,
}

/// OAuth2 存储（基于 Redis）。
#[derive(Clone)]
pub struct OAuth2Store {
    cache: CacheManager,
    config: AuthConfig,
}

impl OAuth2Store {
    /// 创建新的 OAuth2 存储。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `config` - 认证配置。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `OAuth2Store` 实例。
    pub fn new(cache: CacheManager, config: AuthConfig) -> Self {
        Self { cache, config }
    }

    /// 存储 CSRF state（`authorize` 阶段）。
    ///
    /// # Arguments
    ///
    /// * `state` - CSRF state 字符串。
    /// * `client_id` - 关联的客户端 ID。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
    pub async fn store_csrf_state(&self, state: &str, client_id: &str) -> Result<()> {
        let key = format!("auth:oauth2:csrf:{}", state);
        let ttl = self.auth_code_ttl();
        self.cache.ttl().set_with_ttl(&key, client_id, ttl).await?;
        Ok(())
    }

    /// 验证并消费 CSRF state。
    ///
    /// # Arguments
    ///
    /// * `state` - 待验证的 CSRF state。
    ///
    /// # Returns
    ///
    /// state 存在时返回 `Some(client_id)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当 Redis 操作失败时返回 `AuthInfraError`。
    pub async fn verify_csrf_state(&self, state: &str) -> Result<Option<String>> {
        let key = format!("auth:oauth2:csrf:{}", state);
        let value = self.cache.ops().get(&key).await?;
        if value.is_some() {
            // 一次性使用，验证后删除
            self.cache.ops().del(&key).await?;
        }
        Ok(value)
    }

    /// 获取 CSRF state 关联的 `client_id`（不删除）。
    ///
    /// # Arguments
    ///
    /// * `state` - CSRF state 字符串。
    ///
    /// # Returns
    ///
    /// state 存在时返回 `Some(client_id)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取失败时返回 `AuthInfraError`。
    pub async fn get_csrf_state(&self, state: &str) -> Result<Option<String>> {
        let key = format!("auth:oauth2:csrf:{}", state);
        Ok(self.cache.ops().get(&key).await?)
    }

    /// 消费（删除）CSRF state。
    ///
    /// # Arguments
    ///
    /// * `state` - 待删除的 CSRF state。
    ///
    /// # Errors
    ///
    /// 当 Redis 删除失败时返回 `AuthInfraError`。
    pub async fn consume_csrf_state(&self, state: &str) -> Result<()> {
        let key = format!("auth:oauth2:csrf:{}", state);
        self.cache.ops().del(&key).await?;
        Ok(())
    }

    /// 存储授权码（`login` 阶段）。
    ///
    /// # Arguments
    ///
    /// * `auth_code` - 待存储的授权码信息。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
    pub async fn store_authorization_code(&self, auth_code: &AuthorizationCode) -> Result<()> {
        let key = format!("auth:oauth2:authcode:{}", auth_code.code);
        let json = serde_json::to_string(auth_code)?;
        let ttl = self.auth_code_ttl();
        self.cache.ttl().set_with_ttl(&key, &json, ttl).await?;
        Ok(())
    }

    /// 获取并验证授权码（token 交换阶段）。
    ///
    /// N11：以 code 为主 key，换 token 时用 code 反查。
    /// 验证后标记为已使用（防重放）。
    /// 使用 Lua 脚本原子执行：检查已使用 → 获取数据 → 标记已使用 → 删除原始码。
    ///
    /// # Arguments
    ///
    /// * `code` - 授权码字符串。
    ///
    /// # Returns
    ///
    /// 授权码有效时返回 `Some(AuthorizationCode)`，已使用或不存在时返回 `None`。
    ///
    /// # Errors
    ///
    /// 当 Lua 脚本执行或反序列化失败时返回 `AuthInfraError`。
    pub async fn consume_authorization_code(&self, code: &str) -> Result<Option<AuthorizationCode>> {
        let key = format!("auth:oauth2:authcode:{}", code);
        let used_key = format!("auth:oauth2:authcode:{}:used", code);
        let ttl_secs = self.auth_code_ttl().as_secs();

        let keys = &[key.as_str(), used_key.as_str()];
        let args = &[ttl_secs.to_string()];
        let args_str: &[&str] = &[args[0].as_str()];

        let result = self
            .cache
            .script()
            .eval_with_fallback(CONSUME_AUTH_CODE_LUA_SCRIPT, keys, args_str)
            .await?;

        match result {
            // 已使用（EXISTS used_key == 1）
            redis::Value::Int(0) => Ok(None),
            // 授权码数据
            redis::Value::BulkString(bytes) => {
                let json_str = String::from_utf8(bytes)
                    .map_err(|e| crate::error::AuthInfraError::Auth(
                        cmx_traits::auth::AuthError::Internal(format!("UTF-8 解码失败: {}", e))
                    ))?;
                let auth_code: AuthorizationCode = serde_json::from_str(&json_str)?;
                Ok(Some(auth_code))
            }
            // 授权码不存在
            _ => Ok(None),
        }
    }

    /// 获取授权码有效期
    fn auth_code_ttl(&self) -> Duration {
        self.config
            .oauth2
            .as_ref()
            .map(|c| Duration::from_secs(c.auth_code_ttl_secs))
            .unwrap_or(Duration::from_secs(600))
    }

    /// 存储第三方 OAuth2 Provider state。
    ///
    /// # Arguments
    ///
    /// * `state` - CSRF state 字符串。
    /// * `provider` - 关联的 Provider 名称。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
    pub async fn store_provider_state(&self, state: &str, provider: &str) -> Result<()> {
        let key = format!("auth:oauth2:provider:state:{}", state);
        let ttl = self.provider_state_ttl();
        self.cache.ttl().set_with_ttl(&key, provider, ttl).await?;
        Ok(())
    }

    /// 原子消费第三方 OAuth2 Provider state。
    ///
    /// 使用 Lua 脚本原子读取并删除 state，防止并发重放。
    ///
    /// # Arguments
    ///
    /// * `state` - 待消费的 CSRF state。
    ///
    /// # Returns
    ///
    /// state 存在时返回 `Some(provider)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当 Lua 脚本执行或 UTF-8 解码失败时返回 `AuthInfraError`。
    pub async fn consume_provider_state(&self, state: &str) -> Result<Option<String>> {
        let key = format!("auth:oauth2:provider:state:{}", state);
        let keys = &[key.as_str()];
        let result = self.cache.script()
            .eval_with_fallback(CONSUME_PROVIDER_STATE_LUA_SCRIPT, keys, &[])
            .await?;
        match result {
            redis::Value::BulkString(bytes) => {
                let provider = String::from_utf8(bytes)
                    .map_err(|e| crate::error::AuthInfraError::Auth(
                        cmx_traits::auth::AuthError::Internal(format!("UTF-8 解码失败: {}", e))
                    ))?;
                Ok(Some(provider))
            }
            _ => Ok(None),
        }
    }

    /// 存储第三方 OAuth2 回调授权码。
    ///
    /// # Arguments
    ///
    /// * `code` - 一次性回调授权码。
    /// * `token_pair_json` - TokenPair 的 JSON 字符串。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
    pub async fn store_callback_code(&self, code: &str, token_pair_json: &str) -> Result<()> {
        let key = format!("auth:oauth2:provider:callback:{}", code);
        let ttl = self.callback_code_ttl();
        self.cache.ttl().set_with_ttl(&key, token_pair_json, ttl).await?;
        Ok(())
    }

    /// 原子消费第三方 OAuth2 回调授权码。
    ///
    /// 使用 Lua 脚本原子读取并删除回调授权码，防止并发重放。
    ///
    /// # Arguments
    ///
    /// * `code` - 待消费的回调授权码。
    ///
    /// # Returns
    ///
    /// 授权码存在时返回 `Some(json)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当 Lua 脚本执行或 UTF-8 解码失败时返回 `AuthInfraError`。
    pub async fn consume_callback_code(&self, code: &str) -> Result<Option<String>> {
        let key = format!("auth:oauth2:provider:callback:{}", code);
        let keys = &[key.as_str()];
        let result = self.cache.script()
            .eval_with_fallback(CONSUME_CALLBACK_CODE_LUA_SCRIPT, keys, &[])
            .await?;
        match result {
            redis::Value::BulkString(bytes) => {
                let json = String::from_utf8(bytes)
                    .map_err(|e| crate::error::AuthInfraError::Auth(
                        cmx_traits::auth::AuthError::Internal(format!("UTF-8 解码失败: {}", e))
                    ))?;
                Ok(Some(json))
            }
            _ => Ok(None),
        }
    }

    /// 获取 Provider state 有效期
    fn provider_state_ttl(&self) -> Duration {
        self.config
            .oauth2
            .as_ref()
            .map(|c| Duration::from_secs(c.state_ttl_secs))
            .unwrap_or(Duration::from_secs(600))
    }

    /// 获取回调授权码有效期
    fn callback_code_ttl(&self) -> Duration {
        self.config
            .oauth2
            .as_ref()
            .map(|c| Duration::from_secs(c.callback_code_ttl_secs))
            .unwrap_or(Duration::from_secs(30))
    }
}
