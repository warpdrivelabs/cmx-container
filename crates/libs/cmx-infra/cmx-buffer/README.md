# cmx-buffer 缓存管理模块

Redis 缓存操作和分布式锁管理模块，提供完整的缓存功能和分布式锁支持。

## 功能特性

- **bb8 连接池**: 使用 bb8 连接池实现高效的 Redis 连接复用，提升并发性能
- **缓存操作**: 支持字符串、序列化对象的增删改查
- **过期管理**: 支持 TTL 设置、查询、持久化
- **分布式锁**: 支持分布式环境下的锁获取、释放、自动续期
- **批量操作**: 支持批量读写
- **键前缀**: 支持自定义键前缀，避免键冲突
- **全局单例**: 支持全局初始化和获取，方便应用启动时配置

## 快速开始

### 添加依赖

```toml
[dependencies]
cmx-buffer = { path = "crates/libs/cmx-infra/cmx-buffer" }
```

### 基本使用

```rust
use cmx_buffer::{RedisClient, CacheManager, RedisConfig};
use cmx_buffer::config::CacheConfig;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // 1. 创建 Redis 配置
    let config = RedisConfig::new("redis://127.0.0.1:6379")
        .with_key_prefix("myapp:")
        .with_pool_size(10);
    
    // 2. 创建 Redis 客户端（自动创建 bb8 连接池）
    let client = RedisClient::new(config).await.unwrap();
    
    // 3. 创建缓存管理器
    let cache = CacheManager::new(client.clone());
    
    // 4. 使用缓存操作
    let ops = cache.ops();
    
    // 设置缓存
    ops.set("user:1", "Alice").await.unwrap();
    
    // 获取缓存
    let value = ops.get("user:1").await.unwrap();
    println!("User: {:?}", value);
    
    // 设置带过期时间的缓存
    ops.set_ex("temp:data", "value", Duration::from_secs(300)).await.unwrap();
}
```

## bb8 连接池

### 为什么使用连接池？

默认情况下，每次 Redis 操作都会创建新的连接，操作完成后关闭连接。这种方式在高频场景下性能较差。

bb8 连接池通过以下方式提升性能：
- **连接复用**: 多个请求共享连接池中的连接，避免频繁创建/销毁
- **并发处理**: 支持配置 `pool_size` 参数控制最大并发连接数
- **资源管理**: 连接自动归还池中，无需手动管理

### 连接池配置

```rust
use cmx_buffer::RedisConfig;

let config = RedisConfig::new("redis://host:port/db")
    .with_pool_size(20)        // 最大连接数，默认 10
    .with_key_prefix("app:")
    .with_connection_timeout(10)   // 连接超时(秒)
    .with_operation_timeout(5);    // 操作超时(秒)
```

| 方法 | 描述 | 默认值 |
|------|------|--------|
| `with_pool_size(size)` | 连接池最大连接数 | 10 |
| `with_key_prefix(prefix)` | 键前缀 | "cmx:" |
| `with_connection_timeout(sec)` | 连接超时(秒) | 5 |
| `with_operation_timeout(sec)` | 操作超时(秒) | 3 |

### 连接池工作原理

```
┌─────────────────────────────────────────────────────┐
│                    应用代码                          │
│   ┌─────────┐   ┌─────────┐   ┌─────────┐          │
│   │ 请求 1  │   │ 请求 2  │   │ 请求 3  │  ...      │
│   └────┬────┘   └────┬────┘   └────┬────┘          │
│        │             │             │                │
│        └─────────────┼─────────────┘                │
│                      ▼                              │
│            ┌─────────────────┐                      │
│            │   bb8 连接池    │  (max_size: 10)     │
│            │  ┌───────────┐  │                      │
│            │  │ 连接 1    │◄─┼── 请求1             │
│            │  │ 连接 2    │◄─┼── 请求2             │
│            │  │ 连接 3    │◄─┼── 请求3             │
│            │  │   ...     │  │                      │
│            │  └───────────┘  │                      │
│            └────────┬────────┘                      │
│                     │                               │
└─────────────────────┼───────────────────────────────┘
                      ▼
            ┌─────────────────┐
            │    Redis        │
            │   Server        │
            └─────────────────┘
```

### 使用示例

```rust
use cmx_buffer::{RedisClient, CacheManager, RedisConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建带连接池的客户端
    let config = RedisConfig::new("redis://127.0.0.1:6379")
        .with_pool_size(20);  // 配置连接池大小
    
    let client = RedisClient::new(config).await?;
    let cache = CacheManager::new(client);
    let ops = cache.ops();
    
    // 并发请求会自动使用连接池
    // 多个请求可以并发执行，共享连接池
    let futures = vec![
        ops.set("key1", "value1"),
        ops.set("key2", "value2"),
        ops.set("key3", "value3"),
    ];
    
    // 并发执行
    futures::future::join_all(futures).await;
    
    Ok(())
}
```

