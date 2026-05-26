# cmx-buffer

> 基于 Redis 实现的缓存和分布式锁管理模块，提供高效、安全、易用的缓存访问接口和分布式锁机制。

## 项目简介

cmx-buffer 是 cmx-container 项目的缓存和锁管理层，基于 Redis 实现，支持单机和集群两种模式，提供缓存操作、分布式锁、发布订阅等功能。

## 特性

- **多模式支持**：支持单机和集群两种 Redis 部署模式
- **高性能连接**：基于 redis-rs 原生异步多路复用连接（无需外部连接池）
- **分布式锁**：支持原子获取、自动续期、安全释放
- **发布订阅**：支持心跳保活机制，避免连接静默断开
- **丰富操作**：支持字符串、集合、有序集合、TTL 等常用 Redis 数据结构

## 快速开始

### 安装

```toml
[dependencies]
cmx-buffer = "0.1.0"
```

### 基础示例

```rust
use cmx_buffer::{create_redis_client, CacheManager};

let client = create_redis_client("redis://127.0.0.1:6379").await?;
let cache = CacheManager::new(client);

cache.ops().set("key", "value").await?;
let value: Option<String> = cache.ops().get("key").await?;
```

## 模块结构

```
cmx-buffer
├── src/
│   ├── lib.rs              # 库入口
│   ├── cache/              # 缓存操作模块
│   │   ├── ops.rs          # 缓存操作接口
│   │   ├── pubsub.rs       # 发布/订阅功能
│   │   ├── set.rs          # 集合操作
│   │   ├── sorted_set.rs   # 有序集合操作
│   │   └── ttl.rs          # TTL 操作
│   ├── client.rs           # Redis 客户端封装
│   ├── config.rs           # 配置结构定义
│   ├── error.rs            # 错误类型定义
│   ├── lock/               # 分布式锁模块
│   │   └── manager.rs      # 锁管理器
│   └── logging.rs          # 日志记录工具
└── Cargo.toml
```

## 一、Redis 配置

### 1.1 单机模式

单机模式是最简单的部署方式，适用于大多数使用场景。

**通过 URL 创建（最简单）：**

```rust
use cmx_buffer::RedisConfig;

let config = RedisConfig::new("redis://127.0.0.1:6379");
let client = RedisClient::new(config).await?;
```

**通过 Builder 模式配置：**

```rust
use cmx_buffer::{RedisConfig, RedisMode};

let config = RedisConfig::new("redis://127.0.0.1:6379")
    .with_key_prefix("app:")                      // 键前缀，默认 "cmx:"
    .with_connection_timeout(5)                   // 连接超时（秒），默认 5
    .with_operation_timeout(3)                    // 操作超时（秒），默认 3
    .with_heartbeat_interval(30);                 // Pub/Sub 心跳间隔（秒），默认 30

let client = RedisClient::new(config).await?;
```

**通过配置文件读取：**

```toml
[redis]
url = "redis://127.0.0.1:6379"
mode = "standalone"
key_prefix = "myapp:"
connection_timeout = 5
operation_timeout = 3
heartbeat_interval = 30
```

### 1.2 集群模式

集群模式适用于需要高可用和水平扩展的生产环境。

**配置集群节点：**

```rust
use cmx_buffer::{RedisConfig, RedisMode};

let config = RedisConfig::new_cluster(vec![
    "redis://192.168.1.10:6379".to_string(),
    "redis://192.168.1.11:6379".to_string(),
    "redis://192.168.1.12:6379".to_string(),
])
.with_key_prefix("cluster:");

let client = RedisClient::new(config).await?;
```

`RedisConfig::new_cluster()` 会自动设置 `mode = RedisMode::Cluster`，`url` 字段使用第一个节点地址。

**通过配置文件读取集群配置：**

```toml
[redis]
url = "redis://192.168.1.10:6379"
mode = "cluster"
cluster_urls = "redis://192.168.1.10:6379,redis://192.168.1.11:6379,redis://192.168.1.12:6379"
key_prefix = "cluster:"
```

> **注意**：集群模式下 `cluster_urls` 至少需要包含一个有效的节点地址。客户端会自动通过 `CLUSTER SLOTS` 命令发现其他节点。

### 1.3 单机 vs 集群模式对比

