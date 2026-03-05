/// 存储模块，负责数据库连接和迁移
pub mod dbx;

use rand::Rng;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::store::dbx::{Dbx, Error};
use sqlx::{Pool, Postgres};
use tracing::info;

/// 数据库类型枚举
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbType {
    Postgres,
    MySql,
    Sqlite,
}

/// 数据库连接池枚举类型
#[derive(Clone, Debug)]
pub enum DbPool {
    Postgres(Pool<Postgres>),
    // MySql(Pool<MySql>),
    // Sqlite(Pool<Sqlite>),
}

/// 连接池配置
#[allow(non_snake_case)]
#[derive(Clone)]
pub struct PoolConfig {
    /// 最大连接数
    pub max_connections: usize,
    /// 最小空闲连接数
    pub min_connections: usize,
    /// 连接超时时间（秒）
    pub connect_timeout: u64,
    /// 空闲连接超时时间（秒）
    pub idle_timeout: u64,
    /// 最大生命周期（秒）
    pub max_lifetime: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: if cfg!(test) { 1 } else { 10 },
            min_connections: 2,
            connect_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 1800,
        }
    }
}

/// 数据库配置
#[allow(non_snake_case)]
#[derive(Clone)]
pub struct DbConfig {
    /// 数据库类型
    pub db_type: DbType,
    /// 数据库连接 URL
    pub db_url: String,
    /// 连接池配置
    pub pool_config: PoolConfig,
    /// 健康检查间隔（秒）
    pub health_check_interval: u64,
    /// 健康检查超时（秒）
    pub health_check_timeout: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            db_type: DbType::Postgres,
            db_url: "postgresql://localhost/test".to_string(),
            pool_config: PoolConfig::default(),
            health_check_interval: 60,
            health_check_timeout: 5,
        }
    }
}

// 定义注册器类型
pub type DbRegistry = RwLock<HashMap<String, (Dbx, DbConfig)>>;

// 全局静态注册器
pub static GLOBAL_DB_REGISTRY: OnceLock<Arc<DbRegistry>> = OnceLock::new();

/// 获取全局注册器实例（惰性初始化）
fn get_registry() -> &'static Arc<DbRegistry> {
    GLOBAL_DB_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 注册一个数据库连接池
pub async fn register(key: String, config: DbConfig) -> Result<()> {
    let dbx = create_dbx(&config).await?;
    get_registry().write().unwrap().insert(key, (dbx, config));
    Ok(())
}

/// 更新数据库连接池配置
pub async fn update(key: &str, config: DbConfig) -> Result<()> {
    let dbx = create_dbx(&config).await?;
    get_registry().write().unwrap().insert(key.to_string(), (dbx, config));
    Ok(())
}

/// 移除数据库连接池
pub fn remove(key: &str) {
    get_registry().write().unwrap().remove(key);
}

/// 查询数据库连接池
pub fn get(key: &str) -> Option<Dbx> {
    get_registry().read().unwrap().get(key).map(|(dbx, _)| dbx.clone())
}

/// 获取数据库连接，支持超时控制
///
/// # 参数
/// * `key` - 数据库标识符
/// * `timeout` - 超时时间
///
/// # 返回值
/// * `Result<Dbx>` - 成功返回数据库访问对象，失败返回错误
pub async fn get_with_timeout(key: &str, timeout: std::time::Duration) -> Result<Dbx> {
    tokio::time::timeout(timeout, async move {
        loop {
            if let Some(dbx) = get(key) {
                return Ok(dbx);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| Error::ConnectionTimeout)?
}

/// 负载均衡策略：轮询
pub struct RoundRobinLoadBalancing {
    current_index: std::sync::atomic::AtomicUsize,
    db_keys: Vec<String>,
}

impl RoundRobinLoadBalancing {
    /// 创建新的轮询负载均衡器
    pub fn new(db_keys: Vec<String>) -> Self {
        Self {
            current_index: std::sync::atomic::AtomicUsize::new(0),
            db_keys,
        }
    }

    /// 获取下一个数据库键
    pub fn next(&self) -> Option<String> {
        if self.db_keys.is_empty() {
            return None;
        }
        
        let current = self.current_index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let index = current % self.db_keys.len();
        Some(self.db_keys[index].clone())
    }
}

/// 负载均衡策略：随机
pub struct RandomLoadBalancing {
    db_keys: Vec<String>,
    rng: rand::rngs::ThreadRng,
}

impl RandomLoadBalancing {
    /// 创建新的随机负载均衡器
    pub fn new(db_keys: Vec<String>) -> Self {
        Self {
            db_keys,
            rng: rand::thread_rng(),
        }
    }

    /// 获取随机数据库键
    pub fn next(&mut self) -> Option<String> {
        if self.db_keys.is_empty() {
            return None;
        }
        
        let index = self.rng.gen_range(0..self.db_keys.len());
        Some(self.db_keys[index].clone())
    }
}

/// 查询数据库配置
pub fn get_config(key: &str) -> Option<DbConfig> {
    get_registry().read().unwrap().get(key).map(|(_, config)| config.clone())
}

/// 创建数据库访问对象
async fn create_dbx(config: &DbConfig) -> Result<Dbx> {
    let db_pool = new_db_pool(config).await?;
    let dbx = Dbx::new(db_pool, false)?;
    Ok(dbx)
}

/// 启动数据库连接池健康检查和事务超时监控
pub async fn start_monitoring() {
    // 启动健康检查
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            perform_health_check().await;
        }
    });
    
    // 启动事务超时监控
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            check_transaction_timeouts().await;
        }
    });
}

