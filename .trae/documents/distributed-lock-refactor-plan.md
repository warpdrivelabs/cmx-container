# 分布式锁企业级重构方案

## 一、现状分析

### 当前 API 设计问题

| 问题 | 说明 |
|------|------|
| `try_lock` / `try_lock_with_value` 返回裸值 | 返回 `(bool, Option<String>)`，调用方需自行管理锁值和释放，容易遗漏 |
| `LockGuard` 需要外部手动创建 | `runner.rs` 中通过 `try_lock_with_value` 获取后再手动 `LockGuard::new()` + `start_auto_renew_task()` |
| `try_lock` 无自动续期 | 手动构建 `LockGuard` 才能续期，不符合 Redisson 看门狗范式 |
| `unlock(key)` 不安全 | 不验证锁值，可能误删其他持有者的锁 |
| `LockOptions` 仅控制重试 | 缺少 `lease_time`（显式指定锁持有时间）和 `wait_time`（最长等待时间）控制 |
| API 命名不一致 | `try_lock` vs `lock` 语义不清晰，缺少 Redisson 风格的 waitTime/leaseTime 语义 |

### 业务调用现状（3 处）

| 文件 | 当前调用方式 | 问题 |
|------|-------------|------|
| [utils.rs:292](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/utils.rs#L292) | `try_lock_with_value` + 手动 `unlock_with_value` | 裸值管理，无自动续期 |
| [runner.rs:359](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/migration/runner.rs#L359) | `try_lock_with_value` + 手动构建 `LockGuard` + `start_auto_renew_task` | 手动拼装 LockGuard，重复逻辑 |
| [node.rs:323](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/cluster/node.rs#L323) | `lock(key, LockOptions::default())` | 正确使用，无需改动 |

---

## 二、重构方案（参考 Redisson 范式）

### 2.1 新 API 设计

```
LockManager
├── lock(key)                          → Result<LockGuard>    // 阻塞式，默认看门狗续期
├── lock(key, LockOptions)             → Result<LockGuard>    // 阻塞式，自定义选项
├── try_lock(key)                      → Result<Option<LockGuard>>  // 非阻塞，默认看门狗续期
├── try_lock(key, LockOptions)         → Result<Option<LockGuard>>  // 非阻塞，自定义选项
├── is_locked(key)                     → Result<bool>
└── remaining_ttl(key)                 → Result<Option<Duration>>

LockGuard (RAII 自动释放 + 可选看门狗续期)
├── unlock()                           // 手动释放（Drop 也会自动释放）
├── extend(duration)                   // 手动续期
├── is_valid()                         // 检查是否有效
├── key() / lock_value()               // 访问器
├── remaining_ttl()                    // 剩余时间
└── Drop                               // 自动释放

LockOptions
├── wait_time: Option<Duration>        // 最长等待时间（仅 lock 使用）
├── lease_time: Option<Duration>       // 锁持有时间（None=看门狗续期，Some=固定过期不续期）
├── retry_interval: Option<Duration>   // 重试间隔
└── retry_times: Option<u32>           // 重试次数（仅 lock 使用）
```

### 2.2 看门狗机制（参考 Redisson）

| 调用方式 | lease_time | 看门狗续期 | 自动释放 |
|----------|------------|------------|----------|
| `lock(key)` | 未指定 (None) | **启用** - 默认 30s TTL，自动续期 | Drop 自动释放 |
| `lock(key, opts.with_lease_time(10s))` | 10s | **禁用** - 10s 后强制过期 | Drop 尝试释放 + TTL 保底 |
| `try_lock(key)` | 未指定 (None) | **启用** - 默认 30s TTL，自动续期 | Drop 自动释放 |
| `try_lock(key, opts.with_lease_time(10s))` | 10s | **禁用** - 10s 后强制过期 | Drop 尝试释放 + TTL 保底 |

### 2.3 删除的 API

| 删除的 API | 原因 |
|------------|------|
| `try_lock(key) -> Result<bool>` | 返回裸 bool 无法管理锁生命周期 |
| `try_lock_with_value(key) -> Result<(bool, Option<String>)>` | 返回裸值，应由 `try_lock` 返回 `Option<LockGuard>` |
| `unlock(key)` | 不安全，不验证锁值 |
| `extend(key, duration)` | 不安全，不验证锁值 |
| `extend_with_value(key, lock_value, duration)` | 不再暴露裸值 API |
| `unlock_with_value(key, lock_value)` | 不再暴露裸值 API |
| `LockGuard::new()` | 私有化，仅通过 `lock` / `try_lock` 创建 |
| `LockGuard::start_auto_renew_task()` | 私有化，由内部自动管理 |
| `LockGuard::start_auto_renew()` / `stop_auto_renew()` | 由 lease_time 自动决定 |

---

## 三、具体改动文件

### 3.1 核心文件改动

#### 文件 1: `crates/libs/cmx-infra/cmx-buffer/src/lock/manager.rs`

**LockOptions 重构**：
```rust
#[derive(Debug, Clone, Default)]
pub struct LockOptions {
    pub wait_time: Option<Duration>,
    pub lease_time: Option<Duration>,
    pub retry_interval: Option<Duration>,
    pub retry_times: Option<u32>,
}

impl LockOptions {
    pub fn new() -> Self { Self::default() }
    pub fn with_wait_time(mut self, duration: Duration) -> Self { ... }
    pub fn with_lease_time(mut self, duration: Duration) -> Self { ... }
    pub fn with_retry_interval(mut self, interval: Duration) -> Self { ... }
    pub fn with_retry_times(mut self, times: u32) -> Self { ... }
}
```

**LockManager 重构**：
- `lock(key)` → `lock(key, LockOptions::default())` 的便捷方法
- `lock(key, options)` → 阻塞式，根据 `lease_time` 决定是否启动看门狗
- `try_lock(key)` → `try_lock(key, LockOptions::default())` 的便捷方法
- `try_lock(key, options)` → 非阻塞，返回 `Result<Option<LockGuard>>`，根据 `lease_time` 决定是否启动看门狗
- 删除 `try_lock_with_value`、`unlock`、`unlock_with_value`、`extend`、`extend_with_value` 公共方法
- 保留 `is_locked`、`remaining_ttl`
- 将 `unlock_with_value` 和 `extend_with_value` 改为 `pub(crate)` 供 `LockGuard` 内部使用

**LockGuard 重构**：
- `new()` 改为 `pub(crate)`
- `start_auto_renew_task()` 改为 `pub(crate)`
- 删除 `start_auto_renew()`、`stop_auto_renew()`、`is_auto_renew_enabled()`
- 新增 `lease_time: Option<Duration>` 字段，决定是否启动看门狗
- 保留 `unlock()`、`extend()`、`is_valid()`、`key()`、`lock_value()`、`remaining_ttl()`
- `Drop` 保持不变

#### 文件 2: `crates/libs/cmx-infra/cmx-buffer/src/lock/mod.rs`

- 更新 `pub use` 导出（无变化，仍然是 `LockGuard, LockManager, LockOptions`）

#### 文件 3: `crates/libs/cmx-infra/cmx-buffer/src/lib.rs`

- 无变化

#### 文件 4: `crates/libs/cmx-infra/cmx-buffer/src/config.rs`

- `LockConfig` 无变化（`expire_seconds`、`retry_times`、`retry_interval_ms`、`renew_threshold` 保持）

### 3.2 业务调用方改动

#### 文件 5: `crates/libs/cmx-plugin/src/service/utils.rs` (第 292 行)

**改动前**：
```rust
match lm.try_lock_with_value(&lock_key).await {
    Ok((true, Some(lock_value))) => {
        create_plugin_tables(...).await?;
        lm.unlock_with_value(&lock_key, &lock_value).await?;
    }
    Ok(_) => { ... }
    Err(e) => { ... }
}
```

**改动后**：
```rust
match lm.try_lock(&lock_key).await {
    Ok(Some(_guard)) => {
        tracing::info!("获取DDL锁成功，本实例负责创建/升级表: {}", plugin_id);
        create_plugin_tables(...).await?;
        // _guard Drop 自动释放，无需手动 unlock
    }
    Ok(None) => {
        tracing::info!("其他实例正在创建/升级表，跳过DDL: {}", plugin_id);
    }
    Err(e) => {
        tracing::warn!("锁服务异常: {}，继续创建/升级表", e);
        create_plugin_tables(...).await?;
    }
}
```

#### 文件 6: `crates/libs/cmx-infra/cmx-database/src/migration/runner.rs` (第 347-380 行)

**改动前**：手动 `try_lock_with_value` + 手动构建 `LockGuard` + `start_auto_renew_task`

**改动后**：
```rust
async fn try_acquire_migration_lock(&self) -> MigrationResult<Option<cmx_buffer::LockGuard>> {
    let lock_manager = match &self.lock_manager {
        Some(lm) => lm,
        None => { return Ok(None); }
    };
    match lock_manager.try_lock(MIGRATION_LOCK_KEY).await {
        Ok(Some(guard)) => {
            info!("成功获取数据库迁移分布式锁");
            Ok(Some(guard))
        }
        Ok(None) => {
            info!("其他节点正在执行数据库迁移");
            Ok(None)
        }
        Err(e) => {
            warn!("检查迁移锁失败: {:?}", e);
            Err(MigrationError::LockAcquireFailed)
        }
    }
}
```

**改动前 (第 404 行)**：`try_lock_with_value` + 获取到锁后立即丢弃（不释放）

**改动后**：使用 `lock_manager.is_locked(MIGRATION_LOCK_KEY)` 检查锁状态，不再尝试获取

#### 文件 7: `crates/libs/cmx-plugin/src/cluster/node.rs` (第 323 行)

**无改动**：已经使用 `lock(key, LockOptions::default())`，完全兼容

### 3.3 测试代码改动

#### 文件 8: `crates/libs/cmx-infra/cmx-buffer/tests/integration_test.rs`

| 测试 | 改动 |
|------|------|
| `test_lock_try_lock` | 改用 `try_lock` 返回 `Option<LockGuard>`，不再用 `unlock(key)` |
| `test_lock_guard` | 无改动（已用 `lock` + `LockOptions`） |
| `test_lock_extend` | 无改动（已用 `lock` + `guard.extend()`） |
| `test_lock_auto_release` | 无改动 |

---

## 四、实施步骤

### Step 1: 重构 `LockOptions`
- 添加 `wait_time`、`lease_time` 字段及 builder 方法

### Step 2: 重构 `LockGuard`
- 添加 `lease_time` 字段
- 将 `new()` 和 `start_auto_renew_task()` 改为 `pub(crate)`
- 删除 `start_auto_renew()`、`stop_auto_renew()`、`is_auto_renew_enabled()`
- 修改 `start_auto_renew_task()` 逻辑：仅当 `lease_time == None` 时启动看门狗

### Step 3: 重构 `LockManager`
- 新 `try_lock` 返回 `Result<Option<LockGuard>>`
- 新 `lock` 根据 `lease_time` 决定看门狗
- 旧方法改为 `pub(crate)` 或删除

### Step 4: 更新业务调用方
- `utils.rs`: 简化锁使用
- `runner.rs`: 简化锁使用

### Step 5: 更新测试代码
- 适配新 API

### Step 6: 编译验证
- `rtk cargo check` 确保编译通过
- `rtk cargo clippy` 确保无警告

---

## 五、风险与注意事项

1. **`runner.rs` 中 `wait_for_migration_lock`**：当前尝试获取锁后立即释放（用于检测锁是否被释放）。重构后改为 `is_locked()` 检查，语义更清晰
2. **`LockGuard::new()` 私有化**：`runner.rs` 中手动创建 `LockGuard` 的代码需要全部替换为 `try_lock()`
3. **向后不兼容**：不考虑兼容，所有调用方同步修改
