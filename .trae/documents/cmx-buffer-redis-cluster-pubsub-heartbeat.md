# cmx-buffer Redis 集群支持与 Pub/Sub 心跳功能改造计划

## 一、背景分析

### 1.1 当前架构问题

- **`RedisClient`** 当前使用 `bb8::Pool<RedisConnectionManager>` 连接池，但 `Cargo.toml` 中**未声明 bb8/bb8-redis 依赖**（代码无法编译）
- 所有缓存操作通过 `self.client.get_connection().await?` 获取池连接，再通过 `redis::cmd("...").query_async(&mut *conn)` 执行
- **仅支持单机模式**，无法连接 Redis 集群
- **Pub/Sub 订阅者**使用 `PubSub::on_message()` 读取消息，**无心跳机制**，长时间无消息时 TCP 连接可能被静默断开

### 1.2 目标

1. `RedisConfig` 增加单机/集群模式配置，启动时根据配置初始化对应连接
2. 统一异步连接抽象，使用 `ConnectionManager`（单机）或 `ClusterConnection<MultiplexedConnection>`（集群）
3. Pub/Sub 增加心跳功能，定期 PING 保活

### 1.3 为什么不需要连接池

根据 redis-rs 官方文档（https://docs.rs/redis/1.2.1）：

> The multiplexed async connection is **thread-safe and cheaply cloneable**, making it suitable for **concurrent usage without additional pooling**.

- **`MultiplexedConnection`**：单个 TCP 连接上多路复用多个请求/响应，`Clone` 成本极低（仅复制 handle），天然支持高并发
- **`ConnectionManager`**：在 `MultiplexedConnection` 基础上增加自动重连，同样是 `Clone` 的
- **`ClusterConnection<MultiplexedConnection>`**：集群模式下内部为每个节点维护 `MultiplexedConnection`，也是 `Clone` 的

因此，**不需要 bb8/r2d2 等外部连接池**，直接 clone 连接 handle 即可并发使用。

---

## 二、架构设计

### 2.1 统一连接抽象

redis-rs 中 `ConnectionManager`（单机）和 `cluster_async::ClusterConnection`（集群）都实现了 `redis::aio::ConnectionLike` trait。创建枚举 `RedisConnectionRef` 统一封装：

```
RedisConnectionRef (enum, Clone)
├── Standalone(ConnectionManager)                           // 单机模式，自带重连
└── Cluster(ClusterConnection<MultiplexedConnection>)       // 集群模式，自带重连和路由
```

两者都实现了 `redis::aio::ConnectionLike`，通过为枚举实现该 trait，所有现有 `redis::cmd("...").query_async(&mut conn)` 调用无需修改命令格式。

### 2.2 连接创建流程

```
RedisConfig { mode: Standalone }
  → redis::Client::open(url)
  → client.get_connection_manager()
  → RedisConnectionRef::Standalone(ConnectionManager)

RedisConfig { mode: Cluster }
  → redis::cluster::ClusterClient::new(urls)
  → cluster_client.get_async_connection()
  → RedisConnectionRef::Cluster(ClusterConnection)
```

### 2.3 Pub/Sub 心跳设计

使用 `PubSub::split()` 将连接拆分为 `(PubSubSink, PubSubStream)`：
- **消息读取任务**：通过 `PubSubStream`（实现 `Stream<Item=Msg>`）接收消息
- **心跳任务**：定时（默认 30 秒）通过 `PubSubSink::ping()` 发送 PING 保活
- **重连机制**：心跳失败时，尝试重新创建 PubSub 连接并重新订阅

---

## 三、修改文件清单

| 文件 | 修改类型 | 说明 |
|------|----------|------|
| `config.rs` | 修改 | 增加 `RedisMode` 枚举、集群配置字段、心跳配置 |
| `client.rs` | 重写 | 移除 bb8，使用 `RedisConnectionRef` 枚举 |
| `cache/pubsub.rs` | 重写 | 增加心跳保活、断线重连、支持集群模式 |
| `cache/ops.rs` | 修改 | 适配新连接 API（`get_connection()` 不再 async） |
| `cache/set.rs` | 修改 | 同上 |
| `cache/sorted_set.rs` | 修改 | 同上 |
| `cache/ttl.rs` | 修改 | 同上 |
| `cache/mod.rs` | 修改 | 适配新接口 |
| `lock/manager.rs` | 修改 | 适配新连接 API |
| `lock/mod.rs` | 修改 | 适配新接口 |
| `error.rs` | 修改 | 增加 `PubSubError` 变体 |
| `lib.rs` | 修改 | 更新导出 |

---

## 四、详细实施步骤

### 步骤 1：修改 `config.rs` — 增加 Redis 模式和集群配置