## API 参考

### 配置结构

#### RedisConfig

Redis 连接配置。

```rust
use cmx_buffer::RedisConfig;

// 方式一：使用默认配置
let config = RedisConfig::default();

// 方式二：使用构建器
let config = RedisConfig::new("redis://host:port/db")
    .with_pool_size(20)
    .with_key_prefix("app:")
    .with_connection_timeout(10)
    .with_operation_timeout(5);
```

| 方法 | 描述 | 默认值 |
|------|------|--------|
| `new(url)` | 创建配置 | - |
| `with_pool_size(size)` | 连接池大小 | 10 |
| `with_key_prefix(prefix)` | 键前缀 | "cmx:" |
| `with_connection_timeout(sec)` | 连接超时(秒) | 5 |
| `with_operation_timeout(sec)` | 操作超时(秒) | 3 |

#### LockConfig

分布式锁配置。

```rust
use cmx_buffer::LockConfig;

let config = LockConfig::new()
    .with_expire(30)              // 锁过期时间(秒)
    .with_retry_times(3)          // 重试次数
    .with_retry_interval(200)     // 重试间隔(毫秒)
    .with_renew_threshold(0.3);   // 续期阈值(百分比)
```

| 方法 | 描述 | 默认值 |
|------|------|--------|
| `new()` | 创建默认配置 | - |
| `with_expire(seconds)` | 锁过期时间 | 30秒 |
| `with_retry_times(n)` | 获取锁重试次数 | 3次 |
| `with_retry_interval(ms)` | 重试间隔 | 200毫秒 |
| `with_renew_threshold(ratio)` | 续期阈值 | 0.3 (30%) |

### 缓存操作 (CacheOps)

#### 基础操作

```rust
let ops = cache.ops();

// 设置缓存
ops.set("key", "value").await?;

// 获取缓存
let value: Option<String> = ops.get("key").await?;

// 删除缓存
let deleted: bool = ops.del("key").await?;

// 检查存在
let exists: bool = ops.exists("key").await?;

// 设置带过期时间 (秒)
ops.set_ex("key", "value", Duration::from_secs(60)).await?;
```

#### 序列化操作

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
}

// 序列化存储
let user = User { id: 1, name: "Alice" };
ops.set_serialized("user:1", &user).await?;

// 反序列化获取
let user: Option<User> = ops.get_deserialized("user:1").await?;
```

#### 批量操作

```rust
use std::collections::HashMap;

// 批量设置
let mut items = HashMap::new();
items.insert("key1", "value1");
items.insert("key2", "value2");
ops.mset(items).await?;

// 批量获取
let keys = vec!["key1", "key2", "key3"];
let values: Vec<Option<String>> = ops.mget(&keys).await?;

// 批量删除
let keys = vec!["key1", "key2"];
let count: u64 = ops.del_batch(&keys).await?;
```

#### 计数器操作

```rust
// 自增
let count: i64 = ops.incr("counter", 1).await?;

// 自减
let count: i64 = ops.decr("counter", 5).await?;
```

### TTL 操作 (TtlOps)

```rust
let ttl = cache.ttl();

// 设置过期时间
ttl.expire("key", Duration::from_secs(60)).await?;

// 设置过期时间点 (Unix 时间戳)
ttl.expire_at("key", 1700000000).await?;

// 移除过期时间 (永不过期)
ttl.persist("key").await?;

// 查询剩余 TTL
let ttl_remaining: Option<Duration> = ttl.ttl("key").await?;

// 精确到毫秒的 TTL
let ttl_ms: Option<Duration> = ttl.pttl("key").await?;

// 设置值同时设置过期
ttl.set_with_ttl("key", "value", Duration::from_secs(60)).await?;

// 原子操作：仅当键不存在时设置
let success: bool = ttl.setnx("key", "value").await?;
let success: bool = ttl.setnx_ex("key", "value", Duration::from_secs(60)).await?;
```

### 分布式锁 (LockManager)

#### 基本使用

```rust
use cmx_buffer::LockManager;
use cmx_buffer::LockConfig;
use std::time::Duration;

// 创建锁管理器
let lock_manager = LockManager::new(client, LockConfig::new());

// 尝试获取锁（立即返回）
let locked: bool = lock_manager.try_lock("resource:1").await?;

// 获取锁（带重试）
let guard = lock_manager.lock("resource:1").await?;

// 使用锁保护的资源
do_something().await;

// 手动释放
guard.unlock().await?;
```

#### LockGuard 自动释放

```rust
// LockGuard 会在作用域结束时自动释放锁
{
    let guard = lock_manager.lock("resource:1").await?;
    // 处理业务
} // 锁自动释放
```

#### 锁续期（手动）

```rust
let guard = lock_manager.lock("resource:1").await?;

