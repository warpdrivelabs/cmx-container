# 全局 Pub/Sub 订阅者设计方案（v4）

## 一、设计原则

### 1.1 逻辑分离

- **主连接**（`RedisClient.connection`）：用于普通命令（GET/SET/PUBLISH 等）
- **订阅连接**（`GlobalSubscriber` 持有）：专用于 Pub/Sub 订阅和消息接收

### 1.2 复用 RedisClient 配置

`GlobalSubscriber` 从 `RedisClient` 获取配置，基于该配置创建**专用订阅连接**，一直持有，不归还。

### 1.3 依赖 redis-rs 内置重连机制

通过分析 redis-rs 源码发现，**两种模式都有内置的自动重连+自动重新订阅机制**：

| 模式 | 自动重连 | 自动重新订阅 | 配置要求 |
|------|----------|-------------|----------|
| 单机（ConnectionManager） | ✅ 通过 Disconnection push 或 IO 错误触发 | ✅ `set_automatic_resubscription()` | 必须设置 `push_sender` |
| 集群（ClusterConnection） | ✅ 通过命令失败触发 `poll_recover` | ✅ 内置 `SubscriptionTracker` + `resubscribe()` | 必须设置 `push_sender` |

**关键差异**：
- 单机模式：RESP3 的 Disconnection push 可以**被动检测**断线，无需用户命令
- 集群模式：需要**用户命令触发**重连，空闲时不会自动检测断线

**因此，我们需要自己实现心跳检测**，确保集群模式下也能及时发现断线并触发重连。

---

## 二、架构设计

```
┌────────────────────────────────────────────────────────────────┐
│                        cmx-buffer                               │
│                                                                │
│  ┌──────────────┐     ┌────────────────────────────────────┐  │
│  │  RedisClient  │     │       GlobalSubscriber             │  │
│  │              │     │                                    │  │
│  │ connection:  │     │ subscriber_conn: SubscriberConn    │  │
│  │  Connection- │     │   ├── Standalone(ConnectionMgr)    │  │
│  │  Manager /   │     │   │     + set_automatic_           │  │
│  │  Cluster-    │     │   │       resubscription()         │  │
│  │  Connection  │     │   └── Cluster(ClusterConnection)  │  │
│  │              │     │         + 内置 SubscriptionTracker │  │
│  │ config ──────┼────>│                                    │  │
│  │              │     │ push_receiver:                     │  │
│  │              │     │   UnboundedReceiver<PushInfo>      │  │
│  │              │     │                                    │  │
│  │              │     │ handlers:                          │  │
│  │              │     │   DashMap<String, Arc<dyn Handler>>│  │
│  │              │     │                                    │  │
│  │              │     │ 心跳任务: 定期 PING 触发重连检测   │  │
│  └──────────────┘     └────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘

自动重连+重新订阅流程（单机模式）：
  1. 连接断开 → 底层发送 PushKind::Disconnection
  2. ConnectionManager 内部检测到 → 触发 reconnect()
  3. reconnect() 从 SubscriptionTracker 获取订阅列表
  4. 新连接建立后自动执行重新订阅 Pipeline
  5. 消息继续通过 push_sender 到达

自动重连+重新订阅流程（集群模式）：
  1. 心跳 PING 失败 → 触发 poll_recover()
  2. poll_recover() 成功后调用 resubscribe()
  3. resubscribe() 从 SubscriptionTracker 获取订阅列表
  4. 逐个命令路由到正确的节点重新订阅
  5. 消息继续通过 push_sender 到达
```

---

## 三、详细设计

### 3.1 新增类型

#### `ChannelHandler` trait

```rust
/// 频道消息处理器
#[async_trait]
pub trait ChannelHandler: Send + Sync {
    /// 处理收到的消息
    async fn handle(&self, channel: &str, payload: &str);
}
```

#### `FnChannelHandler` 闭包适配器

```rust
/// 基于闭包的频道处理器
pub struct FnChannelHandler {
    f: Box<dyn Fn(&str, &str) + Send + Sync>,
}
```

#### `SubscriberConnection` 枚举

```rust
/// 订阅专用连接，独立于主连接
///
/// 单机模式：ConnectionManager + set_automatic_resubscription()
///   → 自动检测断线（Disconnection push）+ 自动重新订阅
/// 集群模式：ClusterConnection + 内置 SubscriptionTracker
///   → 需要心跳 PING 触发重连 + 自动重新订阅
pub enum SubscriberConnection {
    Standalone(redis::aio::ConnectionManager),
    Cluster(redis::cluster_async::ClusterConnection),
}
```

