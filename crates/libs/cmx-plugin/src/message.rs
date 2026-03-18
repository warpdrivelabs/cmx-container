//! 消息队列模块 - 插件事件通知和消息传递
//!
//! 基于 Redis Pub/Sub 实现插件系统的事件通知机制。
//! 支持插件生命周期事件、部署事件、节点事件等的发布和订阅。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::PluginError;

/// 消息频道名称
pub const CHANNEL_PLUGIN_EVENTS: &str = "cmx:plugin:events";
pub const CHANNEL_DEPLOYMENT_EVENTS: &str = "cmx:plugin:deployment";
pub const CHANNEL_NODE_EVENTS: &str = "cmx:plugin:node";
pub const CHANNEL_SYSTEM_EVENTS: &str = "cmx:plugin:system";

/// 插件事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginEventType {
    /// 插件安装
    Installed,
    /// 插件卸载
    Uninstalled,
    /// 插件激活
    Activated,
    /// 插件停用
    Deactivated,
    /// 插件升级
    Upgraded,
    /// 插件降级
    Downgraded,
    /// 插件回滚
    RolledBack,
    /// 插件错误
    Error,
}

/// 部署事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentEventType {
    /// 部署开始
    Started,
    /// 部署完成
    Completed,
    /// 部署失败
    Failed,
    /// 部署回滚
    RolledBack,
    /// 节点同步
    NodeSynced,
    /// 节点故障
    NodeFailed,
}

/// 节点事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeEventType {
    /// 节点注册
    Registered,
    /// 节点注销
    Unregistered,
    /// 节点上线
    Online,
    /// 节点离线
    Offline,
    /// 节点维护
    Maintenance,
    /// 心跳超时
    HeartbeatTimeout,
}

/// 系统事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemEventType {
    /// 系统启动
    Startup,
    /// 系统关闭
    Shutdown,
    /// 配置变更
    ConfigChanged,
    /// 资源警告
    ResourceWarning,
    /// 安全警告
    SecurityWarning,
}

/// 插件事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEvent {
    /// 事件 ID
    pub event_id: String,
    /// 事件类型
    pub event_type: PluginEventType,
    /// 插件 ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 节点 ID
    pub node_id: Option<String>,
    /// 时间戳
    pub timestamp: i64,
    /// 事件数据
    pub data: HashMap<String, String>,
    /// 错误信息
    pub error: Option<String>,
}

impl PluginEvent {
    /// 创建新的插件事件
    pub fn new(event_type: PluginEventType, plugin_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            plugin_id: plugin_id.into(),
            version: version.into(),
            node_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            data: HashMap::new(),
            error: None,
        }
    }

    /// 设置节点 ID
    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// 添加事件数据
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    /// 设置错误信息
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// 部署事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEvent {
    /// 事件 ID
    pub event_id: String,
    /// 事件类型
    pub event_type: DeploymentEventType,
    /// 操作 ID
    pub operation_id: String,
    /// 插件 ID
    pub plugin_id: String,
    /// 版本
    pub version: String,
    /// 节点 ID
    pub node_id: Option<String>,
    /// 时间戳
    pub timestamp: i64,
    /// 部署策略
    pub strategy: Option<String>,
    /// 错误信息
    pub error: Option<String>,
}

impl DeploymentEvent {
    /// 创建新的部署事件
    pub fn new(
        event_type: DeploymentEventType,
        operation_id: impl Into<String>,
        plugin_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            operation_id: operation_id.into(),
            plugin_id: plugin_id.into(),
            version: version.into(),
            node_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            strategy: None,
            error: None,
        }
    }

    /// 设置节点 ID
    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// 设置部署策略
    pub fn with_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.strategy = Some(strategy.into());
        self
    }

    /// 设置错误信息
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// 节点事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEvent {
    /// 事件 ID
    pub event_id: String,
    /// 事件类型
    pub event_type: NodeEventType,
    /// 节点 ID
    pub node_id: String,
    /// 节点名称
    pub node_name: Option<String>,
    /// 时间戳
    pub timestamp: i64,
    /// 附加数据
    pub data: HashMap<String, String>,
}

