//! 测试用 Mock Redis 后端
//!
//! 仅在 `cargo test` 时编译。提供基于 `HashMap` + 过期时间的轻量 Redis 模拟，
//! 用于测试分布式锁的加锁/解锁/续期/看门狗逻辑，无需依赖真实 Redis 服务。
//!
//! # 支持的命令
//!
//! - `SET key value [NX] [EX secs]`
//! - `GET key`
//! - `DEL key [key ...]`
//! - `EXISTS key`
//! - `TTL key`
//! - `EXPIRE key secs`
//! - `EVAL script numkeys key [arg ...]`（识别 unlock/extend 两类已知脚本）
//! - `PING`

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use redis::{Cmd, Pipeline, RedisError, RedisResult, Value};
use tokio::sync::Mutex;

/// Mock Redis 后端状态
///
/// 使用 `tokio::sync::Mutex` 保证异步上下文下的安全访问，
/// 内部存储 key -> (value, expire_at)。
#[derive(Debug, Default)]
pub struct MockRedisBackend {
    inner: Mutex<MockInner>,
}

/// Mock 后端内部状态
#[derive(Debug, Default)]
struct MockInner {
    /// key -> (value, expire_at)
    data: HashMap<Vec<u8>, MockEntry>,
}

/// Mock 条目：值 + 可选过期时间点
#[derive(Debug, Clone)]
struct MockEntry {
    value: Vec<u8>,
    expire_at: Option<Instant>,
}

impl MockRedisBackend {
    /// 创建一个新的空 Mock 后端
    pub fn new() -> Self {
        Self::default()
    }

    /// 清理已过期的 key
    fn cleanup_expired(data: &mut HashMap<Vec<u8>, MockEntry>) {
        let now = Instant::now();
        data.retain(|_, entry| entry.expire_at.is_none_or(|t| t > now));
    }

    /// 直接写入 key-value（测试辅助方法，绕过 SET NX 语义）
    ///
    /// 用于模拟"其他客户端已持有锁"等场景。
    pub async fn set_raw(&self, key: &[u8], value: &[u8], ttl: Option<Duration>) {
        let mut inner = self.inner.lock().await;
        inner.data.insert(
            key.to_vec(),
            MockEntry {
                value: value.to_vec(),
                expire_at: ttl.map(|d| Instant::now() + d),
            },
        );
    }

    /// 直接读取 key 的原始值（测试辅助方法）
    pub async fn get_raw(&self, key: &[u8]) -> Option<Vec<u8>> {
        let mut inner = self.inner.lock().await;
        Self::cleanup_expired(&mut inner.data);
        inner.data.get(key).map(|e| e.value.clone())
    }

    /// 检查 key 是否存在（测试辅助方法）
    pub async fn exists_raw(&self, key: &[u8]) -> bool {
        let mut inner = self.inner.lock().await;
        Self::cleanup_expired(&mut inner.data);
        inner.data.contains_key(key)
    }

    /// 执行单条命令
    async fn execute(&self, cmd: &Cmd) -> RedisResult<Value> {
        let mut inner = self.inner.lock().await;
        Self::cleanup_expired(&mut inner.data);

        // 解析命令参数
        let args: Vec<Vec<u8>> = cmd
            .args_iter()
            .filter_map(|arg| match arg {
                redis::Arg::Simple(data) => Some(data.to_vec()),
                redis::Arg::Cursor => None,
                _ => None,
            })
            .collect();

        if args.is_empty() {
            return Err(RedisError::from((redis::ErrorKind::Extension, "空命令")));
        }

        let command = String::from_utf8_lossy(&args[0]).to_uppercase();
        match command.as_str() {
            "PING" => Ok(Value::SimpleString("PONG".to_string())),
            "SET" => Self::handle_set(&mut inner.data, &args[1..]),
            "GET" => Self::handle_get(&inner.data, &args[1..]),
            "DEL" => Self::handle_del(&mut inner.data, &args[1..]),
            "EXISTS" => Self::handle_exists(&inner.data, &args[1..]),
            "TTL" => Self::handle_ttl(&inner.data, &args[1..]),
            "EXPIRE" => Self::handle_expire(&mut inner.data, &args[1..]),
            "EVAL" => Self::handle_eval(&mut inner.data, &args[1..]),
            "SELECT" => Ok(Value::SimpleString("OK".to_string())),
            other => Err(RedisError::from((
                redis::ErrorKind::Extension,
                "Mock 不支持的命令",
                format!("Mock 不支持的命令: {}", other),
            ))),
        }
    }