| 特性 | 单机模式 | 集群模式 |
|------|----------|----------|
| 部署复杂度 | 低 | 高 |
| 适用场景 | 开发、测试、小规模生产 | 大规模生产、高可用 |
| 数据分片 | 不支持 | 自动分片（16384 槽位） |
| 故障转移 | 手动 | 自动 |
| 键前缀 | 支持 | 支持 |
| 连接类型 | `ConnectionManager` | `ClusterConnection` |
| 适用规模 | 单节点 | 多节点集群 |

### 1.4 键前缀说明

所有键操作会自动添加前缀，便于在同一个 Redis 实例中隔离不同应用的数据：

```rust
let config = RedisConfig::new("redis://127.0.0.1:6379")
    .with_key_prefix("myapp:");

let client = RedisClient::new(config).await?;
let cache = CacheManager::new(client);

cache.ops().set("user:1", "Alice").await?;
// 实际存储的键为：myapp:user:1
```

## 二、缓存管理器

### 2.1 初始化

```rust
use cmx_buffer::{RedisClient, CacheManager, RedisConfig};

async fn init_cache() -> cmx_buffer::Result<CacheManager> {
    let config = RedisConfig::new("redis://127.0.0.1:6379")
        .with_key_prefix("app:");
    let client = RedisClient::new(config).await?;
    let cache = CacheManager::new(client);
    Ok(cache)
}
```

### 2.2 字符串操作

```rust
use cmx_buffer::CacheManager;

async fn string_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // 设置单个键值
    cache.ops().set("key1", "value1").await?;

    // 设置带 TTL 的键值（秒）
    cache.ops().set_ex("key2", "value2", std::time::Duration::from_secs(3600)).await?;

    // 获取值
    let value: Option<String> = cache.ops().get("key1").await?;
    println!("key1 = {:?}", value);

    // 批量获取
    let values: Vec<Option<String>> = cache.ops().mget(&["key1", "key2"]).await?;

    // 删除键
    let deleted = cache.ops().del("key1").await?;
    println!("Deleted: {}", deleted);

    // 批量删除
    cache.ops().del_batch(&["key2", "key3"]).await?;

    // 键是否存在
    let exists = cache.ops().exists("key1").await?;

    // 自增
    cache.ops().set("counter", "0").await?;
    let new_val: i64 = cache.ops().incr("counter", 1).await?;
    println!("Counter: {}", new_val);

    // 自减
    let new_val: i64 = cache.ops().decr("counter", 2).await?;

    Ok(())
}
```

### 2.3 序列化操作

支持 JSON 序列化，适合存储复杂结构：

```rust
use cmx_buffer::CacheManager;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct User {
    name: String,
    email: String,
}

async fn serialized_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    let user = User {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };

    // 自动序列化为 JSON
    cache.ops().set_serialized("user:1", &user).await?;

    // 自动反序列化
    let loaded: Option<User> = cache.ops().get_deserialized("user:1").await?;
    println!("User: {:?}", loaded);

    Ok(())
}
```

## 三、TTL 操作

```rust
use cmx_buffer::CacheManager;
use std::time::Duration;

async fn ttl_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // 设置带 TTL 的键
    cache.ops().set_ex("temp_key", "temp_value", Duration::from_secs(60)).await?;

    // 获取 TTL（秒）
    let ttl: Option<std::time::Duration> = cache.ttl().ttl("temp_key").await?;
    println!("TTL: {:?}", ttl);

    // 设置 TTL
    cache.ttl().expire("temp_key", Duration::from_secs(120)).await?;

    // 移除 TTL（永不过期）
    cache.ttl().persist("temp_key").await?;

    // 获取精确 TTL（毫秒）
    let pttl: Option<std::time::Duration> = cache.ttl().pttl("temp_key").await?;

    // 设置带 TTL 的值（组合操作）
    cache.ttl().set_with_ttl("session:abc", "data", Duration::from_secs(1800)).await?;

    // 仅当不存在时设置
    cache.ttl().setnx("unique_key", "value").await?;
    cache.ttl().setnx_ex("unique_key_with_ttl", "value", Duration::from_secs(60)).await?;

    Ok(())
}
```

## 四、集合操作