impl NodeEvent {
    /// 创建新的节点事件
    pub fn new(event_type: NodeEventType, node_id: impl Into<String>) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            node_id: node_id.into(),
            node_name: None,
            timestamp: chrono::Utc::now().timestamp(),
            data: HashMap::new(),
        }
    }

    /// 设置节点名称
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.node_name = Some(name.into());
        self
    }

    /// 添加附加数据
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }
}

/// 系统事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    /// 事件 ID
    pub event_id: String,
    /// 事件类型
    pub event_type: SystemEventType,
    /// 时间戳
    pub timestamp: i64,
    /// 消息
    pub message: String,
    /// 附加数据
    pub data: HashMap<String, String>,
}

impl SystemEvent {
    /// 创建新的系统事件
    pub fn new(event_type: SystemEventType, message: impl Into<String>) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            timestamp: chrono::Utc::now().timestamp(),
            message: message.into(),
            data: HashMap::new(),
        }
    }

    /// 添加附加数据
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }
}

/// 消息类型枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// 插件事件
    PluginEvent(PluginEvent),
    /// 部署事件
    DeploymentEvent(DeploymentEvent),
    /// 节点事件
    NodeEvent(NodeEvent),
    /// 系统事件
    SystemEvent(SystemEvent),
}

/// 事件处理器
pub type EventHandler = Box<dyn Fn(Message) + Send + Sync>;

/// 消息队列配置
#[derive(Debug, Clone)]
pub struct MessageQueueConfig {
    /// 是否启用消息队列
    pub enabled: bool,
    /// Redis URL
    pub redis_url: Option<String>,
    /// 订阅的频道列表
    pub subscribe_channels: Vec<String>,
}

impl Default for MessageQueueConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            redis_url: None,
            subscribe_channels: vec![
                CHANNEL_PLUGIN_EVENTS.to_string(),
                CHANNEL_DEPLOYMENT_EVENTS.to_string(),
                CHANNEL_NODE_EVENTS.to_string(),
                CHANNEL_SYSTEM_EVENTS.to_string(),
            ],
        }
    }
}

/// 消息队列 - 基于 Redis Pub/Sub
pub struct MessageQueue {
    config: MessageQueueConfig,
    pubsub: Option<Arc<cmx_buffer::PubSubOps>>,
    handlers: Arc<RwLock<HashMap<String, Vec<EventHandler>>>>,
    subscriber: Option<Arc<cmx_buffer::SharedSubscriber>>,
}

