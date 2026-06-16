//! Hash 操作模块
//!
//! 提供 Redis Hash 数据结构的操作封装，包括 HSET/HGET/HGETALL/HKEYS/HVALS/HLEN/HDEL/HEXISTS/HINCRBY/HMSET/HMGET/HSETNX。

use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;
use std::collections::HashMap;

/// Hash 操作器
pub struct HashOps {
    client: RedisClient,
}

impl HashOps {
    /// 创建新的 Hash 操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /// 设置 Hash 字段值（HSET）
    pub async fn hset(&self, key: &str, field: &str, value: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HSET", &full_key);

        let mut conn = self.client.get_connection();
        let result: u64 = redis::cmd("HSET")
            .arg(&full_key)
            .arg(field)
            .arg(value)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(result > 0)
    }

    /// 仅当字段不存在时设置值（HSETNX）
    pub async fn hsetnx(&self, key: &str, field: &str, value: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection();
        let result: bool = redis::cmd("HSETNX")
            .arg(&full_key)
            .arg(field)
            .arg(value)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(result)
    }

    /// 获取 Hash 字段值（HGET）
    pub async fn hget(&self, key: &str, field: &str) -> Result<Option<String>> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HGET", &full_key);

        let mut conn = self.client.get_connection();
        let value: Option<String> = redis::cmd("HGET")
            .arg(&full_key)
            .arg(field)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(value)
    }

    /// 获取 Hash 所有字段和值（HGETALL）
    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HGETALL", &full_key);

        let mut conn = self.client.get_connection();
        let result: HashMap<String, String> = redis::cmd("HGETALL")
            .arg(&full_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(result)
    }

    /// 获取 Hash 所有字段名（HKEYS）
    pub async fn hkeys(&self, key: &str) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HKEYS", &full_key);

        let mut conn = self.client.get_connection();
        let result: Vec<String> = redis::cmd("HKEYS")
            .arg(&full_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(result)
    }

    /// 获取 Hash 所有字段值（HVALS）
    pub async fn hvals(&self, key: &str) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HVALS", &full_key);

        let mut conn = self.client.get_connection();
        let result: Vec<String> = redis::cmd("HVALS")
            .arg(&full_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(result)
    }

    /// 获取 Hash 字段数量（HLEN）
    pub async fn hlen(&self, key: &str) -> Result<u64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection();
        let result: u64 = redis::cmd("HLEN")
            .arg(&full_key)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(result)
    }

    /// 删除 Hash 字段（HDEL）
    pub async fn hdel(&self, key: &str, fields: &[&str]) -> Result<u64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HDEL", &full_key);

        let mut conn = self.client.get_connection();
        let args: Vec<String> = std::iter::once(full_key.clone())
            .chain(fields.iter().map(|s| s.to_string()))
            .collect();

        let result: u64 = redis::cmd("HDEL")
            .arg(args.as_slice())
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(result)
    }

    /// 检查 Hash 字段是否存在（HEXISTS）
    pub async fn hexists(&self, key: &str, field: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection();
        let result: bool = redis::cmd("HEXISTS")
            .arg(&full_key)
            .arg(field)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok(result)
    }

    /// Hash 字段自增（HINCRBY）
    pub async fn hincrby(&self, key: &str, field: &str, delta: i64) -> Result<i64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HINCRBY", &full_key);

        let mut conn = self.client.get_connection();
        let result: i64 = redis::cmd("HINCRBY")
            .arg(&full_key)
            .arg(field)
            .arg(delta)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(result)
    }

    /// 批量设置 Hash 字段（HMSET）
    pub async fn hmset(&self, key: &str, items: &HashMap<&str, &str>) -> Result<()> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HMSET", &full_key);

        let mut conn = self.client.get_connection();
        let mut cmd = redis::cmd("HMSET");
        cmd.arg(&full_key);
        for (field, value) in items {
            cmd.arg(*field).arg(*value);
        }
        let _: () = cmd.query_async(&mut conn).await.map_err(Error::from)?;

        timer.complete();
        Ok(())
    }

    /// 批量获取 Hash 字段值（HMGET）
    pub async fn hmget(&self, key: &str, fields: &[&str]) -> Result<Vec<Option<String>>> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("HMGET", &full_key);

        let mut conn = self.client.get_connection();
        let mut cmd = redis::cmd("HMGET");
        cmd.arg(&full_key);
        for field in fields {
            cmd.arg(*field);
        }
        let result: Vec<Option<String>> = cmd.query_async(&mut conn).await.map_err(Error::from)?;

        timer.complete();
        Ok(result)
    }
}
