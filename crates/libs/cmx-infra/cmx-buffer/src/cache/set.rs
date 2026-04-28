use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;

// 缓存操作模块 - 集合操作

/// 作者: AI Assistant
/// 日期: 2026-03-16
///
/// 集合操作器
pub struct SetOps {
    client: RedisClient,
}

impl SetOps {
    /// 创建新的集合操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /// 向集合中添加多个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `members` - 要添加的成员列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 成功添加的新成员数量
    pub async fn sadd(&self, key: &str, members: &[&str]) -> Result<u64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("SADD", &full_key);

        let mut conn = self.client.get_connection().await?;
        let args: Vec<String> = std::iter::once(full_key.clone())
            .chain(members.iter().map(|s| s.to_string()))
            .collect();

        let added: u64 = redis::cmd("SADD")
            .arg(args.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(added)
    }

    /// 向集合中添加单个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 要添加的成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功添加
    pub async fn sadd_one(&self, key: &str, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let added: u64 = redis::cmd("SADD")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(added > 0)
    }

    /// 从集合中移除多个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `members` - 要移除的成员列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 成功移除的成员数量
    pub async fn srem(&self, key: &str, members: &[&str]) -> Result<u64> {
        let full_key = self.client.build_key(key);
        let timer = OperationTimer::new("SREM", &full_key);

        let mut conn = self.client.get_connection().await?;
        let args: Vec<String> = std::iter::once(full_key.clone())
            .chain(members.iter().map(|s| s.to_string()))
            .collect();

        let removed: u64 = redis::cmd("SREM")
            .arg(args.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(removed)
    }

    /// 从集合中移除单个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 要移除的成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功移除
    pub async fn srem_one(&self, key: &str, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let removed: u64 = redis::cmd("SREM")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(removed > 0)
    }

    /// 获取集合的所有成员
    ///
    /// # 参数
    /// * `key` - 键名
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 集合中所有成员的列表
    pub async fn smembers(&self, key: &str) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("SMEMBERS")
            .arg(&full_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 检查成员是否存在于集合中
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `member` - 要检查的成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否存在
    pub async fn sismember(&self, key: &str, member: &str) -> Result<bool> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let exists: bool = redis::cmd("SISMEMBER")
            .arg(&full_key)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(exists)
    }

    /// 检查多个成员是否存在于集合中
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `members` - 要检查的成员列表
    ///
    /// # 返回值
    /// * `Result<Vec<bool>>` - 每个成员是否存在的情况列表
    pub async fn smismember(&self, key: &str, members: &[&str]) -> Result<Vec<bool>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let mut cmd = redis::cmd("SMISMEMBER");
        cmd.arg(&full_key);
        for m in members {
            cmd.arg(m);
        }
        let results: Vec<i32> = cmd.query_async(&mut *conn).await.map_err(Error::from)?;
        Ok(results.into_iter().map(|v| v == 1).collect())
    }

    /// 获取集合中成员的数量
    ///
    /// # 参数
    /// * `key` - 键名
    ///
    /// # 返回值
    /// * `Result<u64>` - 成员数量
    pub async fn scard(&self, key: &str) -> Result<u64> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let count: u64 = redis::cmd("SCARD")
            .arg(&full_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 随机移除并返回集合中的一个成员
    ///
    /// # 参数
    /// * `key` - 键名
    ///
    /// # 返回值
    /// * `Result<Option<String>>` - 随机成员
    pub async fn spop(&self, key: &str) -> Result<Option<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let member: Option<String> = redis::cmd("SPOP")
            .arg(&full_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(member)
    }

    /// 随机移除并返回集合中的多个成员
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `count` - 移除数量
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 被移除的成员列表
    pub async fn spop_count(&self, key: &str, count: u64) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("SPOP")
            .arg(&full_key)
            .arg(count)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 随机获取集合中的一个成员（不移除）
    ///
    /// # 参数
    /// * `key` - 键名
    ///
    /// # 返回值
    /// * `Result<Option<String>>` - 随机成员
    pub async fn srandmember(&self, key: &str) -> Result<Option<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let member: Option<String> = redis::cmd("SRANDMEMBER")
            .arg(&full_key)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(member)
    }

