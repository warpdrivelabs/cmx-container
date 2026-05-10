use crate::client::{RedisClient, SubscriberConnection};
use crate::error::{Error, Result};
use crate::logging::OperationTimer;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, trace, warn};

/// 发布/订阅操作器
pub struct PubSubOps {
    client: RedisClient,
}

impl PubSubOps {
    /// 创建新的发布/订阅操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /// 向频道发布消息
    pub async fn publish(&self, channel: &str, message: &str) -> Result<u64> {
        let timer = OperationTimer::new("PUBLISH", channel);

        let mut conn = self.client.get_connection();
        let subscribers: u64 = redis::cmd("PUBLISH")
            .arg(channel)
            .arg(message)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(subscribers)
    }

    /// 向频道发布JSON可序列化的消息
    pub async fn publish_json<T: Serialize>(&self, channel: &str, message: &T) -> Result<u64> {
        let json = serde_json::to_string(message)?;
        self.publish(channel, &json).await
    }

    /// 获取匹配模式的活动频道列表
    pub async fn pubsub_channels(&self, pattern: Option<&str>) -> Result<Vec<String>> {
        let mut conn = self.client.get_connection();

        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("CHANNELS");
        if let Some(p) = pattern {
            cmd.arg(p);
        }

        let channels: Vec<String> = cmd
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(channels)
    }

    /// 获取频道的订阅者数量
    pub async fn pubsub_numsub(&self, channels: &[&str]) -> Result<Vec<(String, u64)>> {
        let mut conn = self.client.get_connection();

        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("NUMSUB");
        for ch in channels {
            cmd.arg(ch);
        }

        let result: Vec<(String, u64)> = cmd
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(result)
    }

    /// 获取模式订阅的数量
    pub async fn pubsub_numpat(&self) -> Result<u64> {
        let mut conn = self.client.get_connection();

        let count: u64 = redis::cmd("PUBSUB")
            .arg("NUMPAT")
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }
}

// ============================================================================
// 全局订阅者 - ChannelHandler + GlobalSubscriber + GlobalSubscriberManager
// ============================================================================

