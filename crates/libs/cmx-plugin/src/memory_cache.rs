//! 内存缓存层模块 - 本地内存缓存
//!
//! 提供本地内存缓存功能，减少对 Redis 的访问压力。
//! 支持 TTL 过期、LRU 淘汰策略。

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
    /// 最后访问时间
    last_accessed: Instant,
    /// 访问次数
    access_count: u64,
}

impl<T: Clone> CacheEntry<T> {
    /// 创建新的缓存条目
    fn new(value: T, ttl: Option<Duration>) -> Self {
        Self {
            value,
            expires_at: ttl.map(|d| Instant::now() + d),
            last_accessed: Instant::now(),
            access_count: 0,
        }
    }

    /// 检查是否过期
    fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Instant::now() > expires_at
        } else {
            false
        }
    }

    /// 访问缓存条目
    fn access(&mut self) -> &T {
        self.last_accessed = Instant::now();
        self.access_count += 1;
        &self.value
    }
}

/// 内存缓存配置
#[derive(Debug, Clone)]
pub struct MemoryCacheConfig {
    /// 最大条目数
    pub max_entries: usize,
    /// 默认 TTL（秒）
    pub default_ttl_seconds: u64,
    /// 清理间隔（秒）
    pub cleanup_interval_seconds: u64,
    /// 是否启用 LRU 淘汰
    pub enable_lru: bool,
}

impl Default for MemoryCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            default_ttl_seconds: 300,
            cleanup_interval_seconds: 60,
            enable_lru: true,
        }
    }
}

/// 内存缓存统计
#[derive(Debug, Clone, Default)]
pub struct MemoryCacheStats {
    /// 总条目数
    pub total_entries: usize,
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 淘汰次数
    pub evictions: u64,
    /// 过期次数
    pub expirations: u64,
}

impl MemoryCacheStats {
    /// 获取命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// 内存缓存 - 线程安全的本地缓存
pub struct MemoryCache<T: Clone + Send + Sync + 'static> {
    config: MemoryCacheConfig,
    cache: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,
    stats: Arc<RwLock<MemoryCacheStats>>,
}

impl<T: Clone + Send + Sync + 'static> MemoryCache<T> {
    /// 创建新的内存缓存
    pub fn new(config: MemoryCacheConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MemoryCacheStats::default())),
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config() -> Self {
        Self::new(MemoryCacheConfig::default())
    }

    /// 获取缓存值
    pub async fn get(&self, key: &str) -> Option<T> {
        let mut cache = self.cache.write().await;
        
        if let Some(entry) = cache.get_mut(key) {
            if entry.is_expired() {
                cache.remove(key);
                let mut stats = self.stats.write().await;
                stats.misses += 1;
                stats.expirations += 1;
                return None;
            }
            
            let value = entry.access().clone();
            let mut stats = self.stats.write().await;
            stats.hits += 1;
            Some(value)
        } else {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
            None
        }
    }

    /// 设置缓存值
    pub async fn set(&self, key: impl Into<String>, value: T) {
        self.set_with_ttl(key, value, None).await;
    }

    /// 设置缓存值（带 TTL）
    pub async fn set_with_ttl(&self, key: impl Into<String>, value: T, ttl: Option<Duration>) {
        let key = key.into();
        let ttl = ttl.or_else(|| Some(Duration::from_secs(self.config.default_ttl_seconds)));
        
        let mut cache = self.cache.write().await;
        
        // 检查是否需要淘汰
        if cache.len() >= self.config.max_entries && !cache.contains_key(&key) {
            self.evict_lru(&mut cache).await;
        }
        
        let entry = CacheEntry::new(value, ttl);
        cache.insert(key, entry);
    }

    /// 删除缓存值
    pub async fn remove(&self, key: &str) -> Option<T> {
        let mut cache = self.cache.write().await;
        cache.remove(key).map(|e| e.value)
    }

    /// 检查键是否存在
    pub async fn contains(&self, key: &str) -> bool {
        let cache = self.cache.read().await;
        cache.get(key).map_or(false, |e| !e.is_expired())
    }

    /// 清空缓存
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// 获取缓存大小
    pub async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// 清理过期条目
    pub async fn cleanup_expired(&self) -> usize {
        let mut cache = self.cache.write().await;
        let before = cache.len();
        
        cache.retain(|_, entry| !entry.is_expired());
        
        let removed = before - cache.len();
        if removed > 0 {
            let mut stats = self.stats.write().await;
            stats.expirations += removed as u64;
        }
        
        removed
    }

    /// 获取统计信息
    pub async fn stats(&self) -> MemoryCacheStats {
        let cache = self.cache.read().await;
        let mut stats = self.stats.read().await.clone();
        stats.total_entries = cache.len();
        stats
    }

    /// 重置统计信息
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.hits = 0;
        stats.misses = 0;
        stats.evictions = 0;
        stats.expirations = 0;
    }

    /// LRU 淘汰
    async fn evict_lru(&self, cache: &mut HashMap<String, CacheEntry<T>>) {
        if !self.config.enable_lru {
            // 简单淘汰：删除第一个
            if let Some(first_key) = cache.keys().next().cloned() {
                cache.remove(&first_key);
            }
        } else {
            // LRU 淘汰：找到最久未访问的条目
            let mut oldest_key: Option<String> = None;
            let mut oldest_time = Instant::now();
            
            for (key, entry) in cache.iter() {
                if entry.last_accessed < oldest_time {
                    oldest_time = entry.last_accessed;
                    oldest_key = Some(key.clone());
                }
            }
            
            if let Some(key) = oldest_key {
                cache.remove(&key);
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }
        }
    }

    /// 获取所有键
    pub async fn keys(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        cache.keys().filter(|k| {
            cache.get(*k).map_or(false, |e| !e.is_expired())
        }).cloned().collect()
    }

    /// 批量获取
    pub async fn get_many(&self, keys: &[&str]) -> HashMap<String, T> {
        let mut result = HashMap::new();
        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;
        
        for key in keys {
            if let Some(entry) = cache.get_mut(*key) {
                if !entry.is_expired() {
                    result.insert(key.to_string(), entry.access().clone());
                    stats.hits += 1;
                } else {
                    cache.remove(*key);
                    stats.misses += 1;
                    stats.expirations += 1;
                }
            } else {
                stats.misses += 1;
            }
        }
        
        result
    }

    /// 批量设置
    pub async fn set_many(&self, items: HashMap<String, T>) {
        for (key, value) in items {
            self.set(key, value).await;
        }
    }

    /// 更新 TTL
    pub async fn update_ttl(&self, key: &str, ttl: Duration) -> bool {
        let mut cache = self.cache.write().await;
        
        if let Some(entry) = cache.get_mut(key) {
            if !entry.is_expired() {
                entry.expires_at = Some(Instant::now() + ttl);
                return true;
            }
        }
        
        false
    }
}