    /// 处理 SET 命令，支持 NX 和 EX 选项
    fn handle_set(data: &mut HashMap<Vec<u8>, MockEntry>, args: &[Vec<u8>]) -> RedisResult<Value> {
        if args.len() < 2 {
            return Err(RedisError::from((
                redis::ErrorKind::Extension,
                "SET 至少需要 key value",
            )));
        }
        let key = args[0].clone();
        let value = args[1].clone();

        // 解析选项 NX / EX secs
        let mut nx = false;
        let mut ex_secs: Option<u64> = None;
        let mut i = 2;
        while i < args.len() {
            let opt = String::from_utf8_lossy(&args[i]).to_uppercase();
            match opt.as_str() {
                "NX" => {
                    nx = true;
                    i += 1;
                }
                "EX" => {
                    if i + 1 >= args.len() {
                        return Err(RedisError::from((
                            redis::ErrorKind::Extension,
                            "EX 缺少参数",
                        )));
                    }
                    let secs_str = String::from_utf8_lossy(&args[i + 1]);
                    ex_secs = secs_str.parse().ok();
                    i += 2;
                }
                "PX" => {
                    if i + 1 >= args.len() {
                        return Err(RedisError::from((
                            redis::ErrorKind::Extension,
                            "PX 缺少参数",
                        )));
                    }
                    let ms_str = String::from_utf8_lossy(&args[i + 1]);
                    if let Ok(ms) = ms_str.parse::<u64>() {
                        ex_secs = Some(ms / 1000);
                    }
                    i += 2;
                }
                _ => {
                    i += 1;
                }
            }
        }

        // NX 模式下 key 已存在则返回 nil
        if nx && data.contains_key(&key) {
            return Ok(Value::Nil);
        }

        let expire_at = ex_secs.map(|s| Instant::now() + Duration::from_secs(s));
        data.insert(key, MockEntry { value, expire_at });
        Ok(Value::SimpleString("OK".to_string()))
    }

    /// 处理 GET 命令
    fn handle_get(data: &HashMap<Vec<u8>, MockEntry>, args: &[Vec<u8>]) -> RedisResult<Value> {
        if args.is_empty() {
            return Err(RedisError::from((
                redis::ErrorKind::Extension,
                "GET 缺少 key",
            )));
        }
        match data.get(&args[0]) {
            Some(entry) => Ok(Value::BulkString(entry.value.clone())),
            None => Ok(Value::Nil),
        }
    }

    /// 处理 DEL 命令，返回删除的 key 数量
    fn handle_del(data: &mut HashMap<Vec<u8>, MockEntry>, args: &[Vec<u8>]) -> RedisResult<Value> {
        let mut count = 0i64;
        for key in args {
            if data.remove(key).is_some() {
                count += 1;
            }
        }
        Ok(Value::Int(count))
    }

    /// 处理 EXISTS 命令，返回存在的 key 数量
    fn handle_exists(data: &HashMap<Vec<u8>, MockEntry>, args: &[Vec<u8>]) -> RedisResult<Value> {
        let mut count = 0i64;
        for key in args {
            if data.contains_key(key) {
                count += 1;
            }
        }
        Ok(Value::Int(count))
    }

    /// 处理 TTL 命令，返回剩余秒数；不存在返回 -2，无过期返回 -1
    fn handle_ttl(data: &HashMap<Vec<u8>, MockEntry>, args: &[Vec<u8>]) -> RedisResult<Value> {
        if args.is_empty() {
            return Err(RedisError::from((
                redis::ErrorKind::Extension,
                "TTL 缺少 key",
            )));
        }
        match data.get(&args[0]) {
            Some(entry) => match entry.expire_at {
                Some(expire_at) => {
                    let now = Instant::now();
                    if expire_at <= now {
                        Ok(Value::Int(-2))
                    } else {
                        let remaining = (expire_at - now).as_secs();
                        Ok(Value::Int(remaining as i64))
                    }
                }
                None => Ok(Value::Int(-1)),
            },
            None => Ok(Value::Int(-2)),
        }
    }

    /// 处理 EXPIRE 命令
    fn handle_expire(
        data: &mut HashMap<Vec<u8>, MockEntry>,
        args: &[Vec<u8>],
    ) -> RedisResult<Value> {
        if args.len() < 2 {
            return Err(RedisError::from((
                redis::ErrorKind::Extension,
                "EXPIRE 缺少参数",
            )));
        }
        let secs_str = String::from_utf8_lossy(&args[1]);
        let secs: u64 = secs_str.parse().map_err(|_| {
            RedisError::from((
                redis::ErrorKind::Extension,
                "EXPIRE 秒数无效",
                format!("EXPIRE 秒数无效: {}", secs_str),
            ))
        })?;

        if let Some(entry) = data.get_mut(&args[0]) {
            entry.expire_at = Some(Instant::now() + Duration::from_secs(secs));
            Ok(Value::Int(1))
        } else {
            Ok(Value::Int(0))
        }
    }

