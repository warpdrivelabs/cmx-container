# cmx-buffer 缓存管理模块规划文档

## 1. 模块概述

### 1.1 模块名称与定位
- **模块名称**: cmx-buffer
- **所属目录**: `crates/libs/cmx-infra/cmx-buffer`
- **定位**: 提供 Redis 缓存操作和分布式锁功能的基础设施模块

### 1.2 核心功能
1. **Redis 缓存操作**: 封装常用的缓存操作（添加、查询、更新、删除、过期设置）
2. **分布式锁**: 实现 Redis 分布式锁，确保分布式环境下的资源竞争安全

### 1.3 设计原则
- 高内聚低耦合
- 清晰的 API 接口
- 完善的错误处理机制
- 详细的日志记录

---

## 2. 技术选型

### 2.1 依赖库
| 库名称 | 版本 | 用途 |
|--------|------|------|
| redis | 0.27 | Redis 客户端 |
| tokio | workspace | 异步运行时 |
| serde | workspace | 序列化/反序列化 |
| serde_json | workspace | JSON 处理 |
| tracing | workspace | 日志记录 |
| thiserror | workspace | 错误类型定义 |
| chrono | workspace | 时间处理 |

---

## 3. 模块架构设计

### 3.1 目录结构
```
cmx-buffer/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口，导出公共接口
│   ├── error.rs            # 错误类型定义
│   ├── config.rs           # 配置结构体
│   ├── client.rs           # Redis 客户端封装
│   ├── cache/
│   │   ├── mod.rs          # 缓存操作模块入口
│   │   ├── ops.rs          # 基础缓存操作（增删改查）
│   │   └── ttl.rs          # 过期时间管理
│   ├── lock/
│   │   ├── mod.rs          # 分布式锁模块入口
│   │   └── manager.rs      # 分布式锁管理器
│   └── logging.rs          # 日志辅助工具
└── tests/
    ├── integration_test.rs # 集成测试
    └── unit_test.rs        # 单元测试
```

### 3.2 核心组件

#### 3.2.1 错误处理 (error.rs)
```rust
// 定义模块专属错误类型
pub enum Error {
    // 连接相关错误
    ConnectionError(String),
    // 操作相关错误
    OperationError(String),
    // 序列化错误
    SerializeError(String),
    // 分布式锁错误
    LockError(String),
    // 超时错误
    TimeoutError(String),
}
```

#### 3.2.2 配置 (config.rs)
```rust
// Redis 连接配置
pub struct RedisConfig {
    // 连接地址
    url: String,
    // 连接池大小
    pool_size: usize,
    // 连接超时时间
    connection_timeout: Duration,
    // 操作超时时间
    operation_timeout: Duration,
    // 默认键前缀
    key_prefix: String,
}

// 分布式锁配置
pub struct LockConfig {
    // 锁过期时间
    expire_duration: Duration,
    // 获取锁重试次数
    retry_times: u32,
    // 重试间隔
    retry_interval: Duration,
}
```

#### 3.2.3 缓存操作 (cache/ops.rs)
```rust
// 基础缓存操作 trait
pub trait CacheOperations {
    // 设置值
    async fn set(&self, key: &str, value: &str) -> Result<()>;
    // 获取值
    async fn get(&self, key: &str) -> Result<Option<String>>;
    // 删除键
    async fn del(&self, key: &str) -> Result<bool>;
    // 检查键是否存在
    async fn exists(&self, key: &str) -> Result<bool>;
    // 设置带过期时间的值
    async fn set_ex(&self, key: &str, value: &str, expire: Duration) -> Result<()>;
}
```

#### 3.2.4 分布式锁 (lock/manager.rs)
```rust
// 分布式锁管理器
pub struct LockManager {
    client: RedisClient,
    config: LockConfig,
}

impl LockManager {
    // 尝试获取锁
    pub async fn try_lock(&self, key: &str) -> Result<bool>;
    // 获取锁（带重试）
    pub async fn lock(&self, key: &str) -> Result<LockGuard>;
    // 释放锁
    pub async fn unlock(&self, key: &str) -> Result<()>;
    // 延长锁的过期时间
    pub async fn extend(&self, key: &str, duration: Duration) -> Result<()>;
}

// 锁 guard，确保作用域结束时自动释放
pub struct LockGuard {
    key: String,
    manager: LockManager,
}
```

