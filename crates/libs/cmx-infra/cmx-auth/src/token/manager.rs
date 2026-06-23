//! Token 生命周期管理器。
//!
//! 负责 Refresh Token 的存储、轮换、撤销，以及 Access Token 黑名单管理。

use std::time::Duration;

use cmx_buffer::CacheManager;

use crate::config::AuthConfig;
use crate::error::Result;
use crate::token::blacklist::Blacklist;
use crate::token::rotation::RefreshRotation;

/// Token 管理器。
///
/// 负责 Refresh Token 的存储/轮换/撤销，以及 Access Token 黑名单管理。
pub struct TokenManager {
    /// Redis 缓存管理器。
    cache: CacheManager,

    /// Access Token 黑名单子模块。
    blacklist: Blacklist,

    /// Refresh Token 轮换子模块（Lua 原子操作）。
    rotation: RefreshRotation,

    /// 认证配置（提供 TTL 等参数）。
    config: AuthConfig,
}

impl TokenManager {
    /// 创建新的 Token 管理器。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `config` - 认证配置。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `TokenManager` 实例。
    pub fn new(cache: CacheManager, config: AuthConfig) -> Self {
        let blacklist = Blacklist::new(cache.clone(), &config.cache);
        let rotation = RefreshRotation::new(cache.clone());
        Self {
            cache,
            blacklist,
            rotation,
            config,
        }
    }

    /// 存储 Refresh Token。
    ///
    /// 将 Refresh Token 写入 Redis（带 TTL），并加入用户的 `refresh_index` 集合便于批量撤销。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `jti` - Refresh Token 的 JTI。
    /// * `device` - 设备类型。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
    pub async fn store_refresh_token(
        &self,
        user_id: &str,
        jti: &str,
        device: &str,
    ) -> Result<()> {
        let ttl = Duration::from_secs(self.config.token.refresh_ttl_secs);

        // SET auth:{user_id}:refresh:{jti} device EX ttl
        let key = format!("auth:{{{}}}:refresh:{}", user_id, jti);
        self.cache.ttl().set_with_ttl(&key, device, ttl).await?;

        // SADD auth:{user_id}:refresh_index jti
        let index_key = format!("auth:{{{}}}:refresh_index", user_id);
        self.cache.set().sadd_one(&index_key, jti).await?;

        // 设置 refresh_index 的 TTL，与 refresh token 保持一致
        self.cache.ttl().expire(&index_key, ttl).await?;

        Ok(())
    }

    /// 撤销指定 Refresh Token。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `jti` - 待撤销的 Refresh Token JTI。
    ///
    /// # Errors
    ///
    /// 当 Redis 删除失败时返回 `AuthInfraError`。
    pub async fn revoke_refresh_token(&self, user_id: &str, jti: &str) -> Result<()> {
        let key = format!("auth:{{{}}}:refresh:{}", user_id, jti);
        self.cache.ops().del(&key).await?;

        let index_key = format!("auth:{{{}}}:refresh_index", user_id);
        self.cache.set().srem_one(&index_key, jti).await?;

        Ok(())
    }

    /// 撤销用户所有 Refresh Token。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Errors
    ///
    /// 当 Redis 删除失败时返回 `AuthInfraError`。
    pub async fn revoke_all_refresh_tokens(&self, user_id: &str) -> Result<()> {
        let index_key = format!("auth:{{{}}}:refresh_index", user_id);
        let jtis = self.cache.set().smembers(&index_key).await?;

        for jti in &jtis {
            let key = format!("auth:{{{}}}:refresh:{}", user_id, jti);
            self.cache.ops().del(&key).await?;
        }
        self.cache.ops().del(&index_key).await?;

        Ok(())
    }

    /// 将 Access Token 加入黑名单。
    ///
    /// # Arguments
    ///
    /// * `jti` - Access Token 的 JTI。
    /// * `remaining_ttl` - Token 剩余有效期（黑名单 TTL 与之一致）。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
    pub async fn blacklist_access_token(
        &self,
        jti: &str,
        remaining_ttl: Duration,
    ) -> Result<()> {
        self.blacklist.add(jti, remaining_ttl).await
    }

    /// 检查 Access Token 是否在黑名单中。
    ///
    /// # Arguments
    ///
    /// * `jti` - Access Token 的 JTI。
    ///
    /// # Returns
    ///
    /// 在黑名单中时返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取失败时返回 `AuthInfraError`。
    pub async fn is_blacklisted(&self, jti: &str) -> Result<bool> {
        self.blacklist.is_blacklisted(jti).await
    }

    /// 本地缓存失效（Pub/Sub 回调）。
    ///
    /// # Arguments
    ///
    /// * `jti` - 待失效的 Access Token JTI。
    pub async fn invalidate_local_cache(&self, jti: &str) {
        self.blacklist.invalidate_local(jti).await;
    }

    /// 获取黑名单引用。
    ///
    /// # Returns
    ///
    /// 返回内部 `Blacklist` 子模块的引用。
    pub fn blacklist(&self) -> &Blacklist {
        &self.blacklist
    }

    /// 获取 Rotation 引用。
    ///
    /// # Returns
    ///
    /// 返回内部 `RefreshRotation` 子模块的引用。
    pub fn rotation(&self) -> &RefreshRotation {
        &self.rotation
    }

    /// 批量本地缓存失效（`revoke_all_tokens` 时使用）。
    pub async fn invalidate_local_cache_all(&self) {
        self.blacklist.invalidate_local_all().await;
    }
}
