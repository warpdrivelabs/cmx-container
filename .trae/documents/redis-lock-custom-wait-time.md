# Redis 分布式锁 - 自定义等待时间优化计划

## 目标

修改 `LockManager::lock()` 方法，支持在调用时传入自定义的等待参数（重试次数、重试间隔），不再强制使用全局 `LockConfig` 的配置。只有未显式设置时才回退到全局配置。

***

## 设计方案

### 新增 `LockOptions` 结构体

在 `manager.rs` 中新增 `LockOptions`，用于封装单次 lock 调用的自定义参数：

```rust
/// 锁获取选项，用于在 lock 调用时覆盖全局配置
///
/// 未设置的字段将使用全局 LockConfig 中的默认值
#[derive(Debug, Clone, Default)]
pub struct LockOptions {
    /// 自定义重试次数，None 使用全局配置
    pub retry_times: Option<u32>,
    /// 自定义重试间隔，None 使用全局配置
    pub retry_interval: Option<Duration>,
}
```

并提供 builder 方法：

* `LockOptions::new()` — 创建空选项（全部使用全局配置）

* `LockOptions::with_retry_times(times)` — 设置重试次数

* `LockOptions::with_retry_interval(duration)` — 设置重试间隔

### 修改 `lock()` 方法签名

**之前**：

```rust
pub async fn lock(&self, key: &str) -> Result<LockGuard>
```

**之后**：

```rust
pub async fn lock(&self, key: &str, options: impl Into<LockOptions>) -> Result<LockGuard>
```

使用 `impl Into<LockOptions>` 使得：

* 传 `LockOptions::default()` 或不传等效（使用全局配置）

* 传自定义 `LockOptions` 覆盖特定字段

**内部逻辑**：从 `options` 和 `self.config` 合并出实际使用的参数：

```rust
let retry_times = options.retry_times.unwrap_or(self.config.retry_times);
let retry_interval = options.retry_interval.unwrap_or(self.config.retry_interval_duration());
```

***

## 修改文件清单

### 1. `cmx-buffer/src/lock/manager.rs`（核心修改）

* **新增** `LockOptions` 结构体及其 builder 方法（约 30 行）

* **修改** `lock()` 方法签名，接收 `options: impl Into<LockOptions>` 参数

* **修改** `lock()` 方法内部逻辑，从 options 解析实际重试参数

* **导出** `LockOptions` 在 `pub` 可见

### 2. `cmx-buffer/src/lock/mod.rs`（导出修改）

* **修改** `pub use` 行，新增导出 `LockOptions`

### 3. `cmx-buffer/src/lib.rs`（公共导出修改）

* **修改** `pub use lock::{...}` 行，新增导出 `LockOptions`

### 4. `cmx-buffer/tests/integration_test.rs`（测试修改）

所有 `lock_manager.lock(key).await` 调用处需更新：

| 行号  | 当前代码                           | 修改后                                                    |
| --- | ------------------------------ | ------------------------------------------------------ |
| 324 | `lock_manager.lock(key).await` | `lock_manager.lock(key, LockOptions::default()).await` |
| 351 | `lock_manager.lock(key).await` | `lock_manager.lock(key, LockOptions::default()).await` |
| 376 | `lock_manager.lock(key).await` | `lock_manager.lock(key, LockOptions::default()).await` |

> 注：`try_lock` 和 `try_lock_with_value` 为非阻塞方法，无需修改。

### 5. 不需要修改的文件

以下调用方使用的是 `try_lock_with_value`（非阻塞），**无需修改**：

* `cmx-database/src/migration/runner.rs`

* `cmx-plugin/src/service/install.rs`

* `cmx-plugin/src/service/upgrade.rs`

***

## 实施步骤

1. **修改** **`manager.rs`**：新增 `LockOptions` 结构体，修改 `lock()` 方法签名和实现
2. **修改** **`mod.rs`**：更新导出列表
3. **修改** **`lib.rs`**：更新公共导出列表
4. **修改** **`integration_test.rs`**：更新所有 `lock()` 调用点
5. **编译验证**：运行 `rtk cargo check` 确认编译通过

