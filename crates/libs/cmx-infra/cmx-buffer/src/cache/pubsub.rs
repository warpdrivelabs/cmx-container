use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;
use futures_util::StreamExt;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

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
        let full_channel = self.client.build_key(channel);
        let timer = OperationTimer::new("PUBLISH", &full_channel);

        let mut conn = self.client.get_connection();
        let subscribers: u64 = redis::cmd("PUBLISH")
            .arg(&full_channel)
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
        let full_channels: Vec<String> = channels
            .iter()
            .map(|c| self.client.build_key(c))
            .collect();

        let mut conn = self.client.get_connection();

        let mut cmd = redis::cmd("PUBSUB");
        cmd.arg("NUMSUB");
        for ch in &full_channels {
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

/// 从订阅接收到的消息
#[derive(Debug, Clone)]
pub struct PubSubMessage {
    /// 频道名称
    pub channel: String,
    /// 消息内容
    pub payload: String,
}

/// 订阅者构建器
pub struct SubscriberBuilder {
    url: String,
    channels: Vec<String>,
    patterns: Vec<String>,
    heartbeat_interval: Duration,
}

impl SubscriberBuilder {
    /// 创建新的订阅者构建器
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            channels: Vec::new(),
            patterns: Vec::new(),
            heartbeat_interval: Duration::from_secs(30),
        }
    }

    /// 设置要订阅的频道
    pub fn channels(mut self, channels: Vec<String>) -> Self {
        self.channels = channels;
        self
    }

    /// 设置要订阅的模式
    pub fn patterns(mut self, patterns: Vec<String>) -> Self {
        self.patterns = patterns;
        self
    }

    /// 设置心跳间隔
    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// 构建订阅者
    pub async fn build(self) -> Result<Subscriber> {
        let client = redis::Client::open(self.url.as_str())?;
        let pubsub = client.get_async_pubsub().await?;

        let (mut sink, stream) = pubsub.split();

        for channel in &self.channels {
            sink.subscribe(channel).await.map_err(|e| {
                Error::PubSubError(format!("订阅频道 {} 失败: {}", channel, e))
            })?;
        }
        for pattern in &self.patterns {
            sink.psubscribe(pattern).await.map_err(|e| {
                Error::PubSubError(format!("订阅模式 {} 失败: {}", pattern, e))
            })?;
        }

        let (tx, rx) = mpsc::channel(100);

        let heartbeat_handle = Self::spawn_heartbeat(sink, self.heartbeat_interval);
        let msg_handle = Self::spawn_message_reader(stream, tx);

        Ok(Subscriber {
            rx,
            heartbeat_handle,
            msg_handle,
        })
    }

    /// 启动心跳任务
    fn spawn_heartbeat(
        mut sink: redis::aio::PubSubSink,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                match sink.ping::<redis::Value>().await {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "PubSub 心跳失败，连接可能已断开");
                        break;
                    }
                }
            }
        })
    }

    /// 启动消息读取任务
    fn spawn_message_reader(
        stream: redis::aio::PubSubStream,
        tx: mpsc::Sender<PubSubMessage>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut stream = Box::pin(stream);
            while let Some(msg) = stream.next().await {
                let channel = msg.get_channel_name().to_string();
                if let Ok(payload) = msg.get_payload::<String>()
                    && tx.send(PubSubMessage { channel, payload }).await.is_err()
                {
                    break;
                }
            }
            info!("PubSub 消息读取任务结束");
        })
    }
}

/// 订阅者（带心跳保活）
pub struct Subscriber {
    rx: mpsc::Receiver<PubSubMessage>,
    heartbeat_handle: tokio::task::JoinHandle<()>,
    msg_handle: tokio::task::JoinHandle<()>,
}

impl Subscriber {
    /// 为给定频道创建新的订阅者（默认 30 秒心跳）
    pub async fn new(url: &str, channels: Vec<String>) -> Result<Self> {
        SubscriberBuilder::new(url)
            .channels(channels)
            .build()
            .await
    }

    /// 使用模式匹配创建订阅者（默认 30 秒心跳）
    pub async fn with_patterns(url: &str, patterns: Vec<String>) -> Result<Self> {
        SubscriberBuilder::new(url)
            .patterns(patterns)
            .build()
            .await
    }

    /// 接收下一条消息
    pub async fn recv(&mut self) -> Option<PubSubMessage> {
        self.rx.recv().await
    }

    /// 尝试接收消息（非阻塞）
    pub fn try_recv(&mut self) -> Option<PubSubMessage> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.heartbeat_handle.abort();
        self.msg_handle.abort();
    }
}

/// 可克隆的共享订阅者
pub struct SharedSubscriber {
    inner: Arc<tokio::sync::Mutex<Subscriber>>,
}

impl SharedSubscriber {
    /// 创建新的共享订阅者
    pub async fn new(url: &str, channels: Vec<String>) -> Result<Self> {
        let subscriber = Subscriber::new(url, channels).await?;
        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(subscriber)),
        })
    }

    /// 接收下一条消息
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