```rust
use cmx_buffer::CacheManager;

async fn set_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // 添加元素
    cache.set().sadd_one("my_set", "member1").await?;
    cache.set().sadd("my_set", &["member2", "member3"]).await?;

    // 获取集合所有成员
    let members: Vec<String> = cache.set().smembers("my_set").await?;

    // 检查元素是否在集合中
    let is_member = cache.set().sismember("my_set", "member1").await?;

    // 批量检查
    let results: Vec<bool> = cache.set().smismember("my_set", &["member1", "member4"]).await?;

    // 获取集合基数
    let count: i64 = cache.set().scard("my_set").await?;

    // 随机获取
    let random: Vec<String> = cache.set().srandmember_count("my_set", 2).await?;

    // 随机弹出
    let popped: Option<String> = cache.set().spop("my_set").await?;

    // 移除元素
    cache.set().srem_one("my_set", "member1").await?;
    cache.set().srem("my_set", &["member2", "member3"]).await?;

    // 集合运算
    cache.set().sadd("set_a", &["a", "b", "c"]).await?;
    cache.set().sadd("set_b", &["b", "c", "d"]).await?;

    let diff: Vec<String> = cache.set().sdiff(&["set_a", "set_b"]).await?; // [a]
    let sinter: Vec<String> = cache.set().sinter(&["set_a", "set_b"]).await?; // [b, c]
    let sunion: Vec<String> = cache.set().sunion(&["set_a", "set_b"]).await?; // [a, b, c, d]

    // 集合运算并存储
    cache.set().sdiffstore("result", &["set_a", "set_b"]).await?;
    cache.set().sinterstore("result", &["set_a", "set_b"]).await?;
    cache.set().sunionstore("result", &["set_a", "set_b"]).await?;

    // 移动元素
    cache.set().smove("set_a", "set_b", "a").await?;

    Ok(())
}
```

## 五、有序集合操作

```rust
use cmx_buffer::CacheManager;

async fn sorted_set_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // 添加元素（带分数）
    cache.sorted_set().zadd_one("leaderboard", "player1", 100.0).await?;
    cache.sorted_set().zadd("leaderboard", &[("player2", 85.0), ("player3", 92.0)]).await?;

    // 按排名范围获取（从小到大）
    let members: Vec<String> = cache.sorted_set().zrange("leaderboard", 0, -1).await?;

    // 按排名范围获取（带分数）
    let with_scores: Vec<(String, f64)> = cache.sorted_set()
        .zrange_with_scores("leaderboard", 0, -1)
        .await?;

    // 按分数范围获取
    let in_range: Vec<String> = cache.sorted_set()
        .zrangebyscore("leaderboard", 80.0, 100.0)
        .await?;

    // 带分数和 LIMIT
    let limited: Vec<(String, f64)> = cache.sorted_set()
        .zrangebyscore_limit("leaderboard", 0.0, 100.0, 0, 10)
        .await?;

    // 获取排名（0 = 最低分）
    let rank: Option<i64> = cache.sorted_set().zrank("leaderboard", "player1").await?;

    // 获取逆序排名（0 = 最高分）
    let rev_rank: Option<i64> = cache.sorted_set().zrevrank("leaderboard", "player1").await?;

    // 获取分数
    let score: Option<f64> = cache.sorted_set().zscore("leaderboard", "player1").await?;

    // 增加分数
    cache.sorted_set().zincrby("leaderboard", "player1", 10.0).await?;

    // 获取集合基数
    let count: i64 = cache.sorted_set().zcard("leaderboard").await?;

    // 获取分数在范围内的成员数
    let count: i64 = cache.sorted_set().zcount("leaderboard", 80.0, 100.0).await?;

    // 移除元素
    cache.sorted_set().zrem_one("leaderboard", "player1").await?;
    cache.sorted_set().zrem("leaderboard", &["player2", "player3"]).await?;

    // 按排名范围移除
    cache.sorted_set().zremrangebyrank("leaderboard", 0, 0).await?; // 移除最低分

    // 按分数范围移除
    cache.sorted_set().zremrangebyscore("leaderboard", 0.0, 50.0).await?;

    // 弹出一个或多个最低/最高分
    let min_members: Vec<(String, f64)> = cache.sorted_set().zpopmin("leaderboard", 1).await?;
    let max_members: Vec<(String, f64)> = cache.sorted_set().zpopmax("leaderboard", 1).await?;

    // 集合运算
    cache.sorted_set().zunionstore("result", &["set1", "set2"]).await?;
    cache.sorted_set().zinterstore("result", &["set1", "set2"]).await?;

    Ok(())
}
```

