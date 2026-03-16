use crate::client::RedisClient;
use crate::config::LockConfig;
use crate::error::{Error, Result};
use crate::logging::LockLog;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: 分布式锁管理器
 */

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

    /**
     * 尝试获取锁（立即返回）
     * @param key 锁键
     * @return Result<bool> 是否获取成功
     */
    pub async fn try_lock(&self, key: &str) -> Result<bool> {
        let lock_key = self.build_lock_key(key);
        let lock_value = Self::generate_lock_value();
        
        let mut conn = self.client.inner().clone();
        
        let result: Option<()> = redis::cmd("SET")
            .arg(&lock_key)
            .arg(&lock_value)
            .arg("NX")
            .arg("EX")
            .arg(self.config.expire_seconds)
            .query_async(&mut conn)
            .await
            .ok();
        
        match result {
            Some(_) => {
                LockLog::lock_acquired(&lock_key);
                Ok(true)
            }
            None => {
                LockLog::lock_failed(&lock_key, "键已存在");
                Ok(false)
            }
        }
    }

    /**
     * 获取锁（带重试机制）
     * @param key 锁键
     * @return Result<LockGuard> 锁守卫
     */
    pub async fn lock(&self, key: &str) -> Result<LockGuard> {
        let lock_key = self.build_lock_key(key);
        
        for attempt in 0..self.config.retry_times {
            if self.try_lock(key).await? {
                return Ok(LockGuard::new(
                    key.to_string(),
                    self.client.clone(),
                    self.config.clone(),
                ));
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

    /**
     * 释放锁
     * @param key 锁键
     * @return Result<()> 释放结果
     */
    pub async fn unlock(&self, key: &str) -> Result<()> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.inner().clone();
        
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
            .arg("")
            .query_async(&mut conn)
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

    /**
     * 延长锁的过期时间
     * @param key 锁键
     * @param duration 新的过期时间
     * @return Result<()> 操作结果
     */
    pub async fn extend(&self, key: &str, duration: Duration) -> Result<()> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.inner().clone();
        
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
            .arg("")
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

    /**
     * 检查锁是否仍然有效
     * @param key 锁键
     * @return Result<bool> 是否有效
     */
    pub async fn is_locked(&self, key: &str) -> Result<bool> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: u64 = redis::cmd("EXISTS")
            .arg(&lock_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;
        
        Ok(result > 0)
    }

    /**
     * 获取锁的剩余时间
     * @param key 锁键
     * @return Option<Duration> 剩余时间
     */
    pub async fn remaining_ttl(&self, key: &str) -> Result<Option<Duration>> {
        let lock_key = self.build_lock_key(key);
        
        let mut conn = self.client.inner().clone();
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

/// 分布式锁守卫，确保作用域结束时自动释放锁
pub struct LockGuard {
    key: String,
    manager: LockManager,
    released: Arc<AtomicBool>,
}

impl LockGuard {
    /// 创建新的锁守卫
    pub fn new(key: String, client: RedisClient, config: LockConfig) -> Self {
        Self {
            key,
            manager: LockManager::new(client, config),
            released: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 手动释放锁
    pub async fn unlock(self) -> Result<()> {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.manager.unlock(&self.key).await?;
        }
        Ok(())
    }

    /// 延长锁的过期时间
    pub async fn extend(&self, duration: Duration) -> Result<()> {
        if !self.released.load(Ordering::SeqCst) {
            self.manager.extend(&self.key, duration).await?;
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
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            let key = self.key.clone();
            let client = self.manager.client.clone();
            let config = self.manager.config.clone();
            
            tokio::spawn(async move {
                let manager = LockManager::new(client, config);
                if let Err(e) = manager.unlock(&key).await {
                    tracing::warn!(key = %key, error = %e, "自动释放锁失败");
                }
            });
        }
    }
}
