pub mod manager;

pub use manager::{LockGuard, LockManager};

use crate::client::RedisClient;
use crate::config::LockConfig;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: 分布式锁模块入口
 */

/// 创建分布式锁管理器
pub fn create_lock_manager(client: RedisClient) -> LockManager {
    LockManager::new_with_default_config(client)
}

/// 创建分布式锁管理器（带自定义配置）
pub fn create_lock_manager_with_config(client: RedisClient, config: LockConfig) -> LockManager {
    LockManager::new(client, config)
}