为 `SubscriberConnection` 实现订阅方法：

```rust
impl SubscriberConnection {
    /// 订阅频道
    pub async fn subscribe(&mut self, channel: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.subscribe(channel).await?;
            }
            Self::Cluster(conn) => {
                conn.subscribe(channel).await?;
            }
        }
        Ok(())
    }

    /// 取消订阅
    pub async fn unsubscribe(&mut self, channel: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.unsubscribe(channel).await?;
            }
            Self::Cluster(conn) => {
                conn.unsubscribe(channel).await?;
            }
        }
        Ok(())
    }

    /// 模式订阅
    pub async fn psubscribe(&mut self, pattern: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.psubscribe(pattern).await?;
            }
            Self::Cluster(conn) => {
                conn.psubscribe(pattern).await?;
            }
        }
        Ok(())
    }

    /// 取消模式订阅
    pub async fn punsubscribe(&mut self, pattern: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.punsubscribe(pattern).await?;
            }
            Self::Cluster(conn) => {
                conn.punsubscribe(pattern).await?;
            }
        }
        Ok(())
    }
}
```

#### `GlobalSubscriber` 结构体

```rust
/// 全局 Pub/Sub 订阅者
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
```

注意：**不再需要 `subscribed_channels` / `subscribed_patterns`**！因为 redis-rs 的 `SubscriptionTracker` 内部已经跟踪了所有订阅，重连时会自动重新订阅。

### 3.2 创建订阅连接

在 `RedisClient` 中新增方法：

```rust
impl RedisClient {
    /// 创建专用的 Pub/Sub 订阅连接
    pub async fn create_subscriber_connection(
        &self,
    ) -> Result<(
        SubscriberConnection,
        tokio::sync::mpsc::UnboundedReceiver<redis::PushInfo>,
    )> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        match self.config.mode {
            RedisMode::Standalone => {
                // 单机：RESP3 + ConnectionManager + push_sender + 自动重新订阅
                let url = Self::ensure_resp3_url(&self.config.url);
                let client = redis::Client::open(url.as_str())?;
                let conn_manager = client
                    .get_connection_manager_with_config(
                        redis::aio::ConnectionManagerConfig::new()
                            .set_push_sender(tx)
                            .set_automatic_resubscription()
                    )
                    .await?;
                Ok((SubscriberConnection::Standalone(conn_manager), rx))
            }
            RedisMode::Cluster => {
                // 集群：RESP3 + ClusterConnection + push_sender
                // 内置 SubscriptionTracker 自动重新订阅
                let urls: Vec<&str> = self.config.cluster_urls.iter().map(|s| s.as_str()).collect();
                let cluster_client = redis::cluster::ClusterClientBuilder::new(urls)
                    .use_protocol(redis::ProtocolVersion::RESP3)
                    .push_sender(tx)
                    .build()?;
                let cluster_conn = cluster_client.get_async_connection().await?;
                Ok((SubscriberConnection::Cluster(cluster_conn), rx))
            }
        }
    }

    /// 确保 URL 包含 protocol=3 参数
    fn ensure_resp3_url(url: &str) -> String {
        if url.contains("protocol=3") {
            url.to_string()
        } else if url.contains('?') {
            format!("{}&protocol=3", url)
        } else {
            format!("{}?protocol=3", url)
        }
    }
}
```

### 3.3 心跳机制

心跳的目的是**触发集群模式下的断线检测和重连**：

```rust
impl GlobalSubscriber {
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
                    tracing::warn!(error = %e, "PubSub 心跳失败，连接可能已断开（redis-rs 将自动重连并重新订阅）");
                }
            }
        });

        self.tasks.lock().await.push(handle);
    }
}
```

### 3.4 消息分发任务

```rust
impl GlobalSubscriber {
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
                                tracing::trace!(channel = %ch, "收到 Pub/Sub 消息");
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
                                tracing::trace!(pattern = %pat, channel = %ch, "收到 Pub/Sub 模式消息");
                                if let Some(handler) = pattern_handlers.get(&pat) {
                                    handler.handle(&ch, &pl).await;
                                }
                            }
                        }
                    }
                    redis::PushKind::Subscribe | redis::PushKind::SSubscribe |
                    redis::PushKind::PSubscribe => {
                        tracing::info!(kind = ?push_info.kind, "订阅确认");
                    }
                    redis::PushKind::Unsubscribe | redis::PushKind::SUnsubscribe |
                    redis::PushKind::PUnsubscribe => {
                        tracing::info!(kind = ?push_info.kind, "取消订阅确认");
                    }
                    redis::PushKind::Disconnection => {
                        tracing::warn!("Pub/Sub 连接断开，redis-rs 将自动重连并重新订阅");
                    }
                    _ => {
                        tracing::trace!(kind = ?push_info.kind, "收到其他推送");
                    }
                }
            }
            tracing::info!("PubSub 消息分发任务结束");
        });

        // 消息分发任务由 tokio 独立管理，JoinHandle drop 后任务仍继续运行
        drop(handle);
    }
}
```

