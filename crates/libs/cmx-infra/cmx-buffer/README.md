# cmx-buffer

> 基于 Redis 实现的缓存和分布式锁管理模块，提供高效、安全、易用的缓存访问接口和分布式锁机制。

## 项目简介

cmx-buffer 是 cmx-container 项目的缓存和锁管理层，基于 Redis 实现，提供缓存操作、分布式锁、发布订阅等功能。

## 快速开始

### 安装

```toml
[dependencies]
cmx-buffer = "0.1.0"
```

### 核心示例

```rust
use cmx_buffer::{create_redis_client, CacheManager};

let client = create_redis_client("redis://127.0.0.1:6379").await?;
let cache = CacheManager::new(client);

cache.ops().set("key", "value").await?;
let value: Option<String> = cache.ops().get("key").await?;
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 缓存操作 | 提供字符串、集合、有序集合、发布/订阅等操作 |
| 分布式锁 | 基于 Redis 的分布式锁，支持自动续期 |
| 客户端封装 | 基于 bb8 连接池的 Redis 客户端封装 |
| 配置管理 | 支持灵活的 Redis 配置、缓存配置和锁配置 |
| 连接池 | 高效的连接池管理机制 |

## 模块结构

```
cmx-buffer
├── src/
│   ├── lib.rs              # 库入口
│   ├── cache/              # 缓存操作模块
│   │   ├── mod.rs
│   │   ├── ops.rs          # 缓存操作接口
│   │   ├── pubsub.rs       # 发布/订阅功能
│   │   ├── set.rs          # 集合操作
│   │   ├── sorted_set.rs   # 有序集合操作
│   │   └── ttl.rs          # TTL 操作
│   ├── client.rs           # Redis 客户端封装
│   ├── config.rs           # 配置结构定义
│   ├── error.rs            # 错误类型定义
│   ├── host_functions.rs   # 主机函数支持
│   ├── lock/               # 分布式锁模块
│   │   └── manager.rs      # 锁管理器
│   └── logging.rs          # 日志记录工具
└── Cargo.toml
```

## 使用指南

### 一、Redis 客户端初始化

#### 1.1 基础连接

```rust
use cmx_buffer::{create_redis_client, RedisClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_redis_client("redis://127.0.0.1:6379").await?;
    println!("Connected to Redis");

    Ok(())
}
```

#### 1.2 连接池配置

```rust
use cmx_buffer::{create_redis_client_with_config, RedisConfig};

let config = RedisConfig::builder()
    .with_url("redis://127.0.0.1:6379")
    .with_pool_size(10)
    .with_timeout(5)
    .with_database(0)
    .with_password(None)
    .build();

let client = create_redis_client_with_config(config).await?;
```

#### 1.3 集群连接

```rust
use cmx_buffer::{create_redis_cluster, RedisClusterConfig};

let config = RedisClusterConfig::new(vec![
    "redis://127.0.0.1:7001",
    "redis://127.0.0.1:7002",
    "redis://127.0.0.1:7003",
]);

let client = create_redis_cluster(config).await?;
```

### 二、缓存管理器

#### 2.1 创建缓存管理器

```rust
use cmx_buffer::{create_redis_client, CacheManager};

let client = create_redis_client("redis://127.0.0.1:6379").await?;
let cache = CacheManager::new(client);
```

#### 2.2 字符串操作

```rust
use cmx_buffer::CacheManager;

async fn string_operations(cache: &CacheManager) -> Result<(), Box<dyn std::error::Error>> {
    // 设置单个键值
    cache.ops().set("key1", "value1").await?;

    // 设置带 TTL 的键值
    cache.ops().set_ex("key2", "value2", 3600).await?;

    // 设置nx（键不存在时才设置）
    cache.ops().set_nx("key3", "value3").await?;

    // 获取值
    let value: Option<String> = cache.ops().get("key1").await?;
    println!("key1 = {:?}", value);

    // 批量获取
    let values: Vec<Option<String>> = cache.ops().mget(&["key1", "key2", "key3"]).await?;

    // 删除键
    cache.ops().del("key1").await?;

    // 批量删除
    cache.ops().mdel(&["key2", "key3"]).await?;

    // 键是否存在
    let exists: bool = cache.ops().exists("key1").await?;

    Ok(())
}
```

#### 2.3 自增自减

```rust
use cmx_buffer::CacheManager;

