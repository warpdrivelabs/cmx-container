/// 连接池管理模块，负责数据库连接池的创建和管理

use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::config::{DbConfig, DbType};
use crate::transaction::Dbx;
use sqlx::{MySql, Pool, Postgres, Sqlite};
use tracing::info;

/// 数据库连接池枚举类型
#[derive(Clone, Debug)]
pub enum DbPool {
    /// PostgreSQL连接池
    Postgres(Pool<Postgres>),
    /// MySQL连接池
    MySql(Pool<MySql>),
    /// SQLite连接池
    Sqlite(Pool<Sqlite>),
}

/// 数据库连接池 trait
pub trait DatabasePool: Send + Sync {
    /// 获取数据库访问对象
    fn get_dbx(&self) -> Dbx;
    /// 获取数据库配置
    fn get_config(&self) -> DbConfig;
}

/// 数据库连接池实现
#[derive(Clone)]
pub struct DatabasePoolImpl {
    dbx: Dbx,
    config: DbConfig,
}

impl DatabasePoolImpl {
    /// 创建新的数据库连接池实现
    pub async fn new(config: DbConfig) -> crate::Result<Self> {
        let dbx = create_dbx(&config).await?;
        Ok(Self {
            dbx,
            config,
        })
    }
}

impl DatabasePool for DatabasePoolImpl {
    fn get_dbx(&self) -> Dbx {
        self.dbx.clone()
    }
    
    fn get_config(&self) -> DbConfig {
        self.config.clone()
    }
}

/// 全局注册器
pub struct DbRegistry {
    pools: RwLock<HashMap<String, Box<dyn DatabasePool>>>,
}

impl DbRegistry {
    /// 创建新的注册器
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }
    
    /// 注册数据库连接池
    pub async fn register(&self, key: String, config: DbConfig) -> crate::Result<()> {
        let pool = DatabasePoolImpl::new(config).await?;
        let mut pools = self.pools.write().unwrap();
        pools.insert(key, Box::new(pool));
        Ok(())
    }
    
    /// 更新数据库连接池配置
    pub async fn update(&self, key: &str, config: DbConfig) -> crate::Result<()> {
        let pool = DatabasePoolImpl::new(config).await?;
        let mut pools = self.pools.write().unwrap();
        pools.insert(key.to_string(), Box::new(pool));
        Ok(())
    }
    
    /// 获取数据库连接池
    pub fn get(&self, key: &str) -> Option<(Dbx, DbConfig)> {
        let pools = self.pools.read().unwrap();
        pools.get(key).map(|pool| (pool.get_dbx(), pool.get_config()))
    }
    
    /// 注销数据库连接池
    pub fn unregister(&self, key: &str) -> Option<Box<dyn DatabasePool>> {
        let mut pools = self.pools.write().unwrap();
        pools.remove(key)
    }
    
    /// 获取所有数据库连接池名称
    pub fn list(&self) -> Vec<String> {
        let pools = self.pools.read().unwrap();
        pools.keys().cloned().collect()
    }
    
    /// 获取数据库访问对象
    pub fn get_db_access(&self, key: &str) -> Option<Dbx> {
        self.get(key).map(|(dbx, _)| dbx)
    }
    
    /// 获取数据库配置
    pub fn get_db_config(&self, key: &str) -> Option<DbConfig> {
        self.get(key).map(|(_, config)| config)
    }
}

// 全局实例
static GLOBAL_REGISTRY: OnceLock<Arc<DbRegistry>> = OnceLock::new();

/// 获取全局注册器实例
pub fn get_registry() -> &'static Arc<DbRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| Arc::new(DbRegistry::new()))
}

/// 创建数据库访问对象
async fn create_dbx(config: &DbConfig) -> crate::Result<Dbx> {
    let db_pool = new_db_pool(config).await?;
    let dbx = Dbx::new(db_pool, false)?;
    Ok(dbx)
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
            info!("创建 PostgreSQL 连接池，连接池配置：{:?}", pool_config);
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
            info!("创建 MySQL 连接池，连接池配置：{:?}", pool_config);
            let pool = MySqlPoolOptions::new()
                .max_connections(pool_config.max_connections as u32)
                .min_connections(pool_config.min_connections as u32)
                .idle_timeout(std::time::Duration::from_secs(pool_config.idle_timeout))
                .max_lifetime(std::time::Duration::from_secs(pool_config.max_lifetime))
                .connect(&config.db_url)
                .await?;
            Ok(DbPool::MySql(pool))
        },
        DbType::Sqlite => {
            info!("创建 SQLite 连接池，连接池配置：{:?}", pool_config);
            let pool = SqlitePoolOptions::new()
                .max_connections(pool_config.max_connections as u32)
                .connect(&config.db_url)
                .await?;
            Ok(DbPool::Sqlite(pool))
        },
    }
}

/// 注册一个数据库连接池
pub async fn register_db_pool(key: String, config: DbConfig) -> crate::Result<()> {
    get_registry().register(key, config).await
}

/// 更新数据库连接池配置
pub async fn update_db_pool(key: &str, config: DbConfig) -> crate::Result<()> {
    get_registry().update(key, config).await
}

/// 移除数据库连接池
pub fn remove_db_pool(key: &str) {
    get_registry().unregister(key);
}

/// 获取数据库访问对象
pub fn get_db_access(key: &str) -> Option<Dbx> {
    get_registry().get_db_access(key)
}

/// 获取数据库配置
pub fn get_db_config(key: &str) -> Option<DbConfig> {
    get_registry().get_db_config(key)
}

/// 获取数据库访问对象，支持超时控制
///
/// # 参数
/// * `key` - 数据库标识符
/// * `timeout` - 超时时间
///
/// # 返回值
/// * `Result<Dbx>` - 成功返回数据库访问对象，失败返回错误
pub async fn get_db_access_with_timeout(key: &str, timeout: std::time::Duration) -> crate::Result<Dbx> {
    tokio::time::timeout(timeout, async move {
        loop {
            if let Some(dbx) = get_db_access(key) {
                return Ok(dbx);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| crate::Error::ConnectionTimeout)?
}