### 3.5 注册 API

```rust
impl GlobalSubscriber {
    /// 注册频道处理器
    pub async fn register_channel(
        &self,
        channel: &str,
        handler: Arc<dyn ChannelHandler>,
    ) -> Result<()> {
        // 如果该频道还没有处理器，发送 SUBSCRIBE 命令
        if !self.handlers.contains_key(channel) {
            let mut conn = self.conn.lock().await;
            conn.subscribe(channel).await?;
            tracing::info!(channel = %channel, "已订阅频道");
        }
        self.handlers.insert(channel.to_string(), handler);
        Ok(())
    }

    /// 注册频道处理器（闭包版本）
    pub async fn register_channel_fn<F>(
        &self,
        channel: &str,
        f: F,
    ) -> Result<()>
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        self.register_channel(channel, Arc::new(FnChannelHandler::new(f))).await
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
            tracing::info!(pattern = %pattern, "已订阅模式");
        }
        self.pattern_handlers.insert(pattern.to_string(), handler);
        Ok(())
    }

    /// 注销频道处理器
    pub async fn unregister_channel(&self, channel: &str) -> Result<()> {
        self.handlers.remove(channel);
        // 如果该频道没有其他处理器，发送 UNSUBSCRIBE
        if !self.handlers.contains_key(channel) {
            let mut conn = self.conn.lock().await;
            conn.unsubscribe(channel).await?;
            tracing::info!(channel = %channel, "已取消订阅频道");
        }
        Ok(())
    }
}
```

### 3.6 GlobalSubscriber 初始化

创建订阅连接后，自动订阅 `RedisConfig` 中配置的频道和模式。

```rust
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
                tracing::info!(channel = %channel, "已自动订阅配置频道");
            }
        }

        // 自动订阅配置中的模式
        if !config.subscribe_patterns.is_empty() {
            let mut conn = subscriber.conn.lock().await;
            for pattern in &config.subscribe_patterns {
                conn.psubscribe(pattern).await?;
                tracing::info!(pattern = %pattern, "已自动订阅配置模式");
            }
        }

        // 启动消息分发任务
        subscriber.start_dispatch_task(rx);

        // 启动心跳任务
        subscriber.start_heartbeat_task().await;

        Ok(subscriber)
    }
}
```

### 3.7 全局单例

```rust
static GLOBAL_SUBSCRIBER: OnceLock<Arc<GlobalSubscriber>> = OnceLock::new();

pub struct GlobalSubscriberManager;

impl GlobalSubscriberManager {
    /// 初始化全局订阅者
    pub async fn initialize() -> Result<()> {
        let cache_manager = GlobalCacheManager::get();
        let subscriber = GlobalSubscriber::new(cache_manager.client()).await?;
        GLOBAL_SUBSCRIBER
            .set(Arc::new(subscriber))
            .map_err(|_| Error::ConfigError("全局订阅者已初始化".to_string()))?;
        Ok(())
    }

    /// 获取全局订阅者
    pub fn get() -> &'static Arc<GlobalSubscriber> {
        GLOBAL_SUBSCRIBER.get().expect("全局订阅者未初始化")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_SUBSCRIBER.get().is_some()
    }
}
```

---

## 四、TOML 配置

### 4.1 RedisConfig 新增字段

```rust
pub struct RedisConfig {
    // ... 原有字段 ...

    /// 启动时自动订阅的频道列表
    #[serde(default)]
    pub subscribe_channels: Vec<String>,

    /// 启动时自动订阅的模式列表
    #[serde(default)]
    pub subscribe_patterns: Vec<String>,
}
```

### 4.2 dev.toml 配置示例

```toml
[redis]
url = "redis://192.168.137.95:32496/13"
# Redis 运行模式：standalone（单机）或 cluster（集群）
# mode = "standalone"
# 集群节点地址（集群模式时必填，逗号分隔）
# cluster_urls = "redis://node1:6379,redis://node2:6379,redis://node3:6379"
# Pub/Sub 心跳间隔（秒），0 表示禁用
# heartbeat_interval = 30
# 启动时自动订阅的频道（逗号分隔，需填写完整频道名，不会自动加前缀）
# subscribe_channels = "cmx:plugin:changed"
# 启动时自动订阅的模式（逗号分隔，支持通配符 * ? []，需填写完整模式，不会自动加前缀）
# subscribe_patterns = "cmx:*"
```

