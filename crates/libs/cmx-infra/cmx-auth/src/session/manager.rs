//! 会话管理器。
//!
//! 提供会话创建/查询/销毁/互踢功能。

use std::time::Duration;

use chrono::Utc;
use cmx_buffer::CacheManager;
use moka::future::Cache;
use serde::{Deserialize, Serialize};

use crate::config::AuthConfig;
use crate::error::Result;

/// 用户会话（存储在 Redis Hash 中）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    /// 会话 ID。
    pub session_id: String,
    /// 用户 ID。
    pub user_id: String,
    /// 设备类型。
    pub device_type: String,
    /// 设备 ID。
    pub device_id: String,
    /// 登录时间。
    pub login_at: i64,
    /// 最后活跃时间。
    pub last_active_at: i64,
    /// 客户端 IP。
    pub ip: Option<String>,
    /// `User-Agent`。
    pub user_agent: Option<String>,
    /// 当前 Access Token JTI（用于 `revoke_all` 时加入黑名单）。
    pub access_jti: String,
    /// 当前 Refresh Token JTI（用于 Rotation 追踪）。
    pub refresh_jti: String,
    /// Access Token 过期时间（Unix 时间戳，2.2 修复：用于黑名单精确 TTL）。
    pub access_expires_at: i64,
}

/// 会话管理器。
///
/// 负责用户会话的创建/查询/销毁/互踢，配合 moka 本地缓存
/// 减少 Redis Hash 查询次数。
#[derive(Clone)]
pub struct SessionManager {
    /// Redis 缓存管理器（用于存储会话 Hash 与在线用户集合）。
    cache: CacheManager,

    /// 认证配置（提供会话相关参数）。
    config: AuthConfig,

    /// 本地缓存：key = `"session_active:{user_id}:{device}"`，value = 会话是否活跃。
    /// 在配置启用本地缓存时生效，否则为容量为 0 的空缓存。
    local_cache: Cache<String, bool>,
}

impl SessionManager {
    /// 创建新的会话管理器。
    ///
    /// 根据配置决定是否启用 moka 本地缓存及其 TTL 和容量。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `config` - 认证配置。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `SessionManager` 实例。
    pub fn new(cache: CacheManager, config: AuthConfig) -> Self {
        let local_cache = if config.cache.enable_local_cache {
            Cache::builder()
                .time_to_live(Duration::from_secs(config.cache.local_ttl_secs))
                .max_capacity(config.cache.local_cache_max_entries)
                .build()
        } else {
            Cache::builder().max_capacity(0).build()
        };
        Self {
            cache,
            config,
            local_cache,
        }
    }

    /// 创建会话。
    ///
    /// 将会话写入 Redis Hash，加入在线用户集合，并创建独立 TTL 的 `session_detail` key。
    /// 创建后使本地缓存失效并更新 Prometheus 指标。
    ///
    /// # Arguments
    ///
    /// * `session` - 待创建的会话信息。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入或序列化失败时返回 `AuthInfraError`。
    pub async fn create_session(&self, session: &UserSession) -> Result<()> {
        let key = format!("auth:{{{}}}:session", session.user_id);
        let json = serde_json::to_string(session)?;

        // HSET auth:{user_id}:session device_type session_json
        self.cache.hash().hset(&key, &session.device_type, &json).await?;

        // SADD auth:online:users user_id
        let is_new_user = self.cache.set().sadd_one("auth:online:users", &session.user_id).await?;

        // 创建 session_detail key（独立 TTL，自动过期）
        let detail_key = format!("auth:{}:session_detail", session.session_id);
        let detail_json = serde_json::to_string(session)?;
        let idle_timeout = Duration::from_secs(self.config.session.idle_timeout_secs);
        self.cache.ttl().set_with_ttl(&detail_key, &detail_json, idle_timeout).await?;

        // 创建会话后使本地缓存失效
        self.invalidate_local(&session.user_id, &session.device_type).await;

        // 2.1 修复：更新 Prometheus 指标
        crate::metrics::inc_active_sessions();
        if is_new_user {
            self.refresh_online_users_metric().await;
        }

        Ok(())
    }

