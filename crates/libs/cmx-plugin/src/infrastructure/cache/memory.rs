//! 内存缓存模块
//! 
//! 提供基于内存的缓存实现

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    /// 缓存值
    value: T,
    /// 过期时间
    expires_at: Option<Instant>,
}

impl<T> CacheEntry<T> {
    fn new(value: T, ttl: Option<Duration>) -> Self {
        Self {
            value,
            expires_at: ttl.map(|t| Instant::now() + t),
        }
    }
    
    fn is_expired(&self) -> bool {
        self.expires_at.map(|t| Instant::now() > t).unwrap_or(false)
    }
}

/// 内存缓存
#[derive(Debug)]
pub struct MemoryCache<T> {
    /// 缓存数据
    data: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,
    /// 默认TTL
    default_ttl: Option<Duration>,
}

impl<T: Clone> MemoryCache<T> {
    /// 创建新的内存缓存
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: None,
        }
    }
    
    /// 设置默认TTL
    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }
    
    /// 获取缓存
    pub async fn get(&self, key: &str) -> Option<T> {
        let data = self.data.read().await;
        data.get(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(entry.value.clone())
            }
        })
    }
    
    /// 设置缓存
    pub async fn set(&self, key: &str, value: T) {
        self.set_with_ttl(key, value, self.default_ttl).await;
    }
    
    /// 设置缓存（带TTL）
    pub async fn set_with_ttl(&self, key: &str, value: T, ttl: Option<Duration>) {
        let mut data = self.data.write().await;
        data.insert(key.to_string(), CacheEntry::new(value, ttl));
    }
    
    /// 删除缓存
    pub async fn remove(&self, key: &str) -> Option<T> {
        let mut data = self.data.write().await;
        data.remove(key).map(|entry| entry.value)
    }
    
    /// 清空缓存
    pub async fn clear(&self) {
        let mut data = self.data.write().await;
        data.clear();
    }
    
    /// 清理过期缓存
    pub async fn cleanup_expired(&self) {
        let mut data = self.data.write().await;
        data.retain(|_, entry| !entry.is_expired());
    }
    
    /// 获取缓存数量
    pub async fn len(&self) -> usize {
        let data = self.data.read().await;
        data.len()
    }
    
    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

impl<T: Clone> Default for MemoryCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for MemoryCache<T> {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            default_ttl: self.default_ttl,
        }
    }
}
