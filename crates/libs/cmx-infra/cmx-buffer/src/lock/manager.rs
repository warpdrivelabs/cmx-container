use crate::client::RedisClient;
use crate::config::LockConfig;
use crate::error::{Error, Result};
use crate::logging::LockLog;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// 分布式锁管理器

/// 作者: AI Assistant
/// 日期: 2026-03-16
/// 分布式锁管理器
#[derive(Clone)]
pub struct LockManager {
    client: RedisClient,
    config: LockConfig,
}

impl LockManager {
    /// 创建新的锁管理器
    pub fn new(client: RedisClient, config: LockConfig) -> Self {
        Self { client, config }
    }

    /// 从 Redis 客户端创建锁管理器（使用默认配置）
    pub fn new_with_default_config(client: RedisClient) -> Self {
        Self {
            client,
            config: LockConfig::new(),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &LockConfig {
        &self.config
    }

    /// 构建锁的键名
    fn build_lock_key(&self, key: &str) -> String {
        format!("{}lock:{}", self.client.key_prefix(), key)
    }

    /// 生成唯一的锁值
    fn generate_lock_value() -> String {
        Uuid::new_v4().to_string()
    }

    /// 尝试获取锁（立即返回）
    ///
    /// # 参数
    /// * `key` - 锁键
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否获取成功
    pub async fn try_lock(&self, key: &str) -> Result<bool> {
        let (success, _) = self.try_lock_with_value(key).await?;
        Ok(success)
    }

    /// 尝试获取锁（立即返回，返回锁值）
    ///
    /// # 参数
    /// * `key` - 锁键
    ///
    /// # 返回值
    /// * `Result<(bool, Option<String>)>` - 是否获取成功及锁值
    pub async fn try_lock_with_value(&self, key: &str) -> Result<(bool, Option<String>)> {
        let lock_key = self.build_lock_key(key);
        let lock_value = Self::generate_lock_value();
        
        let mut conn = self.client.get_connection().await?;
        
        let result: Option<()> = redis::cmd("SET")
            .arg(&lock_key)
            .arg(&lock_value)
            .arg("NX")
            .arg("EX")
            .arg(self.config.expire_seconds)
            .query_async(&mut *conn)
            .await
            .ok();
        
        match result {
            Some(_) => {
                LockLog::lock_acquired(&lock_key);
                Ok((true, Some(lock_value)))
            }
            None => {
                LockLog::lock_failed(&lock_key, "键已存在");
                Ok((false, None))
            }
        }
    }

    /// 获取锁（带重试机制）
    ///
    /// # 参数
    /// * `key` - 锁键
    ///
    /// # 返回值
    /// * `Result<LockGuard>` - 锁守卫
    pub async fn lock(&self, key: &str) -> Result<LockGuard> {
        let lock_key = self.build_lock_key(key);
        
        for attempt in 0..self.config.retry_times {
            let (success, lock_value) = self.try_lock_with_value(key).await?;
            
            if success {
                let guard = LockGuard::new(
                    key.to_string(),
                    lock_value.unwrap(),
                    self.client.clone(),
                    self.config.clone(),
                );
                
                guard.start_auto_renew_task().await;
                
                return Ok(guard);
            }
            
            if attempt < self.config.retry_times - 1 {
                LockLog::lock_failed(&lock_key, "重试获取锁");
                tokio::time::sleep(self.config.retry_interval_duration()).await;
            }
        }
        
        LockLog::lock_conflict(&lock_key);
        Err(Error::LockConflictError(format!(
            "获取锁失败: {}, 已重试 {} 次",
            lock_key,
            self.config.retry_times
        )))
    }

    /// 释放锁（需要提供锁值以验证所有权）
    ///
    /// # 参数
    /// * `key` - 锁键
    /// * `lock_value` - 锁值
    ///
    /// # 返回值
    /// * `Result<()>` - 释放结果
    pub async fn unlock_with_value(&self, key: &str, lock_value: &str) -> Result<()> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.get_connection().await?;
        
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
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        if result > 0 {
            LockLog::lock_released(&lock_key);
            Ok(())
        } else {
            LockLog::lock_failed(&lock_key, "锁不存在或已被释放");
            Err(Error::LockError("锁不存在或已被释放".to_string()))
        }
    }

    /// 释放锁（使用旧方式，不验证锁值）
    ///
    /// # 参数
    /// * `key` - 锁键
    ///
    /// # 返回值
    /// * `Result<()>` - 释放结果
    pub async fn unlock(&self, key: &str) -> Result<()> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.get_connection().await?;
        
        let result: i64 = redis::cmd("DEL")
            .arg(&lock_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        if result > 0 {
            LockLog::lock_released(&lock_key);
            Ok(())
        } else {
            LockLog::lock_failed(&lock_key, "锁不存在或已被释放");
            Err(Error::LockError("锁不存在或已被释放".to_string()))
        }
    }

    /// 延长锁的过期时间（需要提供锁值以验证所有权）
    ///
    /// # 参数
    /// * `key` - 锁键
    /// * `lock_value` - 锁值
    /// * `duration` - 新的过期时间
    ///
    /// # 返回值
    /// * `Result<()>` - 操作结果
    pub async fn extend_with_value(&self, key: &str, lock_value: &str, duration: Duration) -> Result<()> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.get_connection().await?;
        
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
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        if result > 0 {
            LockLog::lock_renewed(&lock_key, duration.as_secs());
            Ok(())
        } else {
            Err(Error::LockError("锁不存在或已过期".to_string()))
        }
    }

