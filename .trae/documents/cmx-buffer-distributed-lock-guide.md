# cmx-buffer 分布式锁使用指南

## 一、概述

cmx-buffer 提供了基于 Redis 的分布式锁实现，支持自动续期、手动释放等特性。

## 二、锁获取方式对比

| 方法                    | 重试机制    | 返回值                              | 自动续期  | 需要手动释放  |
| --------------------- | ------- | -------------------------------- | ----- | ------- |
| `try_lock`            | 无，立即返回  | `Result<bool>`                   | **否** | 是       |
| `try_lock_with_value` | 无，立即返回  | `Result<(bool, Option<String>)>` | **否** | 是       |
| `lock`                | 有，按配置重试 | `Result<LockGuard>`              | **是** | 否（自动释放） |

### 1. try\_lock / try\_lock\_with\_value

```rust
// try_lock - 只返回是否获取成功
let success = lock_manager.try_lock("my_key").await?;

// try_lock_with_value - 返回是否成功及锁值（用于安全释放）
let (success, lock_value) = lock_manager.try_lock_with_value("my_key").await?;
```

**特点**：

* 立即返回，不阻塞

* 使用 Redis `SET key value NX EX expire_seconds` 命令，**有过期时间**（默认 30 秒）

* **不支持自动续期**：锁会在 30 秒后自动过期释放

* 需要**手动调用** **`unlock_with_value`** **释放锁**

> ⚠️ **重要**：如果业务执行时间超过 `expire_seconds` 且未手动释放，锁会自动过期。

### 2. lock

```rust
let guard = lock_manager.lock("my_key", LockOptions::default()).await?;
//guard 超出作用域时自动释放锁
```

**特点**：

* 支持重试机制（默认重试 3 次，间隔 200ms）

* **自动续期**：当 TTL 低于 `expire_seconds * renew_threshold` 时自动续期

* **自动释放**：实现 `Drop` trait，作用域结束时自动释放

## 三、LockGuard 自动续期机制

### 3.1 续期触发条件

```rust
// manager.rs 第 468-469 行
let renew_interval = Duration::from_secs(config.expire_seconds / 2);
let threshold_secs = (config.expire_seconds as f64 * config.renew_threshold) as u64;
```

* 续期检查间隔：`expire_seconds / 2`（默认 15 秒）

* 触发阈值：`expire_seconds * renew_threshold`（默认 30%，即 9 秒）

### 3.2 续期逻辑

当 TTL < 9 秒时，自动将锁过期时间重置为 `expire_seconds`（默认 30 秒）。

### 3.3 续期终止条件

* 手动调用 `guard.unlock().await` 释放锁

* `guard.stop_auto_renew()` 停止自动续期

* 锁已过期或被其他进程释放

## 四、手动释放锁

使用 `try_lock` / `try_lock_with_value` 获取的锁需要手动释放：

```rust
let (success, lock_value) = lock_manager.try_lock_with_value("my_key").await?;
if success {
    // ... 业务逻辑
    lock_manager.unlock_with_value("my_key", &lock_value.unwrap()).await?;
}
```

### 释放方式对比

| 方法                                   | 安全性 | 说明                |
| ------------------------------------ | --- | ----------------- |
| `unlock_with_value(key, lock_value)` | 安全  | 仅当锁值匹配时释放，防止误删他人锁 |
| `unlock(key)`                        | 不安全 | 直接删除锁，可能误删他人持有的锁  |

**推荐使用** **`unlock_with_value`**，因为它使用 Lua 脚本保证原子性：

```lua
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
```

## 五、配置参数

```rust
pub struct LockConfig {
    pub expire_seconds: u64,      // 锁过期时间（秒），默认 30
    pub retry_times: u32,         // 重试次数，默认 3
    pub retry_interval_ms: u64,    // 重试间隔（毫秒），默认 200
    pub renew_threshold: f64,      // 续期阈值（百分比），默认 0.3
}
```

