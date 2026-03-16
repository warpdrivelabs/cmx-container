# cmx-buffer 缓存管理模块

Redis 缓存操作和分布式锁管理模块，提供完整的缓存功能和分布式锁支持。

## 功能特性

- **缓存操作**: 支持字符串、序列化对象的增删改查
- **过期管理**: 支持 TTL 设置、查询、持久化
- **分布式锁**: 支持分布式环境下的锁获取、释放、自动续期
- **批量操作**: 支持批量读写
- **键前缀**: 支持自定义键前缀，避免键冲突

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
    
    // 2. 创建 Redis 客户端
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
    .with_expire(30)           // 锁过期时间(秒)
    .with_retry_times(3)      // 重试次数
    .with_retry_interval(200); // 重试间隔(毫秒)
```

| 方法 | 描述 | 默认值 |
|------|------|--------|
| `new()` | 创建默认配置 | - |
| `with_expire(seconds)` | 锁过期时间 | 30秒 |
| `with_retry_times(n)` | 获取锁重试次数 | 3次 |
| `with_retry_interval(ms)` | 重试间隔 | 200毫秒 |

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

#### 锁续期

```rust
let guard = lock_manager.lock("resource:1").await?;

// 在业务处理过程中延长锁
guard.extend(Duration::from_secs(60)).await?;
```

#### 检查锁状态

```rust
// 检查锁是否有效
let is_locked: bool = lock_manager.is_locked("resource:1").await?;

// 获取锁剩余时间
let remaining: Option<Duration> = lock_manager.remaining_ttl("resource:1").await?;
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
            Error::OperationError(msg) => println!("操作错误: {}", msg),
            Error::LockError(msg) => println!("锁错误: {}", msg),
            _ => println!("其他错误: {}", e),
        }
    }
}
```

错误类型：

- `Error::ConnectionError` - Redis 连接错误
- `Error::PoolError` - 连接池错误
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
- `DEBUG` - 详细操作信息
- `INFO` - 重要操作记录（连接、锁获取/释放）
- `WARN` - 潜在问题（重试、超时）
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

```rust
use cmx_buffer::{
    RedisClient, CacheManager, LockManager,
    RedisConfig, LockConfig, Error
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
    
    // 2. 创建客户端
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
