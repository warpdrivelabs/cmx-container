# cmx-buffer 缓存管理模块规划文档

## 1. 模块概述

### 1.1 模块名称与定位
- **模块名称**: cmx-buffer
- **所属目录**: `crates/libs/cmx-infra/cmx-buffer`
- **定位**: 提供 Redis 缓存操作和分布式锁功能的基础设施模块

### 1.2 核心功能
1. **Redis 缓存操作**: 封装常用的缓存操作（添加、查询、更新、删除、过期设置）
2. **分布式锁**: 实现 Redis 分布式锁，确保分布式环境下的资源竞争安全
3. **连接池管理**: 使用 bb8 连接池实现高效的连接复用

### 1.3 设计原则
- 高内聚低耦合
- 清晰的 API 接口
- 完善的错误处理机制
- 详细的日志记录
- 连接池复用提升性能

---

## 2. 技术选型

### 2.1 依赖库
| 库名称 | 版本 | 用途 |
|--------|------|------|
| redis | 0.27 | Redis 客户端 |
| bb8 | 0.8 | 连接池管理 |
| bb8-redis | 0.17 | Redis 连接池支持 |
| tokio | workspace | 异步运行时 |
| serde | workspace | 序列化/反序列化 |
| serde_json | workspace | JSON 处理 |
| tracing | workspace | 日志记录 |
| thiserror | workspace | 错误类型定义 |
| uuid | workspace | 锁值生成 |

---

## 3. 模块架构设计

### 3.1 目录结构
```
cmx-buffer/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs              # 模块入口，导出公共接口
│   ├── error.rs            # 错误类型定义
│   ├── config.rs           # 配置结构体
│   ├── client.rs           # Redis 客户端封装（bb8 连接池）
│   ├── cache/
│   │   ├── mod.rs          # 缓存操作模块入口
│   │   ├── ops.rs          # 基础缓存操作（字符串）
│   │   ├── ttl.rs          # 过期时间管理
│   │   ├── sorted_set.rs   # 有序集合操作
│   │   ├── set.rs          # 集合操作
│   │   └── pubsub.rs       # 发布/订阅操作
│   ├── lock/
│   │   ├── mod.rs          # 分布式锁模块入口
│   │   └── manager.rs      # 分布式锁管理器（含自动续期）
│   └── logging.rs          # 日志辅助工具
└── tests/
    └── integration_test.rs # 集成测试
```

### 3.2 核心组件

#### 3.2.1 错误处理 (error.rs)
```rust
// 定义模块专属错误类型
pub enum Error {
    // 连接相关错误
    ConnectionError(String),
    // 连接池错误
    PoolError(String),
    // 操作相关错误
    OperationError(String),
    // 序列化错误
    SerializeError(String),
    // 分布式锁错误
    LockError(String),
    // 分布式锁冲突错误
    LockConflictError(String),
    // 超时错误
    TimeoutError(String),
}
```

#### 3.2.2 配置 (config.rs)
```rust
// Redis 连接配置
pub struct RedisConfig {
    url: String,
    pool_size: usize,              // 连接池大小
    connection_timeout: Duration,  // 连接超时时间
    operation_timeout: Duration,   // 操作超时时间
    key_prefix: String,            // 键前缀
}

// 分布式锁配置
pub struct LockConfig {
    expire_seconds: u64,           // 锁过期时间（秒）
    retry_times: u32,              // 获取锁重试次数
    retry_interval: Duration,      // 重试间隔
    renew_threshold: f64,          // 自动续期阈值（0.0-1.0）
}
```

#### 3.2.3 Redis 客户端 (client.rs) - bb8 连接池
```rust
pub struct RedisClient {
    pool: Pool<RedisConnectionManager>,
    config: RedisConfig,
    cache_config: CacheConfig,
    lock_config: LockConfig,
}

impl RedisClient {
    pub async fn new(config: RedisConfig) -> Result<Self>;
    pub async fn get_connection(&self) -> Result<PooledConnection>;
    pub async fn is_connected(&self) -> bool;
    pub async fn close(&self) -> Result<()>;
    pub fn build_key(&self, key: &str) -> String;
}
```

