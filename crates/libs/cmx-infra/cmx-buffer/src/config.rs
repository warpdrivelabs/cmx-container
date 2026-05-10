//! cmx-buffer 配置结构体定义
//!
//! 提供 Redis 连接配置、分布式锁配置和缓存操作配置的定义。

use cmx_utils::Config;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Redis 运行模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RedisMode {
    /// 单机模式
    #[default]
    Standalone,
    /// 集群模式
    Cluster,
}

/// Redis 连接配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis 连接地址 (redis://host:port)，单机模式使用
    pub url: String,
    /// Redis 运行模式（单机或集群）
    #[serde(default)]
    pub mode: RedisMode,
    /// 集群节点地址列表，集群模式使用
    #[serde(default)]
    pub cluster_urls: Vec<String>,
    /// Pub/Sub 心跳间隔（秒），0 表示禁用心跳
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    /// 连接超时时间（秒）
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    /// 操作超时时间（秒）
    #[serde(default = "default_operation_timeout")]
    pub operation_timeout: u64,
    /// 默认键前缀
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
    /// 启动时自动订阅的频道列表
    #[serde(default)]
    pub subscribe_channels: Vec<String>,
    /// 启动时自动订阅的模式列表
    #[serde(default)]
    pub subscribe_patterns: Vec<String>,
}

fn default_heartbeat_interval() -> u64 {
    30
}

fn default_connection_timeout() -> u64 {
    5
}

fn default_operation_timeout() -> u64 {
    3
}

fn default_key_prefix() -> String {
    "cmx:".to_string()
}

impl RedisConfig {
    /// 创建新的 Redis 配置（单机模式）
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            mode: RedisMode::Standalone,
            cluster_urls: Vec::new(),
            heartbeat_interval: default_heartbeat_interval(),
            connection_timeout: default_connection_timeout(),
            operation_timeout: default_operation_timeout(),
            key_prefix: default_key_prefix(),
            subscribe_channels: Vec::new(),
            subscribe_patterns: Vec::new(),
        }
    }

    /// 创建集群模式的 Redis 配置
    pub fn new_cluster(urls: Vec<String>) -> Self {
        let url = urls.first().cloned().unwrap_or_default();
        Self {
            url,
            mode: RedisMode::Cluster,
            cluster_urls: urls,
            heartbeat_interval: default_heartbeat_interval(),
            connection_timeout: default_connection_timeout(),
            operation_timeout: default_operation_timeout(),
            key_prefix: default_key_prefix(),
            subscribe_channels: Vec::new(),
            subscribe_patterns: Vec::new(),
        }
    }

    /// 从通用 Config 读取 Redis 配置
    pub fn from_config(config: &Config) -> Self {
        let url = config.get_string("redis.url").unwrap();

        let mode_str = config
            .get_string("redis.mode")
            .unwrap_or_else(|_| "standalone".to_string());
        let mode = match mode_str.to_lowercase().as_str() {
            "cluster" => RedisMode::Cluster,
            _ => RedisMode::Standalone,
        };

        let cluster_urls = config
            .get_string("redis.cluster_urls")
            .map(|s| s.split(',').map(|u| u.trim().to_string()).collect())
            .unwrap_or_default();

        let heartbeat_interval = config
            .get_int("redis.heartbeat_interval")
            .unwrap_or(default_heartbeat_interval() as i64);

        let connection_timeout = config
            .get_int("redis.connection_timeout")
            .unwrap_or(default_connection_timeout() as i64);
        let operation_timeout = config
            .get_int("redis.operation_timeout")
            .unwrap_or(default_operation_timeout() as i64);

        let subscribe_channels = config
            .get_string("redis.subscribe_channels")
            .map(|s| s.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect())
            .unwrap_or_default();

        let subscribe_patterns = config
            .get_string("redis.subscribe_patterns")
            .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
            .unwrap_or_default();

        RedisConfig {
            url,
            mode,
            cluster_urls,
            heartbeat_interval: heartbeat_interval as u64,
            connection_timeout: connection_timeout as u64,
            operation_timeout: operation_timeout as u64,
            key_prefix: default_key_prefix(),
            subscribe_channels,
            subscribe_patterns,
        }
    }

    /// 设置运行模式
    pub fn with_mode(mut self, mode: RedisMode) -> Self {
        self.mode = mode;
        self
    }

    /// 设置集群节点地址
    pub fn with_cluster_urls(mut self, urls: Vec<String>) -> Self {
        self.cluster_urls = urls;
        self
    }

    /// 设置 Pub/Sub 心跳间隔（秒）
    pub fn with_heartbeat_interval(mut self, secs: u64) -> Self {
        self.heartbeat_interval = secs;
        self
    }

    /// 设置连接超时
    pub fn with_connection_timeout(mut self, timeout: u64) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// 设置操作超时
    pub fn with_operation_timeout(mut self, timeout: u64) -> Self {
        self.operation_timeout = timeout;
        self
    }

    /// 设置键前缀
    pub fn with_key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    /// 设置启动时自动订阅的频道列表
    pub fn with_subscribe_channels(mut self, channels: Vec<String>) -> Self {
        self.subscribe_channels = channels;
        self
    }

    /// 设置启动时自动订阅的模式列表
    pub fn with_subscribe_patterns(mut self, patterns: Vec<String>) -> Self {
        self.subscribe_patterns = patterns;
        self
    }

    /// 获取连接超时 Duration
    pub fn connection_timeout_duration(&self) -> Duration {
        Duration::from_secs(self.connection_timeout)
    }

    /// 获取操作超时 Duration
    pub fn operation_timeout_duration(&self) -> Duration {
        Duration::from_secs(self.operation_timeout)
    }

    /// 获取心跳间隔 Duration
    pub fn heartbeat_interval_duration(&self) -> Duration {
        Duration::from_secs(self.heartbeat_interval)
    }

    /// 判断是否为集群模式
    pub fn is_cluster(&self) -> bool {
        self.mode == RedisMode::Cluster
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self::new("redis://127.0.0.1:6379")
    }
}

