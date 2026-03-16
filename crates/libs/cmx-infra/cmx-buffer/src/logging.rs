use std::time::Instant;
use tracing::{debug, error, info, warn};

///! 日志辅助工具模块

/// 作者: AI Assistant
/// 日期: 2026-03-16

/// 操作计时器，用于记录操作耗时
pub struct OperationTimer {
    operation: &'static str,
    key: String,
    start: Instant,
}

impl OperationTimer {
    /// 创建新的操作计时器
    pub fn new(operation: &'static str, key: impl Into<String>) -> Self {
        let key = key.into();
        debug!(operation = operation, key = %key, "开始操作");
        Self {
            operation,
            key,
            start: Instant::now(),
        }
    }

    /// 完成操作并记录成功日志
    pub fn complete(self) {
        let elapsed = self.start.elapsed();
        debug!(
            operation = self.operation,
            key = %self.key,
            elapsed_ms = elapsed.as_millis(),
            "操作完成"
        );
    }

    /// 完成操作并记录成功日志（带额外信息）
    pub fn complete_with_info(self, info: &str) {
        let elapsed = self.start.elapsed();
        info!(
            operation = self.operation,
            key = %self.key,
            elapsed_ms = elapsed.as_millis(),
            info = info,
            "操作完成"
        );
    }

    /// 记录操作失败
    pub fn fail<E: std::error::Error>(self, err: &E) {
        let elapsed = self.start.elapsed();
        error!(
            operation = self.operation,
            key = %self.key,
            elapsed_ms = elapsed.as_millis(),
            error = %err,
            "操作失败"
        );
    }
}

/// 缓存操作日志辅助
pub struct CacheLog {
    operation: &'static str,
}

impl CacheLog {
    /// 创建新的缓存日志辅助
    pub fn new(operation: &'static str) -> Self {
        Self { operation }
    }

    /// 记录设置操作
    pub fn set(&self, key: &str, value_size: Option<usize>) {
        if let Some(size) = value_size {
            debug!(operation = self.operation, key = %key, value_size = size, "设置缓存");
        } else {
            debug!(operation = self.operation, key = %key, "设置缓存");
        }
    }

    /// 记录获取操作
    pub fn get(&self, key: &str, found: bool) {
        debug!(operation = self.operation, key = %key, found = found, "获取缓存");
    }

    /// 记录删除操作
    pub fn del(&self, key: &str, affected: bool) {
        debug!(operation = self.operation, key = %key, affected = affected, "删除缓存");
    }

    /// 记录过期设置操作
    pub fn expire(&self, key: &str, success: bool, ttl_secs: Option<u64>) {
        debug!(
            operation = self.operation,
            key = %key,
            success = success,
            ttl_secs = ttl_secs,
            "设置过期时间"
        );
    }
}

/// 分布式锁日志辅助
pub struct LockLog;

impl LockLog {
    /// 记录获取锁成功
    pub fn lock_acquired(key: &str) {
        info!(key = %key, "获取分布式锁成功");
    }

    /// 记录获取锁失败
    pub fn lock_failed(key: &str, reason: &str) {
        warn!(key = %key, reason = reason, "获取分布式锁失败");
    }

    /// 记录释放锁
    pub fn lock_released(key: &str) {
        info!(key = %key, "释放分布式锁");
    }

    /// 记录锁续期
    pub fn lock_renewed(key: &str, new_ttl: u64) {
        info!(key = %key, new_ttl = new_ttl, "分布式锁续期成功");
    }

    /// 记录锁冲突
    pub fn lock_conflict(key: &str) {
        warn!(key = %key, "分布式锁冲突");
    }
}

/// 连接日志辅助
pub struct ConnLog;

impl ConnLog {
    /// 记录连接建立
    pub fn connected(url: &str) {
        info!(url = %url, "Redis 连接已建立");
    }

    /// 记录连接断开
    pub fn disconnected(url: &str) {
        warn!(url = %url, "Redis 连接已断开");
    }

    /// 记录连接错误
    pub fn connection_error(url: &str, err: &str) {
        error!(url = %url, error = %err, "Redis 连接错误");
    }

    /// 记录连接池状态
    pub fn pool_status(pool_size: usize, available: usize) {
        debug!(
            pool_size = pool_size,
            available = available,
            "连接池状态"
        );
    }
}
