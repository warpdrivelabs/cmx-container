use crate::client::RedisClient;
use crate::config::LockConfig;
use crate::error::{Error, Result};
use crate::logging::LockLog;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// 锁获取选项，用于在 `lock` / `try_lock` 调用时覆盖全局配置。
///
/// # 看门狗机制（参考 Redisson）
///
/// - `lease_time = None`（默认）：启用看门狗自动续期，锁不会过期直到显式释放。
/// - `lease_time = Some(duration)`：禁用看门狗，锁在指定时间后强制过期。
///
/// # 等待时间（仅 `try_lock_with_options` 使用）
///
/// - `wait_time = None`（默认）：不等待，立即返回。
/// - `wait_time = Some(duration)`：限时等待，超时未获取到则返回 `None`。
#[derive(Debug, Clone, Default)]
pub struct LockOptions {
    /// 最长等待时间。仅 `try_lock_with_options` 使用，`None` 表示不等待。
    pub wait_time: Option<Duration>,
    /// 锁持有时间。`None` 表示启用看门狗续期，`Some` 表示固定过期时间。
    pub lease_time: Option<Duration>,
    /// 重试间隔。用于 `lock` 和 `try_lock_with_options` 的重试等待。
    pub retry_interval: Option<Duration>,
}

impl LockOptions {
    /// 创建一个新的 `LockOptions` 实例，所有字段均为默认值。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最长等待时间。
    ///
    /// 仅 `try_lock_with_options` 使用。设置后会在指定时间内重试获取锁。
    pub fn with_wait_time(mut self, duration: Duration) -> Self {
        self.wait_time = Some(duration);
        self
    }

    /// 设置锁持有时间。
    ///
    /// 设置后禁用看门狗，锁在指定时间后强制过期。
    pub fn with_lease_time(mut self, duration: Duration) -> Self {
        self.lease_time = Some(duration);
        self
    }

    /// 设置重试间隔。
    ///
    /// 用于控制 `lock` 和 `try_lock_with_options` 在获取失败后的等待时间。
    pub fn with_retry_interval(mut self, interval: Duration) -> Self {
        self.retry_interval = Some(interval);
        self
    }
}

/// Redis 分布式锁管理器，提供基于 Redisson 范式的锁获取和释放机制。
///
/// # 核心特性
///
/// - **RAII 自动释放**：`LockGuard` 在离开作用域时自动释放锁。
/// - **看门狗自动续期**：未指定 `lease_time` 时启用，防止锁在业务处理期间过期。
/// - **安全的值比较释放**：使用 Lua 脚本保证锁值匹配才释放，防止误删他人锁。
///
/// # 与 Redisson 的方法对照
///
/// | Redisson | cmx-buffer | 等待行为 |
/// |----------|------------|----------|
/// | `lock()` | `lock(key)` | 无限等待 |
/// | `lock(leaseTime, unit)` | `lock_with_options(key, opts.with_lease_time())` | 无限等待，持有固定时间 |
/// | `tryLock()` | `try_lock(key)` | 不等待 |
/// | `tryLock(waitTime, unit)` | `try_lock_with_options(key, opts.with_wait_time())` | 限时等待 |
#[derive(Clone)]
pub struct LockManager {
    client: RedisClient,
    config: LockConfig,
}

impl LockManager {
    /// 创建一个新的 `LockManager` 实例。
    ///
    /// # Arguments
    ///
    /// * `client` - Redis 客户端实例。
    /// * `config` - 锁配置，包含全局默认过期时间和续期参数。
    pub fn new(client: RedisClient, config: LockConfig) -> Self {
        Self { client, config }
    }

    /// 使用默认配置创建一个新的 `LockManager` 实例。
    ///
    /// 默认配置参见 `LockConfig::new()`。
    ///
    /// # Arguments
    ///
    /// * `client` - Redis 客户端实例。
    pub fn new_with_default_config(client: RedisClient) -> Self {
        Self {
            client,
            config: LockConfig::new(),
        }
    }

    /// 返回锁管理器的全局配置引用。
    pub fn config(&self) -> &LockConfig {
        &self.config
    }

    /// 返回锁管理器使用的 Redis 客户端引用。
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /// 根据业务键构建完整的 Redis 锁键。
    fn build_lock_key(&self, key: &str) -> String {
        format!("{}lock:{}", self.client.key_prefix(), key)
    }

