//! 在线用户统计。
//!
//! 基于 Redis Set 实现在线用户集合，配合定时清理过期会话。

use cmx_buffer::CacheManager;

use crate::error::Result;

/// 在线用户追踪器。
///
/// 封装对 `auth:online:users` Redis Set 的读取与维护操作，
/// 与 `SessionManager` 协同维护在线用户统计。
pub struct OnlineTracker {
    /// Redis 缓存管理器。
    cache: CacheManager,
}

impl OnlineTracker {
    /// 创建新的在线追踪器。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `OnlineTracker` 实例。
    pub fn new(cache: CacheManager) -> Self {
        Self { cache }
    }

    /// 获取在线用户数。
    ///
    /// # Returns
    ///
    /// 返回 `auth:online:users` 集合的元素数量。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取失败时返回 `AuthInfraError`。
    pub async fn online_count(&self) -> Result<u64> {
        let count = self.cache.set().scard("auth:online:users").await?;
        Ok(count)
    }

    /// 检查用户是否在线。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 用户在在线集合中时返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取失败时返回 `AuthInfraError`。
    pub async fn is_online(&self, user_id: &str) -> Result<bool> {
        let exists = self
            .cache
            .set()
            .sismember("auth:online:users", user_id)
            .await?;
        Ok(exists)
    }

    /// 从在线集合移除用户。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 待移除的用户 ID。
    ///
    /// # Returns
    ///
    /// 成功移除时返回 `Ok(true)`，用户不在集合中时返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 删除失败时返回 `AuthInfraError`。
    pub async fn remove_online(&self, user_id: &str) -> Result<bool> {
        let removed = self
            .cache
            .set()
            .srem_one("auth:online:users", user_id)
            .await?;
        Ok(removed)
    }
}