/// 检查事务超时
async fn check_transaction_timeouts() {
    // 默认事务超时时间：300秒（5分钟）
    let default_timeout = std::time::Duration::from_secs(300);
    
    let long_running_txs = check_long_running_transactions(default_timeout);
    
    for tx_meta in long_running_txs {
        info!("检测到长时间运行的事务: txn_id={}, db_id={}, 运行时间={:?}", 
              tx_meta.txn_id, tx_meta.db_id, tx_meta.created_at.elapsed());
        
        // 尝试获取数据库连接并回滚事务
        if let Some(dbx) = get(&tx_meta.db_id) {
            let _ = dbx.rollback_txn().await;
            info!("已自动回滚超时事务: txn_id={}", tx_meta.txn_id);
        }
    }
}

/// 执行健康检查
async fn perform_health_check() {
    let registry = get_registry();
    let db_keys: Vec<String> = registry.read().unwrap().keys().cloned().collect();
    
    for key in db_keys {
        let db_entry = {
            let registry_read = registry.read().unwrap();
            registry_read.get(&key).cloned()
        };
        
        if let Some((dbx, config)) = db_entry {
            let _ = check_db_health(&dbx, &config).await;
        }
    }
}

/// 检查数据库健康状态
async fn check_db_health(dbx: &Dbx, config: &DbConfig) -> Result<()> {
    let timeout = tokio::time::Duration::from_secs(config.health_check_timeout);
    
    tokio::time::timeout(timeout, async {
        match dbx.db() {
            DbPool::Postgres(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            },
            // DbPool::MySql(pool) => {
            //     sqlx::query("SELECT 1").execute(pool).await?;
            // },
            // DbPool::Sqlite(pool) => {
            //     sqlx::query("SELECT 1").execute(pool).await?;
            // },
        }
        Ok(())
    }).await.map_err(|_| Error::ConnectionTimeout)?
}

/// 事务元数据
#[derive(Debug, Clone)]
pub struct TransactionMetadata {
    /// 事务ID
    pub txn_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 创建时间
    pub created_at: std::time::Instant,
    /// 状态
    pub status: TransactionStatus,
}

/// 事务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Active,
    Committed,
    RolledBack,
}

// 全局事务注册表
pub static GLOBAL_TXN_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, TransactionMetadata>>>> = OnceLock::new();

/// 获取全局事务注册表
fn get_txn_registry() -> &'static Arc<RwLock<HashMap<String, TransactionMetadata>>> {
    GLOBAL_TXN_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 注册事务
pub fn register_txn(txn_id: String, db_id: String) {
    let metadata = TransactionMetadata {
        txn_id: txn_id.clone(),
        db_id,
        created_at: std::time::Instant::now(),
        status: TransactionStatus::Active,
    };
    get_txn_registry().write().unwrap().insert(txn_id, metadata);
}

/// 更新事务状态
pub fn update_txn_status(txn_id: &str, status: TransactionStatus) {
    if let Some(metadata) = get_txn_registry().write().unwrap().get_mut(txn_id) {
        metadata.status = status;
    }
}

