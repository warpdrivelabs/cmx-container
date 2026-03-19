//! 多层缓存协调模块
//! 
//! 整合内存缓存和Redis缓存

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;

use super::memory::MemoryCache;
use super::redis::RedisCache;

/// 缓存值类型
#[derive(Debug, Clone)]
pub enum CacheValue {
    /// 字符串值
    String(String),
    /// JSON值
    Json(serde_json::Value),
    /// 二进制值
    Binary(Vec<u8>),
}

impl CacheValue {
    /// 转换为字符串
    pub fn as_string(&self) -> Option<&str> {
        match self {
            CacheValue::String(s) => Some(s),
            CacheValue::Json(v) => Some(v.as_str().unwrap_or("")),
            _ => None,
        }
    }
    
    /// 转换为 JSON 值
    pub fn as_json(&self) -> Option<&serde_json::Value> {
        match self {
            CacheValue::Json(v) => Some(v),
            _ => None,
        }
    }
    
    /// 转换为二进制
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            CacheValue::Binary(b) => Some(b),
            _ => None,
        }
    }
}

/// 缓存策略
#[derive(Debug, Clone)]
pub struct CacheStrategy {
    /// L1 缓存 TTL（秒）
    pub l1_ttl_seconds: u64,
    /// L2 缓存 TTL（秒）
    pub l2_ttl_seconds: u64,
    /// 是否启用 L1 缓存
    pub enable_l1: bool,
    /// 是否启用 L2 缓存
    pub enable_l2: bool,
}

impl Default for CacheStrategy {
    fn default() -> Self {
        Self {
            l1_ttl_seconds: 300,
            l2_ttl_seconds: 3600,
            enable_l1: true,
            enable_l2: true,
        }
    }
}

/// 多层缓存协调器
/// 
/// 提供两级缓存架构：
/// - L1: 内存缓存，快速访问
/// - L2: Redis 缓存，分布式共享
pub struct LayeredCacheManager {
    /// 内存缓存（L1）
    memory_cache: Arc<MemoryCache<CacheValue>>,
    /// Redis缓存（L2）
    redis_cache: Option<Arc<RedisCache>>,
    /// 缓存策略
    strategy: CacheStrategy,
}

impl LayeredCacheManager {
    /// 创建新的缓存管理器
    pub fn new(strategy: CacheStrategy) -> Self {
        Self {
            memory_cache: Arc::new(MemoryCache::new()),
            redis_cache: None,
            strategy,
        }
    }
    
    /// 设置 Redis 缓存
    pub fn with_redis(mut self, redis_cache: Arc<RedisCache>) -> Self {
        self.redis_cache = Some(redis_cache);
        self.strategy.enable_l2 = true;
        self
    }
    
    /// 从 CacheManager 创建
    pub fn with_cache_manager(cache_manager: Arc<cmx_buffer::CacheManager>) -> Self {
        let redis_cache = Arc::new(RedisCache::new(cache_manager));
        Self {
            memory_cache: Arc::new(MemoryCache::new()),
            redis_cache: Some(redis_cache),
            strategy: CacheStrategy::default(),
        }
    }
    
    /// 获取缓存
    /// 
    /// 先查 L1 内存缓存，未命中则查 L2 Redis 缓存。
    /// 如果 L2 命中，会回填到 L1。
    pub async fn get(&self, key: &str) -> Option<CacheValue> {
        // 先查 L1 内存缓存
        if self.strategy.enable_l1 {
            if let Some(value) = self.memory_cache.get(key).await {
                return Some(value);
            }
        }
        
        // 再查 L2 Redis 缓存
        if self.strategy.enable_l2 {
            if let Some(ref redis) = self.redis_cache {
                // 尝试从 Redis 获取字符串值
                if let Ok(Some(json_str)) = redis.get_string(key).await {
                    // 尝试解析为 JSON
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        let value = CacheValue::Json(json_value);
                        // 回填到 L1
                        if self.strategy.enable_l1 {
                            let l1_ttl = Duration::from_secs(self.strategy.l1_ttl_seconds);
                            self.memory_cache.set_with_ttl(key, value.clone(), Some(l1_ttl)).await;
                        }
                        return Some(value);
                    }
                    // 不是 JSON，作为字符串处理
                    let value = CacheValue::String(json_str);
                    if self.strategy.enable_l1 {
                        let l1_ttl = Duration::from_secs(self.strategy.l1_ttl_seconds);
                        self.memory_cache.set_with_ttl(key, value.clone(), Some(l1_ttl)).await;
                    }
                    return Some(value);
                }
            }
        }
        