// 在业务处理过程中延长锁
guard.extend(Duration::from_secs(60)).await?;
```

#### 自动续期功能

获取锁后自动启动后台任务，根据 `renew_threshold` 配置自动续期（默认启用）：

```rust
use cmx_buffer::LockConfig;

// 配置自动续期
let lock_config = LockConfig::new()
    .with_expire(30)           // 锁过期时间 30 秒
    .with_renew_threshold(0.3); // 当剩余时间低于 30*0.3=9 秒时自动续期

let lock_manager = LockManager::new(client, lock_config);

// 获取锁后自动启动后台续期任务
let guard = lock_manager.lock("resource:1").await?;

// 获取锁剩余时间
let ttl: Option<Duration> = guard.remaining_ttl().await?;

// 停止自动续期（如需要长时间持有锁且不需要续期）
guard.stop_auto_renew();

// 重新启动自动续期
guard.start_auto_renew();

// 检查自动续期是否启用
let enabled: bool = guard.is_auto_renew_enabled();

// 获取锁的值（UUID）
let lock_value: &str = guard.lock_value();
```

**注意**：调用 `lock()` 获取锁时会自动启动自动续期任务，无需手动调用。

#### 检查锁状态

```rust
// 检查锁是否有效
let is_locked: bool = lock_manager.is_locked("resource:1").await?;

// 获取锁剩余时间
let remaining: Option<Duration> = lock_manager.remaining_ttl("resource:1").await?;
```

### 全局单例模式

#### GlobalCacheManager 全局缓存管理器

在应用启动时初始化，之后可在代码任意位置获取使用：

```rust
use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisConfig};

fn main() {
    // 在应用启动时初始化
    let redis_config = RedisConfig::new("redis://192.168.1.100:6379/0")
        .with_key_prefix("myapp:");
    
    // 初始化全局缓存管理器
    GlobalCacheManager::initialize(redis_config.clone()).unwrap();
    
    // 初始化全局锁管理器
    GlobalLockManager::initialize(redis_config).unwrap();
    
    // 之后在代码任意位置使用
    let cache = GlobalCacheManager::get();
    let ops = cache.ops();
    
    // 或者获取克隆
    let cache = GlobalCacheManager::get_cloned();
}
```

#### GlobalLockManager 全局锁管理器

```rust
use cmx_buffer::{GlobalLockManager, LockConfig};

// 使用默认配置初始化
GlobalLockManager::initialize(redis_config).unwrap();

// 或带自定义锁配置
let lock_config = LockConfig::new()
    .with_expire(60)
    .with_renew_threshold(0.3);
GlobalLockManager::initialize_with_redis_config(redis_config, lock_config).unwrap();

// 获取全局锁管理器
let lock_manager = GlobalLockManager::get();

// 使用分布式锁
let guard = lock_manager.lock("resource_key").await?;
```

#### 全局管理器 API

所有全局管理器提供以下方法：

```rust
// 初始化（使用默认配置）
GlobalXxxManager::initialize(redis_config)?;

// 初始化（带完整配置）
GlobalXxxManager::initialize_with_configs(redis_config, cache_config, lock_config)?;

// 获取引用（不可变）
let manager = GlobalXxxManager::get();

// 获取可变引用
let mut manager = GlobalXxxManager::get_mut();

// 获取克隆
let manager = GlobalXxxManager::get_cloned();

// 检查是否已初始化
if GlobalXxxManager::is_initialized() { ... }
```

## 错误处理

所有操作都返回 `Result` 类型：

```rust
use cmx_buffer::Error;

match ops.get("key").await {
    Ok(Some(value)) => println!("Got: {}", value),
    Ok(None) => println!("Key not found"),
    Err(e) => {
        match e {
            Error::ConnectionError(msg) => println!("连接错误: {}", msg),
            Error::PoolError(msg) => println!("连接池错误: {}", msg),
            Error::OperationError(msg) => println!("操作错误: {}", msg),
            Error::LockError(msg) => println!("锁错误: {}", msg),
            _ => println!("其他错误: {}", e),
        }
    }
}
```

错误类型：

- `Error::ConnectionError` - Redis 连接错误
- `Error::PoolError` - 连接池错误（获取连接失败、连接池耗尽）
- `Error::OperationError` - 缓存操作错误
- `Error::SerializeError` - 序列化/反序列化错误
- `Error::LockError` - 分布式锁错误
- `Error::TimeoutError` - 操作超时
- `Error::LockConflictError` - 锁冲突

## 日志

模块使用 `tracing` 进行日志记录：

```rust
// 设置日志级别
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