/// 从 redis::Value 提取字符串
fn value_to_string(value: &redis::Value) -> Option<String> {
    match value {
        redis::Value::BulkString(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
        redis::Value::SimpleString(s) => Some(s.clone()),
        _ => None,
    }
}

/// 频道消息处理器
#[async_trait]
pub trait ChannelHandler: Send + Sync {
    /// 处理收到的消息
    async fn handle(&self, channel: &str, payload: &str);
}

/// 基于闭包的频道处理器
pub struct FnChannelHandler {
    f: Box<dyn Fn(&str, &str) + Send + Sync>,
}

impl FnChannelHandler {
    /// 创建闭包处理器
    pub fn new(f: impl Fn(&str, &str) + Send + Sync + 'static) -> Self {
        Self { f: Box::new(f) }
    }
}

#[async_trait]
impl ChannelHandler for FnChannelHandler {
    async fn handle(&self, channel: &str, payload: &str) {
        (self.f)(channel, payload);
    }
}

/// 全局 Pub/Sub 订阅者
///
/// 从 RedisClient 获取配置，创建专用的订阅连接。
/// 支持单机和集群两种模式，使用 RESP3 + push_sender 统一消息接收。
/// 其他模块可以注册指定频道的处理器，消息自动路由到对应的处理器。
/// 内置自动重连和自动重新订阅机制（依赖 redis-rs SubscriptionTracker）。
pub struct GlobalSubscriber {
    /// 订阅专用连接
    conn: Arc<tokio::sync::Mutex<SubscriberConnection>>,
    /// 频道处理器注册表
    handlers: DashMap<String, Arc<dyn ChannelHandler>>,
    /// 模式处理器注册表
    pattern_handlers: DashMap<String, Arc<dyn ChannelHandler>>,
    /// 心跳间隔
    heartbeat_interval: Duration,
    /// 后台任务 JoinHandle
    tasks: tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl GlobalSubscriber {
    /// 从 RedisClient 创建全局订阅者
    ///
    /// 创建订阅连接后，自动订阅 RedisConfig 中配置的频道和模式。
    /// 这些是"预订阅"，确保连接在处理器注册前就已订阅，
    /// 后续调用 register_channel 时会跳过已订阅的频道。
    pub async fn new(client: &RedisClient) -> Result<Self> {
        let config = client.config().clone();
        let heartbeat_interval = config.heartbeat_interval_duration();

        let (conn, rx) = client.create_subscriber_connection().await?;

        let subscriber = Self {
            conn: Arc::new(tokio::sync::Mutex::new(conn)),
            handlers: DashMap::new(),
            pattern_handlers: DashMap::new(),
            heartbeat_interval,
            tasks: tokio::sync::Mutex::new(Vec::new()),
        };

        // 自动订阅配置中的频道
        if !config.subscribe_channels.is_empty() {
            let mut conn = subscriber.conn.lock().await;
            for channel in &config.subscribe_channels {
                conn.subscribe(channel).await?;
                info!(channel = %channel, "已自动订阅配置频道");
            }
        }

        // 自动订阅配置中的模式
        if !config.subscribe_patterns.is_empty() {
            let mut conn = subscriber.conn.lock().await;
            for pattern in &config.subscribe_patterns {
                conn.psubscribe(pattern).await?;
                info!(pattern = %pattern, "已自动订阅配置模式");
            }
        }

        subscriber.start_dispatch_task(rx);
        subscriber.start_heartbeat_task().await;

        Ok(subscriber)
    }

    /// 注册频道处理器
    pub async fn register_channel(
        &self,
        channel: &str,
        handler: Arc<dyn ChannelHandler>,
    ) -> Result<()> {
        if !self.handlers.contains_key(channel) {
            let mut conn = self.conn.lock().await;
            conn.subscribe(channel).await?;
            info!(channel = %channel, "已订阅频道");
        }
        self.handlers.insert(channel.to_string(), handler);
        Ok(())
    }

    /// 注册频道处理器（闭包版本）
    pub async fn register_channel_fn<F>(&self, channel: &str, f: F) -> Result<()>
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        self.register_channel(channel, Arc::new(FnChannelHandler::new(f)))
            .await
    }

    /// 注册模式处理器
    pub async fn register_pattern(
        &self,
        pattern: &str,
        handler: Arc<dyn ChannelHandler>,
    ) -> Result<()> {
        if !self.pattern_handlers.contains_key(pattern) {
            let mut conn = self.conn.lock().await;
            conn.psubscribe(pattern).await?;
            info!(pattern = %pattern, "已订阅模式");
        }
        self.pattern_handlers
            .insert(pattern.to_string(), handler);
        Ok(())
    }

    /// 注销频道处理器
    pub async fn unregister_channel(&self, channel: &str) -> Result<()> {
        self.handlers.remove(channel);
        if !self.handlers.contains_key(channel) {
            let mut conn = self.conn.lock().await;
            conn.unsubscribe(channel).await?;
            info!(channel = %channel, "已取消订阅频道");
        }
        Ok(())
    }

    /// 启动心跳任务
    async fn start_heartbeat_task(&self) {
        let conn = self.conn.clone();
        let interval = self.heartbeat_interval;

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let mut conn = conn.lock().await;
                let result: std::result::Result<redis::Value, redis::RedisError> = match &mut *conn {
                    SubscriberConnection::Standalone(c) => {
                        redis::cmd("PING").query_async(c).await
                    }
                    SubscriberConnection::Cluster(c) => {
                        redis::cmd("PING").query_async(c).await
                    }
                };
                if let Err(e) = result {
                    warn!(error = %e, "PubSub 心跳失败，连接可能已断开（redis-rs 将自动重连并重新订阅）");
                }
            }
        });

        self.tasks.lock().await.push(handle);
    }

    /// 启动消息分发任务
    fn start_dispatch_task(
        &self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<redis::PushInfo>,
    ) {
        let handlers = self.handlers.clone();
        let pattern_handlers = self.pattern_handlers.clone();

        let handle = tokio::spawn(async move {
            while let Some(push_info) = rx.recv().await {
                match push_info.kind {
                    redis::PushKind::Message | redis::PushKind::SMessage => {
                        if push_info.data.len() >= 2 {
                            let channel = value_to_string(&push_info.data[0]);
                            let payload = value_to_string(&push_info.data[1]);
                            if let (Some(ch), Some(pl)) = (channel, payload) {
                                trace!(channel = %ch, "收到 Pub/Sub 消息");
                                if let Some(handler) = handlers.get(&ch) {
                                    handler.handle(&ch, &pl).await;
                                }
                            }
                        }
                    }
                    redis::PushKind::PMessage => {
                        if push_info.data.len() >= 3 {
                            let pattern = value_to_string(&push_info.data[0]);
                            let channel = value_to_string(&push_info.data[1]);
                            let payload = value_to_string(&push_info.data[2]);
                            if let (Some(pat), Some(ch), Some(pl)) = (pattern, channel, payload) {
                                trace!(pattern = %pat, channel = %ch, "收到 Pub/Sub 模式消息");
                                if let Some(handler) = pattern_handlers.get(&pat) {
                                    handler.handle(&ch, &pl).await;
                                }
                            }
                        }
                    }
                    redis::PushKind::Subscribe
                    | redis::PushKind::SSubscribe
                    | redis::PushKind::PSubscribe => {
                        info!(kind = ?push_info.kind, "订阅确认");
                    }
                    redis::PushKind::Unsubscribe
                    | redis::PushKind::SUnsubscribe
                    | redis::PushKind::PUnsubscribe => {
                        info!(kind = ?push_info.kind, "取消订阅确认");
                    }
                    redis::PushKind::Disconnection => {
                        warn!("Pub/Sub 连接断开，redis-rs 将自动重连并重新订阅");
                    }
                    _ => {
                        trace!(kind = ?push_info.kind, "收到其他推送");
                    }
                }
            }
            info!("PubSub 消息分发任务结束");
        });

        // 消息分发任务由 tokio 独立管理，JoinHandle drop 后任务仍继续运行
        drop(handle);
    }
}

/// 全局订阅者管理器
pub struct GlobalSubscriberManager;

static GLOBAL_SUBSCRIBER: std::sync::OnceLock<Arc<GlobalSubscriber>> = std::sync::OnceLock::new();

impl GlobalSubscriberManager {
    /// 初始化全局订阅者（从 GlobalCacheManager 的 RedisClient 创建）
    pub async fn initialize() -> Result<()> {
        let cache_manager = crate::cache::GlobalCacheManager::get();
        let subscriber = GlobalSubscriber::new(cache_manager.client()).await?;
        GLOBAL_SUBSCRIBER
            .set(Arc::new(subscriber))
            .map_err(|_| Error::ConfigError("全局订阅者已初始化".to_string()))?;
        Ok(())
    }

    /// 获取全局订阅者
    pub fn get() -> &'static Arc<GlobalSubscriber> {
        GLOBAL_SUBSCRIBER
            .get()
            .expect("全局订阅者未初始化，请先调用 GlobalSubscriberManager::initialize()")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_SUBSCRIBER.get().is_some()
    }
}