        None
    }
    
    /// 设置缓存
    /// 
    /// 同时设置 L1 和 L2 缓存。
    pub async fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>) {
        let l1_ttl = ttl.or_else(|| Some(Duration::from_secs(self.strategy.l1_ttl_seconds)));
        let l2_ttl = ttl.or_else(|| Some(Duration::from_secs(self.strategy.l2_ttl_seconds)));
        
        // 设置 L1 内存缓存
        if self.strategy.enable_l1 {
            self.memory_cache.set_with_ttl(key, value.clone(), l1_ttl).await;
        }
        
        // 设置 L2 Redis 缓存
        if self.strategy.enable_l2 {
            if let Some(ref redis) = self.redis_cache {
                match &value {
                    CacheValue::String(s) => {
                        let _ = redis.set_string(key, s, l2_ttl).await;
                    }
                    CacheValue::Json(v) => {
                        let _ = redis.set_json(key, v, l2_ttl).await;
                    }
                    CacheValue::Binary(b) => {
                        // 二进制数据转为 base64 存储
                        let encoded = base64::engine::general_purpose::STANDARD.encode(b);
                        let _ = redis.set_string(key, &encoded, l2_ttl).await;
                    }
                }
            }
        }
    }
    
    /// 删除缓存
    /// 
    /// 同时删除 L1 和 L2 缓存。
    pub async fn delete(&self, key: &str) {
        // 删除 L1 内存缓存
        self.memory_cache.remove(key).await;
        
        // 删除 L2 Redis 缓存
        if self.strategy.enable_l2 {
            if let Some(ref redis) = self.redis_cache {
                let _ = redis.delete(key).await;
            }
        }
    }
    
    /// 清空所有缓存
    /// 
    /// 清空 L1 内存缓存和 L2 Redis 缓存。
    pub async fn clear(&self) {
        // 清空 L1 内存缓存
        self.memory_cache.clear().await;
        
        // 注意：不清空 L2 Redis 缓存，因为可能包含其他应用的缓存
        // 如果需要清空 Redis 缓存，应该使用专门的清理方法
    }
    
    /// 获取或设置缓存
    /// 
    /// 如果缓存不存在，使用提供的函数获取值并设置缓存。
    pub async fn get_or_set<F, Fut>(&self, key: &str, f: F, ttl: Option<Duration>) -> Option<CacheValue>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Option<CacheValue>>,
    {
        // 先尝试获取
        if let Some(value) = self.get(key).await {
            return Some(value);
        }
        
        // 调用函数获取值
        if let Some(value) = f().await {
            self.set(key, value.clone(), ttl).await;
            return Some(value);
        }
        
        None
    }
    
    /// 检查缓存是否存在
    /// 
    /// 检查 L1 或 L2 缓存中是否存在指定的键。
    pub async fn exists(&self, key: &str) -> bool {
        // 检查 L1
        if self.strategy.enable_l1 && self.memory_cache.get(key).await.is_some() {
            return true;
        }
        
        // 检查 L2
        if self.strategy.enable_l2 {
            if let Some(ref redis) = self.redis_cache {
                if let Ok(true) = redis.exists(key).await {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// 获取内存缓存引用
    pub fn memory_cache(&self) -> &Arc<MemoryCache<CacheValue>> {
        &self.memory_cache
    }
    
    /// 获取 Redis 缓存引用
    pub fn redis_cache(&self) -> Option<&Arc<RedisCache>> {
        self.redis_cache.as_ref()
    }
    
    /// 获取缓存策略
    pub fn strategy(&self) -> &CacheStrategy {
        &self.strategy
    }
    
    /// 设置缓存策略
    pub fn set_strategy(&mut self, strategy: CacheStrategy) {
        self.strategy = strategy;
    }
}

impl Default for LayeredCacheManager {
    fn default() -> Self {
        Self::new(CacheStrategy::default())
    }
}
