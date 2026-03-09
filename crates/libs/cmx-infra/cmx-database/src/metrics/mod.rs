/// 性能指标模块，负责连接池性能指标的采集和管理

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// 连接池性能指标
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    /// 数据库标识符
    pub db_id: String,
    /// 最大连接数
    pub max_connections: usize,
    /// 当前连接数
    pub current_connections: usize,
    /// 空闲连接数
    pub idle_connections: usize,
    /// 等待队列长度
    pub wait_queue_length: usize,
    /// 平均获取连接时间（毫秒）
    pub avg_acquire_time_ms: f64,
    /// 连接使用率
    pub connection_usage: f64,
    /// 健康状态
    pub health_status: bool,
}

/// 连接使用统计
#[derive(Debug, Default, Clone)]
pub struct ConnectionStats {
    /// 总获取次数
    pub total_acquires: u64,
    /// 总获取时间（毫秒）
    pub total_acquire_time_ms: u64,
    /// 最大获取时间（毫秒）
    pub max_acquire_time_ms: u64,
    /// 等待队列长度
    pub wait_queue_length: usize,
}

// 全局连接统计注册表
pub static GLOBAL_CONNECTION_STATS: OnceLock<Arc<RwLock<HashMap<String, ConnectionStats>>>> = OnceLock::new();

/// 获取全局连接统计注册表
fn get_connection_stats_registry() -> &'static Arc<RwLock<HashMap<String, ConnectionStats>>> {
    GLOBAL_CONNECTION_STATS.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 记录连接获取时间
pub fn record_connection_acquire(db_id: &str, time_ms: u64) {
    let mut registry = get_connection_stats_registry().write().unwrap();
    let stats = registry.entry(db_id.to_string()).or_default();
    stats.total_acquires += 1;
    stats.total_acquire_time_ms += time_ms;
    if time_ms > stats.max_acquire_time_ms {
        stats.max_acquire_time_ms = time_ms;
    }
}

/// 增加等待队列长度
pub fn increment_wait_queue(db_id: &str) {
    let mut registry = get_connection_stats_registry().write().unwrap();
    let stats = registry.entry(db_id.to_string()).or_default();
    stats.wait_queue_length += 1;
}

/// 减少等待队列长度
pub fn decrement_wait_queue(db_id: &str) {
    let mut registry = get_connection_stats_registry().write().unwrap();
    let stats = registry.entry(db_id.to_string()).or_default();
    if stats.wait_queue_length > 0 {
        stats.wait_queue_length -= 1;
    }
}

/// 获取连接池性能指标
pub fn get_pool_metrics(db_id: &str) -> Option<PoolMetrics> {
    let config = crate::connection::get_db_config(db_id)?;
    let stats = get_connection_stats_registry().read().unwrap().get(db_id).cloned().unwrap_or_default();
    
    // 计算平均获取时间
    let avg_acquire_time_ms = if stats.total_acquires > 0 {
        stats.total_acquire_time_ms as f64 / stats.total_acquires as f64
    } else {
        0.0
    };
    
    // 假设当前连接数和空闲连接数（实际应该从连接池获取）
    let current_connections = 0; // 实际应该从连接池获取
    let idle_connections = 0; // 实际应该从连接池获取
    
    // 计算连接使用率
    let connection_usage = if config.pool_config.max_connections > 0 {
        current_connections as f64 / config.pool_config.max_connections as f64
    } else {
        0.0
    };
    
    // 健康状态（实际应该通过健康检查结果获取）
    let health_status = true; // 实际应该通过健康检查结果获取
    
    Some(PoolMetrics {
        db_id: db_id.to_string(),
        max_connections: config.pool_config.max_connections,
        current_connections,
        idle_connections,
        wait_queue_length: stats.wait_queue_length,
        avg_acquire_time_ms,
        connection_usage,
        health_status,
    })
}

/// 获取所有连接池性能指标
pub fn get_all_pool_metrics() -> Vec<PoolMetrics> {
    let registry = crate::connection::get_registry();
    let db_keys = registry.list();
    
    db_keys
        .into_iter()
        .filter_map(|key| get_pool_metrics(&key))
        .collect()
}
