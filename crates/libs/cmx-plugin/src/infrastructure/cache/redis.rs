//! Redis缓存模块
//! 
//! 提供基于Redis的缓存实现，封装 cmx-buffer 的 CacheManager

use std::sync::Arc;
use std::time::Duration;

use cmx_buffer::{CacheManager, CacheOps};

/// Redis缓存管理器
/// 
/// 封装 cmx-buffer 的 CacheManager，提供插件系统专用的缓存操作。
pub struct RedisCache {
    /// 缓存管理器
    cache_manager: Arc<CacheManager>,
    /// 键前缀
    key_prefix: String,
}

impl RedisCache {
    /// 创建新的Redis缓存
    pub fn new(cache_manager: Arc<CacheManager>) -> Self {
        Self {
            cache_manager,
            key_prefix: "plugin:".to_string(),
        }
    }
    
    /// 创建带前缀的Redis缓存
    pub fn with_prefix(cache_manager: Arc<CacheManager>, key_prefix: &str) -> Self {
        Self {
            cache_manager,
            key_prefix: key_prefix.to_string(),
        }
    }
    
    /// 构建完整的键名
    fn build_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }
    
    /// 获取缓存操作器
    pub fn ops(&self) -> CacheOps {
        self.cache_manager.ops()
    }
    
    /// 获取字符串值
    /// 
    /// 从 Redis 获取缓存的字符串值。
    pub async fn get_string(&self, key: &str) -> Result<Option<String>, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ops()
            .get(&full_key)
            .await
            .map_err(|e| format!("获取缓存失败: {}", e))
    }
    
    /// 设置字符串值
    /// 
    /// 设置 Redis 缓存的字符串值。
    pub async fn set_string(&self, key: &str, value: &str, ttl: Option<Duration>) -> Result<(), String> {
        let full_key = self.build_key(key);
        if let Some(duration) = ttl {
            self.cache_manager.ops()
                .set_ex(&full_key, value, duration)
                .await
                .map_err(|e| format!("设置缓存失败: {}", e))
        } else {
            self.cache_manager.ops()
                .set(&full_key, value)
                .await
                .map_err(|e| format!("设置缓存失败: {}", e))
        }
    }
    
    /// 获取 JSON 值
    /// 
    /// 从 Redis 获取并反序列化 JSON 值。
    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ops()
            .get_deserialized(&full_key)
            .await
            .map_err(|e| format!("获取JSON缓存失败: {}", e))
    }
    
    /// 设置 JSON 值
    /// 
    /// 序列化并设置 Redis 缓存的 JSON 值。
    pub async fn set_json<T: serde::Serialize>(&self, key: &str, value: &T, ttl: Option<Duration>) -> Result<(), String> {
        let full_key = self.build_key(key);
        let json = serde_json::to_string(value)
            .map_err(|e| format!("序列化失败: {}", e))?;
        
        if let Some(duration) = ttl {
            self.cache_manager.ops()
                .set_ex(&full_key, &json, duration)
                .await
                .map_err(|e| format!("设置JSON缓存失败: {}", e))
        } else {
            self.cache_manager.ops()
                .set(&full_key, &json)
                .await
                .map_err(|e| format!("设置JSON缓存失败: {}", e))
        }
    }
    
    /// 删除缓存
    /// 
    /// 从 Redis 删除指定的缓存键。
    pub async fn delete(&self, key: &str) -> Result<bool, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ops()
            .del(&full_key)
            .await
            .map_err(|e| format!("删除缓存失败: {}", e))
    }
    
    /// 批量删除缓存
    /// 
    /// 从 Redis 删除多个缓存键。
    pub async fn delete_batch(&self, keys: &[&str]) -> Result<u64, String> {
        let full_keys: Vec<String> = keys.iter().map(|k| self.build_key(k)).collect();
        let keys_refs: Vec<&str> = full_keys.iter().map(|s| s.as_str()).collect();
        self.cache_manager.ops()
            .del_batch(&keys_refs)
            .await
            .map_err(|e| format!("批量删除缓存失败: {}", e))
    }
    
    /// 检查键是否存在
    /// 
    /// 检查 Redis 中是否存在指定的缓存键。
    pub async fn exists(&self, key: &str) -> Result<bool, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ops()
            .exists(&full_key)
            .await
            .map_err(|e| format!("检查键存在失败: {}", e))
    }
    
    /// 设置过期时间
    /// 
    /// 为 Redis 缓存键设置过期时间。
    pub async fn expire(&self, key: &str, ttl: Duration) -> Result<bool, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ttl()
            .expire(&full_key, ttl)
            .await
            .map_err(|e| format!("设置过期时间失败: {}", e))
    }
    
    /// 获取剩余过期时间
    /// 
    /// 获取 Redis 缓存键的剩余过期时间（秒）。
    pub async fn ttl(&self, key: &str) -> Result<i64, String> {
        let full_key = self.build_key(key);
        let ttl_opt = self.cache_manager.ttl()
            .ttl(&full_key)
            .await
            .map_err(|e| format!("获取TTL失败: {}", e))?;
        
        match ttl_opt {
            Some(duration) => Ok(duration.as_secs() as i64),
            None => Ok(-1),
        }
    }
    
    /// 自增
    /// 
    /// 将 Redis 缓存键的值增加指定数量。
    pub async fn incr(&self, key: &str, delta: i64) -> Result<i64, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ops()
            .incr(&full_key, delta)
            .await
            .map_err(|e| format!("自增失败: {}", e))
    }
    
    /// 自减
    /// 
    /// 将 Redis 缓存键的值减少指定数量。
    pub async fn decr(&self, key: &str, delta: i64) -> Result<i64, String> {
        let full_key = self.build_key(key);
        self.cache_manager.ops()
            .decr(&full_key, delta)
            .await
            .map_err(|e| format!("自减失败: {}", e))
    }
    
    /// 获取缓存管理器引用
    pub fn cache_manager(&self) -> &Arc<CacheManager> {
        &self.cache_manager
    }
    
    /// 获取键前缀
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }
}