**新增 `RedisMode` 枚举：**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RedisMode {
    Standalone,
    Cluster,
}
```

**`RedisConfig` 新增字段：**
```rust
pub struct RedisConfig {
    pub url: String,                              // 原有，单机模式使用
    pub mode: RedisMode,                          // 新增：单机或集群模式
    pub cluster_urls: Vec<String>,                // 新增：集群节点地址列表
    pub heartbeat_interval: u64,                  // 新增：Pub/Sub 心跳间隔（秒）
    // ... 原有字段保持不变 ...
}
```

**`from_config()` 方法增加读取 `redis.mode`、`redis.cluster_urls`、`redis.heartbeat_interval`**

**Builder 方法增加：**
- `with_mode(mode)` - 设置模式
- `with_cluster_urls(urls)` - 设置集群地址
- `with_heartbeat_interval(secs)` - 设置心跳间隔

---

### 步骤 2：修改 `error.rs` — 增加错误变体

```rust
#[error("Pub/Sub 错误: {0}")]
PubSubError(String),
```

---

### 步骤 3：重写 `client.rs` — 核心连接抽象

**移除：** `bb8::Pool`、`bb8_redis::RedisConnectionManager` 相关所有代码

**新增 `RedisConnectionRef` 枚举：**
```rust
/// Redis 异步连接引用，统一封装单机和集群连接
#[derive(Clone)]
pub enum RedisConnectionRef {
    /// 单机模式 - 使用 ConnectionManager（自带断线重连）
    Standalone(redis::aio::ConnectionManager),
    /// 集群模式 - 使用 ClusterConnection（自带重连和路由）
    Cluster(redis::cluster_async::ClusterConnection),
}
```

**为 `RedisConnectionRef` 实现 `redis::aio::ConnectionLike` trait：**
```rust
impl redis::aio::ConnectionLike for RedisConnectionRef {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            Self::Standalone(conn) => conn.req_packed_command(cmd),
            Self::Cluster(conn) => conn.req_packed_command(cmd),
        }
    }
    fn req_packed_commands<'a>(&'a mut self, cmd: &'a redis::Pipeline, offset: usize, count: usize) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            Self::Standalone(conn) => conn.req_packed_commands(cmd, offset, count),
            Self::Cluster(conn) => conn.req_packed_commands(cmd, offset, count),
        }
    }
    fn get_db(&self) -> i64 {
        match self {
            Self::Standalone(conn) => conn.get_db(),
            Self::Cluster(conn) => conn.get_db(),
        }
    }
}
```

**重构 `RedisClient`：**
```rust
#[derive(Clone)]
pub struct RedisClient {
    connection: RedisConnectionRef,
    config: RedisConfig,
    cache_config: CacheConfig,
    lock_config: LockConfig,
}
```

**关键方法变更：**
- `new(config)` → 根据 `config.mode` 创建 `Standalone` 或 `Cluster` 连接
- `get_connection()` → 返回 `RedisConnectionRef`（**不再 async**，直接 clone）
- 移除 `pool()` 方法
- `is_connected()` → 使用新连接发送 PING

**单机模式初始化流程：**
```rust
let client = redis::Client::open(url)?;
let conn_manager = client.get_connection_manager().await?;
RedisConnectionRef::Standalone(conn_manager)
```

**集群模式初始化流程：**
```rust
let cluster_client = redis::cluster::ClusterClient::new(urls)?;
let cluster_conn = cluster_client.get_async_connection().await?;
RedisConnectionRef::Cluster(cluster_conn)
```

---

### 步骤 4：适配所有缓存操作模块

所有操作模块（`ops.rs`、`set.rs`、`sorted_set.rs`、`ttl.rs`）的变更模式相同：

**之前：**
```rust
let mut conn = self.client.get_connection().await?;
redis::cmd("SET").arg(...).query_async(&mut *conn).await
```

**之后：**
```rust
let mut conn = self.client.get_connection();
redis::cmd("SET").arg(...).query_async(&mut conn).await
```

关键变更：
1. `get_connection()` 不再是 `async`，移除 `.await`
2. 返回类型从 `bb8::PooledConnection` 变为 `RedisConnectionRef`
3. `query_async(&mut conn)` 直接使用（不需要 `&mut *conn` 解引用）

**需要修改的文件及方法：**
- `ops.rs`: `set`, `get`, `del`, `del_batch`, `exists`, `set_ex`, `mget`, `mset`, `incr`, `decr`, `r#type`
- `set.rs`: 所有 SADD/SREM/SMEMBERS 等方法
- `sorted_set.rs`: 所有 ZADD/ZREM/ZRANGE 等方法
- `ttl.rs`: 所有 EXPIRE/TTL/PERSIST 等方法

---

### 步骤 5：适配分布式锁模块

**`lock/manager.rs` 变更模式：**
所有 `get_connection().await?` → `get_connection()`

涉及方法：
- `try_lock_with_value`、`unlock_with_value`、`unlock`
- `extend_with_value`、`extend`
- `is_locked`、`remaining_ttl`
- `LockGuard::remaining_ttl`、`LockGuard::start_auto_renew_task`

