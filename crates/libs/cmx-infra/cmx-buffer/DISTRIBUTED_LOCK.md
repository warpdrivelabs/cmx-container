# cmx-buffer 分布式锁使用指南

## 一、概述

cmx-buffer 分布式锁基于 Redis 实现，参考 [Redisson](https://github.com/redisson/redisson) 的设计范式，提供以下核心能力：

- **RAII 自动释放**：`LockGuard` 通过 `Drop` trait 在作用域结束时自动释放锁
- **看门狗自动续期**：后台异步任务定期检查并续期，防止业务未完成时锁过期
- **安全释放**：使用 Lua 脚本验证锁值后才删除，防止误删其他持有者的锁
- **灵活控制**：通过 `lease_time` 参数精确控制看门狗行为

## 二、API 对比（对标 Redisson）

### 2.1 Redisson vs cmx-buffer 方法对照表

| Redisson | cmx-buffer | 等待行为 | 看门狗 |
|----------|-----------|----------|--------|
| `lock()` | `lock(key)` | **无限等待** | 启用 |
| `lock(leaseTime, unit)` | `lock_with_options(key, opts.with_lease_time())` | **无限等待** | 禁用，持有 leaseTime 后过期 |
| `tryLock()` | `try_lock(key)` | **不等待，立即返回** | 启用 |
| `tryLock(waitTime, unit)` | `try_lock_with_options(key, opts.with_wait_time())` | **限时等待 waitTime** | 启用 |
| `tryLock(waitTime, leaseTime, unit)` | `try_lock_with_options(key, opts.with_wait_time().with_lease_time())` | **限时等待 waitTime** | 禁用，持有 leaseTime 后过期 |

### 2.2 LockManager 方法对比

| 方法 | 等待行为 | 返回值 | 看门狗 | 适用场景 |
|------|----------|--------|--------|----------|
| `lock(key)` | 无限等待 | `Result<LockGuard>` | 启用 | 必须拿到锁，不在乎等多久 |
| `lock_with_options(key, opts)` | 无限等待 | `Result<LockGuard>` | 可控 | 必须拿到锁，控制持有时间 |
| `try_lock(key)` | 不等待 | `Result<Option<LockGuard>>` | 启用 | 快速检查，获取不到就放弃 |
| `try_lock_with_options(key, opts)` | 可控 | `Result<Option<LockGuard>>` | 可控 | 限时等待或立即返回 |
| `is_locked(key)` | 不等待 | `Result<bool>` | - | 检查锁是否被占用 |
| `remaining_ttl(key)` | 不等待 | `Result<Option<Duration>>` | - | 查看锁剩余时间 |

### 2.3 try_lock vs lock 核心区别

```
lock:       尝试 → 失败 → 等待 → 重试 → ... → 永远等下去直到成功（无限循环）
try_lock:   尝试一次 → 成功返回 Some(guard) / 失败返回 None（立即返回）
try_lock_with_options(wait_time):
            尝试 → 失败 → 等待 → 重试 → ... → 超过 waitTime 返回 None
```

| 特性 | `lock` | `try_lock` | `try_lock_with_options` |
|------|--------|-----------|------------------------|
| 等待行为 | **无限等待** | **不等待** | **限时等待** |
| 返回值 | `Result<LockGuard>` | `Result<Option<LockGuard>>` | `Result<Option<LockGuard>>` |
| 获取失败 | 永远不会失败（除非 Redis 异常） | 返回 `Ok(None)` | 超时返回 `Ok(None)` |
| 适用场景 | 后台必须执行成功的任务 | 高并发快速失败 | 有限时间内尽量获取 |

### 2.4 看门狗机制

看门狗（Watchdog）是后台自动续期任务，由 `lease_time` 参数控制：

| `lease_time` | 看门狗 | 锁行为 |
|--------------|--------|--------|
| `None`（默认） | **启用** | 锁不会过期，直到 `LockGuard` Drop 或手动 `unlock()` |
| `Some(10s)` | **禁用** | 锁在 10 秒后强制过期，即使业务未完成 |

**续期逻辑**：
- 检查间隔：`expire_seconds / 2`（默认 15 秒）
- 触发阈值：`expire_seconds * renew_threshold`（默认 30%，即 9 秒）
- 当 TTL < 阈值时，自动重置为 `expire_seconds`

## 三、LockOptions 详解

```rust
use cmx_buffer::LockOptions;
use std::time::Duration;

let options = LockOptions::new()
    .with_wait_time(Duration::from_secs(5))     // 最长等待时间（仅 try_lock_with_options 使用）
    .with_lease_time(Duration::from_secs(10))   // 锁持有时间，None=看门狗续期
    .with_retry_interval(Duration::from_millis(200)); // 重试间隔
```

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `wait_time` | `Option<Duration>` | `None` | 最长等待时间。仅 `try_lock_with_options` 使用。`None`=不等待 |
| `lease_time` | `Option<Duration>` | `None` | 锁持有时间。`None`=看门狗续期，`Some`=固定过期 |
| `retry_interval` | `Option<Duration>` | 全局配置（200ms） | 重试间隔，`lock` 和 `try_lock_with_options` 均使用 |

## 四、LockConfig 配置

`LockConfig` 是全局锁配置，在创建 `LockManager` 时指定：

```rust
use cmx_buffer::LockConfig;

let config = LockConfig::new()
    .with_expire(30)            // 锁过期时间（秒），默认 30
    .with_retry_times(3)        // 重试次数，默认 3
    .with_retry_interval(200)   // 重试间隔（毫秒），默认 200
    // renew_threshold 在配置中默认 0.3（30%）
```

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `expire_seconds` | `u64` | `30` | 锁的 TTL（秒），看门狗续期时重置为此值 |
| `retry_times` | `u32` | `3` | `lock()` 方法的默认重试次数 |
| `retry_interval_ms` | `u64` | `200` | `lock()` 方法的默认重试间隔 |
| `renew_threshold` | `f64` | `0.3` | 看门狗续期阈值（百分比），TTL 低于此比例时触发续期 |

## 五、LockGuard API

`LockGuard` 是 RAII 风格的锁守卫，通过 `try_lock` 或 `lock` 获取：

| 方法 | 说明 |
|------|------|
| `guard.unlock().await` | 手动释放锁（通常不需要，Drop 自动释放） |
| `guard.extend(duration).await` | 手动延长锁过期时间 |
| `guard.is_valid()` | 检查锁是否仍然有效 |
| `guard.key()` | 获取锁的键名 |
| `guard.lock_value()` | 获取锁的唯一值（UUID） |
| `guard.remaining_ttl().await` | 获取锁的剩余 TTL |

**生命周期**：
```
lock_manager.try_lock("key") → Ok(Some(guard))
                                    ↓
                            业务逻辑执行中...
                            （看门狗自动续期）
                                    ↓
                        guard 离开作用域 → Drop → 自动释放锁
```

## 六、使用场景

### 6.1 场景一：防止重复操作（非阻塞）

适用于：DDL 操作、初始化任务、多个实例只需一个执行的场景。

```rust
use cmx_buffer::LockManager;

async fn ensure_single_execution(lm: &LockManager) -> cmx_buffer::Result<()> {
    match lm.try_lock("task:init").await {
        Ok(Some(_guard)) => {
            tracing::info!("本实例负责执行初始化");
            do_initialization().await?;
        }
        Ok(None) => {
            tracing::info!("其他实例正在执行初始化，跳过");
        }
        Err(e) => {
            tracing::warn!("锁服务异常: {}，继续执行", e);
            do_initialization().await?;
        }
    }
    Ok(())
}
```

### 6.2 场景二：长时间任务（阻塞式 + 看门狗）

适用于：数据迁移、批处理、需要保证执行完成的任务。

```rust
use cmx_buffer::LockManager;

async fn long_running_task(lm: &LockManager) -> cmx_buffer::Result<()> {
    let _guard = lm.lock("task:migration").await?;
    tracing::info!("获取锁成功，开始数据迁移");

    run_data_migration().await?;

    // _guard Drop 自动释放，看门狗自动续期保证锁不会过期
    Ok(())
}
```

### 6.3 场景三：限时等待获取锁（tryLock(waitTime)）

适用于：有限时间内尽量获取锁，超时就放弃。

```rust
use cmx_buffer::{LockManager, LockOptions};
use std::time::Duration;

async fn wait_for_lock(lm: &LockManager) -> cmx_buffer::Result<()> {
    // 最多等 5 秒，获取不到就放弃
    match lm
        .try_lock_with_options("task:order", LockOptions::new()
            .with_wait_time(Duration::from_secs(5)))
        .await
    {
        Ok(Some(_guard)) => {
            tracing::info!("5 秒内获取锁成功");
            process_order().await?;
        }
        Ok(None) => {
            tracing::warn!("等待 5 秒仍未获取锁，放弃");
        }
        Err(e) => {
            tracing::error!("锁异常: {}", e);
        }
    }
    Ok(())
}
```

### 6.4 场景四：限时等待 + 指定持有时间（tryLock(waitTime, leaseTime)）

适用于：限时获取锁，且锁只需持有固定时间。

```rust
use cmx_buffer::{LockManager, LockOptions};
use std::time::Duration;

async fn bounded_lock(lm: &LockManager) -> cmx_buffer::Result<()> {
    // 最多等 3 秒，锁只持有 10 秒（禁用看门狗）
    match lm
        .try_lock_with_options("task:quick", LockOptions::new()
            .with_wait_time(Duration::from_secs(3))
            .with_lease_time(Duration::from_secs(10)))
        .await
    {
        Ok(Some(_guard)) => {
            tracing::info!("获取锁成功，最多持有 10 秒");
            do_quick_task().await?;
        }
        Ok(None) => {
            tracing::warn!("3 秒内未获取锁");
        }
        Err(e) => {
            tracing::error!("锁异常: {}", e);
        }
    }
    Ok(())
}
```

### 6.5 场景五：无限等待 + 指定持有时间（lock(leaseTime)）

适用于：不在乎等多久，但锁只需持有固定时间。

```rust
use cmx_buffer::{LockManager, LockOptions};
use std::time::Duration;

async fn infinite_wait_bounded_hold(lm: &LockManager) -> cmx_buffer::Result<()> {
    // 无限等待直到获取锁，但锁只持有 30 秒（禁用看门狗）
    let _guard = lm
        .lock_with_options("task:batch", LockOptions::new()
            .with_lease_time(Duration::from_secs(30)))
        .await?;

    tracing::info!("获取锁成功，最多持有 30 秒");
    do_batch_task().await?;
    Ok(())
}
```

### 6.5 场景五：等待其他节点完成

适用于：等待其他实例完成初始化后再启动。

```rust
use cmx_buffer::LockManager;
use std::time::{Duration, Instant};

async fn wait_for_completion(lm: &LockManager) -> bool {
    let timeout = Duration::from_secs(60);
    let poll_interval = Duration::from_secs(5);
    let start = Instant::now();

    while start.elapsed() < timeout {
        tokio::time::sleep(poll_interval).await;

        match lm.is_locked("task:migration").await {
            Ok(false) => {
                tracing::info!("其他节点已完成");
                return true;
            }
            Ok(true) => {
                tracing::debug!("仍在执行中...");
            }
            Err(e) => {
                tracing::warn!("锁检查失败: {}", e);
            }
        }
    }

    tracing::warn!("等待超时");
    false
}
```

### 6.6 场景六：全局锁管理器

适用于：应用级别的锁管理，通过全局单例在任意位置访问。

```rust
use cmx_buffer::{GlobalLockManager, GlobalCacheManager, RedisConfig};

async fn init_global() -> cmx_buffer::Result<()> {
    let redis_config = RedisConfig::new("redis://127.0.0.1:6379");

    GlobalCacheManager::initialize(redis_config.clone()).await?;
    GlobalLockManager::initialize(redis_config).await?;

    Ok(())
}

async fn use_global_lock() -> cmx_buffer::Result<()> {
    let lm = GlobalLockManager::get();

    match lm.try_lock("global:task").await {
        Ok(Some(_guard)) => {
            do_task().await?;
        }
        Ok(None) => tracing::info!("任务正在被其他实例执行"),
        Err(e) => tracing::warn!("锁异常: {}", e),
    }

    Ok(())
}
```

## 七、错误处理

```rust
use cmx_buffer::Error;

async fn handle_lock_errors(lm: &cmx_buffer::LockManager) {
    // try_lock 不会返回 LockConflictError（获取失败返回 Ok(None)）
    match lm.try_lock("key").await {
        Ok(Some(guard)) => { /* 成功 */ }
        Ok(None) => { /* 锁被占用 */ }
        Err(e) => {
            match e {
                Error::ConnectionError(msg) => {
                    tracing::error!("Redis 连接失败: {}", msg);
                }
                Error::TimeoutError(msg) => {
                    tracing::error!("操作超时: {}", msg);
                }
                _ => {
                    tracing::error!("锁操作失败: {}", e);
                }
            }
        }
    }

    // lock 在超过重试次数后返回 LockConflictError
    match lm.lock("key").await {
        Ok(guard) => { /* 成功 */ }
        Err(Error::LockConflictError(msg)) => {
            tracing::warn!("锁冲突，获取失败: {}", msg);
        }
        Err(e) => {
            tracing::error!("锁操作异常: {}", e);
        }
    }
}
```

## 八、常见问题（FAQ）

### Q1: `try_lock` 和 `lock` 怎么选择？

**用 `lock()`**：
- 后台必须执行成功的任务，不在乎等多久（如数据迁移、批处理）
- 需要无限等待直到获取锁

**用 `try_lock()`**：
- 立即返回，获取不到就放弃（如秒杀、防重复提交）
- 多实例中只需一个执行（如 DDL 操作、初始化）

**用 `try_lock_with_options(wait_time)`**：
- 有限时间内尽量获取（如订单处理，最多等 5 秒）
- 超时就走降级逻辑

### Q2: 锁会自动释放吗？

**是的**。所有通过 `try_lock` / `lock` 获取的 `LockGuard` 都会在离开作用域时（Drop）自动释放锁。无需手动调用 `unlock()`。

### Q3: 看门狗什么情况下会启动？

- `lease_time = None`（默认）：看门狗**自动启动**
- `lease_time = Some(duration)`：看门狗**不启动**，锁在指定时间后强制过期

### Q4: 如果服务崩溃，锁会怎样？

- **看门狗启用时**：看门狗任务随之终止，锁会在 `expire_seconds`（默认 30 秒）后自动过期释放
- **指定 `lease_time` 时**：锁会在 `lease_time` 到期后自动释放
- **建议**：`expire_seconds` 应大于正常业务执行时间，但不宜过长，以免故障时锁释放延迟过大

### Q5: `expire_seconds` 和 `lease_time` 有什么区别？

| 参数 | 作用域 | 说明 |
|------|--------|------|
| `expire_seconds`（LockConfig） | 全局 | 看门狗续期时重置的 TTL 值，默认 30 秒 |
| `lease_time`（LockOptions） | 单次调用 | 指定锁的持有时间。设置后禁用看门狗，锁在此时间后强制过期 |

### Q6: 为什么不用 `unlock_with_value` 了？

旧版 API 暴露了 `unlock_with_value(key, lock_value)` 方法，要求开发者自行管理锁值。新版 API 通过 `LockGuard` 的 RAII 机制自动处理释放，不再需要手动管理锁值。安全释放逻辑（Lua 脚本验证所有权）已内置在 `LockGuard::Drop` 中。

### Q7: 如何手动提前释放锁？

```rust
let guard = lm.lock("key").await?;
do_work().await?;
guard.unlock().await?;  // 手动释放，不等 Drop
```

### Q8: 如何检查锁是否被占用？

```rust
// 方式一：is_locked（不获取锁，仅检查）
let locked = lm.is_locked("key").await?;

// 方式二：try_lock（尝试获取，获取不到返回 None）
match lm.try_lock("key").await {
    Ok(Some(_guard)) => { /* 锁之前未被占用，现在已被我们持有 */ }
    Ok(None) => { /* 锁被占用 */ }
    Err(e) => { /* 异常 */ }
}
```

### Q9: 多个实例同时 `try_lock` 同一个 key 会怎样？

Redis 的 `SET NX EX` 命令保证原子性，只有一个实例会成功获取锁。其他实例会收到 `Ok(None)`。

### Q10: 锁的粒度如何设计？

- **粗粒度**（如 `lock("plugin:ddl:{plugin_id}")`）：整个插件的 DDL 操作串行化
- **细粒度**（如 `lock("order:process:{order_id}")`）：每个订单独立加锁，互不影响
- **建议**：根据业务并发需求选择粒度，粒度越细并发越高，但管理复杂度也越高

## 九、最佳实践

1. **优先使用 `lock()`**：自动续期 + 自动释放，最安全
2. **短任务用 `try_lock` + `lease_time`**：精确控制，避免看门狗开销
3. **合理设置 `expire_seconds`**：大于正常业务执行时间，小于可接受的故障恢复时间
4. **锁粒度要合理**：太粗影响并发，太细增加复杂度
5. **总是处理 `Err`**：Redis 可能不可用，业务应有降级方案
6. **避免嵌套锁**：多个锁的嵌套容易导致死锁
