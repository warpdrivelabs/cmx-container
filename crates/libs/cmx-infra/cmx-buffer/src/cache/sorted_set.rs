use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;

///! 缓存操作模块 - 有序集合操作

/// 作者: AI Assistant
/// 日期: 2026-03-16

/// 有序集合操作器
pub struct SortedSetOps {
    client: RedisClient,
}

impl SortedSetOps {
    /// 创建新的有序集合操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /// 向有序集合中添加带分数的多个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `items` - 成员和分数的元组列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 成功添加的新成员数量
    pub async fn zadd(&self, key: &str, items: &[(f64, &str)]) -> Result<u64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("ZADD", &full_key);

        let mut conn = self.client.get_connection().await?;

        let mut cmd = redis::cmd("ZADD");
        cmd.arg(&full_key);
        for (score, member) in items {
            cmd.arg(score).arg(member);
        }

        let added: u64 = cmd.query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(added)
    }

    /// 向有序集合中添加单个带分数的成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `score` - 分数
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功添加
    pub async fn zadd_one(&self, key: &str, score: f64, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("ZADD", &full_key);

        let mut conn = self.client.get_connection().await?;
        let added: u64 = redis::cmd("ZADD")
            .arg(&full_key)
            .arg(score)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(added > 0)
    }

    /// 仅当成员不存在时才添加（NX选项）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `score` - 分数
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功添加
    pub async fn zadd_nx(&self, key: &str, score: f64, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let added: u64 = redis::cmd("ZADD")
            .arg(&full_key)
            .arg("NX")
            .arg(score)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(added > 0)
    }

    /// 仅当成员存在时才更新（XX选项）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `score` - 分数
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功更新
    pub async fn zadd_xx(&self, key: &str, score: f64, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let updated: u64 = redis::cmd("ZADD")
            .arg(&full_key)
            .arg("XX")
            .arg(score)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(updated > 0)
    }

    /// 从有序集合中移除多个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `members` - 要移除的成员列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 成功移除的成员数量
    pub async fn zrem(&self, key: &str, members: &[&str]) -> Result<u64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("ZREM", &full_key);

        let mut conn = self.client.get_connection().await?;
        let args: Vec<String> = std::iter::once(full_key.clone())
            .chain(members.iter().map(|s| s.to_string()))
            .collect();

        let removed: u64 = redis::cmd("ZREM")
            .arg(args.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(removed)
    }

    /// 从有序集合中移除单个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 要移除的成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功移除
    pub async fn zrem_one(&self, key: &str, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let removed: u64 = redis::cmd("ZREM")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(removed > 0)
    }

    /// 获取指定索引范围内的成员（按分数升序）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `start` - 起始索引
    /// * `stop` - 结束索引（-1表示最后一个）
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 成员列表
    pub async fn zrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("ZRANGE")
            .arg(&full_key)
            .arg(start)
            .arg(stop)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取指定索引范围内的成员及其分数（按分数升序）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `start` - 起始索引
    /// * `stop` - 结束索引
    ///
    /// # 返回值
    /// * `Result<Vec<(String, f64)>>` - 成员及其分数的列表
    pub async fn zrange_with_scores(&self, key: &str, start: i64, stop: i64) -> Result<Vec<(String, f64)>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<(String, f64)> = redis::cmd("ZRANGE")
            .arg(&full_key)
            .arg(start)
            .arg(stop)
            .arg("WITHSCORES")
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取指定索引范围内的成员（按分数降序）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `start` - 起始索引
    /// * `stop` - 结束索引
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 成员列表
    pub async fn zrevrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("ZREVRANGE")
            .arg(&full_key)
            .arg(start)
            .arg(stop)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取指定索引范围内的成员及其分数（按分数降序）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `start` - 起始索引
    /// * `stop` - 结束索引
    ///
    /// # 返回值
    /// * `Result<Vec<(String, f64)>>` - 成员及其分数的列表
    pub async fn zrevrange_with_scores(&self, key: &str, start: i64, stop: i64) -> Result<Vec<(String, f64)>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<(String, f64)> = redis::cmd("ZREVRANGE")
            .arg(&full_key)
            .arg(start)
            .arg(stop)
            .arg("WITHSCORES")
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取指定分数范围内的成员（按分数升序）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `min` - 最小分数
    /// * `max` - 最大分数
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 成员列表
    pub async fn zrangebyscore(&self, key: &str, min: f64, max: f64) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("ZRANGEBYSCORE")
            .arg(&full_key)
            .arg(min)
            .arg(max)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取指定分数范围内的成员（带限制）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `min` - 最小分数
    /// * `max` - 最大分数
    /// * `offset` - 偏移量
    /// * `count` - 数量限制
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 成员列表
    pub async fn zrangebyscore_limit(&self, key: &str, min: f64, max: f64, offset: i64, count: i64) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("ZRANGEBYSCORE")
            .arg(&full_key)
            .arg(min)
            .arg(max)
            .arg("LIMIT")
            .arg(offset)
            .arg(count)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取成员的分数
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<Option<f64>>` - 成员的分数（如果存在）
    pub async fn zscore(&self, key: &str, member: &str) -> Result<Option<f64>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let score: Option<f64> = redis::cmd("ZSCORE")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(score)
    }

    /// 获取成员的排名（按分数升序，从0开始）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<Option<u64>>` - 成员的排名（如果存在）
    pub async fn zrank(&self, key: &str, member: &str) -> Result<Option<u64>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let rank: Option<u64> = redis::cmd("ZRANK")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(rank)
    }

    /// 获取成员的排名（按分数降序，从0开始）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<Option<u64>>` - 成员的排名（如果存在）
    pub async fn zrevrank(&self, key: &str, member: &str) -> Result<Option<u64>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let rank: Option<u64> = redis::cmd("ZREVRANK")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(rank)
    }

    /// 获取有序集合中成员的数量
    ///
    /// # 参数
    /// * `key` - 键名
    ///
    /// # 返回值
    /// * `Result<u64>` - 成员数量
    pub async fn zcard(&self, key: &str) -> Result<u64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let count: u64 = redis::cmd("ZCARD")
            .arg(&full_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 获取指定分数范围内的成员数量
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `min` - 最小分数
    /// * `max` - 最大分数
    ///
    /// # 返回值
    /// * `Result<u64>` - 成员数量
    pub async fn zcount(&self, key: &str, min: f64, max: f64) -> Result<u64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let count: u64 = redis::cmd("ZCOUNT")
            .arg(&full_key)
            .arg(min)
            .arg(max)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 增加成员的分数
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `delta` - 增量
    /// * `member` - 成员
    ///
    /// # 返回值
    /// * `Result<f64>` - 增加后的新分数
    pub async fn zincrby(&self, key: &str, delta: f64, member: &str) -> Result<f64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let new_score: f64 = redis::cmd("ZINCRBY")
            .arg(&full_key)
            .arg(delta)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(new_score)
    }

    /// 按排名范围移除成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `start` - 起始排名
    /// * `stop` - 结束排名
    ///
    /// # 返回值
    /// * `Result<u64>` - 成功移除的成员数量
    pub async fn zremrangebyrank(&self, key: &str, start: i64, stop: i64) -> Result<u64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let removed: u64 = redis::cmd("ZREMRANGEBYRANK")
            .arg(&full_key)
            .arg(start)
            .arg(stop)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(removed)
    }

    /// 按分数范围移除成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `min` - 最小分数
    /// * `max` - 最大分数
    ///
    /// # 返回值
    /// * `Result<u64>` - 成功移除的成员数量
    pub async fn zremrangebyscore(&self, key: &str, min: f64, max: f64) -> Result<u64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let removed: u64 = redis::cmd("ZREMRANGEBYSCORE")
            .arg(&full_key)
            .arg(min)
            .arg(max)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(removed)
    }

    /// 弹出分数最低的成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `count` - 弹出数量
    ///
    /// # 返回值
    /// * `Result<Vec<(String, f64)>>` - 弹出的成员及其分数列表
    pub async fn zpopmin(&self, key: &str, count: u64) -> Result<Vec<(String, f64)>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<(String, f64)> = redis::cmd("ZPOPMIN")
            .arg(&full_key)
            .arg(count)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 弹出分数最高的成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `count` - 弹出数量
    ///
    /// # 返回值
    /// * `Result<Vec<(String, f64)>>` - 弹出的成员及其分数列表
    pub async fn zpopmax(&self, key: &str, count: u64) -> Result<Vec<(String, f64)>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<(String, f64)> = redis::cmd("ZPOPMAX")
            .arg(&full_key)
            .arg(count)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取有序集合的并集并存储到新集合
    ///
    /// # 参数
    /// * `dest` - 目标键名
    /// * `keys` - 源键名列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 结果集合中成员数量
    pub async fn zunionstore(&self, dest: &str, keys: &[&str]) -> Result<u64> {
        let full_dest = self.client.build_key(dest);
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let count: u64 = redis::cmd("ZUNIONSTORE")
            .arg(&full_dest)
            .arg(full_keys.len())
            .arg(full_keys.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 获取有序集合的交集并存储到新集合
    ///
    /// # 参数
    /// * `dest` - 目标键名
    /// * `keys` - 源键名列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 结果集合中成员数量
    pub async fn zinterstore(&self, dest: &str, keys: &[&str]) -> Result<u64> {
        let full_dest = self.client.build_key(dest);
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let count: u64 = redis::cmd("ZINTERSTORE")
            .arg(&full_dest)
            .arg(full_keys.len())
            .arg(full_keys.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }
}
