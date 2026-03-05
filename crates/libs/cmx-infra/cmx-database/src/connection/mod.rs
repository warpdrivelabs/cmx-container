/// 连接池管理模块，负责数据库连接池的创建和管理

use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::config::{DbConfig, DbType};
use crate::transaction::Dbx;
use sqlx::{Pool, Postgres};
use tracing::info;

/// 数据库连接池枚举类型
#[derive(Clone, Debug)]
pub enum DbPool {
    Postgres(Pool<Postgres>),
    // MySql(Pool<MySql>),
    // Sqlite(Pool<Sqlite>),
}

// 定义注册器类型
pub type DbRegistry = RwLock<HashMap<String, (Dbx, DbConfig)>>;

// 全局静态注册器
pub static GLOBAL_DB_REGISTRY: OnceLock<Arc<DbRegistry>> = OnceLock::new();

/// 获取全局注册器实例（惰性初始化）
pub fn get_registry() -> &'static Arc<DbRegistry> {
    GLOBAL_DB_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 注册一个数据库连接池
pub async fn register_db_pool(key: String, config: DbConfig) -> crate::Result<()> {
    let dbx = create_dbx(&config).await?;
    get_registry().write().unwrap().insert(key, (dbx, config));
    Ok(())
}

/// 更新数据库连接池配置
pub async fn update_db_pool(key: &str, config: DbConfig) -> crate::Result<()> {
    let dbx = create_dbx(&config).await?;
    get_registry().write().unwrap().insert(key.to_string(), (dbx, config));
    Ok(())
}

/// 移除数据库连接池
pub fn remove_db_pool(key: &str) {
    get_registry().write().unwrap().remove(key);
}

/// 获取数据库访问对象
pub fn get_db_access(key: &str) -> Option<Dbx> {
    get_registry().read().unwrap().get(key).map(|(dbx, _)| dbx.clone())
}

/// 获取数据库配置
pub fn get_db_config(key: &str) -> Option<DbConfig> {
    get_registry().read().unwrap().get(key).map(|(_, config)| config.clone())
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
            Err(sqlx::Error::InvalidArgument("MySQL 支持未启用".to_string()))
        },
        DbType::Sqlite => {
            Err(sqlx::Error::InvalidArgument("SQLite 支持未启用".to_string()))
        },
    }
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