    /// 获取会话。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `device_type` - 设备类型。
    ///
    /// # Returns
    ///
    /// 会话存在时返回 `Ok(Some(UserSession))`，否则返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取或反序列化失败时返回 `AuthInfraError`。
    pub async fn get_session(
        &self,
        user_id: &str,
        device_type: &str,
    ) -> Result<Option<UserSession>> {
        let key = format!("auth:{{{}}}:session", user_id);
        let json = self.cache.hash().hget(&key, device_type).await?;

        match json {
            Some(json_str) => {
                let session: UserSession = serde_json::from_str(&json_str)?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// 获取用户所有会话的设备类型。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 返回用户所有会话的设备类型列表。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取失败时返回 `AuthInfraError`。
    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<String>> {
        let key = format!("auth:{{{}}}:session", user_id);
        let devices = self.cache.hash().hkeys(&key).await?;
        Ok(devices)
    }

    /// 销毁会话。
    ///
    /// 删除会话 Hash field 和 `session_detail` key，
    /// 当用户无剩余会话时从在线用户集合移除，并更新 Prometheus 指标。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `device_type` - 设备类型。
    ///
    /// # Returns
    ///
    /// 成功删除时返回 `Ok(true)`，会话不存在时返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 删除失败时返回 `AuthInfraError`。
    pub async fn destroy_session(&self, user_id: &str, device_type: &str) -> Result<bool> {
        let key = format!("auth:{{{}}}:session", user_id);

        // 先获取 session 以便删除 session_detail
        if let Ok(Some(session)) = self.get_session(user_id, device_type).await {
            let detail_key = format!("auth:{}:session_detail", session.session_id);
            let _ = self.cache.ops().del(&detail_key).await;
        }

        let deleted = self.cache.hash().hdel(&key, &[device_type]).await?;

        // 检查用户是否还有其他会话
        let remaining = self.cache.hash().hlen(&key).await?;
        if remaining == 0 {
            // 无剩余会话，从在线用户集合移除
            self.cache
                .set()
                .srem_one("auth:online:users", user_id)
                .await?;
            // 2.1 修复：更新在线用户数指标
            self.refresh_online_users_metric().await;
        }

        // 销毁会话后使本地缓存失效
        self.invalidate_local(user_id, device_type).await;

        // 2.1 修复：更新活跃会话数指标
        if deleted > 0 {
            crate::metrics::dec_active_sessions();
        }

        Ok(deleted > 0)
    }

    /// 刷新会话心跳。
    ///
    /// 更新会话的 `last_active_at` 为当前时间，并刷新 `session_detail` 的 TTL。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `device_type` - 设备类型。
    ///
    /// # Returns
    ///
    /// 会话存在并刷新成功时返回 `Ok(true)`，会话不存在时返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读写或反序列化失败时返回 `AuthInfraError`。
    pub async fn heartbeat(&self, user_id: &str, device_type: &str) -> Result<bool> {
        let key = format!("auth:{{{}}}:session", user_id);
        let json = self.cache.hash().hget(&key, device_type).await?;

        match json {
            Some(json_str) => {
                let mut session: UserSession = serde_json::from_str(&json_str)?;
                session.last_active_at = Utc::now().timestamp();
                let updated = serde_json::to_string(&session)?;
                self.cache.hash().hset(&key, device_type, &updated).await?;

                // 刷新 session_detail TTL
                let detail_key = format!("auth:{}:session_detail", session.session_id);
                let idle_timeout = Duration::from_secs(self.config.session.idle_timeout_secs);
                let _ = self.cache.ttl().expire(&detail_key, idle_timeout).await;

                // 心跳更新后使本地缓存失效
                self.invalidate_local(user_id, device_type).await;

                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 检查会话是否活跃。
    ///
    /// 先查 moka 本地缓存，未命中时查 Redis 并写入本地缓存。
    /// 活跃判定：会话存在且距 `last_active_at` 未超过 `idle_timeout_secs`。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `device` - 设备类型。
    ///
    /// # Returns
    ///
    /// 会话活跃时返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取或反序列化失败时返回 `AuthInfraError`。
    pub async fn is_session_active(&self, user_id: &str, device: &str) -> Result<bool> {
        let cache_key = format!("session_active:{}:{}", user_id, device);

        // 先查本地缓存
        if let Some(active) = self.local_cache.get(&cache_key).await {
            return Ok(active);
        }

        // 查 Redis
        let key = format!("auth:{{{}}}:session", user_id);
        let json = self.cache.hash().hget(&key, device).await?;

        let active = match json {
            Some(json_str) => {
                let session: UserSession = serde_json::from_str(&json_str)?;
                let elapsed = Utc::now().timestamp() - session.last_active_at;
                elapsed < self.config.session.idle_timeout_secs as i64
            }
            None => false,
        };

        // 写入本地缓存
        self.local_cache.insert(cache_key, active).await;
        Ok(active)
    }

    /// 使指定用户/设备的本地缓存失效。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `device` - 设备类型。
    pub async fn invalidate_local(&self, user_id: &str, device: &str) {
        let cache_key = format!("session_active:{}:{}", user_id, device);
        self.local_cache.invalidate(&cache_key).await;
    }

    /// 使所有本地缓存失效。
    pub async fn invalidate_local_all(&self) {
        self.local_cache.invalidate_all();
    }

    /// 刷新在线用户数 Prometheus 指标。
    async fn refresh_online_users_metric(&self) {
        match self.cache.set().smembers("auth:online:users").await {
            Ok(members) => {
                crate::metrics::set_online_users(members.len() as i64);
            }
            Err(e) => {
                tracing::warn!(error = %e, "获取在线用户数失败，指标未更新");
            }
        }
    }
}