    /// 处理 EVAL 命令，识别 unlock/extend 两类已知脚本
    ///
    /// 已知脚本：
    /// 1. unlock: `if redis.call("get", KEYS[1]) == ARGV[1] then return redis.call("del", KEYS[1]) else return 0 end`
    /// 2. extend: `if redis.call("get", KEYS[1]) == ARGV[1] then return redis.call("expire", KEYS[1], ARGV[2]) else return 0 end`
    fn handle_eval(data: &mut HashMap<Vec<u8>, MockEntry>, args: &[Vec<u8>]) -> RedisResult<Value> {
        // args[0] = script, args[1] = numkeys, args[2..] = keys + argv
        if args.len() < 2 {
            return Err(RedisError::from((
                redis::ErrorKind::Extension,
                "EVAL 参数不足",
            )));
        }
        let script = String::from_utf8_lossy(&args[0]).to_string();
        let numkeys_str = String::from_utf8_lossy(&args[1]);
        let numkeys: usize = numkeys_str.parse().map_err(|_| {
            RedisError::from((
                redis::ErrorKind::Extension,
                "EVAL numkeys 无效",
                format!("EVAL numkeys 无效: {}", numkeys_str),
            ))
        })?;

        if args.len() < 2 + numkeys {
            return Err(RedisError::from((
                redis::ErrorKind::Extension,
                "EVAL keys 数量不足",
            )));
        }
        let keys: Vec<&[u8]> = (0..numkeys).map(|i| args[2 + i].as_slice()).collect();
        let argv: Vec<&[u8]> = args[2 + numkeys..].iter().map(|v| v.as_slice()).collect();

        // 通过脚本内容识别 unlock / extend 两种语义
        let script_lower = script.to_lowercase();
        let is_unlock = script_lower.contains("\"del\"") || script.contains("del(");
        let is_extend = script_lower.contains("\"expire\"") || script.contains("expire(");

        if is_unlock {
            // 仅当 GET key == argv[0] 时删除 key
            if keys.is_empty() || argv.is_empty() {
                return Ok(Value::Int(0));
            }
            let expected_value = argv[0];
            let key = keys[0];
            let matched = data
                .get(key)
                .map(|entry| entry.value.as_slice() == expected_value)
                .unwrap_or(false);
            if matched {
                data.remove(key);
                Ok(Value::Int(1))
            } else {
                Ok(Value::Int(0))
            }
        } else if is_extend {
            // 仅当 GET key == argv[0] 时执行 EXPIRE key argv[1]
            if keys.is_empty() || argv.len() < 2 {
                return Ok(Value::Int(0));
            }
            let expected_value = argv[0];
            let secs_str = String::from_utf8_lossy(argv[1]);
            let secs: u64 = match secs_str.parse() {
                Ok(s) => s,
                Err(_) => return Ok(Value::Int(0)),
            };
            let key = keys[0].to_vec();
            let matched = data
                .get(&key)
                .map(|entry| entry.value.as_slice() == expected_value)
                .unwrap_or(false);
            if matched {
                if let Some(entry) = data.get_mut(&key) {
                    entry.expire_at = Some(Instant::now() + Duration::from_secs(secs));
                }
                Ok(Value::Int(1))
            } else {
                Ok(Value::Int(0))
            }
        } else {
            // 未识别的脚本返回 nil
            Ok(Value::Nil)
        }
    }
}

/// Mock 连接句柄，包装 `Arc<MockRedisBackend>` 实现 `ConnectionLike`。
///
/// 使用 newtype 模式绕过孤儿规则（不能为外部类型 `Arc<T>` 实现外部 trait）。
#[derive(Debug, Clone)]
pub struct MockConnection(pub Arc<MockRedisBackend>);

impl redis::aio::ConnectionLike for MockConnection {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> redis::RedisFuture<'a, Value> {
        let backend = self.0.clone();
        Box::pin(async move { backend.execute(cmd).await })
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipeline: &'a Pipeline,
        _offset: usize,
        _count: usize,
    ) -> redis::RedisFuture<'a, Vec<Value>> {
        let backend = self.0.clone();
        Box::pin(async move {
            let mut results = Vec::with_capacity(pipeline.cmd_iter().count());
            for cmd in pipeline.cmd_iter() {
                results.push(backend.execute(cmd).await?);
            }
            Ok(results)
        })
    }

    fn get_db(&self) -> i64 {
        0
    }
}
