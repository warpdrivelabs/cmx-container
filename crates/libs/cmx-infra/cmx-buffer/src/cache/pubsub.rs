use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use futures_util::StreamExt;

///! 缓存操作模块 - 发布/订阅操作

/// 作者: AI Assistant
/// 日期: 2026-03-16

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
    ///
    /// # 参数
    /// * `channel` - 频道名称
    /// * `message` - 要发布的消息
    ///
    /// # 返回值
    /// * `Result<u64>` - 接收到消息的订阅者数量
    pub async fn publish(&self, channel: &str, message: &str) -> Result<u64> {
        let full_channel = self.client.build_key(channel);
        let timer = OperationTimer::new("PUBLISH", &full_channel);

        let mut conn = self.client.get_connection().await?;
        let subscribers: u64 = redis::cmd("PUBLISH")
            .arg(&full_channel)
            .arg(message)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(subscribers)
    }

    /// 向频道发布JSON可序列化的消息
    ///
    /// # 参数
    /// * `channel` - 频道名称
    /// * `message` - 要发布的可序列化消息
    ///
    /// # 返回值
    /// * `Result<u64>` - 接收到消息的订阅者数量
    pub async fn publish_json<T: Serialize>(&self, channel: &str, message: &T) -> Result<u64> {
        let json = serde_json::to_string(message)?;
        self.publish(channel, &json).await
    }

    /// 获取匹配模式的活动频道列表
    ///
    /// # 参数
    /// * `pattern` - 可选的匹配模式
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 活动频道名称列表
    pub async fn pubsub_channels(&self, pattern: Option<&str>) -> Result<Vec<String>> {
        let mut conn = self.client.get_connection().await?;

        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("CHANNELS");
        if let Some(p) = pattern {
            cmd.arg(p);
        }

        let channels: Vec<String> = cmd.query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(channels)
    }

    /// 获取频道的订阅者数量
    ///
    /// # 参数
    /// * `channels` - 频道名称数组
    ///
    /// # 返回值
    /// * `Result<Vec<(String, u64)>>` - 频道及其订阅者数量的元组列表
    pub async fn pubsub_numsub(&self, channels: &[&str]) -> Result<Vec<(String, u64)>> {
        let full_channels: Vec<String> = channels.iter()
            .map(|c| self.client.build_key(c))
            .collect();

        let mut conn = self.client.get_connection().await?;

        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("NUMSUB");
        for ch in &full_channels {
            cmd.arg(ch);
        }

        let result: Vec<(String, u64)> = cmd.query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(result)
    }

    /// 获取模式订阅的数量
    ///
    /// # 返回值
    /// * `Result<u64>` - 使用模式匹配订阅的客户端总数
    pub async fn pubsub_numpat(&self) -> Result<u64> {
        let mut conn = self.client.get_connection().await?;

        let count: u64 = redis::cmd("PUBSUB")
            .arg("NUMPAT")
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }
}

/// 从订阅接收到的消息
#[derive(Debug, Clone)]
pub struct PubSubMessage {
    /// 频道名称
    pub channel: String,
    /// 消息内容
    pub payload: String,
}

/// 订阅者
pub struct Subscriber {
    rx: mpsc::Receiver<PubSubMessage>,
    _handle: tokio::task::JoinHandle<()>,
}

impl Subscriber {
    /// 为给定频道创建新的订阅者
    ///
    /// # 参数
    /// * `url` - Redis 服务器连接 URL
    /// * `channels` - 要订阅的频道列表
    ///
    /// # 返回值
    /// * `Result<Self>` - 订阅者实例
    pub async fn new(url: &str, channels: Vec<String>) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let mut pubsub = client.get_async_pubsub().await?;

        for channel in &channels {
            pubsub.subscribe(channel).await?;
        }

        let (tx, rx) = mpsc::channel(100);

        let handle = tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                let channel = msg.get_channel_name().to_string();
                if let Ok(payload) = msg.get_payload::<String>() {
                    let _ = tx.send(PubSubMessage { channel, payload }).await;
                }
            }
        });

        Ok(Self { rx, _handle: handle })
    }

    /// 使用模式匹配创建订阅者
    ///
    /// # 参数
    /// * `url` - Redis 服务器连接 URL
    /// * `patterns` - 要订阅的模式列表
    ///
    /// # 返回值
    /// * `Result<Self>` - 订阅者实例
    pub async fn with_patterns(url: &str, patterns: Vec<String>) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let mut pubsub = client.get_async_pubsub().await?;

        for pattern in &patterns {
            pubsub.psubscribe(pattern).await?;
        }

        let (tx, rx) = mpsc::channel(100);

        let handle = tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                let channel = msg.get_channel_name().to_string();
                if let Ok(payload) = msg.get_payload::<String>() {
                    let _ = tx.send(PubSubMessage { channel, payload }).await;
                }
            }
        });

        Ok(Self { rx, _handle: handle })
    }

    /// 接收下一条消息
    ///
    /// # 返回值
    /// * `Option<PubSubMessage>` - 如果有消息则返回Some(消息)，否则返回None
    pub async fn recv(&mut self) -> Option<PubSubMessage> {
        self.rx.recv().await
    }

    /// 尝试接收消息（非阻塞）
    ///
    /// # 返回值
    /// * `Option<PubSubMessage>` - 如果有消息则返回Some(消息)，否则返回None
    pub fn try_recv(&mut self) -> Option<PubSubMessage> {
        self.rx.try_recv().ok()
    }
}

/// 可克隆的共享订阅者
pub struct SharedSubscriber {
    inner: Arc<tokio::sync::Mutex<Subscriber>>,
}

impl SharedSubscriber {
    /// 创建新的共享订阅者
    ///
    /// # 参数
    /// * `url` - Redis 服务器连接 URL
    /// * `channels` - 要订阅的频道列表
    ///
    /// # 返回值
    /// * `Result<Self>` - 共享订阅者实例
    pub async fn new(url: &str, channels: Vec<String>) -> Result<Self> {
        let subscriber = Subscriber::new(url, channels).await?;
        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(subscriber)),
        })
    }

    /// 接收下一条消息
    ///
    /// # 返回值
    /// * `Option<PubSubMessage>` - 如果有消息则返回Some(消息)，否则返回None
    pub async fn recv(&self) -> Option<PubSubMessage> {
        self.inner.lock().await.recv().await
    }
}

impl Clone for SharedSubscriber {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
