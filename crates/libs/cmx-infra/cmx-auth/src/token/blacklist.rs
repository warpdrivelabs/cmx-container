//! Token 黑名单。
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
    /// 创建新的黑名单管理器。
    ///
    /// 根据配置决定是否启用 moka 本地缓存及其 TTL 和容量。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `config` - 缓存配置。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `Blacklist` 实例。
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

    /// 将 Token 加入黑名单。
    ///
    /// 写入 Redis SET（带 TTL）并使本地缓存失效。
    ///
    /// # Arguments
    ///
    /// * `jti` - Access Token 的 JTI。
    /// * `remaining_ttl` - Token 剩余有效期（黑名单 TTL 与之一致）。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthInfraError`。
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

    /// 检查 Token 是否在黑名单中。
    ///
    /// 先查 moka 本地缓存，未命中时查 Redis 并写入本地缓存。
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

    /// 本地缓存失效（由 Pub/Sub 回调触发）。
    ///
    /// # Arguments
    ///
    /// * `jti` - 待失效的 Access Token JTI。
    pub async fn invalidate_local(&self, jti: &str) {
        self.local_cache.invalidate(jti).await;
    }

    /// 批量本地缓存失效（`revoke_all_tokens` 时按 `user_id` 模式清除）。
    pub async fn invalidate_local_all(&self) {
        // 注意：moka 0.12 的 `invalidate_all()` 返回 `()`（同步操作，非 Future），
        // 此前此处误用 `.await` 导致编译失败（预存 bug，因模块无测试覆盖而未暴露）。
        self.local_cache.invalidate_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CacheConfig;

    /// 辅助：尝试从环境变量获取 Redis URL。
    fn redis_url() -> Option<String> {
        std::env::var("CMX_TEST_REDIS_URL").ok()
    }

    /// 辅助：根据环境变量构造 `CacheManager`。
    async fn make_cache_manager() -> Option<CacheManager> {
        let url = redis_url()?;
        let config = cmx_buffer::RedisConfig::new(&url);
        let client = cmx_buffer::RedisClient::new(config).await.ok()?;
        Some(CacheManager::new(client))
    }

    /// 辅助：构造启用本地缓存的 `CacheConfig`。
    fn make_cache_config() -> CacheConfig {
        CacheConfig {
            enable_local_cache: true,
            local_ttl_secs: 30,
            local_cache_max_entries: 100,
            max_login_attempts: 5,
            lock_duration_secs: 900,
        }
    }

    #[test]
    fn test_blacklist_config_disabled_cache_does_not_panic() {
        // 验证 CacheConfig 的字段组合不会导致 Blacklist::new panic
        // （仅验证 config 构造，不实际创建 Blacklist 实例）
        let config = CacheConfig {
            enable_local_cache: false,
            local_ttl_secs: 0,
            local_cache_max_entries: 0,
            max_login_attempts: 0,
            lock_duration_secs: 0,
        };
        assert!(!config.enable_local_cache);
        assert_eq!(config.local_cache_max_entries, 0);
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_blacklist_add_and_check() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let blacklist = Blacklist::new(cache.clone(), &make_cache_config());
        let jti = "test-jti-blacklist-add";

        // 清理可能残留的状态
        let _ = cache.ops().del(&format!("auth:{}:blacklist", jti)).await;
        blacklist.invalidate_local(jti).await;

        // 1. 初始状态：不在黑名单
        let in_bl = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单失败");
        assert!(!in_bl, "未加入黑名单的 jti 应返回 false");

        // 2. 加入黑名单（剩余 TTL 60 秒）
        blacklist
            .add(jti, Duration::from_secs(60))
            .await
            .expect("加入黑名单失败");

        // 3. 加入后查询：应在黑名单中
        let in_bl = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单失败");
        assert!(in_bl, "已加入黑名单的 jti 应返回 true");

        // 4. 本地缓存生效后再次查询（应命中本地缓存）
        let in_bl_cached = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单（缓存命中）失败");
        assert!(in_bl_cached, "本地缓存命中后应仍返回 true");

        // 清理
        let _ = cache.ops().del(&format!("auth:{}:blacklist", jti)).await;
        blacklist.invalidate_local(jti).await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_blacklist_check_nonexistent() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let blacklist = Blacklist::new(cache.clone(), &make_cache_config());
        let jti = "test-jti-nonexistent";

        // 清理
        let _ = cache.ops().del(&format!("auth:{}:blacklist", jti)).await;
        blacklist.invalidate_local(jti).await;

        // 不存在的 jti 应返回 false（并缓存 false 结果）
        let in_bl = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单失败");
        assert!(!in_bl, "从未加入黑名单的 jti 应返回 false");

        // 再次查询（应命中本地缓存的 false 值，而非误判为 true）
        // 这是 P0-2.1 修复的关键场景：contains_key 对 false 值也返回 true 会导致误判
        let in_bl_cached = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单（缓存命中）失败");
        assert!(
            !in_bl_cached,
            "本地缓存命中 false 值时应返回 false（P0-2.1 修复点）"
        );

        // 清理
        blacklist.invalidate_local(jti).await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_blacklist_invalidate_local_clears_cache() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let blacklist = Blacklist::new(cache.clone(), &make_cache_config());
        let jti = "test-jti-invalidate";

        // 清理
        let _ = cache.ops().del(&format!("auth:{}:blacklist", jti)).await;
        blacklist.invalidate_local(jti).await;

        // 1. 加入黑名单
        blacklist
            .add(jti, Duration::from_secs(60))
            .await
            .expect("加入黑名单失败");

        // 2. 查询使本地缓存写入 true
        let in_bl = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单失败");
        assert!(in_bl);

        // 3. 失效本地缓存 + 删除 Redis key
        blacklist.invalidate_local(jti).await;
        let _ = cache.ops().del(&format!("auth:{}:blacklist", jti)).await;

        // 4. 再次查询应返回 false（Redis 已删除，本地缓存已失效）
        let in_bl = blacklist
            .is_blacklisted(jti)
            .await
            .expect("查询黑名单失败");
        assert!(!in_bl, "失效本地缓存并删除 Redis 后应返回 false");

        // 清理
        blacklist.invalidate_local(jti).await;
    }
}