async fn counter_operations(cache: &CacheManager) -> Result<(), Box<dyn std::error::Error>> {
    // 设置计数器
    cache.ops().set("counter", "0").await?;

    // 自增
    let new_val: i64 = cache.ops().incr("counter", 1).await?;
    println!("Counter: {}", new_val);

    // 自增指定数值
    let new_val: i64 = cache.ops().incr("counter", 5).await?;
    println!("Counter: {}", new_val);

    // 自减
    let new_val: i64 = cache.ops().decr("counter", 2).await?;
    println!("Counter: {}", new_val);

    Ok(())
}
```

### 三、TTL 操作

```rust
use cmx_buffer::CacheManager;

async fn ttl_operations(cache: &CacheManager) -> Result<(), Box<dyn std::error::Error>> {
    // 设置带 TTL 的键
    cache.ops().set_ex("temp_key", "temp_value", 60).await?;

    // 获取 TTL
    let ttl: i64 = cache.ttl().ttl("temp_key").await?;
    println!("TTL: {} seconds", ttl);

    // 设置 TTL
    cache.ttl().expire("temp_key", 120).await?;

    // 移除 TTL（永不过期）
    cache.ttl().persist("temp_key").await?;

    // 获取键的剩余生存时间（毫秒）
    let ttl_ms: i64 = cache.ttl().pttl("temp_key").await?;

    Ok(())
}
```

### 四、集合操作

```rust
use cmx_buffer::CacheManager;

async fn set_operations(cache: &CacheManager) -> Result<(), Box<dyn std::error::Error>> {
    // 添加元素到集合
    cache.set().sadd("my_set", "member1").await?;
    cache.set().sadd("my_set", &["member2", "member3", "member4"]).await?;

    // 获取集合所有成员
    let members: Vec<String> = cache.set().smembers("my_set").await?;

    // 检查元素是否在集合中
    let is_member: bool = cache.set().sismember("my_set", "member1").await?;

    // 获取集合基数（元素个数）
    let cardinality: i64 = cache.set().scard("my_set").await?;

    // 随机获取元素
    let random: Option<String> = cache.set().srandmember("my_set", 1).await?;

    // 随机弹出一个元素
    let popped: Option<String> = cache.set().spop("my_set").await?;

    // 从集合中移除元素
    cache.set().srem("my_set", "member1").await?;

    Ok(())
}
```

### 五、有序集合操作

```rust
use cmx_buffer::CacheManager;