## 六、发布订阅

### 6.1 发布消息

```rust
use cmx_buffer::CacheManager;

async fn publish_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // 发布到频道
    let subscribers = cache.pubsub().publish("news", "Breaking news!").await?;
    println!("Subscribers: {}", subscribers);

    // 发布 JSON 消息
    let data = serde_json::json!({"type": "alert", "msg": "Test"});
    cache.pubsub().publish_json("notifications", &data).await?;

    // 获取匹配模式的频道
    let channels: Vec<String> = cache.pubsub().pubsub_channels(Some("news:*")).await?;

    // 获取频道订阅者数量
    let numsub: Vec<(String, u64)> = cache.pubsub().pubsub_numsub(&["news", "sports"]).await?;

    // 获取模式订阅数量
    let numpat: u64 = cache.pubsub().pubsub_numpat().await?;

    Ok(())
}
```

### 6.2 订阅频道（带心跳）

`Subscriber` 使用独立连接订阅频道，支持心跳保活，避免长时间无消息时连接被断开。

```rust
use cmx_buffer::{Subscriber, PubSubMessage};
use std::time::Duration;

async fn subscribe_channels() -> cmx_buffer::Result<()> {
    let mut subscriber = Subscriber::new(
        "redis://127.0.0.1:6379",
        vec!["news".to_string(), "sports".to_string()],
    ).await?;

    println!("已订阅 news 和 sports 频道");

    // 接收消息
    while let Some(msg) = subscriber.recv().await {
        println!("[{}] {}", msg.channel, msg.payload);
    }

    Ok(())
}
```

**使用 SubscriberBuilder 精细控制：**

```rust
use cmx_buffer::{SubscriberBuilder, PubSubMessage};
use std::time::Duration;

async fn advanced_subscribe() -> cmx_buffer::Result<()> {
    let mut subscriber = SubscriberBuilder::new("redis://127.0.0.1:6379")
        .channels(vec!["news".to_string()])          // 订阅的频道
        .patterns(vec!["user:*".to_string()])         // 订阅的模式
        .heartbeat_interval(Duration::from_secs(20))  // 心跳间隔（秒），默认 30
        .build()
        .await?;

    while let Some(msg) = subscriber.recv().await {
        println!("[{}] {}", msg.channel, msg.payload);
    }

    Ok(())
}
```

> **心跳机制**：Subscriber 内部会定时（默认 30 秒）向 Redis 发送 PING 命令保活。如果心跳失败，会自动断开连接并终止消息接收任务。长时间运行的订阅服务建议开启心跳。

**使用 SharedSubscriber 跨任务共享订阅：**

```rust
use cmx_buffer::SharedSubscriber;

async fn shared_subscribe() -> cmx_buffer::Result<()> {
    let subscriber = SharedSubscriber::new(
        "redis://127.0.0.1:6379",
        vec!["updates".to_string()],
    ).await?;

    // clone 跨任务使用
    let sub2 = subscriber.clone();
    tokio::spawn(async move {
        while let Some(msg) = sub2.recv().await {
            println!("Task2: [{}] {}", msg.channel, msg.payload);
        }
    });

    while let Some(msg) = subscriber.recv().await {
        println!("Task1: [{}] {}", msg.channel, msg.payload);
    }

    Ok(())
}
```

## 七、分布式锁

> 详细文档请参阅 [DISTRIBUTED_LOCK.md](./DISTRIBUTED_LOCK.md)

### 7.1 核心概念

cmx-buffer 分布式锁参考 Redisson 设计，提供 RAII 自动释放 + 可选看门狗自动续期机制：

| 方法 | 等待行为 | 返回值 | 看门狗 |
|------|----------|--------|--------|
| `lock(key)` | 无限等待 | `Result<LockGuard>` | 启用 |
| `lock_with_options(key, opts)` | 无限等待 | `Result<LockGuard>` | 由 `lease_time` 控制 |
| `try_lock(key)` | 不等待 | `Result<Option<LockGuard>>` | 启用 |
| `try_lock_with_options(key, opts)` | 可控 | `Result<Option<LockGuard>>` | 由 `lease_time` 控制 |