---

## 4. API 接口设计

### 4.1 缓存操作 API

#### 4.1.1 同步/批量操作
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `set` | 设置缓存值 | key, value | `Result<()>` |
| `get` | 获取缓存值 | key | `Result<Option<String>>` |
| `set_ex` | 设置带过期时间的缓存 | key, value, expire | `Result<()>` |
| `del` | 删除缓存 | key | `Result<bool>` |
| `exists` | 检查键是否存在 | key | `Result<bool>` |
| `expire` | 设置键的过期时间 | key, duration | `Result<bool>` |
| `ttl` | 获取键的剩余过期时间 | key | `Result<Option<Duration>>` |

#### 4.1.2 序列化支持
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `set_serialized` | 设置序列化后的值 | key, value: T | `Result<()>` |
| `get_deserialized` | 获取并反序列化值 | key | `Result<Option<T>>` |

#### 4.1.3 批量操作
| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `mget` | 批量获取 | keys: Vec<&str> | `Result<Vec<Option<String>>>` |
| `mset` | 批量设置 | items: HashMap<&str, &str> | `Result<()>` |

### 4.2 分布式锁 API

| 方法 | 描述 | 参数 | 返回值 |
|------|------|------|--------|
| `try_lock` | 尝试获取锁 | key | `Result<bool>` |
| `lock` | 获取锁（阻塞重试） | key | `Result<LockGuard>` |
| `unlock` | 释放锁 | key | `Result<()>` |
| `extend` | 延长锁的过期时间 | key, duration | `Result<()>` |

---

## 5. 错误处理机制

### 5.1 错误分类
1. **连接错误**: Redis 连接失败、连接池耗尽
2. **操作错误**: 缓存操作失败、键类型不匹配
3. **序列化错误**: 值序列化/反序列化失败
4. **分布式锁错误**: 获取锁失败、锁超时、锁冲突
5. **超时错误**: 操作超时

### 5.2 错误传播
- 所有公开方法返回 `Result<T>`
- 使用 `thiserror` 定义结构化错误类型
- 保留底层错误上下文，便于调试

---

## 6. 日志记录设计

### 6.1 日志级别
- **DEBUG**: 详细的操作信息（键名、值大小）
- **INFO**: 重要操作记录（连接、锁获取/释放）
- **WARN**: 潜在问题（重试、超时）
- **ERROR**: 操作失败

### 6.2 日志内容
- 操作类型和目标
- 操作结果（成功/失败）
- 耗时统计
- 错误详情（失败时）

---

## 7. 单元测试设计

### 7.1 测试覆盖
1. **缓存操作测试**
   - 基本 CRUD 操作
   - 过期时间设置
   - 批量操作
   - 序列化/反序列化

2. **分布式锁测试**
   - 锁获取与释放
   - 锁自动过期
   - 锁重试机制
   - 多线程并发

3. **错误处理测试**
   - 连接失败处理
   - 超时处理
   - 并发冲突处理

---

## 8. 实现步骤

### 阶段一：基础框架搭建
1. 创建模块目录结构
2. 配置 Cargo.toml 依赖
3. 实现错误类型定义
4. 实现配置结构体

### 阶段二：Redis 客户端封装
1. 实现 Redis 连接池管理
2. 实现基础缓存操作
3. 实现序列化支持

### 阶段三：分布式锁实现
1. 实现锁管理器
2. 实现锁获取逻辑
3. 实现锁自动释放（RAII guard）

### 阶段四：日志与监控
1. 添加日志记录
2. 添加性能监控（可选）

### 阶段五：测试与完善
1. 编写单元测试
2. 编写集成测试
3. 代码优化与文档

---

## 9. 验收标准

### 9.1 功能验收
- [ ] 缓存基本操作（SET/GET/DEL/EXISTS）正常工作
- [ ] 过期时间（SETEX/EXPIRE/TTL）正常工作
- [ ] 序列化/反序列化正常工作
- [ ] 批量操作正常工作
- [ ] 分布式锁获取/释放正常工作
- [ ] 锁自动释放机制正常工作

### 9.2 质量验收
- [ ] 代码无编译警告
- [ ] 单元测试通过
- [ ] 错误处理完善
- [ ] 日志记录完整
- [ ] API 设计清晰易用