/// 分布式锁配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockConfig {
    /// 锁过期时间（秒）
    #[serde(default = "default_lock_expire")]
    pub expire_seconds: u64,
    /// 获取锁重试次数
    #[serde(default = "default_retry_times")]
    pub retry_times: u32,
    /// 重试间隔（毫秒）
    #[serde(default = "default_retry_interval")]
    pub retry_interval_ms: u64,
    /// 锁续期阈值（百分比），当剩余时间低于此百分比时自动续期
    #[serde(default = "default_renew_threshold")]
    pub renew_threshold: f64,
}

fn default_lock_expire() -> u64 {
    30
}

fn default_retry_times() -> u32 {
    3
}

fn default_retry_interval() -> u64 {
    200
}

fn default_renew_threshold() -> f64 {
    0.3
}

impl LockConfig {
    /// 创建新的锁配置
    pub fn new() -> Self {
        Self {
            expire_seconds: default_lock_expire(),
            retry_times: default_retry_times(),
            retry_interval_ms: default_retry_interval(),
            renew_threshold: default_renew_threshold(),
        }
    }

    /// 设置锁过期时间
    pub fn with_expire(mut self, seconds: u64) -> Self {
        self.expire_seconds = seconds;
        self
    }

    /// 设置重试次数
    pub fn with_retry_times(mut self, times: u32) -> Self {
        self.retry_times = times;
        self
    }

    /// 设置重试间隔
    pub fn with_retry_interval(mut self, ms: u64) -> Self {
        self.retry_interval_ms = ms;
        self
    }

    /// 获取锁过期时间 Duration
    pub fn expire_duration(&self) -> Duration {
        Duration::from_secs(self.expire_seconds)
    }

    /// 获取重试间隔 Duration
    pub fn retry_interval_duration(&self) -> Duration {
        Duration::from_millis(self.retry_interval_ms)
    }
}

impl Default for LockConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// 缓存操作配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// 默认过期时间（秒），0 表示永不过期
    #[serde(default)]
    pub default_ttl: u64,
    /// 是否启用键前缀
    #[serde(default = "default_enable_prefix")]
    pub enable_prefix: bool,
}

fn default_enable_prefix() -> bool {
    true
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: 0,
            enable_prefix: true,
        }
    }
}

impl CacheConfig {
    /// 创建新的缓存配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置默认 TTL
    pub fn with_default_ttl(mut self, ttl: u64) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// 获取默认 TTL Duration
    pub fn default_ttl_duration(&self) -> Option<Duration> {
        if self.default_ttl > 0 {
            Some(Duration::from_secs(self.default_ttl))
        } else {
            None
        }
    }
}