#### 3.2.4 缓存操作 (cache/ops.rs)
```rust
pub struct CacheOps {
    client: RedisClient,
}

impl CacheOps {
    pub async fn set(&self, key: &str, value: &str) -> Result<()>;
    pub async fn get(&self, key: &str) -> Result<Option<String>>;
    pub async fn del(&self, key: &str) -> Result<bool>;
    pub async fn exists(&self, key: &str) -> Result<bool>;
    pub async fn set_ex(&self, key: &str, value: &str, expire: Duration) -> Result<()>;
    pub async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<String>>>;
    pub async fn mset(&self, items: HashMap<&str, &str>) -> Result<()>;
    pub async fn incr(&self, key: &str, delta: i64) -> Result<i64>;
    pub async fn decr(&self, key: &str, delta: i64) -> Result<i64>;
    pub async fn set_serialized<T: Serialize>(&self, key: &str, value: &T) -> Result<()>;
    pub async fn get_deserialized<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
}
```

#### 3.2.5 分布式锁 (lock/manager.rs) - 含自动续期
```rust
pub struct LockManager {
    client: RedisClient,
    config: LockConfig,
}

impl LockManager {
    pub async fn try_lock(&self, key: &str) -> Result<bool>;
    pub async fn lock(&self, key: &str) -> Result<LockGuard>;
    pub async fn unlock(&self, key: &str) -> Result<()>;
    pub async fn extend(&self, key: &str, duration: Duration) -> Result<()>;
    pub async fn is_locked(&self, key: &str) -> Result<bool>;
    pub async fn remaining_ttl(&self, key: &str) -> Result<Option<Duration>>;
}

// 锁 guard，支持自动续期
pub struct LockGuard {
    key: String,
    lock_value: String,
    client: RedisClient,
    config: LockConfig,
    released: Arc<AtomicBool>,
    auto_renew: Arc<AtomicBool>,
}

impl LockGuard {
    pub async fn unlock(self) -> Result<()>;
    pub async fn extend(&self, duration: Duration) -> Result<()>;
    pub fn start_auto_renew(&self);
    pub fn stop_auto_renew(&self);
    pub fn is_valid(&self) -> bool;
}
```

---

## 4. API 接口设计

### 4.1 缓存操作 API

#### 4.1.1 基础操作
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `set` | 设置缓存值 | key, value | `Result<()>` |
| `get` | 获取缓存值 | key | `Result<Option<String>>` |
| `set_ex` | 设置带过期时间的缓存 | key, value, expire | `Result<()>` |
| `del` | 删除缓存 | key | `Result<bool>` |
| `exists` | 检查键是否存在 | key | `Result<bool>` |

#### 4.1.2 过期时间管理
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `expire` | 设置键的过期时间 | key, duration | `Result<bool>` |
| `expire_at` | 设置键的过期时间戳 | key, timestamp | `Result<bool>` |
| `persist` | 移除键的过期时间 | key | `Result<bool>` |
| `ttl` | 获取键的剩余过期时间 | key | `Result<Option<Duration>>` |
| `pttl` | 获取键的剩余过期时间（毫秒） | key | `Result<Option<Duration>>` |

#### 4.1.3 序列化支持
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `set_serialized` | 设置序列化后的值 | key, value: T | `Result<()>` |
| `get_deserialized` | 获取并反序列化值 | key | `Result<Option<T>>` |

#### 4.1.4 批量操作
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `mget` | 批量获取 | keys: &[&str] | `Result<Vec<Option<String>>>` |
| `mset` | 批量设置 | items: HashMap | `Result<()>` |
| `del_batch` | 批量删除 | keys: &[&str] | `Result<u64>` |

#### 4.1.5 计数器操作
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `incr` | 自增 | key, delta | `Result<i64>` |
| `decr` | 自减 | key, delta | `Result<i64>` |

