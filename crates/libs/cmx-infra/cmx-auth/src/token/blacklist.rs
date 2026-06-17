//! Token 黑名单
//!
//! 基于 Redis SET + moka 本地缓存实现，支持 Pub/Sub 主动失效。

use std::time::Duration;

use cmx_buffer::CacheManager;
use moka::future::Cache;

use crate::config::CacheConfig;
use crate::error::Result;

/// Token 黑名单管理器。
///
/// 基于 Redis SET + moka 本地缓存实现 Access Token 撤销，
/// 支持通过 Pub/Sub 主动失效本地缓存。
pub struct Blacklist {
    /// Redis 缓存管理器。
    cache: CacheManager,

    /// 本地缓存：key = `jti`，value = `true` 表示在黑名单，`false` 表示不在。
    /// 使用 `time_to_live` 而非 `time_to_idle`，避免高频访问 key 内存膨胀。
    local_cache: Cache<String, bool>,
}

impl Blacklist {
    /// 创建新的黑名单管理器
    pub fn new(cache: CacheManager, config: &CacheConfig) -> Self {
        let local_cache = if config.enable_local_cache {
            Cache::builder()
                // P0-2.1 修复 + 5.5 修复：使用 time_to_live 而非 time_to_idle
                // time_to_live 从插入时算起 TTL，time_to_idle 从最后访问算起，
                // 频繁访问的 key 用 time_to_idle 永远不过期，导致内存膨胀
                .time_to_live(Duration::from_secs(config.local_ttl_secs))
                .max_capacity(config.local_cache_max_entries)
                .build()
        } else {
            Cache::builder()
                .max_capacity(0)
                .build()
        };

        Self { cache, local_cache }
    }

    /// 将 Token 加入黑名单
    pub async fn add(&self, jti: &str, remaining_ttl: Duration) -> Result<()> {
        // Redis: SET auth:{jti}:blacklist "1" EX remaining_ttl
        self.cache
            .ttl()
            .set_with_ttl(&format!("auth:{}:blacklist", jti), "1", remaining_ttl)
            .await?;

        // 本地缓存主动失效
        self.local_cache.invalidate(jti).await;

        Ok(())
    }

    /// 检查 Token 是否在黑名单中
    pub async fn is_blacklisted(&self, jti: &str) -> Result<bool> {
        // P0-2.1 修复：先查本地缓存，根据缓存值返回
        // 旧 Bug：contains_key 对 false 值也返回 true，导致未撤销 Token 被误判
        if let Some(in_blacklist) = self.local_cache.get(&jti.to_string()).await {
            return Ok(in_blacklist);
        }

        // 查 Redis
        let exists = self
            .cache
            .ops()
            .exists(&format!("auth:{}:blacklist", jti))
            .await?;

        // 写入本地缓存（无论 true/false 都缓存，moka time_to_live 保证过期）
        self.local_cache
            .insert(jti.to_string(), exists)
            .await;

        Ok(exists)
    }

    /// 本地缓存失效（由 Pub/Sub 回调触发）
    pub async fn invalidate_local(&self, jti: &str) {
        self.local_cache.invalidate(jti).await;
    }

    /// 批量本地缓存失效（revoke_all_tokens 时按 user_id 模式清除）
    pub async fn invalidate_local_all(&self) {
        self.local_cache.invalidate_all();
    }
}