    /// 延长锁的过期时间
    ///
    /// # 参数
    /// * `key` - 锁键
    /// * `duration` - 新的过期时间
    ///
    /// # 返回值
    /// * `Result<()>` - 操作结果
    pub async fn extend(&self, key: &str, duration: Duration) -> Result<()> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.get_connection().await?;
        
        let result: i64 = redis::cmd("EXPIRE")
            .arg(&lock_key)
            .arg(duration.as_secs())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        if result > 0 {
            LockLog::lock_renewed(&lock_key, duration.as_secs());
            Ok(())
        } else {
            Err(Error::LockError("锁不存在或已过期".to_string()))
        }
    }

    /// 检查锁是否仍然有效
    ///
    /// # 参数
    /// * `key` - 锁键
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否有效
    pub async fn is_locked(&self, key: &str) -> Result<bool> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.get_connection().await?;
        let result: u64 = redis::cmd("EXISTS")
            .arg(&lock_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        Ok(result > 0)
    }

    /// 获取锁的剩余时间
    ///
    /// # 参数
    /// * `key` - 锁键
    ///
    /// # 返回值
    /// * `Option<Duration>` - 剩余时间
    pub async fn remaining_ttl(&self, key: &str) -> Result<Option<Duration>> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.get_connection().await?;
        let result: i64 = redis::cmd("TTL")
            .arg(&lock_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        if result > 0 {
            Ok(Some(Duration::from_secs(result as u64)))
        } else {
            Ok(None)
        }
    }
}

/// 分布式锁守卫，确保作用域结束时自动释放锁，支持自动续期
pub struct LockGuard {
    key: String,
    lock_value: String,
    client: RedisClient,
    config: LockConfig,
    released: Arc<AtomicBool>,
    auto_renew: Arc<AtomicBool>,
}

impl LockGuard {
    /// 创建新的锁守卫
    pub fn new(key: String, lock_value: String, client: RedisClient, config: LockConfig) -> Self {
        Self {
            key,
            lock_value,
            client,
            config,
            released: Arc::new(AtomicBool::new(false)),
            auto_renew: Arc::new(AtomicBool::new(true)),
        }
    }

    /// 启动自动续期
    /// 当锁的剩余时间低于 renew_threshold * expire_duration 时自动续期
    pub fn start_auto_renew(&self) {
        self.auto_renew.store(true, Ordering::SeqCst);
    }

    /// 停止自动续期
    pub fn stop_auto_renew(&self) {
        self.auto_renew.store(false, Ordering::SeqCst);
    }