## 六、你的代码分析

[utils.rs#L292](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/utils.rs#L292) 使用的是 `try_lock_with_value`：

```rust
match lm.try_lock_with_value(&lock_key).await {
    Ok((true, Some(lock_value))) => {
        // ... 业务逻辑
        // ✅ 正确：手动释放了锁
        if let Err(e) = lm.unlock_with_value(&lock_key, &lock_value).await {
            tracing::debug!("释放DDL锁失败（将等待TTL过期）: {}", e);
        }
    }
    // ...
}
```

**结论**：你的代码使用正确！`try_lock_with_value` 获取的锁需要手动释放，而你的代码确实调用了 `unlock_with_value` 进行释放。

## 七、常见问题

### Q: try\_lock 没有自动续期，设计合理吗？

**设计是合理的**。`try_lock` 和 `lock` 适用于不同场景：

| 场景                 | 推荐方法       | 说明          |
| ------------------ | ---------- | ----------- |
| 快速检查锁存在性           | `try_lock` | 获取不到就放弃，不阻塞 |
| 长时间持有锁             | `lock`     | 有自动续期，更安全   |
| 短时任务（确定可在 TTL 内完成） | `try_lock` | 无需自动续期负担    |

如果 `try_lock` 也需要自动续期，应该使用 `lock()` 方法。

### Q: 锁是自动释放还是必须手动释放？

**取决于获取方式**：

| 获取方式       | 释放方式       | 说明                   |
| ---------- | ---------- | -------------------- |
| `lock()`   | 自动释放（Drop） | 推荐，无需关心释放            |
| `try_lock` | 必须手动释放     | 开发者需确保在 finally 块中释放 |

## 八、企业级应用建议

### 8.1 优先使用 `lock()`

```rust
// ✅ 推荐：自动续期 + 自动释放，更安全
let guard = lock_manager.lock("my_key", LockOptions::default()).await?;
```

### 8.2 必须用 try\_lock 时的最佳实践

如果业务必须使用 `try_lock`（如快速检查场景），建议：

```rust
let (success, lock_value) = lock_manager.try_lock_with_value("my_key").await?;
if success {
    let _lock_value = lock_value.unwrap();
    // 使用 defer/finally 模式确保释放
    defer {
        // 即使panic也会执行
    }
}
```

但在 Rust 中没有 defer，建议使用 `std::mem::drop` 或封装作用域：

```rust
async fn with_lock<F, R>(lm: &LockManager, key: &str, f: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    let (success, lock_value) = lm.try_lock_with_value(key).await?;
    if !success {
        return Err(Error::LockConflictError("获取锁失败".to_string()));
    }

    let result = f().await;

    // 无论业务成功与否，都释放锁
    lm.unlock_with_value(key, &lock_value.unwrap()).await?;

    result
}
```

### 8.3 企业级分布式锁 checklist

* [ ] 锁的过期时间 `expire_seconds` 应大于正常业务执行时间

* [ ] 长时间任务使用 `lock()` 而非 `try_lock`

* [ ] 使用 `unlock_with_value` 而非 `unlock`（安全释放）

* [ ] 考虑网络分区时的锁安全（Redis 单点故障建议用 Redlock）

* [ ] 监控锁的平均持有时间和竞争情况

### Q: 如果服务崩溃，锁会怎样？

* 使用 `lock()` 获取的锁：虽然会自动释放，但需要等到下一次检查时才发现（最长延迟 15 秒）

* 使用 `try_lock` 获取的锁：需要等 TTL 过期（默认 30 秒）

## 八、最佳实践

1. **优先使用** **`lock()`**：自动续期 + 自动释放，更安全
2. **如果用** **`try_lock`**，务必确保在 `finally` 块或错误处理中释放锁
3. **使用** **`unlock_with_value`** **而不是** **`unlock`**：更安全
4. **合理设置** **`expire_seconds`**：太短可能导致业务未完成就释放，太长可能导致故障时锁释放延迟