### 7.2 基础使用

```rust
use cmx_buffer::{RedisClient, LockManager, RedisConfig};

async fn basic_lock_usage() -> cmx_buffer::Result<()> {
    let config = RedisConfig::new("redis://127.0.0.1:6379");
    let client = RedisClient::new(config).await?;
    let lock_manager = LockManager::new_with_default_config(client);

    // 非阻塞获取锁
    match lock_manager.try_lock("my_resource").await {
        Ok(Some(_guard)) => {
            println!("获取锁成功");
            // 执行关键操作...
            // _guard Drop 时自动释放锁，无需手动 unlock
        }
        Ok(None) => println!("锁已被占用"),
        Err(e) => println!("锁服务异常: {}", e),
    }

    Ok(())
}
```

### 7.3 阻塞式获取锁（带重试）

```rust
use cmx_buffer::LockManager;

async fn blocking_lock(lock_manager: &LockManager) -> cmx_buffer::Result<()> {
    // 自动重试获取锁（默认重试 3 次，间隔 200ms）
    let _guard = lock_manager.lock("my_resource").await?;
    println!("锁获取成功，看门狗自动续期已开启");

    // 执行长时间任务（锁会自动续期）
    run_long_task().await?;

    // _guard Drop 时自动释放
    Ok(())
}
```

### 7.4 自定义锁选项

```rust
use cmx_buffer::{LockManager, LockOptions};
use std::time::Duration;

async fn custom_options(lock_manager: &LockManager) -> cmx_buffer::Result<()> {
    // lock + leaseTime：无限等待，锁只持有 10 秒（禁用看门狗）
    let _guard = lock_manager
        .lock_with_options("short_task", LockOptions::new()
            .with_lease_time(Duration::from_secs(10)))
        .await?;

    // tryLock + waitTime：最多等 5 秒，获取不到就放弃
    match lock_manager
        .try_lock_with_options("order_task", LockOptions::new()
            .with_wait_time(Duration::from_secs(5)))
        .await?
    {
        Some(_guard) => tracing::info!("5 秒内获取锁成功"),
        None => tracing::warn!("等待 5 秒未获取锁，放弃"),
    }

    // tryLock + waitTime + leaseTime：最多等 3 秒，锁持有 10 秒
    let _guard = lock_manager
        .try_lock_with_options("quick_task", LockOptions::new()
            .with_wait_time(Duration::from_secs(3))
            .with_lease_time(Duration::from_secs(10)))
        .await?;

    Ok(())
}
```

### 7.5 锁配置

```rust
use cmx_buffer::{LockConfig, RedisClient, LockManager, RedisConfig};

async fn configure_lock() -> cmx_buffer::Result<()> {
    let lock_config = LockConfig::new()
        .with_expire(60)            // 锁过期时间（秒），默认 30
        .with_retry_times(5)        // 重试次数，默认 3
        .with_retry_interval(200);  // 重试间隔（毫秒），默认 200

    let redis_config = RedisConfig::new("redis://127.0.0.1:6379");
    let client = RedisClient::new(redis_config).await?;
    let lock_manager = LockManager::new(client, lock_config);

    let _guard = lock_manager.lock("resource").await?;
    Ok(())
}
```

## 八、错误处理

```rust
use cmx_buffer::{Error, Result};

async fn handle_errors() -> Result<()> {
    // 缓存操作可能返回的错误类型
    let value = cache.ops().get("key").await;

    match value {
        Ok(Some(v)) => println!("Value: {}", v),
        Ok(None) => println!("Key not found"),
        Err(e) => {
            match e {
                Error::ConnectionError(msg) => {
                    eprintln!("Redis 连接失败: {}", msg);
                }
                Error::TimeoutError(msg) => {
                    eprintln!("操作超时: {}", msg);
                }
                Error::OperationError(msg) => {
                    eprintln!("操作失败: {}", msg);
                }
                Error::LockError(msg) => {
                    eprintln!("锁错误: {}", msg);
                }
                Error::LockConflictError(msg) => {
                    eprintln!("锁冲突: {}", msg);
                }
                Error::SerializeError(msg) => {
                    eprintln!("序列化失败: {}", msg);
                }
                Error::PubSubError(msg) => {
                    eprintln!("Pub/Sub 错误: {}", msg);
                }
                _ => {
                    eprintln!("未知错误: {}", e);
                }
            }
        }
    }

    Ok(())
}
```

