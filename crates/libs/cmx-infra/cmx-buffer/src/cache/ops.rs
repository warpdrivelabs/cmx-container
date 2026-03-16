use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: 缓存操作模块 - 基础操作
 */

/// 缓存操作器
#[derive(Clone)]
pub struct CacheOps {
    client: RedisClient,
}

impl CacheOps {
    /// 创建新的缓存操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    // ==================== 基础操作 ====================

    /**
     * 设置缓存值
     * @param key 键
     * @param value 值
     */
    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("SET", &full_key);
        
        let mut conn = self.client.inner().clone();
        let _: () = conn.set(&full_key, value).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(())
    }

    /**
     * 获取缓存值
     * @param key 键
     * @return Option<String> 值
     */
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("GET", &full_key);
        
        let mut conn = self.client.inner().clone();
        let value: Option<String> = conn.get(&full_key).await.map_err(Error::from)?;
        
        timer.complete();
        
        debug!(key = %full_key, found = value.is_some(), "获取缓存");
        Ok(value)
    }

    /**
     * 删除缓存
     * @param key 键
     * @return bool 是否删除成功
     */
    pub async fn del(&self, key: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("DEL", &full_key);
        
        let mut conn = self.client.inner().clone();
        let result: u64 = conn.del(&full_key).await.map_err(Error::from)?;
        
        let affected = result > 0;
        timer.complete();
        Ok(affected)
    }

    /**
     * 批量删除缓存
     * @param keys 键列表
     * @return u64 删除的数量
     */
    pub async fn del_batch(&self, keys: &[&str]) -> Result<u64> {
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();
        let timer = OperationTimer::new("DEL_BATCH", "batch");
        
        let mut conn = self.client.inner().clone();
        let result: u64 = conn.del(full_keys.as_slice()).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(result)
    }

    /**
     * 检查键是否存在
     * @param key 键
     * @return bool 是否存在
     */
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: u64 = conn.exists(&full_key).await.map_err(Error::from)?;
        
        Ok(result > 0)
    }

    /**
     * 设置带过期时间的缓存
     * @param key 键
     * @param value 值
     * @param expire 过期时间
     */
    pub async fn set_ex(&self, key: &str, value: &str, expire: Duration) -> Result<()> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("SET_EX", &full_key);
        
        let mut conn = self.client.inner().clone();
        let _: () = conn.set_ex(&full_key, value, expire.as_secs()).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(())
    }

    // ==================== 序列化操作 ====================

    /**
     * 设置序列化的缓存值
     * @param key 键
     * @param value 值（可序列化）
     */
    pub async fn set_serialized<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.set(key, &json).await
    }

    /**
     * 获取并反序列化缓存值
     * @param key 键
     * @return Option<T> 反序列化后的值
     */
    pub async fn get_deserialized<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let value = self.get(key).await?;
        match value {
            Some(json) => {
                let deserialized: T = serde_json::from_str(&json)?;
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    // ==================== 批量操作 ====================

    /**
     * 批量获取缓存
     * @param keys 键列表
     * @return Vec<Option<String>> 值列表
     */
    pub async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<String>>> {
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();
        let timer = OperationTimer::new("MGET", "batch");
        
        let mut conn = self.client.inner().clone();
        let result: Vec<Option<String>> = conn.mget(full_keys.as_slice()).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(result)
    }

    /**
     * 批量设置缓存
     * @param items 键值对
     */
    pub async fn mset(&self, items: HashMap<&str, &str>) -> Result<()> {
        let timer = OperationTimer::new("MSET", "batch");
        
        let mut full_items: Vec<(String, String)> = Vec::new();
        for (k, v) in items {
            full_items.push((self.client.build_key(k), v.to_string()));
        }
        
        let mut conn = self.client.inner().clone();
        let _: () = conn.mset(full_items.as_slice()).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(())
    }

    // ==================== 自增/自减操作 ====================

    /**
     * 自增
     * @param key 键
     * @param delta 增量
     * @return i64 自增后的值
     */
    pub async fn incr(&self, key: &str, delta: i64) -> Result<i64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("INCR", &full_key);
        
        let mut conn = self.client.inner().clone();
        let value: i64 = conn.incr(&full_key, delta).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(value)
    }

    /**
     * 自减
     * @param key 键
     * @param delta 减量
     * @return i64 自减后的值
     */
    pub async fn decr(&self, key: &str, delta: i64) -> Result<i64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("DECR", &full_key);
        
        let mut conn = self.client.inner().clone();
        let value: i64 = conn.decr(&full_key, delta).await.map_err(Error::from)?;
        
        timer.complete();
        Ok(value)
    }

    // ==================== 键类型操作 ====================

    /**
     * 获取键的类型
     * @param key 键
     * @return String 键类型
     */
    pub async fn r#type(&self, key: &str) -> Result<String> {
        let full_key = self.client.build_key(key);
        
        let mut conn = self.client.inner().clone();
        let result: String = redis::cmd("TYPE")
            .arg(&full_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;
        
        Ok(result)
    }
}