    /// 检查自动续期是否启用
    pub fn is_auto_renew_enabled(&self) -> bool {
        self.auto_renew.load(Ordering::SeqCst)
    }

    /// 手动释放锁
    pub async fn unlock(self) -> Result<()> {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.auto_renew.store(false, Ordering::SeqCst);
            let manager = LockManager::new(self.client.clone(), self.config.clone());
            manager.unlock_with_value(&self.key, &self.lock_value).await?;
        }
        Ok(())
    }

    /// 延长锁的过期时间
    pub async fn extend(&self, duration: Duration) -> Result<()> {
        if !self.released.load(Ordering::SeqCst) {
            let manager = LockManager::new(self.client.clone(), self.config.clone());
            manager.extend_with_value(&self.key, &self.lock_value, duration).await?;
        }
        Ok(())
    }

    /// 检查锁是否仍然有效
    pub fn is_valid(&self) -> bool {
        !self.released.load(Ordering::SeqCst)
    }

    /// 获取锁的键
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 获取锁的值
    pub fn lock_value(&self) -> &str {
        &self.lock_value
    }

    /// 获取锁的剩余时间
    pub async fn remaining_ttl(&self) -> Result<Option<Duration>> {
        let lock_key = format!("{}lock:{}", self.client.key_prefix(), self.key);
        let mut conn = self.client.get_connection().await?;
        let result: i64 = redis::cmd("TTL")
            .arg(&lock_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;
        
        if result > 0 {
            Ok(Some(Duration::from_secs(result as u64)))
        } else {
            Ok(None)
        }
    }

    /// 检查是否需要续期（基于 renew_threshold）
    /// 需要在获取锁后启动自动续期任务
    pub async fn start_auto_renew_task(&self) {
        if !self.auto_renew.load(Ordering::SeqCst) {
            return;
        }

        let key = self.key.clone();
        let lock_value = self.lock_value.clone();
        let client = self.client.clone();
        let config = self.config.clone();
        let released = self.released.clone();
        let auto_renew = self.auto_renew.clone();

        tokio::spawn(async move {
            let renew_interval = Duration::from_secs(config.expire_seconds / 2);
            let threshold_secs = (config.expire_seconds as f64 * config.renew_threshold) as u64;

            loop {
                if released.load(Ordering::SeqCst) || !auto_renew.load(Ordering::SeqCst) {
                    break;
                }

                tokio::time::sleep(renew_interval).await;

                if released.load(Ordering::SeqCst) || !auto_renew.load(Ordering::SeqCst) {
                    break;
                }

                let lock_key = format!("{}lock:{}", client.key_prefix(), key);
                
                let mut conn = match client.get_connection().await {
                    Ok(c) => c,
                    Err(_) => break,
                };

                let ttl: i64 = redis::cmd("TTL")
                    .arg(&lock_key)
                    .query_async(&mut *conn)
                    .await.unwrap_or(-1);

                if ttl < 0 {
                    break;
                }

                if (ttl as u64) < threshold_secs {
                    let lua_script = r#"
                        if redis.call("get", KEYS[1]) == ARGV[1] then
                            return redis.call("expire", KEYS[1], ARGV[2])
                        else
                            return 0
                        end
                    "#;

                    let mut conn = match client.get_connection().await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!(key = %key, error = %e, "获取连接失败");
                            break;
                        }
                    };

                    let _: i64 = match redis::cmd("EVAL")
                        .arg(lua_script)
                        .arg(1)
                        .arg(&lock_key)
                        .arg(lock_value.as_str())
                        .arg(config.expire_seconds)
                        .query_async(&mut *conn)
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
        if !self.released.swap(true, Ordering::SeqCst) {
            let key = self.key.clone();
            let lock_value = self.lock_value.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            
            tokio::spawn(async move {
                let manager = LockManager::new(client, config);
                if let Err(e) = manager.unlock_with_value(&key, &lock_value).await {
                    tracing::warn!(key = %key, error = %e, "自动释放锁失败");
                }
            });
        }
    }
}