/// 插件专用内存缓存类型别名
pub type PluginMemoryCache = MemoryCache<MemoryCacheValue>;

/// 内存缓存值类型
#[derive(Debug, Clone)]
pub enum MemoryCacheValue {
    /// 字符串值
    String(String),
    /// JSON 值
    Json(serde_json::Value),
    /// 字节数组
    Bytes(Vec<u8>),
    /// 插件信息缓存
    PluginInfo(crate::types::PluginInfo),
}

/// 插件内存缓存管理器
pub struct PluginMemoryCacheManager {
    /// 插件信息缓存
    plugin_info: PluginMemoryCache,
    /// 插件列表缓存
    plugin_list: MemoryCache<Vec<crate::types::PluginInfo>>,
    /// 依赖关系缓存
    dependencies: MemoryCache<Vec<String>>,
    /// 版本信息缓存
    versions: MemoryCache<Vec<String>>,
}

impl PluginMemoryCacheManager {
    /// 创建新的插件内存缓存管理器
    pub fn new(config: MemoryCacheConfig) -> Self {
        Self {
            plugin_info: MemoryCache::new(config.clone()),
            plugin_list: MemoryCache::new(config.clone()),
            dependencies: MemoryCache::new(config.clone()),
            versions: MemoryCache::new(config),
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config() -> Self {
        Self::new(MemoryCacheConfig::default())
    }

    /// 获取插件信息缓存
    pub fn plugin_info(&self) -> &PluginMemoryCache {
        &self.plugin_info
    }

    /// 获取插件列表缓存
    pub fn plugin_list(&self) -> &MemoryCache<Vec<crate::types::PluginInfo>> {
        &self.plugin_list
    }

    /// 获取依赖关系缓存
    pub fn dependencies(&self) -> &MemoryCache<Vec<String>> {
        &self.dependencies
    }

    /// 获取版本信息缓存
    pub fn versions(&self) -> &MemoryCache<Vec<String>> {
        &self.versions
    }

    /// 清空所有缓存
    pub async fn clear_all(&self) {
        self.plugin_info.clear().await;
        self.plugin_list.clear().await;
        self.dependencies.clear().await;
        self.versions.clear().await;
    }

    /// 清理所有过期条目
    pub async fn cleanup_all(&self) -> usize {
        let mut total = 0;
        total += self.plugin_info.cleanup_expired().await;
        total += self.plugin_list.cleanup_expired().await;
        total += self.dependencies.cleanup_expired().await;
        total += self.versions.cleanup_expired().await;
        total
    }

    /// 获取总统计信息
    pub async fn total_stats(&self) -> MemoryCacheStats {
        let mut total = MemoryCacheStats::default();
        
        let info_stats = self.plugin_info.stats().await;
        let list_stats = self.plugin_list.stats().await;
        let dep_stats = self.dependencies.stats().await;
        let ver_stats = self.versions.stats().await;
        
        total.total_entries = info_stats.total_entries + list_stats.total_entries 
            + dep_stats.total_entries + ver_stats.total_entries;
        total.hits = info_stats.hits + list_stats.hits + dep_stats.hits + ver_stats.hits;
        total.misses = info_stats.misses + list_stats.misses + dep_stats.misses + ver_stats.misses;
        total.evictions = info_stats.evictions + list_stats.evictions + dep_stats.evictions + ver_stats.evictions;
        total.expirations = info_stats.expirations + list_stats.expirations + dep_stats.expirations + ver_stats.expirations;
        
        total
    }
}

/// 缓存键生成器
pub struct CacheKeyBuilder;

impl CacheKeyBuilder {
    /// 构建插件信息缓存键
    pub fn plugin_info(plugin_id: &str) -> String {
        format!("plugin:info:{}", plugin_id)
    }

    /// 构建插件列表缓存键
    pub fn plugin_list(filter_hash: &str) -> String {
        format!("plugin:list:{}", filter_hash)
    }

    /// 构建依赖关系缓存键
    pub fn dependencies(plugin_id: &str) -> String {
        format!("plugin:deps:{}", plugin_id)
    }

    /// 构建版本列表缓存键
    pub fn versions(plugin_id: &str) -> String {
        format!("plugin:versions:{}", plugin_id)
    }

    /// 构建节点信息缓存键
    pub fn node_info(node_id: &str) -> String {
        format!("node:info:{}", node_id)
    }

    /// 构建服务信息缓存键
    pub fn service_info(service_id: &str) -> String {
        format!("service:info:{}", service_id)
    }
}
