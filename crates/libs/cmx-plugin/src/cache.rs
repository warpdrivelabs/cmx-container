//! 缓存管理模块 - 插件数据缓存
//!
//! 集成 cmx-buffer 的 Redis 缓存功能，提供插件信息的缓存加速。

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 插件缓存键前缀
pub const CACHE_KEY_PREFIX: &str = "cmx:plugin:";

/// 插件缓存键
pub struct PluginCacheKey {
    plugin_id: String,
}

impl PluginCacheKey {
    /// 创建插件缓存键
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
        }
    }

    /// 获取插件信息键
    pub fn info(&self) -> String {
        format!("{}info:{}", CACHE_KEY_PREFIX, self.plugin_id)
    }

    /// 获取插件版本列表键
    pub fn versions(&self) -> String {
        format!("{}versions:{}", CACHE_KEY_PREFIX, self.plugin_id)
    }

    /// 获取插件状态键
    pub fn status(&self) -> String {
        format!("{}status:{}", CACHE_KEY_PREFIX, self.plugin_id)
    }

    /// 获取插件依赖键
    pub fn dependencies(&self) -> String {
        format!("{}deps:{}", CACHE_KEY_PREFIX, self.plugin_id)
    }

    /// 获取插件实例键
    pub fn instance(&self) -> String {
        format!("{}instance:{}", CACHE_KEY_PREFIX, self.plugin_id)
    }
}

/// 插件缓存值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCacheValue {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub install_path: String,
    pub activated: bool,
    pub updated_at: i64,
}

/// 插件缓存管理器
pub struct PluginCacheManager {
    cache: cmx_buffer::CacheManager,
    lock_manager: cmx_buffer::LockManager,
}

impl PluginCacheManager {
    /// 创建新的插件缓存管理器
    pub fn new(cache: cmx_buffer::CacheManager, lock_manager: cmx_buffer::LockManager) -> Self {
        Self { cache, lock_manager }
    }

    /// 缓存插件信息
    pub async fn cache_plugin(&self, value: &PluginCacheValue) -> Result<(), PluginCacheError> {
        let key = PluginCacheKey::new(&value.plugin_id);
        let cache_value = serde_json::to_string(value)
            .map_err(|e| PluginCacheError::Serialize(e.to_string()))?;

        self.cache.ops()
            .set_ex(&key.info(), &cache_value, Duration::from_secs(3600))
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 获取缓存的插件信息
    pub async fn get_cached_plugin(&self, plugin_id: &str) -> Result<Option<PluginCacheValue>, PluginCacheError> {
        let key = PluginCacheKey::new(plugin_id);
        
        let cache_value: Option<String> = self.cache.ops()
            .get(&key.info())
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        match cache_value {
            Some(v) => {
                let value: PluginCacheValue = serde_json::from_str(&v)
                    .map_err(|e| PluginCacheError::Deserialize(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// 清除插件缓存
    pub async fn invalidate_plugin(&self, plugin_id: &str) -> Result<(), PluginCacheError> {
        let key = PluginCacheKey::new(plugin_id);
        
        // 清除所有相关缓存
        self.cache.ops()
            .del(&key.info())
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        self.cache.ops()
            .del(&key.versions())
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        self.cache.ops()
            .del(&key.status())
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        self.cache.ops()
            .del(&key.dependencies())
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 缓存插件状态
    pub async fn cache_plugin_status(&self, plugin_id: &str, status: &str) -> Result<(), PluginCacheError> {
        let key = PluginCacheKey::new(plugin_id);
        
        self.cache.ops()
            .set_ex(&key.status(), status, Duration::from_secs(300))
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 获取缓存的插件状态
    pub async fn get_cached_status(&self, plugin_id: &str) -> Result<Option<String>, PluginCacheError> {
        let key = PluginCacheKey::new(plugin_id);
        
        self.cache.ops()
            .get(&key.status())
            .await
            .map_err(|e| PluginCacheError::Operation(e.to_string()))
    }

    /// 使用分布式锁获取插件操作锁
    pub async fn lock_plugin(&self, plugin_id: &str) -> Result<cmx_buffer::LockGuard, PluginCacheError> {
        let lock_key = format!("{}lock:{}", CACHE_KEY_PREFIX, plugin_id);
        
        let guard = self.lock_manager
            .lock(&lock_key)
            .await
            .map_err(|e| PluginCacheError::Lock(e.to_string()))?;

        Ok(guard)
    }

    /// 批量缓存插件信息
    pub async fn batch_cache_plugins(&self, values: &[PluginCacheValue]) -> Result<(), PluginCacheError> {
        for value in values {
            self.cache_plugin(value).await?;
        }
        Ok(())
    }

    /// 获取所有缓存的插件 ID（使用方提供键列表）
    pub async fn get_all_cached_plugin_ids(&self, _plugin_ids: Vec<String>) -> Result<Vec<String>, PluginCacheError> {
        // 注意：CacheOps 没有 keys 方法，由使用方提供插件 ID 列表
        Ok(_plugin_ids)
    }

    /// 清除所有插件缓存（使用方提供键列表）
    pub async fn clear_all(&self, keys: Vec<String>) -> Result<(), PluginCacheError> {
        // 注意：CacheOps 没有 keys 方法，由使用方提供键列表
        // 逐个删除键
        for key in keys {
            self.cache.ops()
                .del(&key)
                .await
                .map_err(|e| PluginCacheError::Operation(e.to_string()))?;
        }
        Ok(())
    }
}

/// 插件缓存错误
#[derive(Debug, thiserror::Error)]
pub enum PluginCacheError {
    #[error("缓存操作错误: {0}")]
    Operation(String),
    #[error("序列化错误: {0}")]
    Serialize(String),
    #[error("反序列化错误: {0}")]
    Deserialize(String),
    #[error("分布式锁错误: {0}")]
    Lock(String),
}

impl From<cmx_buffer::Error> for PluginCacheError {
    fn from(err: cmx_buffer::Error) -> Self {
        PluginCacheError::Operation(err.to_string())
    }
}