#### 4.1.6 有序集合操作 (SortedSetOps)
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `zadd` | 添加有序集合成员 | key, items | `Result<u64>` |
| `zadd_one` | 添加单个成员 | key, score, member | `Result<bool>` |
| `zadd_nx` | 仅不存在时添加 | key, score, member | `Result<bool>` |
| `zadd_xx` | 仅存在时更新 | key, score, member | `Result<bool>` |
| `zrem` | 移除成员 | key, members | `Result<u64>` |
| `zrem_one` | 移除单个成员 | key, member | `Result<bool>` |
| `zrange` | 获取索引范围成员 | key, start, stop | `Result<Vec<String>>` |
| `zrange_with_scores` | 获取成员及分数 | key, start, stop | `Result<Vec<(String, f64)>>` |
| `zrevrange` | 获取索引范围成员（降序） | key, start, stop | `Result<Vec<String>>` |
| `zrevrange_with_scores` | 获取成员及分数（降序） | key, start, stop | `Result<Vec<(String, f64)>>` |
| `zrangebyscore` | 按分数范围查询 | key, min, max | `Result<Vec<String>>` |
| `zrangebyscore_limit` | 按分数范围查询（限数量） | key, min, max, offset, count | `Result<Vec<String>>` |
| `zscore` | 获取成员分数 | key, member | `Result<Option<f64>>` |
| `zrank` | 获取成员排名（升序） | key, member | `Result<Option<u64>>` |
| `zrevrank` | 获取成员排名（降序） | key, member | `Result<Option<u64>>` |
| `zcard` | 获取集合大小 | key | `Result<u64>` |
| `zcount` | 统计分数范围内成员 | key, min, max | `Result<u64>` |
| `zincrby` | 增加成员分数 | key, delta, member | `Result<f64>` |
| `zremrangebyrank` | 按排名移除 | key, start, stop | `Result<u64>` |
| `zremrangebyscore` | 按分数范围移除 | key, min, max | `Result<u64>` |
| `zpopmin` | 弹出最低分成员 | key, count | `Result<Vec<(String, f64)>>` |
| `zpopmax` | 弹出最高分成员 | key, count | `Result<Vec<(String, f64)>>` |
| `zunionstore` | 并集存储 | dest, keys | `Result<u64>` |
| `zinterstore` | 交集存储 | dest, keys | `Result<u64>` |

#### 4.1.7 集合操作 (SetOps)
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `sadd` | 添加成员 | key, members | `Result<u64>` |
| `sadd_one` | 添加单个成员 | key, member | `Result<bool>` |
| `srem` | 移除成员 | key, members | `Result<u64>` |
| `srem_one` | 移除单个成员 | key, member | `Result<bool>` |
| `smembers` | 获取所有成员 | key | `Result<Vec<String>>` |
| `sismember` | 检查成员是否存在 | key, member | `Result<bool>` |
| `smismember` | 检查多个成员 | key, members | `Result<Vec<bool>>` |
| `scard` | 获取集合大小 | key | `Result<u64>` |
| `spop` | 随机弹出成员 | key | `Result<Option<String>>` |
| `spop_count` | 随机弹出多个成员 | key, count | `Result<Vec<String>>` |
| `srandmember` | 随机获取成员 | key | `Result<Option<String>>` |
| `srandmember_count` | 随机获取多个成员 | key, count | `Result<Vec<String>>` |
| `sdiff` | 差集 | keys | `Result<Vec<String>>` |
| `sinter` | 交集 | keys | `Result<Vec<String>>` |
| `sunion` | 并集 | keys | `Result<Vec<String>>` |
| `sdiffstore` | 差集存储 | dest, keys | `Result<u64>` |
| `sinterstore` | 交集存储 | dest, keys | `Result<u64>` |
| `sunionstore` | 并集存储 | dest, keys | `Result<u64>` |
| `smove` | 移动成员 | source, dest, member | `Result<bool>` |

#### 4.1.8 发布/订阅操作 (PubSubOps)
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `publish` | 发布消息 | channel, message | `Result<u64>` |
| `publish_json` | 发布JSON消息 | channel, message | `Result<u64>` |
| `pubsub_channels` | 获取活动频道 | pattern | `Result<Vec<String>>` |
| `pubsub_numsub` | 获取频道订阅数 | channels | `Result<Vec<(String, u64)>>` |
| `pubsub_numpat` | 获取模式订阅数 | - | `Result<u64>` |

#### 4.1.9 发布/订阅类型 (PubSub Types)
| 类型 | 描述 |
|------|------|
| `PubSubMessage` | 发布/订阅消息结构，包含 channel 和 payload 字段 |
| `Subscriber` | 订阅者，用于接收频道消息 |
| `SharedSubscriber` | 可克隆的共享订阅者 |

### 4.2 分布式锁 API

| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `try_lock` | 尝试获取锁 | key | `Result<bool>` |
| `try_lock_with_value` | 尝试获取锁（返回锁值） | key | `Result<(bool, Option<String>)>` |
| `lock` | 获取锁（阻塞重试，自动续期） | key | `Result<LockGuard>` |
| `unlock` | 释放锁 | key | `Result<()>` |
| `unlock_with_value` | 释放锁（验证锁值） | key, lock_value | `Result<()>` |
| `extend` | 延长锁的过期时间 | key, duration | `Result<()>` |
| `is_locked` | 检查锁是否有效 | key | `Result<bool>` |
| `remaining_ttl` | 获取锁剩余时间 | key | `Result<Option<Duration>>` |

### 4.3 LockGuard API

| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `unlock` | 手动释放锁 | self | `Result<()>` |
| `extend` | 延长锁的过期时间 | duration | `Result<()>` |
| `start_auto_renew` | 启动自动续期 | - | `()` |
| `stop_auto_renew` | 停止自动续期 | - | `()` |
| `is_valid` | 检查锁是否有效 | - | `bool` |

---

## 5. bb8 连接池特性

### 5.1 连接池配置
- **连接池大小**: 可配置的最大连接数
- **连接复用**: 所有操作从连接池获取连接，执行完后归还
- **并发安全**: 支持多并发请求

### 5.2 使用方式
```rust
let config = RedisConfig::new("redis://localhost:6379")
    .with_pool_size(10);

let client = RedisClient::new(config).await?;

// 操作自动从连接池获取连接
let value = ops.get("key").await?;
```

---

## 6. 分布式锁自动续期特性

### 6.1 自动续期机制
- 当锁的剩余时间低于 `expire_seconds * renew_threshold` 时自动续期
- 默认 `renew_threshold` 为 0.3（30%）
- 续期检查间隔为 `expire_seconds / 2`

### 6.2 使用方式
```rust
let lock_config = LockConfig::new()
    .with_expire(30)              // 30秒过期
    .with_renew_threshold(0.3);   // 剩余30%时自动续期

let lock_manager = LockManager::new(client, lock_config);

// lock() 方法自动启动自动续期任务
let guard = lock_manager.lock("resource_key").await?;

// 作用域结束时自动释放锁
```

---

## 7. 错误处理机制

### 7.1 错误分类
1. **连接错误**: Redis 连接失败
2. **连接池错误**: 连接池耗尽、获取连接失败
3. **操作错误**: 缓存操作失败、键类型不匹配
4. **序列化错误**: 值序列化/反序列化失败
5. **分布式锁错误**: 获取锁失败、锁超时、锁冲突
6. **超时错误**: 操作超时

### 7.2 错误传播
- 所有公开方法返回 `Result<T>`
- 使用 `thiserror` 定义结构化错误类型
- 保留底层错误上下文，便于调试

---

## 8. 日志记录设计

### 8.1 日志级别
- **DEBUG**: 详细的操作信息（键名、值大小）
- **INFO**: 重要操作记录（连接、锁获取/释放）
- **WARN**: 潜在问题（重试、超时）
- **ERROR**: 操作失败

### 8.2 日志内容
- 操作类型和目标
- 操作结果（成功/失败）
- 耗时统计
- 错误详情（失败时）

---

## 9. 测试设计

### 9.1 集成测试覆盖
1. **缓存操作测试**
   - 基本 CRUD 操作
   - 过期时间设置
   - 批量操作
   - 序列化/反序列化

2. **分布式锁测试**
   - 锁获取与释放
   - 锁自动过期
   - 锁重试机制
   - 锁自动续期
   - LockGuard 自动释放

### 9.2 测试环境
- Redis 地址: 192.168.137.95:32496
- 数据库: 13
- 键前缀: cmx-buffer-test:

---

## 10. 实现步骤

### 阶段一：基础框架搭建
- [x] 创建模块目录结构
- [x] 配置 Cargo.toml 依赖（含 bb8 连接池）
- [x] 实现错误类型定义
- [x] 实现配置结构体

### 阶段二：Redis 客户端封装
- [x] 实现 bb8 连接池管理
- [x] 实现基础缓存操作
- [x] 实现序列化支持

### 阶段三：分布式锁实现
- [x] 实现锁管理器
- [x] 实现锁获取逻辑
- [x] 实现锁自动释放（RAII guard）
- [x] 实现自动续期功能

### 阶段四：日志与监控
- [x] 添加日志记录
- [x] 添加操作计时

### 阶段五：测试与完善
- [x] 编写集成测试
- [x] 代码优化与文档

---

## 11. 验收标准

### 11.1 功能验收
- [x] 缓存基本操作（SET/GET/DEL/EXISTS）正常工作
- [x] 过期时间（SETEX/EXPIRE/TTL）正常工作
- [x] 序列化/反序列化正常工作
- [x] 批量操作正常工作
- [x] 分布式锁获取/释放正常工作
- [x] 锁自动释放机制正常工作
- [x] 锁自动续期功能正常工作

### 11.2 性能验收
- [x] bb8 连接池正常工作
- [x] 连接复用提升性能

### 11.3 质量验收
- [x] 代码无编译警告
- [x] 集成测试通过
- [x] 错误处理完善
- [x] 日志记录完整
- [x] API 设计清晰易用