impl MessageQueue {
    /// 创建新的消息队列
    pub fn new(config: MessageQueueConfig) -> Self {
        Self {
            config,
            pubsub: None,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            subscriber: None,
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config() -> Self {
        Self::new(MessageQueueConfig::default())
    }

    /// 连接到 Redis
    pub async fn connect(&mut self, cache_manager: &cmx_buffer::CacheManager) -> Result<(), MessageQueueError> {
        if !self.config.enabled {
            return Ok(());
        }

        let pubsub = cmx_buffer::PubSubOps::new(cache_manager.client().clone());
        self.pubsub = Some(Arc::new(pubsub));

        log::info!("消息队列已连接到 Redis");
        Ok(())
    }

    /// 启动订阅
    pub async fn start_subscriber(&mut self) -> Result<(), MessageQueueError> {
        if !self.config.enabled {
            return Ok(());
        }

        let redis_url = match &self.config.redis_url {
            Some(url) => url.clone(),
            None => return Err(MessageQueueError::NotConnected),
        };

        let channels = self.config.subscribe_channels.clone();
        let subscriber = cmx_buffer::SharedSubscriber::new(&redis_url, channels).await?;
        self.subscriber = Some(Arc::new(subscriber));

        log::info!("消息队列订阅已启动");
        Ok(())
    }

    /// 发布插件事件
    pub async fn publish_plugin_event(&self, event: PluginEvent) -> Result<(), MessageQueueError> {
        self.publish(CHANNEL_PLUGIN_EVENTS, Message::PluginEvent(event)).await
    }

    /// 发布部署事件
    pub async fn publish_deployment_event(&self, event: DeploymentEvent) -> Result<(), MessageQueueError> {
        self.publish(CHANNEL_DEPLOYMENT_EVENTS, Message::DeploymentEvent(event)).await
    }

    /// 发布节点事件
    pub async fn publish_node_event(&self, event: NodeEvent) -> Result<(), MessageQueueError> {
        self.publish(CHANNEL_NODE_EVENTS, Message::NodeEvent(event)).await
    }

    /// 发布系统事件
    pub async fn publish_system_event(&self, event: SystemEvent) -> Result<(), MessageQueueError> {
        self.publish(CHANNEL_SYSTEM_EVENTS, Message::SystemEvent(event)).await
    }

    /// 发布消息
    pub async fn publish(&self, channel: &str, message: Message) -> Result<(), MessageQueueError> {
        let pubsub = self.pubsub.as_ref().ok_or(MessageQueueError::NotConnected)?;
        
        pubsub.publish_json(channel, &message).await?;
        
        log::debug!("消息已发布到频道: {}", channel);
        Ok(())
    }

    /// 接收消息（阻塞）
    pub async fn recv(&self) -> Option<cmx_buffer::PubSubMessage> {
        if let Some(subscriber) = &self.subscriber {
            subscriber.recv().await
        } else {
            None
        }
    }

    /// 注册事件处理器
    pub async fn register_handler(&self, channel: &str, handler: EventHandler) {
        let mut handlers = self.handlers.write().await;
        handlers.entry(channel.to_string()).or_default().push(handler);
    }

    /// 处理接收到的消息
    pub async fn process_message(&self, msg: cmx_buffer::PubSubMessage) {
        let channel = msg.channel.clone();
        let message: Result<Message, _> = serde_json::from_str(&msg.payload);
        
        match message {
            Ok(msg) => {
                let handlers = self.handlers.read().await;
                if let Some(channel_handlers) = handlers.get(&channel) {
                    for handler in channel_handlers {
                        handler(msg.clone());
                    }
                }
            }
            Err(e) => {
                log::warn!("解析消息失败: {}", e);
            }
        }
    }

    /// 运行消息循环
    pub async fn run(&self) {
        loop {
            if let Some(msg) = self.recv().await {
                self.process_message(msg).await;
            }
        }
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        self.pubsub.is_some()
    }
}

/// 消息队列错误
#[derive(Debug, thiserror::Error)]
pub enum MessageQueueError {
    #[error("未连接到 Redis")]
    NotConnected,
    #[error("发布消息失败: {0}")]
    PublishError(#[from] cmx_buffer::Error),
    #[error("序列化错误: {0}")]
    SerializeError(#[from] serde_json::Error),
}

/// 消息队列构建器
pub struct MessageQueueBuilder {
    config: MessageQueueConfig,
}

impl MessageQueueBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            config: MessageQueueConfig::default(),
        }
    }

    /// 设置是否启用
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    /// 设置 Redis URL
    pub fn redis_url(mut self, url: impl Into<String>) -> Self {
        self.config.redis_url = Some(url.into());
        self
    }

    /// 添加订阅频道
    pub fn subscribe_channel(mut self, channel: impl Into<String>) -> Self {
        self.config.subscribe_channels.push(channel.into());
        self
    }

    /// 构建消息队列
    pub fn build(self) -> MessageQueue {
        MessageQueue::new(self.config)
    }
}

impl Default for MessageQueueBuilder {
    fn default() -> Self {
        Self::new()
    }
}
