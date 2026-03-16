pub mod ops;
pub mod ttl;

pub use ops::CacheOps;
pub use ttl::TtlOps;

use crate::client::RedisClient;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: 缓存操作模块入口
 */

/// 缓存管理器
pub struct CacheManager {
    client: RedisClient,
}

impl CacheManager {
    /// 创建新的缓存管理器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取缓存操作器
    pub fn ops(&self) -> CacheOps {
        CacheOps::new(self.client.clone())
    }

    /// 获取 TTL 操作器
    pub fn ttl(&self) -> TtlOps {
        TtlOps::new(self.client.clone())
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }
}