---

### 步骤 6：重写 `cache/pubsub.rs` — 增加心跳和集群支持

**核心改动：使用 `split()` 替代 `on_message()`**

当前代码使用 `pubsub.on_message()` 获取消息流，但 `on_message()` 借用了 `&mut pubsub`，导致无法同时调用 `ping()`。改用 `split()` 拆分为独立的 sink/stream：

```rust
let (sink, stream) = pubsub.split();
```

**心跳机制实现：**

1. 创建 `PubSub` 连接后，调用 `split()` 获取 `(PubSubSink, PubSubStream)`
2. 通过 `sink.subscribe(channels)` 订阅频道
3. 启动**心跳任务**：每隔 `heartbeat_interval` 调用 `sink.ping()` 保活
4. 启动**消息读取任务**：从 `stream`（`impl Stream<Item=Msg>`）读取消息并通过 `mpsc::channel` 转发给用户
5. 心跳失败时：记录警告日志，触发重连流程

**心跳任务伪代码：**
```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(heartbeat_interval);
    loop {
        interval.tick().await;
        if sink.ping::<String>().await.is_err() {
            tracing::warn!("PubSub 心跳失败，连接可能已断开");
            break;
        }
    }
});
```

**消息转发任务伪代码：**
```rust
tokio::spawn(async move {
    let mut stream = Box::pin(stream);
    while let Some(msg) = stream.next().await {
        let channel = msg.get_channel_name().to_string();
        if let Ok(payload) = msg.get_payload::<String>() {
            if tx.send(PubSubMessage { channel, payload }).await.is_err() {
                break;
            }
        }
    }
});
```

**新增 `SubscriberBuilder` 构建器：**
```rust
pub struct SubscriberBuilder {
    url: String,
    channels: Vec<String>,
    patterns: Vec<String>,
    heartbeat_interval: Duration,
}

// 使用方式
let subscriber = SubscriberBuilder::new("redis://127.0.0.1:6379")
    .channels(vec!["channel1".to_string()])
    .heartbeat_interval(Duration::from_secs(30))
    .build()
    .await?;
```

**保留向后兼容：**
- `Subscriber::new(url, channels)` 仍可用，默认心跳 30 秒
- `Subscriber::with_patterns(url, patterns)` 仍可用

**Subscriber 结构体重构：**
```rust
pub struct Subscriber {
    rx: mpsc::Receiver<PubSubMessage>,
    heartbeat_handle: tokio::task::JoinHandle<()>,
    msg_handle: tokio::task::JoinHandle<()>,
}
```

---

### 步骤 7：修改 `cache/mod.rs` 和 `lib.rs` — 导出更新

**`lib.rs` 新增导出：**
- `RedisMode`
- `RedisConnectionRef`
- `SubscriberBuilder`

**`cache/mod.rs`** 新增导出 `SubscriberBuilder`

---

### 步骤 8：验证编译

1. 运行 `rtk cargo check` 确保编译通过
2. 运行 `rtk cargo clippy` 检查代码质量
3. 运行 `rtk cargo test` 确保单元测试通过

---

## 五、关键设计决策

| 决策点 | 选择 | 原因 |
|--------|------|------|
| 连接池 vs 多路复用 | **多路复用** | redis-rs 文档明确说明 `MultiplexedConnection` 线程安全、可低成本克隆，无需连接池 |
| 单机连接类型 | `ConnectionManager` | 自带断线重连，生产环境必备；`Clone` 成本低 |
| 集群连接类型 | `ClusterConnection<MultiplexedConnection>` | redis-rs 标准做法，内部管理多节点连接，自带路由和重连 |
| 连接抽象方式 | 枚举 + `ConnectionLike` trait 实现 | 零运行时开销，编译期分发，所有 `query_async` 调用无需修改 |
| Pub/Sub 心跳方式 | `split()` + 定时 `sink.ping()` | redis-rs 官方支持的方式，sink 和 stream 独立运行 |
| 向后兼容 | 保留旧 API 签名 | 降低调用方修改成本 |

## 六、风险和注意事项

1. **`RedisConnectionRef` 生命周期**：`req_packed_command` 返回 `RedisFuture<'a, Value>` 即 `Pin<Box<dyn Future + 'a + Send>>`，枚举 match 分发时两个分支返回相同类型，无问题
2. **集群模式下部分命令受限**：如 `KEYS *`、`FLUSHALL` 等全局命令在集群模式下行为不同，需在文档中说明
3. **集群 Pub/Sub 需要 RESP3**：集群 pub/sub 功能需要 RESP3 协议支持，当前版本先只支持单机模式的 Pub/Sub 心跳
4. **心跳任务的取消**：Subscriber drop 时通过 `JoinHandle::abort()` 正确取消心跳任务
5. **`pool_size` 配置**：移除 bb8 后 `pool_size` 字段不再有实际意义，保留但标注为 deprecated 或移除
