//! Token 生命周期管理器
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
    /// 创建新的 Token 管理器
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

    /// 存储 Refresh Token
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

    /// 撤销指定 Refresh Token
    pub async fn revoke_refresh_token(&self, user_id: &str, jti: &str) -> Result<()> {
        let key = format!("auth:{{{}}}:refresh:{}", user_id, jti);
        self.cache.ops().del(&key).await?;

        let index_key = format!("auth:{{{}}}:refresh_index", user_id);
        self.cache.set().srem_one(&index_key, jti).await?;

        Ok(())
    }

    /// 撤销用户所有 Refresh Token
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

    /// 将 Access Token 加入黑名单
    pub async fn blacklist_access_token(
        &self,
        jti: &str,
        remaining_ttl: Duration,
    ) -> Result<()> {
        self.blacklist.add(jti, remaining_ttl).await
    }

    /// 检查 Access Token 是否在黑名单中
    pub async fn is_blacklisted(&self, jti: &str) -> Result<bool> {
        self.blacklist.is_blacklisted(jti).await
    }

    /// 本地缓存失效（Pub/Sub 回调）
    pub async fn invalidate_local_cache(&self, jti: &str) {
        self.blacklist.invalidate_local(jti).await;
    }

    /// 获取黑名单引用
    pub fn blacklist(&self) -> &Blacklist {
        &self.blacklist
    }

    /// 获取 Rotation 引用
    pub fn rotation(&self) -> &RefreshRotation {
        &self.rotation
    }

    /// 批量本地缓存失效（revoke_all_tokens 时使用）
    pub async fn invalidate_local_cache_all(&self) {
        self.blacklist.invalidate_local_all().await;
    }
}
