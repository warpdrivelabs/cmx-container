# cmx-buffer

> 基于 Redis 实现的缓存和分布式锁管理模块，提供高效、安全、易用的缓存访问接口和分布式锁机制。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

## 项目简介

cmx-buffer 是 cmx-container 项目的缓存和锁管理层，基于 Redis 实现，支持单机和集群两种模式，提供缓存操作、分布式锁、发布订阅等功能。

## 特性

- **多模式支持**：支持单机和集群两种 Redis 部署模式
- **高性能连接**：基于 redis-rs 原生异步多路复用连接（单机 ConnectionManager / 集群 ClusterConnection，均自带断线重连，无需外部连接池）
- **分布式锁**：支持原子获取、自动续期（看门狗）、安全释放
- **发布订阅**：GlobalSubscriber 回调式订阅，心跳保活 + 断线自动重连重订阅
- **丰富操作**：支持字符串、哈希、集合、有序集合、TTL、Lua 脚本等常用 Redis 能力
- **测试友好**：内置 Mock Redis 后端（HashMap 模拟，支持分布式锁单测）
- **Wasm host**：BufferHostFunctions 供 WebAssembly 插件经 host 调用缓存能力

## 快速开始

### 安装

```toml
[dependencies]
cmx-buffer = { workspace = true }
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
│   ├── lib.rs              # 库入口（create_redis_client 等便捷函数）
│   ├── cache/              # 缓存操作模块
│   │   ├── mod.rs          # CacheManager + GlobalCacheManager 全局单例
│   │   ├── ops.rs          # 缓存操作接口（字符串/序列化）
│   │   ├── hash.rs         # 哈希表操作（HashOps）
│   │   ├── pubsub.rs       # 发布/订阅 + GlobalSubscriber / ChannelHandler
│   │   ├── script.rs       # Lua 脚本操作（ScriptOps）
│   │   ├── set.rs          # 集合操作
│   │   ├── sorted_set.rs   # 有序集合操作
│   │   └── ttl.rs          # TTL 操作
│   ├── client.rs           # Redis 客户端封装 + GlobalRedisClient / SharedRedisClient
│   ├── config.rs           # 配置结构定义（RedisConfig / CacheConfig / LockConfig）
│   ├── error.rs            # 错误类型定义
│   ├── host_functions.rs   # Wasm host 函数（BufferHostFunctions）
│   ├── lock/               # 分布式锁模块
│   │   ├── mod.rs          # GlobalLockManager + create_lock_manager 工厂
│   │   └── manager.rs      # LockManager / LockGuard / LockOptions / LockConfig
│   ├── logging.rs          # 日志记录工具
│   └── mock.rs             # 测试用 Mock Redis 后端（MockRedisBackend）
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

### 6.2 订阅频道（回调式）

`GlobalSubscriber` 使用独立订阅连接（单机 / 集群均支持，RESP3 + push_sender
统一接收），消息按频道自动路由到注册的处理器；断线时由后台分发任务自动重建
连接并重新订阅（显式销毁旧连接，确保无重复订阅）。

```rust
use std::sync::Arc;
use cmx_buffer::{ChannelHandler, GlobalSubscriber, RedisClient};

async fn subscribe_channels(client: &RedisClient) -> cmx_buffer::Result<()> {
    let subscriber = GlobalSubscriber::new(client).await?;

    // 方式一：闭包处理器（内部包装为 FnChannelHandler）
    subscriber
        .register_channel_fn("news", |channel, payload| {
            println!("[{}] {}", channel, payload);
        })
        .await?;

    // 方式二：实现 ChannelHandler trait 的结构体处理器
    struct AlertHandler;
    #[async_trait::async_trait]
    impl ChannelHandler for AlertHandler {
        async fn handle(&self, channel: &str, payload: &str) {
            println!("[alert:{}] {}", channel, payload);
        }
    }
    subscriber
        .register_channel("alerts", Arc::new(AlertHandler))
        .await?;

    // 模式订阅（通配符）与取消订阅
    subscriber.register_pattern("user:*", Arc::new(AlertHandler)).await?;
    subscriber.unregister_channel("news").await?;

    Ok(())
}
```

> **心跳与重连**：订阅连接按 `RedisConfig.heartbeat_interval`（默认 30 秒，可用
> `with_heartbeat_interval` 调整）定时 PING 保活；心跳失败或连接断开时自动重连重订阅，
> 处理器无需重新注册。长时间运行的订阅服务建议保持心跳开启。

**使用 GlobalSubscriberManager 全局单例：**

```rust
use cmx_buffer::GlobalSubscriberManager;

async fn init_global_subscriber() -> cmx_buffer::Result<()> {
    // 应用启动时初始化一次（创建 GlobalSubscriber 并自动订阅预订阅频道/模式）
    GlobalSubscriberManager::initialize().await?;

    // 任意位置获取并注册处理器
    let subscriber = GlobalSubscriberManager::get();
    subscriber
        .register_channel_fn("updates", |ch, msg| {
            println!("Task: [{}] {}", ch, msg);
        })
        .await?;

    Ok(())
}
```

## 七、哈希与 Lua 脚本操作

### 7.1 哈希操作（HashOps）

通过 `cache.hash()` 获取哈希操作接口，适合存储对象字段级数据：

```rust
use cmx_buffer::CacheManager;
use std::collections::HashMap;

