use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;
use redis::AsyncCommands;
use std::time::Duration;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: 缓存操作模块 - 过期时间管理
 */

/// TTL 操作器
pub struct TtlOps {
    client: RedisClient,
}

impl TtlOps {
    /// 创建新的 TTL 操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /**
     * 设置键的过期时间
     * @param key 键
     * @param duration 过期时间
     * @return bool 是否设置成功
     */
    pub async fn expire(&self, key: &str, duration: Duration) -> Result<bool> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("EXPIRE", &full_key);
        
        let mut conn = self.client.inner().clone();
        let result: bool = conn.expire(&full_key, duration.as_secs() as i64).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(result)
    }

    /**
     * 设置键的过期时间（Unix 时间戳）
     * @param key 键
     * @param timestamp Unix 时间戳（秒）
     * @return bool 是否设置成功
     */
    pub async fn expire_at(&self, key: &str, timestamp: i64) -> Result<bool> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: bool = redis::cmd("EXPIREAT")
            .arg(&full_key)
            .arg(timestamp)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;
        
        Ok(result)
    }

    /**
     * 移除键的过期时间（永不过期）
     * @param key 键
     * @return bool 是否移除成功
     */
    pub async fn persist(&self, key: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: bool = conn.persist(&full_key).await.map_err(Error::from)?;
        
        Ok(result)
    }

    /**
     * 获取键的剩余过期时间
     * @param key 键
     * @return Option<Duration> 剩余过期时间
     */
    pub async fn ttl(&self, key: &str) -> Result<Option<Duration>> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: i64 = conn.ttl(&full_key).await.map_err(Error::from)?;
        
        if result == -1 {
            Ok(None)
        } else if result == -2 {
            Ok(Some(Duration::ZERO))
        } else {
            Ok(Some(Duration::from_secs(result as u64)))
        }
    }

    /**
     * 获取键的精确剩余过期时间（毫秒）
     * @param key 键
     * @return Option<Duration> 剩余过期时间（毫秒精度）
     */
    pub async fn pttl(&self, key: &str) -> Result<Option<Duration>> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: i64 = redis::cmd("PTTL")
            .arg(&full_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;
        
        if result == -1 {
            Ok(None)
        } else if result == -2 {
            Ok(Some(Duration::ZERO))
        } else {
            Ok(Some(Duration::from_millis(result as u64)))
        }
    }

    /**
     * 设置键的值并同时设置过期时间
     * @param key 键
     * @param value 值
     * @param duration 过期时间
     */
    pub async fn set_with_ttl(&self, key: &str, value: &str, duration: Duration) -> Result<()> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let _: () = redis::cmd("SET")
            .arg(&full_key)
            .arg(value)
            .arg("EX")
            .arg(duration.as_secs())
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;
        
        Ok(())
    }

    /**
     * 设置键的值（仅当键不存在时）
     * @param key 键
     * @param value 值
     * @return bool 是否设置成功
     */
    pub async fn setnx(&self, key: &str, value: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: bool = conn.set_nx(&full_key, value).await.map_err(Error::from)?;
        
        Ok(result)
    }

    /**
     * 设置键的值（仅当键不存在时）并设置过期时间
     * @param key 键
     * @param value 值
     * @param duration 过期时间
     * @return bool 是否设置成功
     */
    pub async fn setnx_ex(&self, key: &str, value: &str, duration: Duration) -> Result<bool> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: Option<()> = redis::cmd("SET")
            .arg(&full_key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(duration.as_secs())
            .query_async(&mut conn)
            .await
            .ok();
        
        Ok(result.is_some())
    }
}