/// 获取事务元数据
pub fn get_txn_metadata(txn_id: &str) -> Option<TransactionMetadata> {
    get_txn_registry().read().unwrap().get(txn_id).cloned()
}

/// 获取活跃事务列表
pub fn get_active_transactions() -> Vec<TransactionMetadata> {
    get_txn_registry()
        .read()
        .unwrap()
        .values()
        .filter(|meta| meta.status == TransactionStatus::Active)
        .cloned()
        .collect()
}

/// 清理已完成的事务
pub fn cleanup_completed_transactions() {
    let mut registry = get_txn_registry().write().unwrap();
    registry.retain(|_, meta| meta.status == TransactionStatus::Active);
}

/// 检查长时间运行的事务
pub fn check_long_running_transactions(timeout: std::time::Duration) -> Vec<TransactionMetadata> {
    get_txn_registry()
        .read()
        .unwrap()
        .values()
        .filter(|meta| {
            meta.status == TransactionStatus::Active && meta.created_at.elapsed() > timeout
        })
        .cloned()
        .collect()
}

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
    let config = get_config(db_id)?;
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
    let registry = get_registry();
    let db_keys: Vec<String> = registry.read().unwrap().keys().cloned().collect();
    
    db_keys
        .into_iter()
        .filter_map(|key| get_pool_metrics(&key))
        .collect()
}

/// 创建新的数据库连接池
///
/// # 参数
/// * `config` - 数据库配置
///
/// # 返回值
/// * `sqlx::Result<DbPool>` - 成功返回数据库连接池，失败返回错误
pub async fn new_db_pool(config: &DbConfig) -> sqlx::Result<DbPool> {
    let pool_config = &config.pool_config;
    
    match config.db_type {
        DbType::Postgres => {
            info!("创建 PostgreSQL 数据库连接池");
            let pool = PgPoolOptions::new()
                .max_connections(pool_config.max_connections as u32)
                .min_connections(pool_config.min_connections as u32)
                .idle_timeout(std::time::Duration::from_secs(pool_config.idle_timeout))
                .max_lifetime(std::time::Duration::from_secs(pool_config.max_lifetime))
                .connect(&config.db_url)
                .await?;
            Ok(DbPool::Postgres(pool))
        },
        DbType::MySql => {
            Err(sqlx::Error::InvalidArgument("MySQL 支持未启用".to_string()))
        },
        DbType::Sqlite => {
            Err(sqlx::Error::InvalidArgument("SQLite 支持未启用".to_string()))
        },
    }
}

pub type Result<T> = core::result::Result<T, Error>;
/// 数据库连接管理器，持有数据访问所需的资源
#[derive(Clone)]
pub struct DbManager {
    /// 数据库访问对象
    dbx: Dbx,
}

impl DbManager {
    /// 创建新的数据库连接管理器
    ///
    /// # 参数
    /// * `id` - 数据库标识符
    /// * `config` - 数据库配置
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回 DbManager 实例，失败返回错误
    pub async fn new(id: &str, config: &DbConfig) -> Result<Self> {
        // 注册数据库连接池
        register(id.to_string(), config.clone()).await?;
        
        // 获取数据库访问对象
        let dbx = get(id).ok_or(Error::DbNotFound(id.to_string()))?;
        let mm = DbManager { dbx };

        Ok(mm)
    }

    /// 从已注册的数据库创建管理器
    ///
    /// # 参数
    /// * `id` - 数据库标识符
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回 DbManager 实例，失败返回错误
    pub fn from_registered(id: &str) -> Result<Self> {
        let dbx = get(id).ok_or(Error::DbNotFound(id.to_string()))?;
        Ok(DbManager { dbx })
    }

    /// 创建带有事务的 DbManager
    ///
    /// # 返回值
    /// * `Result<DbManager>` - 成功返回带有事务的 DbManager，失败返回错误
    pub fn new_with_txn(&self) -> Result<DbManager> {
        let dbx = Dbx::new(self.dbx.db().clone(), true)?;
        Ok(DbManager { dbx })
    }

    /// 获取数据库访问对象
    ///
    /// # 返回值
    /// * `&Dbx` - 数据库访问对象引用
    pub fn dbx(&self) -> &Dbx {
        &self.dbx
    }
}