async fn hash_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // 设置/读取单个字段
    cache.hash().hset("user:1", "name", "Alice").await?;
    cache.hash().hset("user:1", "email", "alice@example.com").await?;
    let name: Option<String> = cache.hash().hget("user:1", "name").await?;

    // 仅当字段不存在时设置
    cache.hash().hsetnx("user:1", "name", "Bob").await?;

    // 读取全部字段 / 仅键 / 仅值 / 字段数
    let all: HashMap<String, String> = cache.hash().hgetall("user:1").await?;
    let keys: Vec<String> = cache.hash().hkeys("user:1").await?;
    let vals: Vec<String> = cache.hash().hvals("user:1").await?;
    let len: u64 = cache.hash().hlen("user:1").await?;

    // 批量设置 / 批量读取
    let items: HashMap<&str, &str> = HashMap::from([("city", "BJ"), ("age", "30")]);
    cache.hash().hmset("user:1", &items).await?;
    let values: Vec<Option<String>> = cache.hash().hmget("user:1", &["city", "age"]).await?;

    // 字段存在性 / 自增 / 删除字段
    let exists: bool = cache.hash().hexists("user:1", "age").await?;
    let age: i64 = cache.hash().hincrby("user:1", "age", 1).await?;
    let removed: u64 = cache.hash().hdel("user:1", &["city", "age"]).await?;

    Ok(())
}
```

### 7.2 Lua 脚本操作（ScriptOps）

通过 `cache.script()` 执行 Lua 脚本，保证多命令原子性（cmx-auth 的 Refresh Rotation
等即基于此实现）：

```rust
use cmx_buffer::CacheManager;

async fn script_operations(cache: &CacheManager) -> cmx_buffer::Result<()> {
    // EVAL：直接执行脚本（keys 与 args 分开传）
    let script = r"
        local current = redis.call('GET', KEYS[1])
        if current then return current end
        redis.call('SET', KEYS[1], ARGV[1])
        return ARGV[1]
    ";
    let result = cache.script().eval(script, &["lock:key"], &["owner-1"]).await?;

    // SCRIPT LOAD + EVALSHA：预加载脚本，之后用 SHA1 执行，节省带宽
    let sha1: String = cache.script().script_load(script).await?;
    let result = cache.script().evalsha(&sha1, &["lock:key"], &["owner-2"]).await?;

    // 检查脚本是否已缓存在服务端
    let cached: Vec<bool> = cache.script().script_exists(&[&sha1]).await?;

    Ok(())
}
```

> 另有 `eval_with_fallback`：优先 `EVALSHA` 执行，遇到 `NOSCRIPT` 错误时自动回退重新 `EVAL`。

## 八、分布式锁

> 详细文档请参阅 [DISTRIBUTED_LOCK.md](./DISTRIBUTED_LOCK.md)

### 8.1 核心概念

cmx-buffer 分布式锁参考 Redisson 设计，提供 RAII 自动释放 + 可选看门狗自动续期机制：

| 方法 | 等待行为 | 返回值 | 看门狗 |
|------|----------|--------|--------|
| `lock(key)` | 无限等待 | `Result<LockGuard>` | 启用 |
| `lock_with_options(key, opts)` | 无限等待 | `Result<LockGuard>` | 由 `lease_time` 控制 |
| `try_lock(key)` | 不等待 | `Result<Option<LockGuard>>` | 启用 |
| `try_lock_with_options(key, opts)` | 可控 | `Result<Option<LockGuard>>` | 由 `lease_time` 控制 |

### 8.2 基础使用

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

### 8.3 阻塞式获取锁（带重试）

```rust
use cmx_buffer::LockManager;

async fn blocking_lock(lock_manager: &LockManager) -> cmx_buffer::Result<()> {
    // 阻塞式获取：失败后按 retry_interval（默认 200ms）无限重试，直到获取成功
    let _guard = lock_manager.lock("my_resource").await?;
    println!("锁获取成功，看门狗自动续期已开启");

    // 执行长时间任务（锁会自动续期）
    run_long_task().await?;

    // _guard Drop 时自动释放
    Ok(())
}
```

### 8.4 自定义锁选项

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

### 8.5 锁配置

```rust
use cmx_buffer::{LockConfig, RedisClient, LockManager, RedisConfig};

async fn configure_lock() -> cmx_buffer::Result<()> {
    // LockConfig 字段：expire_seconds（锁过期秒数，默认 30）/
    // retry_interval_ms（重试间隔毫秒，默认 200）/
    // renew_threshold（看门狗续期阈值比例，默认 0.3）
    let lock_config = LockConfig::new()
        .with_expire(60)            // 锁过期时间（秒），默认 30
        .with_retry_interval(500);  // 重试间隔（毫秒），默认 200

    let redis_config = RedisConfig::new("redis://127.0.0.1:6379");
    let client = RedisClient::new(redis_config).await?;
    let lock_manager = LockManager::new(client, lock_config);

    let _guard = lock_manager.lock("resource").await?;
    Ok(())
}
```

## 九、错误处理

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
                Error::PoolError(msg) => {
                    eprintln!("连接池错误: {}", msg);
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
                Error::KeyTypeError(msg) => {
                    eprintln!("键类型不匹配: {}", msg);
                }
                Error::ConfigError(msg) => {
                    eprintln!("配置错误: {}", msg);
                }
                Error::PubSubError(msg) => {
                    eprintln!("Pub/Sub 错误: {}", msg);
                }
                Error::UnknownError(msg) => {
                    eprintln!("未知错误: {}", msg);
                }
            }
        }
    }

    Ok(())
}
```

## 十、全局单例

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

## 十一、完整示例

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