### 4.3 配置说明

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `subscribe_channels` | 逗号分隔字符串 | 空 | 启动时自动 SUBSCRIBE 的频道，需填写完整频道名 |
| `subscribe_patterns` | 逗号分隔字符串 | 空 | 启动时自动 PSUBSCRIBE 的模式，支持 `*` `?` `[]` 通配符 |

**注意**：配置中的频道名/模式是**原样传给 Redis**，不会自动加 `key_prefix` 前缀。

---

## 五、修改文件清单

| 文件 | 修改类型 | 说明 |
|------|----------|------|
| `client.rs` | 修改 | 新增 `create_subscriber_connection()`、`ensure_resp3_url()`、`SubscriberConnection` |
| `config.rs` | 修改 | 新增 `subscribe_channels`、`subscribe_patterns` 字段及 `from_config` 解析 |
| `cache/pubsub.rs` | 修改 | 新增 `ChannelHandler`、`FnChannelHandler`、`GlobalSubscriber`、`GlobalSubscriberManager`；删除旧 `Subscriber`、`SubscriberBuilder`、`SharedSubscriber`、`PubSubMessage` |
| `cache/mod.rs` | 修改 | 导出新类型，移除旧类型导出 |
| `lib.rs` | 修改 | 导出新类型，移除旧类型导出 |
| `Cargo.toml` | 修改 | 新增 `dashmap` 依赖 |
| `dev.toml` | 修改 | 新增 `subscribe_channels`、`subscribe_patterns` 配置示例 |
| `cmx-plugin/error.rs` | 修改 | `PluginError::Cache` 改为 `#[from] cmx_buffer::Error` |
| `cmx-plugin/manager.rs` | 修改 | 使用 `GlobalSubscriber` 替代旧 `Subscriber`，频道名直接使用常量不加 `build_key` |

---

## 六、使用示例

### cmx-plugin 中的使用

```rust
// 初始化时（在 GlobalCacheManager::initialize() 之后）
if !cmx_buffer::GlobalSubscriberManager::is_initialized() {
    cmx_buffer::GlobalSubscriberManager::initialize().await?;
}

// 注册频道处理器
let subscriber = cmx_buffer::GlobalSubscriberManager::get();
let full_channel = crate::cluster::notification::PLUGIN_CHANGE_CHANNEL.to_string();

subscriber.register_channel_fn(&full_channel, move |channel, payload| {
    tracing::trace!(channel = %channel, "收到 Redis Pub/Sub 消息");
    let handler = handler.clone();
    let payload = payload.to_string();
    tokio::spawn(async move {
        match serde_json::from_str::<PluginChangeNotification>(&payload) {
            Ok(notification) => handler.handle(&notification).await,
            Err(e) => tracing::debug!("解析通知失败: {}", e),
        }
    });
}).await?;
```

**注意**：`PLUGIN_CHANGE_CHANNEL` 常量本身已包含完整频道名（如 `"cmx:plugin:changed"`），不需要再用 `build_key()` 添加前缀。

---

## 七、关键设计决策

| 决策点 | 选择 | 原因 |
|--------|------|------|
| 重连+重新订阅 | 依赖 redis-rs 内置机制 | ConnectionManager 和 ClusterConnection 都已内置，无需重复实现 |
| 单机自动重新订阅 | `set_automatic_resubscription()` | redis-rs 原生支持，配合 Disconnection push 被动检测断线 |
| 集群自动重新订阅 | 内置 `SubscriptionTracker` | 只需设置 `push_sender`，无需额外配置 |
| 心跳目的 | 触发集群模式的重连检测 | 集群模式空闲时不会自动检测断线，需要 PING 命令触发 |
| 订阅跟踪 | 依赖 redis-rs `SubscriptionTracker` | 不需要自己维护 `subscribed_channels`，重连时自动重新订阅 |
| 处理器注册表 | `DashMap` | 并发安全，注册和消息分发可同时进行 |
| 旧 Subscriber | 已删除 | 不需要向后兼容，统一使用 GlobalSubscriber |
| 频道名构建 | 直接使用常量，不用 `build_key()` | 频道常量已包含完整前缀，`build_key()` 会重复添加前缀 |
| 配置预订阅 | 可选，通过 TOML 配置 | 不配置也不影响功能，业务代码通过 `register_channel_fn` 动态注册即可 |