async fn sorted_set_operations(cache: &CacheManager) -> Result<(), Box<dyn std::error::Error>> {
    // 添加元素（带分数）
    cache.sorted_set().zadd("leaderboard", "player1", 100.0).await?;
    cache.sorted_set().zadd("leaderboard", "player2", 85.0).await?;
    cache.sorted_set().zadd("leaderboard", "player3", 92.0).await?;

    // 按分数范围获取成员
    let members: Vec<(String, f64)> = cache.sorted_set()
        .zrange_with_scores("leaderboard", 0, -1)
        .await?;

    // 获取排名（从高到低）
    let rank: Option<i64> = cache.sorted_set().zrevrank("leaderboard", "player1").await?;

    // 获取分数
    let score: Option<f64> = cache.sorted_set().zscore("leaderboard", "player1").await?;

    // 按分数范围查询
    let members: Vec<String> = cache.sorted_set()
        .zrangebyscore("leaderboard", 80.0, 100.0)
        .await?;

    // 增加分数
    cache.sorted_set().zincrby("leaderboard", "player1", 10.0).await?;

    // 按排名删除
    cache.sorted_set().zremrangebyrank("leaderboard", 0, 0).await?;

    Ok(())
}
```

### 六、分布式锁

#### 6.1 基础使用

```rust
use cmx_buffer::{create_redis_client, LockManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_redis_client("redis://127.0.0.1:6379").await?;
    let lock_manager = LockManager::new(client);

    // 获取锁
    let lock_guard = lock_manager.acquire("my_lock").await?;
    println!("Lock acquired: {}", lock_guard.key());

    // 执行关键操作
    do_critical_work().await?;

    // 释放锁（自动调用，或者超出作用域自动释放）
    drop(lock_guard);

    Ok(())
}
```

#### 6.2 带超时获取锁

```rust
use cmx_buffer::{create_redis_client, LockManager, LockOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_redis_client("redis://127.0.0.1:6379").await?;
    let lock_manager = LockManager::new(client);

    // 配置锁选项
    let options = LockOptions::builder()
        .with_timeout(30)           // 锁超时时间（秒）
        .with_retry_times(3)        // 重试次数
        .with_retry_interval(100)   // 重试间隔（毫秒）
        .with_auto_renew(true)      // 自动续期
        .build();

    // 尝试获取锁
    match lock_manager.acquire_with_options("resource_lock", options).await {
        Ok(lock_guard) => {
            println!("Lock acquired!");
            // 使用锁
            do_work().await?;
            // 锁会在 drop 时自动释放
        }
        Err(_) => {
            println!("Failed to acquire lock");
        }
    }

    Ok(())
}
```

#### 6.3 锁自动续期

```rust
use cmx_buffer::{create_redis_client, LockManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_redis_client("redis://127.0.0.1:6379").await?;
    let lock_manager = LockManager::new(client);

    // 创建一个会自动续期的锁
    let lock_guard = lock_manager
        .acquire_with_ttl("long_running_task", 60)  // 60秒超时
        .await?;

    // 执行长时间任务（锁会自动续期）
    run_long_task().await?;

    // 手动完成时释放锁
    lock_guard.release().await?;

    Ok(())
}
```

### 七、发布订阅

#### 7.1 发布消息

```rust
use cmx_buffer::{create_redis_client, PubSubManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_redis_client("redis://127.0.0.1:6379").await?;
    let pubsub = PubSubManager::new(client);

    // 发布消息到频道
    pubsub.publish("news", "Breaking news content").await?;
    pubsub.publish("notifications", r#"{"type":"alert","msg":"Test"}"#).await?;

    // 发布到多个频道
    pubsub.publish_many(&[("channel1", "msg1"), ("channel2", "msg2")]).await?;

    Ok(())
}
```

#### 7.2 订阅频道

```rust
use cmx_buffer::{create_redis_client, PubSubManager, SubscriptionHandler};
use async_trait::async_trait;
use std::sync::Arc;

struct MyHandler;

#[async_trait]
impl SubscriptionHandler for MyHandler {
    async fn handle(&self, channel: &str, message: &str) {
        println!("[{}] {}", channel, message);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_redis_client("redis://127.0.0.1:6379").await?;
    let pubsub = PubSubManager::new(client);

    let handler: Arc<dyn SubscriptionHandler> = Arc::new(MyHandler {});

    // 订阅单个频道
    pubsub.subscribe("news", handler.clone()).await?;

    // 订阅多个频道
    pubsub.subscribe_many(&["notifications", "alerts"], handler.clone()).await?;

    // 使用模式匹配订阅
    pubsub.psubscribe("user:*", handler.clone()).await?;

    // 取消订阅
    pubsub.unsubscribe("news").await?;
    pubsub.punsubscribe("user:*").await?;

    Ok(())
}
```

### 八、错误处理

```rust
use cmx_buffer::{BufferError, CacheManager};

async fn handle_errors() -> Result<(), Box<dyn std::error::Error>> {
    let result = cache.ops().get("nonexistent_key").await;

    match result {
        Ok(Some(value)) => println!("Value: {}", value),
        Ok(None) => println!("Key not found"),
        Err(e) => {
            match e {
                BufferError::ConnectionFailed(msg) => {
                    eprintln!("Redis connection failed: {}", msg);
                }
                BufferError::Timeout => {
                    eprintln!("Operation timed out");
                }
                BufferError::KeyNotFound(key) => {
                    eprintln!("Key not found: {}", key);
                }
                BufferError::SerializationFailed(msg) => {
                    eprintln!("Serialization failed: {}", msg);
                }
                BufferError::LockAcquisitionFailed(key) => {
                    eprintln!("Failed to acquire lock: {}", key);
                }
                BufferError::LockReleasedByOther => {
                    eprintln!("Lock was released by another process");
                }
            }
        }
    }

    Ok(())
}
```

### 九、完整示例

```rust
use cmx_buffer::{
    create_redis_client, CacheManager, LockManager,
    RedisConfig, CacheConfig, LockConfig,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置 Redis 连接
    let redis_config = RedisConfig::builder()
        .with_url("redis://127.0.0.1:6379")
        .with_pool_size(10)
        .with_timeout(5)
        .build();

    let client = create_redis_client_with_config(redis_config).await?;

    // 2. 创建缓存管理器
    let cache = CacheManager::new(client.clone());

    // 3. 创建锁管理器
    let lock_manager = LockManager::new(client.clone());

    // 4. 使用缓存
    cache.ops().set("user:001", r#"{"name":"张三","email":"zhangsan@example.com"}"#).await?;

    let user_json: Option<String> = cache.ops().get("user:001").await?;
    println!("User: {:?}", user_json);

    // 5. 使用分布式锁
    let lock_guard = lock_manager.acquire("process:order:12345").await?;

    // 在锁保护下执行操作
    process_order(12345).await?;

    // 6. 释放锁
    lock_guard.release().await?;

    println!("All operations completed");
    Ok(())
}

async fn process_order(order_id: i64) -> Result<(), Box<dyn std::error::Error>> {
    // 处理订单逻辑
    println!("Processing order: {}", order_id);
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    println!("Order processed: {}", order_id);
    Ok(())
}
```
