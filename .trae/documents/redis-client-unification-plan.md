# Redis 客户端统一管理方案

## 一、现状问题分析

### 1.1 三套独立的 RedisClient 实例

重构前 `cmx-buffer` 模块中存在 **3 个全局单例**，每个都独立创建自己的 `RedisClient`（含独立 bb8 连接池）：

| 全局单例 | 文件位置 | 初始化方式 | RedisClient 来源 |
|---------|---------|-----------|-----------------|
| `GlobalCacheManager` | `cache/mod.rs` | async | 自行创建 `RedisClient::new()` |
| `GlobalLockManager` | `lock/mod.rs` | async/sync | 自行创建 `RedisClient::new()` |
| `GlobalRedisClient` | `client.rs` | sync (block_on) | 自行创建 `RedisClient::new()` |

**结果**：启动时创建了 **3 个独立的 bb8 连接池**，浪费 Redis 连接资源。

### 1.2 GlobalRedisClient 未被初始化的严重 Bug

重构前启动流程 `init_cache()` 中只初始化了 `GlobalCacheManager` 和 `GlobalLockManager`，
`GlobalRedisClient` 从未被初始化。但 `cmx-plugin` 的 `manager.rs:547` 中直接调用了
`GlobalRedisClient::get()`，如果启用集群模式，此处必定 panic。

### 1.3 职责混乱

- `GlobalRedisClient` 只在 `cmx-plugin` 中被使用，且仅为了获取 `config().url` 和 `build_key()`
- 这些功能 `CacheManager.client()` 完全可以提供

---

## 二、重构方案

### 2.1 核心思路：单一 RedisClient 源头

**以 `GlobalCacheManager` 为唯一的 RedisClient 源头**，其他模块通过它获取客户端：

```
init_cache()
  └─ 创建 1 个 RedisClient
      ├─ GlobalCacheManager (持有 RedisClient)
      └─ GlobalLockManager  (共享同一 RedisClient)
```

### 2.2 已完成的变更

#### 步骤 1：修改 `GlobalCacheManager` 的初始化方法

**文件**: `crates/libs/cmx-infra/cmx-buffer/src/cache/mod.rs`

- 新增 `initialize_with_client(client: RedisClient)` 方法，接受外部传入的 `RedisClient`
- 保留原有 `initialize(redis_config)` 不变（向后兼容）

#### 步骤 2：修改 `GlobalLockManager` 的初始化方法

**文件**: `crates/libs/cmx-infra/cmx-buffer/src/lock/mod.rs`

- 新增 `initialize_with_client(client: RedisClient)` 方法
- 保留原有 `initialize(redis_config)` 不变（向后兼容）

#### 步骤 3：修改启动流程 `init_cache()`

**文件**: `crates/web/web-server/src/config.rs`

- 先创建唯一的 `RedisClient`
- 将同一个 `client` clone 后分别传给 `GlobalCacheManager` 和 `GlobalLockManager`

```rust
pub async fn init_cache() {
    let redis_config = config.get_as::<RedisConfig>("redis").unwrap();
    // 创建唯一的 RedisClient 实例，共享给 CacheManager 和 LockManager
    let client = RedisClient::new(redis_config)
        .await
        .expect("Redis 客户端创建失败");
    GlobalCacheManager::initialize_with_client(client.clone())
        .expect("redis初始化失败");
    GlobalLockManager::initialize_with_client(client)
        .expect("redis分布式锁初始化失败");
}
```

#### 步骤 4：消除 `cmx-plugin` 对 `GlobalRedisClient` 的依赖

**文件**: `crates/libs/cmx-plugin/src/core/manager.rs`

将 `GlobalRedisClient::get()` 替换为 `GlobalCacheManager::get().client()`。

#### 步骤 5：彻底删除 `GlobalRedisClient`

**文件**: `crates/libs/cmx-infra/cmx-buffer/src/client.rs`

- 删除 `GLOBAL_REDIS_CLIENT`、`GLOBAL_REDIS_CLIENT_MUTEX` 静态变量
- 删除 `GlobalRedisClient` 结构体及其所有方法
- 清理不再需要的 `use std::sync::{OnceLock, Mutex}` 导入

#### 步骤 6：清理 `lib.rs` 导出

**文件**: `crates/libs/cmx-infra/cmx-buffer/src/lib.rs`

- 移除 `GlobalRedisClient` 的导出
- 移除 `#[allow(deprecated)]` 标注

---

## 三、最终变更文件

| 文件 | 变更类型 |
|------|---------|
| `cmx-buffer/src/cache/mod.rs` | 新增 `initialize_with_client()` 方法 |
| `cmx-buffer/src/lock/mod.rs` | 新增 `initialize_with_client()` 方法 |
| `cmx-buffer/src/client.rs` | 删除 `GlobalRedisClient` 全部代码 |
| `cmx-buffer/src/lib.rs` | 移除 `GlobalRedisClient` 导出 |
| `web-server/src/config.rs` | 重构 `init_cache()` 共享单一 RedisClient |
| `cmx-plugin/src/core/manager.rs` | 替换为 `GlobalCacheManager::get().client()` |

## 四、验证结果

- `cargo check` 编译通过
- `cargo clippy` 无新增警告
- 项目中已无 `GlobalRedisClient` 的任何引用