    /// 随机获取集合中的多个成员（不移除）
    ///
    /// # 参数
    /// * `key` - 键名
    /// * `count` - 获取数量（正数获取不重复，负数获取可能重复）
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 随机成员列表
    pub async fn srandmember_count(&self, key: &str, count: i64) -> Result<Vec<String>> {
        let full_key = self.client.build_key(key);

        let mut conn = self.client.get_connection().await?;
        let members: Vec<String> = redis::cmd("SRANDMEMBER")
            .arg(&full_key)
            .arg(count)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(members)
    }

    /// 获取集合之间的差集
    ///
    /// # 参数
    /// * `keys` - 集合键名列表（第一个集合减去后续集合）
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 差集结果
    pub async fn sdiff(&self, keys: &[&str]) -> Result<Vec<String>> {
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let diff: Vec<String> = redis::cmd("SDIFF")
            .arg(full_keys.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(diff)
    }

    /// 获取集合之间的交集
    ///
    /// # 参数
    /// * `keys` - 集合键名列表
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 交集结果
    pub async fn sinter(&self, keys: &[&str]) -> Result<Vec<String>> {
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let inter: Vec<String> = redis::cmd("SINTER")
            .arg(full_keys.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(inter)
    }

    /// 获取集合之间的并集
    ///
    /// # 参数
    /// * `keys` - 集合键名列表
    ///
    /// # 返回值
    /// * `Result<Vec<String>>` - 并集结果
    pub async fn sunion(&self, keys: &[&str]) -> Result<Vec<String>> {
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let union: Vec<String> = redis::cmd("SUNION")
            .arg(full_keys.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(union)
    }

    /// 将集合的差集结果存储到新集合
    ///
    /// # 参数
    /// * `dest` - 目标集合键名
    /// * `keys` - 源集合键名列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 结果集合中成员数量
    pub async fn sdiffstore(&self, dest: &str, keys: &[&str]) -> Result<u64> {
        let full_dest = self.client.build_key(dest);
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let args: Vec<String> = std::iter::once(full_dest.clone())
            .chain(full_keys)
            .collect();

        let count: u64 = redis::cmd("SDIFFSTORE")
            .arg(args.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 将集合的交集结果存储到新集合
    ///
    /// # 参数
    /// * `dest` - 目标集合键名
    /// * `keys` - 源集合键名列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 结果集合中成员数量
    pub async fn sinterstore(&self, dest: &str, keys: &[&str]) -> Result<u64> {
        let full_dest = self.client.build_key(dest);
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let args: Vec<String> = std::iter::once(full_dest.clone())
            .chain(full_keys)
            .collect();

        let count: u64 = redis::cmd("SINTERSTORE")
            .arg(args.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 将集合的并集结果存储到新集合
    ///
    /// # 参数
    /// * `dest` - 目标集合键名
    /// * `keys` - 源集合键名列表
    ///
    /// # 返回值
    /// * `Result<u64>` - 结果集合中成员数量
    pub async fn sunionstore(&self, dest: &str, keys: &[&str]) -> Result<u64> {
        let full_dest = self.client.build_key(dest);
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection().await?;
        let args: Vec<String> = std::iter::once(full_dest.clone())
            .chain(full_keys)
            .collect();

        let count: u64 = redis::cmd("SUNIONSTORE")
            .arg(args.as_slice())
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(count)
    }

    /// 将成员从一个集合移动到另一个集合
    ///
    /// # 参数
    /// * `source` - 源集合键名
    /// * `dest` - 目标集合键名
    /// * `member` - 要移动的成员
    ///
    /// # 返回值
    /// * `Result<bool>` - 是否成功移动
    pub async fn smove(&self, source: &str, dest: &str, member: &str) -> Result<bool> {
        let full_source = self.client.build_key(source);
        let full_dest = self.client.build_key(dest);

        let mut conn = self.client.get_connection().await?;
        let moved: bool = redis::cmd("SMOVE")
            .arg(&full_source)
            .arg(&full_dest)
            .arg(member)
            .query_async(&mut *conn)
            .await
            .map_err(Error::from)?;

        Ok(moved)
    }
}