日志级别：
- `DEBUG` - 详细操作信息（键名、连接池状态）
- `INFO` - 重要操作记录（连接、锁获取/释放）
- `WARN` - 潜在问题（重试、超时、连接池耗尽）
- `ERROR` - 操作失败

## 测试

### 单元测试

```bash
cargo test -p cmx-buffer
```

### 集成测试

需要运行 Redis 服务器：

```bash
# 使用 Docker 启动 Redis
docker run -d -p 6379:6379 redis

# 运行集成测试
cargo test -p cmx-buffer --test integration_test
```

## 完整示例

### 使用全局单例模式

```rust
use cmx_buffer::{
    GlobalCacheManager, GlobalLockManager,
    RedisConfig, LockConfig
};
use std::collections::HashMap;
use std::time::Duration;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct CacheData {
    name: String,
    value: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置
    let redis_config = RedisConfig::new("redis://192.168.1.100:6379/0")
        .with_key_prefix("myapp:")
        .with_pool_size(10);
    
    let lock_config = LockConfig::new()
        .with_expire(60)
        .with_retry_times(3)
        .with_retry_interval(200)
        .with_renew_threshold(0.3);
    
    // 2. 初始化全局管理器
    GlobalCacheManager::initialize(redis_config.clone())?;
    GlobalLockManager::initialize_with_redis_config(redis_config, lock_config)?;
    
    // 3. 使用缓存
    let cache = GlobalCacheManager::get();
    let ops = cache.ops();
    
    // 字符串操作
    ops.set("config:version", "1.0.0").await?;
    let version = ops.get("config:version").await?;
    println!("Version: {:?}", version);
    
    // 序列化操作
    let data = CacheData {
        name: "test".to_string(),
        value: 42,
    };
    ops.set_serialized("data:1", &data).await?;
    let retrieved: Option<CacheData> = ops.get_deserialized("data:1").await?;
    println!("Data: {:?}", retrieved);
    
    // 4. 使用分布式锁（带自动续期）
    let lock = GlobalLockManager.get();
    let lock_key = "resource:processing";
    
    let guard = lock.lock(lock_key).await?;
    println!("获取锁成功，剩余 TTL: {:?}", guard.remaining_ttl().await?);
    
    // 执行业务逻辑（锁会自动续期）
    do_processing().await;
    
    // 手动释放（也可以等待 guard 超出作用域自动释放）
    guard.unlock().await?;
    
    println!("完成");
    Ok(())
}

async fn do_processing() {
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

### 使用本地管理器实例

```rust
use cmx_buffer::{
    RedisClient, CacheManager, LockManager,
    RedisConfig, LockConfig
};
use std::collections::HashMap;
use std::time::Duration;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct CacheData {
    name: String,
    value: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置
    let redis_config = RedisConfig::new("redis://192.168.1.100:6379/0")
        .with_key_prefix("myapp:")
        .with_pool_size(10);
    
    let lock_config = LockConfig::new()
        .with_expire(30)
        .with_retry_times(3)
        .with_retry_interval(200);
    
    // 2. 创建客户端（自动创建 bb8 连接池）
    let client = RedisClient::new(redis_config).await?;
    
    // 3. 创建管理器
    let cache = CacheManager::new(client.clone());
    let lock_manager = LockManager::new(client, lock_config);
    
    // 4. 缓存操作示例
    let ops = cache.ops();
    
    // 字符串操作
    ops.set("config:version", "1.0.0").await?;
    let version = ops.get("config:version").await?;
    println!("Version: {:?}", version);
    
    // 序列化操作
    let data = CacheData {
        name: "test".to_string(),
        value: 42,
    };
    ops.set_serialized("data:1", &data).await?;
    let retrieved: Option<CacheData> = ops.get_deserialized("data:1").await?;
    println!("Data: {:?}", retrieved);
    
    // 批量操作
    let mut items = HashMap::new();
    items.insert("batch:1", "value1");
    items.insert("batch:2", "value2");
    items.insert("batch:3", "value3");
    ops.mset(items).await?;
    
    let keys = vec!["batch:1", "batch:2", "batch:3"];
    let values = ops.mget(&keys).await?;
    println!("Batch values: {:?}", values);
    
    // 5. 分布式锁示例
    let lock_key = "resource:processing";
    
    // 尝试获取锁
    if lock_manager.try_lock(lock_key).await? {
        println!("获取锁成功");
        
        // 处理业务
        do_processing().await;
        
        // 释放锁
        lock_manager.unlock(lock_key).await?;
    } else {
        println!("锁已被占用");
    }
    
    // 或者使用 guard（推荐）
    {
        let _guard = lock_manager.lock(lock_key).await?;
        // 处理业务
    } // 自动释放锁
    
    println!("完成");
    Ok(())
}

async fn do_processing() {
    // 模拟业务处理
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

## 许可证

MIT