    /// 生成一个唯一的锁值，用于安全释放时验证持有者身份。
    fn generate_lock_value() -> String {
        Uuid::new_v4().to_string()
    }

    /// 阻塞式获取锁，无限等待直到成功，默认启用看门狗自动续期。
    ///
    /// 等价于 Redisson 的 `lock()`：无限期阻塞等待，直到获取锁。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键，用于标识需要互斥访问的资源。
    ///
    /// # Returns
    ///
    /// 成功时返回 `LockGuard`。失败时返回错误（Redis 连接异常）。
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // 无限等待，看门狗自动续期
    /// let guard = lock_manager.lock("my_key").await?;
    /// ```
    pub async fn lock(&self, key: &str) -> Result<LockGuard> {
        self.lock_with_options(key, LockOptions::default()).await
    }

    /// 阻塞式获取锁，无限等待直到成功，可指定 `leaseTime`。
    ///
    /// 等价于 Redisson 的 `lock(leaseTime, TimeUnit)`：
    /// 无限期阻塞等待，但锁只持有 `leaseTime` 时间后强制过期（禁用看门狗）。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    /// * `options` - 锁获取选项，可指定 `lease_time` 等参数。
    ///
    /// # Returns
    ///
    /// 成功时返回 `LockGuard`。失败时返回错误。
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // 无限等待，但锁只持有 10 秒
    /// let guard = lock_manager.lock_with_options("my_key", LockOptions::new()
    ///     .with_lease_time(Duration::from_secs(10)))
    ///     .await?;
    /// ```
    pub async fn lock_with_options(
        &self,
        key: &str,
        options: impl Into<LockOptions>,
    ) -> Result<LockGuard> {
        let options = options.into();
        let retry_interval = options
            .retry_interval
            .unwrap_or(self.config.retry_interval_duration());

        loop {
            match self.acquire_lock(key, &options).await {
                Ok(Some(guard)) => return Ok(guard),
                Ok(None) => {
                    LockLog::lock_failed(&self.build_lock_key(key), "重试获取锁");
                    tokio::time::sleep(retry_interval).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 非阻塞获取锁，立即返回，默认启用看门狗自动续期。
    ///
    /// 等价于 Redisson 的 `tryLock()`：立即尝试获取锁，获取不到直接返回 `None`。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    ///
    /// # Returns
    ///
    /// * `Ok(Some(LockGuard))` - 获取锁成功。
    /// * `Ok(None)` - 锁已被其他持有者占用。
    /// * `Err` - Redis 操作异常。
    ///
    /// # Examples
    ///
    /// ```ignore
    /// match lock_manager.try_lock("my_key").await {
    ///     Ok(Some(_guard)) => { /* 获取成功 */ }
    ///     Ok(None) => { /* 锁被占用 */ }
    ///     Err(e) => { /* 异常 */ }
    /// }
    /// ```
    pub async fn try_lock(&self, key: &str) -> Result<Option<LockGuard>> {
        self.acquire_lock(key, &LockOptions::default()).await
    }

    /// 限时等待获取锁，可指定 `waitTime` 和 `leaseTime`。
    ///
    /// 等价于 Redisson 的 `tryLock(waitTime, leaseTime, TimeUnit)`：
    ///
    /// - `wait_time` 设置时：限时等待，超时未获取到返回 `None`。
    /// - `wait_time` 不设置时：立即返回，等价于 `try_lock()`。
    /// - `lease_time` 设置时：禁用看门狗，锁在指定时间后强制过期。
    /// - `lease_time` 不设置时：启用看门狗自动续期。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    /// * `options` - 锁获取选项，可指定 `wait_time` 和 `lease_time`。
    ///
    /// # Returns
    ///
    /// * `Ok(Some(LockGuard))` - 在指定时间内获取锁成功。
    /// * `Ok(None)` - 超时未获取到锁，或 `wait_time` 未设置且锁被占用。
    /// * `Err` - Redis 操作异常。
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // 限时等待 5 秒，看门狗续期
    /// let guard = lock_manager.try_lock_with_options("my_key", LockOptions::new()
    ///     .with_wait_time(Duration::from_secs(5)))
    ///     .await?;
    ///
    /// // 限时等待 5 秒，锁只持有 10 秒
    /// let guard = lock_manager.try_lock_with_options("my_key", LockOptions::new()
    ///     .with_wait_time(Duration::from_secs(5))
    ///     .with_lease_time(Duration::from_secs(10)))
    ///     .await?;
    ///
    /// // 不等待（立即返回），锁只持有 10 秒
    /// let guard = lock_manager.try_lock_with_options("my_key", LockOptions::new()
    ///     .with_lease_time(Duration::from_secs(10)))
    ///     .await?;
    /// ```
    pub async fn try_lock_with_options(
        &self,
        key: &str,
        options: impl Into<LockOptions>,
    ) -> Result<Option<LockGuard>> {
        let options = options.into();

        match options.wait_time {
            Some(wait_time) => {
                let retry_interval = options
                    .retry_interval
                    .unwrap_or(self.config.retry_interval_duration());
                let start = std::time::Instant::now();

                loop {
                    match self.acquire_lock(key, &options).await {
                        Ok(Some(guard)) => return Ok(Some(guard)),
                        Ok(None) => {
                            let elapsed = start.elapsed();
                            if elapsed >= wait_time {
                                return Ok(None);
                            }
                            let remaining = wait_time - elapsed;
                            // 如果本次睡眠后下次的开始时间会超过 wait_time，
                            // 只睡眠 remaining（最后一次尝试）
                            let sleep_time = if elapsed + retry_interval > wait_time {
                                remaining
                            } else {
                                retry_interval
                            };
                            tokio::time::sleep(sleep_time).await;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
            None => self.acquire_lock(key, &options).await,
        }
    }

    /// 执行原子的 SET NX EX 操作获取锁。
    ///
    /// 内部方法，供 `lock`、`try_lock` 等公开方法调用。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    /// * `options` - 锁获取选项，包含 `lease_time` 等参数。
    ///
    /// # Returns
    ///
    /// * `Ok(Some(LockGuard))` - 获取锁成功。
    /// * `Ok(None)` - 锁已被其他持有者占用。
    /// * `Err` - Redis 操作异常。
    async fn acquire_lock(&self, key: &str, options: &LockOptions) -> Result<Option<LockGuard>> {
        let lock_key = self.build_lock_key(key);
        let lock_value = Self::generate_lock_value();
        let lease_seconds = options
            .lease_time
            .map(|d| d.as_secs())
            .unwrap_or(self.config.expire_seconds);

        let mut conn = self.client.get_connection();

        let result: std::result::Result<Option<String>, redis::RedisError> = redis::cmd("SET")
            .arg(&lock_key)
            .arg(&lock_value)
            .arg("NX")
            .arg("EX")
            .arg(lease_seconds)
            .query_async(&mut conn)
            .await;

        match result {
            Ok(Some(_)) => {
                LockLog::lock_acquired(&lock_key);

                let enable_watchdog = options.lease_time.is_none();

                let guard = LockGuard::new(
                    key.to_string(),
                    lock_value,
                    self.client.clone(),
                    self.config.clone(),
                    enable_watchdog,
                );

                if enable_watchdog {
                    guard.start_auto_renew_task().await;
                }

                Ok(Some(guard))
            }
            Ok(None) => {
                LockLog::lock_failed(&lock_key, "锁已被其他持有者占用");
                Ok(None)
            }
            Err(e) => {
                LockLog::lock_failed(&lock_key, "Redis错误");
                Err(Error::from(e))
            }
        }
    }

    /// 使用 Lua 脚本安全释放锁，仅当锁值匹配时才删除。
    ///
    /// 内部方法，供 `LockGuard` 的 `Drop` 和 `unlock` 调用。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    /// * `lock_value` - 释放时验证的锁值。
    ///
    /// # Returns
    ///
    /// 成功返回 `Ok(())`，失败时返回错误。
    pub(crate) async fn unlock_with_value(&self, key: &str, lock_value: &str) -> Result<()> {
        let lock_key = self.build_lock_key(key);

        let mut conn = self.client.get_connection();

        // Lua 脚本：仅当锁值匹配时才删除，保证释放操作的安全性
        let lua_script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
        "#;

        let result: i64 = redis::cmd("EVAL")
            .arg(lua_script)
            .arg(1)
            .arg(&lock_key)
            .arg(lock_value)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        if result > 0 {
            LockLog::lock_released(&lock_key);
        }
        Ok(())
    }

    /// 使用 Lua 脚本安全续期锁，仅当锁值匹配时才续期。
    ///
    /// 内部方法，供 `LockGuard::extend` 调用。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    /// * `lock_value` - 续期时验证的锁值。
    /// * `duration` - 续期后的新过期时间。
    ///
    /// # Returns
    ///
    /// 成功返回 `Ok(())`，失败时返回错误。
    pub(crate) async fn extend_with_value(
        &self,
        key: &str,
        lock_value: &str,
        duration: Duration,
    ) -> Result<()> {
        let lock_key = self.build_lock_key(key);

        let mut conn = self.client.get_connection();

        // Lua 脚本：仅当锁值匹配时才续期，保证续期操作的安全性
        let lua_script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("expire", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let result: i64 = redis::cmd("EVAL")
            .arg(lua_script)
            .arg(1)
            .arg(&lock_key)
            .arg(lock_value)
            .arg(duration.as_secs())
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        if result > 0 {
            LockLog::lock_renewed(&lock_key, duration.as_secs());
            Ok(())
        } else {
            Err(Error::LockError("锁不存在或已过期".to_string()))
        }
    }

    /// 检查指定键的锁是否已被占用。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - 锁已被占用。
    /// * `Ok(false)` - 锁未被占用。
    /// * `Err` - Redis 操作异常。
    pub async fn is_locked(&self, key: &str) -> Result<bool> {
        let lock_key = self.build_lock_key(key);

        let mut conn = self.client.get_connection();
        let result: u64 = redis::cmd("EXISTS")
            .arg(&lock_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(result > 0)
    }

    /// 获取指定锁的剩余 TTL（Time To Live）。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    ///
    /// # Returns
    ///
    /// * `Ok(Some(Duration))` - 锁存在，返回剩余生存时间。
    /// * `Ok(None)` - 锁不存在或已过期。
    /// * `Err` - Redis 操作异常。
    pub async fn remaining_ttl(&self, key: &str) -> Result<Option<Duration>> {
        let lock_key = self.build_lock_key(key);

        let mut conn = self.client.get_connection();
        let result: i64 = redis::cmd("TTL")
            .arg(&lock_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        if result > 0 {
            Ok(Some(Duration::from_secs(result as u64)))
        } else {
            Ok(None)
        }
    }
}

/// 分布式锁守卫，实现 RAII 自动释放机制。
///
/// 通过 `LockManager::lock()` 或 `LockManager::try_lock()` 获取。
/// 当 `lease_time` 未指定时启用看门狗自动续期。
/// 作用域结束时（`Drop`）自动释放锁。
///
/// # 看门狗机制
///
/// 看门狗后台任务会在锁即将过期时自动续期，防止业务处理期间锁过期：
///
/// - **启用条件**：`lease_time = None`（默认）。
/// - **续期间隔**：`expire_seconds / 2`。
/// - **续期阈值**：当 TTL 低于 `expire_seconds * renew_threshold` 时触发续期。
pub struct LockGuard {
    key: String,
    lock_value: String,
    client: RedisClient,
    config: LockConfig,
    released: Arc<AtomicBool>,
    enable_watchdog: bool,
}

impl LockGuard {
    /// 创建一个新的 `LockGuard` 实例。
    ///
    /// 内部方法，由 `LockManager::acquire_lock` 调用。
    ///
    /// # Arguments
    ///
    /// * `key` - 锁的业务键。
    /// * `lock_value` - 锁的唯一值，用于安全释放。
    /// * `client` - Redis 客户端。
    /// * `config` - 锁配置。
    /// * `enable_watchdog` - 是否启用看门狗续期。
    pub(crate) fn new(
        key: String,
        lock_value: String,
        client: RedisClient,
        config: LockConfig,
        enable_watchdog: bool,
    ) -> Self {
        Self {
            key,
            lock_value,
            client,
            config,
            released: Arc::new(AtomicBool::new(false)),
            enable_watchdog,
        }
    }

    /// 手动释放锁。
    ///
    /// 释放后锁会立即失效，后续调用 `is_valid()` 将返回 `false`。
    /// 建议使用 RAII 方式（让 `LockGuard` 离开作用域自动 `Drop`），除非需要立即释放。
    ///
    /// # Returns
    ///
    /// 成功返回 `Ok(())`，失败时返回错误（已释放或 Redis 异常）。
    pub async fn unlock(mut self) -> Result<()> {
        if !self.released.swap(true, Ordering::SeqCst) {
            let manager = LockManager::new(self.client.clone(), self.config.clone());
            manager
                .unlock_with_value(&self.key, &self.lock_value)
                .await?;
        }
        Ok(())
    }

    /// 手动续期锁的持有时间。
    ///
    /// 仅当锁未被释放且值匹配时续期成功。
    ///
    /// # Arguments
    ///
    /// * `duration` - 续期后的新过期时间。
    ///
    /// # Returns
    ///
    /// 成功返回 `Ok(())`，失败时返回错误（已释放、锁不存在或 Redis 异常）。
    pub async fn extend(&self, duration: Duration) -> Result<()> {
        if !self.released.load(Ordering::SeqCst) {
            let manager = LockManager::new(self.client.clone(), self.config.clone());
            manager
                .extend_with_value(&self.key, &self.lock_value, duration)
                .await?;
        }
        Ok(())
    }

    /// 检查锁是否仍然有效（未被释放）。
    ///
    /// # Returns
    ///
    /// * `true` - 锁未被释放。
    /// * `false` - 锁已被释放或从未成功获取。
    pub fn is_valid(&self) -> bool {
        !self.released.load(Ordering::SeqCst)
    }

    /// 返回锁的业务键。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 返回锁的唯一值。
    ///
    /// 锁值用于安全释放和续期时的验证。
    pub fn lock_value(&self) -> &str {
        &self.lock_value
    }

    /// 获取该锁的剩余 TTL。
    ///
    /// # Returns
    ///
    /// * `Ok(Some(Duration))` - 锁存在，返回剩余生存时间。
    /// * `Ok(None)` - 锁不存在或已过期。
    /// * `Err` - Redis 操作异常。
    pub async fn remaining_ttl(&self) -> Result<Option<Duration>> {
        let lock_key = format!("{}lock:{}", self.client.key_prefix(), self.key);
        let mut conn = self.client.get_connection();
        let result: i64 = redis::cmd("TTL")
            .arg(&lock_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        if result > 0 {
            Ok(Some(Duration::from_secs(result as u64)))
        } else {
            Ok(None)
        }
    }

    /// 启动看门狗自动续期任务。
    ///
    /// 后台定时检查锁的 TTL，当即将过期时自动续期。
    /// 任务在锁被释放或 Redis 连接异常时自动停止。
    pub(crate) async fn start_auto_renew_task(&self) {
        if !self.enable_watchdog {
            return;
        }

        let key = self.key.clone();
        let lock_value = self.lock_value.clone();
        let client = self.client.clone();
        let config = self.config.clone();
        let released = self.released.clone();

        tokio::spawn(async move {
            let renew_interval = Duration::from_secs(config.expire_seconds / 2);
            let threshold_secs = (config.expire_seconds as f64 * config.renew_threshold) as u64;

            loop {
                // 检查是否已释放
                if released.load(Ordering::SeqCst) {
                    break;
                }

                tokio::time::sleep(renew_interval).await;

                // 再次检查是否已释放
                if released.load(Ordering::SeqCst) {
                    break;
                }

                let lock_key = format!("{}lock:{}", client.key_prefix(), key);

                let mut conn = client.get_connection();

                let ttl: i64 = redis::cmd("TTL")
                    .arg(&lock_key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(-1);

                if ttl < 0 {
                    break;
                }

                // 当 TTL 低于阈值时触发续期
                if (ttl as u64) < threshold_secs {
                    let lua_script = r#"
                        if redis.call("get", KEYS[1]) == ARGV[1] then
                            return redis.call("expire", KEYS[1], ARGV[2])
                        else
                            return 0
                        end
                    "#;

                    let mut conn = client.get_connection();

                    let _: i64 = match redis::cmd("EVAL")
                        .arg(lua_script)
                        .arg(1)
                        .arg(&lock_key)
                        .arg(lock_value.as_str())
                        .arg(config.expire_seconds)
                        .query_async(&mut conn)
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(key = %key, error = %e, "自动续期失败");
                            break;
                        }
                    };
                }
            }
        });
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // 使用原子操作确保只释放一次
        if !self.released.swap(true, Ordering::SeqCst) {
            let key = self.key.clone();
            let lock_value = self.lock_value.clone();
            let client = self.client.clone();
            let config = self.config.clone();

            // 异步释放锁，不阻塞当前线程
            tokio::spawn(async move {
                let manager = LockManager::new(client, config);
                if let Err(e) = manager.unlock_with_value(&key, &lock_value).await {
                    tracing::warn!(key = %key, error = %e, "自动释放锁失败");
                }
            });
        }
    }
}
