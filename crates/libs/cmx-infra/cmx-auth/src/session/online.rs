//! 在线用户统计
//!
//! 基于 Redis Set 实现在线用户集合，配合定时清理过期会话。

use cmx_buffer::CacheManager;

use crate::error::Result;

/// 在线用户追踪器
pub struct OnlineTracker {
    cache: CacheManager,
}

impl OnlineTracker {
    /// 创建新的在线追踪器
    pub fn new(cache: CacheManager) -> Self {
        Self { cache }
    }

    /// 获取在线用户数
    pub async fn online_count(&self) -> Result<u64> {
        let count = self.cache.set().scard("auth:online:users").await?;
        Ok(count)
    }

    /// 检查用户是否在线
    pub async fn is_online(&self, user_id: &str) -> Result<bool> {
        let exists = self
            .cache
            .set()
            .sismember("auth:online:users", user_id)
            .await?;
        Ok(exists)
    }

    /// 从在线集合移除用户
    pub async fn remove_online(&self, user_id: &str) -> Result<bool> {
        let removed = self
            .cache
            .set()
            .srem_one("auth:online:users", user_id)
            .await?;
        Ok(removed)
    }
}
