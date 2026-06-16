//! Lua 脚本执行模块
//!
//! 提供 Redis Lua 脚本执行封装，包括 EVAL 和 EVALSHA 命令。
//! cmx-auth 的 Refresh Token Rotation 等原子操作依赖此模块。

use crate::client::RedisClient;
use crate::error::{Error, Result};
use crate::logging::OperationTimer;

/// Lua 脚本操作器
pub struct ScriptOps {
    client: RedisClient,
}

impl ScriptOps {
    /// 创建新的 Lua 脚本操作器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }

    /// 执行 Lua 脚本（EVAL）
    ///
    /// # 参数
    /// - `script`: Lua 脚本内容
    /// - `keys`: Redis Key 列表（在脚本中通过 KEYS[1], KEYS[2]... 访问）
    /// - `args`: 参数列表（在脚本中通过 ARGV[1], ARGV[2]... 访问）
    ///
    /// # 返回
    /// 脚本返回值（以 redis::Value 形式）
    pub async fn eval(
        &self,
        script: &str,
        keys: &[&str],
        args: &[&str],
    ) -> Result<redis::Value> {
        let timer = OperationTimer::new("EVAL", "lua_script");

        // 构建 key 列表（带前缀）
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection();
        let mut cmd = redis::cmd("EVAL");
        cmd.arg(script);
        cmd.arg(full_keys.len() as i64);
        for key in &full_keys {
            cmd.arg(key.as_str());
        }
        for arg in args {
            cmd.arg(*arg);
        }

        let result: redis::Value = cmd.query_async(&mut conn).await.map_err(Error::from)?;
        timer.complete();
        Ok(result)
    }

    /// 执行 Lua 脚本（EVALSHA）
    ///
    /// 使用脚本的 SHA1 校验和执行，减少网络传输开销。
    /// 如果脚本不在服务端缓存中（返回 NOSCRIPT 错误），应回退到 eval。
    ///
    /// # 参数
    /// - `sha1`: 脚本的 SHA1 校验和
    /// - `keys`: Redis Key 列表
    /// - `args`: 参数列表
    pub async fn evalsha(
        &self,
        sha1: &str,
        keys: &[&str],
        args: &[&str],
    ) -> Result<redis::Value> {
        let timer = OperationTimer::new("EVALSHA", sha1);

        // 构建 key 列表（带前缀）
        let full_keys: Vec<String> = keys.iter().map(|k| self.client.build_key(k)).collect();

        let mut conn = self.client.get_connection();
        let mut cmd = redis::cmd("EVALSHA");
        cmd.arg(sha1);
        cmd.arg(full_keys.len() as i64);
        for key in &full_keys {
            cmd.arg(key.as_str());
        }
        for arg in args {
            cmd.arg(*arg);
        }

        let result: redis::Value = cmd.query_async(&mut conn).await.map_err(Error::from)?;
        timer.complete();
        Ok(result)
    }

    /// 加载 Lua 脚本到服务端缓存（SCRIPT LOAD）
    ///
    /// 返回脚本的 SHA1 校验和，后续可用 evalsha 执行。
    pub async fn script_load(&self, script: &str) -> Result<String> {
        let timer = OperationTimer::new("SCRIPT_LOAD", "lua_script");

        let mut conn = self.client.get_connection();
        let sha1: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(script)
            .query_async(&mut conn)
            .await
            .map_err(Error::from)?;

        timer.complete();
        Ok(sha1)
    }

    /// 检查脚本是否在服务端缓存中（SCRIPT EXISTS）
    ///
    /// # 参数
    /// - `sha1_list`: SHA1 校验和列表
    ///
    /// # 返回
    /// 与 sha1_list 一一对应的布尔值列表，true 表示存在
    pub async fn script_exists(&self, sha1_list: &[&str]) -> Result<Vec<bool>> {
        let mut conn = self.client.get_connection();
        let mut cmd = redis::cmd("SCRIPT");
        cmd.arg("EXISTS");
        for sha1 in sha1_list {
            cmd.arg(*sha1);
        }
        let results: Vec<i32> = cmd.query_async(&mut conn).await.map_err(Error::from)?;
        Ok(results.into_iter().map(|v| v == 1).collect())
    }

    /// 执行 Lua 脚本（EVALSHA 优先，自动回退 EVAL）
    ///
    /// 先尝试 EVALSHA，如果脚本不在缓存中则回退到 EVAL 并自动缓存。
    /// 适用于高频调用的已知脚本。
    pub async fn eval_with_fallback(
        &self,
        script: &str,
        keys: &[&str],
        args: &[&str],
    ) -> Result<redis::Value> {
        // 先加载脚本获取 SHA1
        let sha1 = self.script_load(script).await?;
        // 尝试 EVALSHA
        match self.evalsha(&sha1, keys, args).await {
            Ok(result) => Ok(result),
            Err(Error::OperationError(msg)) if msg.contains("NOSCRIPT") => {
                // 脚本不在缓存中，回退到 EVAL
                tracing::warn!("EVALSHA NOSCRIPT，回退到 EVAL");
                self.eval(script, keys, args).await
            }
            Err(e) => Err(e),
        }
    }
}