## 九、全局单例

cmx-buffer 提供全局单例管理器，方便在应用各处访问缓存和锁：

```rust
use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisConfig};

async fn init_global() -> cmx_buffer::Result<()> {
    let redis_config = RedisConfig::new("redis://127.0.0.1:6379")
        .with_key_prefix("app:");

    // 初始化全局缓存管理器
    GlobalCacheManager::initialize(redis_config.clone()).await?;

    // 初始化全局锁管理器
    GlobalLockManager::initialize(redis_config).await?;

    // 在应用任意位置访问
    let cache = GlobalCacheManager::get();
    cache.ops().set("key", "value").await?;

    Ok(())
}
```

## 十、完整示例

```rust
use cmx_buffer::{
    RedisConfig, RedisClient, CacheManager, LockManager,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> cmx_buffer::Result<()> {
    // 1. 配置并创建客户端（单机模式）
    let config = RedisConfig::new("redis://127.0.0.1:6379")
        .with_key_prefix("demo:")
        .with_heartbeat_interval(30);

    let client = RedisClient::new(config).await?;
    println!("已连接到 Redis");

    // 2. 创建缓存管理器
    let cache = CacheManager::new(client.clone());

    // 3. 基本缓存操作
    cache.ops().set("user:1", "Alice").await?;
    cache.ops().set_ex("session:abc", "xyz", Duration::from_secs(3600)).await?;

    let user = cache.ops().get("user:1").await?;
    println!("User: {:?}", user);

    // 4. 使用分布式锁保护关键操作
    let lock_manager = LockManager::new_with_default_config(client.clone());
    match lock_manager.try_lock("order:12345").await {
        Ok(Some(_guard)) => {
            println!("已获取锁，正在处理订单...");
            process_order(12345).await?;
            println!("订单处理完成");
            // _guard Drop 自动释放锁
        }
        Ok(None) => println!("订单正在被其他实例处理"),
        Err(e) => println!("锁服务异常: {}", e),
    }

    // 5. 集合操作
    cache.set().sadd("online_users", &["alice", "bob", "charlie"]).await?;
    let users: Vec<String> = cache.set().smembers("online_users").await?;
    println!("在线用户: {:?}", users);

    // 6. 有序集合（排行榜）
    cache.sorted_set().zadd("leaderboard", "Alice", 1500.0).await?;
    cache.sorted_set().zadd("leaderboard", "Bob", 1200.0).await?;
    cache.sorted_set().zadd("leaderboard", "Charlie", 1800.0).await?;

    let top3: Vec<(String, f64)> = cache.sorted_set()
        .zrange_with_scores("leaderboard", 0, 2)
        .await?;
    println!("排行榜前三: {:?}", top3);

    println!("所有操作完成");
    Ok(())
}

async fn process_order(order_id: i64) -> cmx_buffer::Result<()> {
    println!("正在处理订单 #{}", order_id);
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("订单 #{} 处理完成", order_id);
    Ok(())
}
```

**集群模式示例：**

```rust
use cmx_buffer::{RedisConfig, RedisClient, CacheManager};

#[tokio::main]
async fn main() -> cmx_buffer::Result<()> {
    // 创建集群配置
    let config = RedisConfig::new_cluster(vec![
        "redis://192.168.1.10:6379".to_string(),
        "redis://192.168.1.11:6379".to_string(),
        "redis://192.168.1.12:6379".to_string(),
    ])
    .with_key_prefix("cluster:")
    .with_heartbeat_interval(30);

    let client = RedisClient::new(config).await?;
    println!("已连接到 Redis 集群");

    let cache = CacheManager::new(client);

    // 集群模式下自动路由到正确的节点
    cache.ops().set("key", "value").await?;
    let value = cache.ops().get("key").await?;

    println!("Value: {:?}", value);
    Ok(())
}
```
