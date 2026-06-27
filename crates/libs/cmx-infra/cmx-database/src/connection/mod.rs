/// 连接池管理模块，负责数据库连接池的创建和管理
use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;


use crate::config::{DbConfig, DbType};
use crate::executor::{ResultConverter, bind_data_value_mysql, bind_data_value_postgres, bind_data_value_sqlite};
use crate::transaction::Dbx;
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use sea_query_sqlx::SqlxValues;
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

impl DbPool {
    /// 在连接池上执行无参数的 SQL 语句并返回受影响的行数。
    ///
    /// 在底层根据数据库类型（PostgreSQL、MySQL、SQLite）自动分发到对应的 sqlx 运行时执行。
    ///
    /// # Arguments
    ///
    /// * `sql` - 待执行的 SQL 语句，不包含参数占位符。
    ///
    /// # Returns
    ///
    /// 成功时返回受影响的行数（`u64`）。对 SELECT 类语句返回 0。
    ///
    /// # Errors
    ///
    /// 返回底层的 sqlx 执行错误，包括连接失败、SQL 语法错误等。
    pub async fn execute(&self, sql: &str) -> crate::Result<u64> {
        match self {
            DbPool::Postgres(pool) => {
                let result = sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
                Ok(result.rows_affected())
            }
            DbPool::MySql(pool) => {
                let result = sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
                Ok(result.rows_affected())
            }
            DbPool::Sqlite(pool) => {
                let result = sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
                Ok(result.rows_affected())
            }
        }
    }

    /// 在连接池上执行无参数的 SQL 查询并返回 DataSet。
    ///
    /// 在底层根据数据库类型自动选择对应的结果转换器，将 sqlx 行数据转换为统一的 DataSet 格式。
    ///
    /// # Arguments
    ///
    /// * `sql` - 待执行的 SQL 查询语句，不包含参数占位符。
    /// * `dataset_id` - 查询结果的唯一标识，用于构建返回的 DataSet schema。
    ///
    /// # Returns
    ///
    /// 成功时返回包含查询结果的 `DataSet`。空结果集返回空 DataSet（schema 列信息仍保留）。
    ///
    /// # Errors
    ///
    /// 返回底层的 sqlx 执行错误或结果转换错误。
    pub async fn query(&self, sql: &str, dataset_id: &str) -> crate::Result<DataSet> {
        match self {
            DbPool::Postgres(pool) => {
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
                Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
            }
            DbPool::MySql(pool) => {
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
                Ok(ResultConverter::convert_mysql_rows(rows, dataset_id))
            }
            DbPool::Sqlite(pool) => {
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
                Ok(ResultConverter::convert_sqlite_rows(rows, dataset_id))
            }
        }
    }

    /// 在连接池上执行带 DataValue 参数的 SQL 语句并返回受影响的行数。
    ///
    /// 根据数据库类型选择对应的参数绑定函数，将 DataValue 参数安全地绑定到 SQL 语句中。
    ///
    /// # Arguments
    ///
    /// * `sql` - 待执行的 SQL 语句，包含 `?` 占位符。
    /// * `params` - DataValue 数组，每个元素按顺序绑定到 SQL 中的占位符。
    ///
    /// # Returns
    ///
    /// 成功时返回受影响的行数（`u64`）。
    ///
    /// # Errors
    ///
    /// * 参数数量与占位符不匹配时返回 sqlx 错误。
    /// * 参数类型与数据库不兼容时返回 sqlx 错误。
    pub async fn execute_with_datavalues(
        &self,
        sql: &str,
        params: &[DataValue],
    ) -> crate::Result<u64> {
        match self {
            DbPool::Postgres(pool) => {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                for param in params {
                    query = bind_data_value_postgres(query, param);
                }
                let result = query.execute(pool).await?;
                Ok(result.rows_affected())
            }
            DbPool::MySql(pool) => {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                for param in params {
                    query = bind_data_value_mysql(query, param);
                }
                let result = query.execute(pool).await?;
                Ok(result.rows_affected())
            }
            DbPool::Sqlite(pool) => {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                for param in params {
                    query = bind_data_value_sqlite(query, param);
                }
                let result = query.execute(pool).await?;
                Ok(result.rows_affected())
            }
        }
    }

    /// 在连接池上执行带 DataValue 参数的 SQL 查询并返回 DataSet。
    ///
    /// 根据数据库类型选择对应的参数绑定函数和结果转换器，完成从参数绑定到结果转换的全流程。
    ///
    /// # Arguments
    ///
    /// * `sql` - 待执行的 SQL 查询语句，包含 `?` 占位符。
    /// * `params` - DataValue 数组，每个元素按顺序绑定到 SQL 中的占位符。
    /// * `dataset_id` - 查询结果的唯一标识，用于构建返回的 DataSet schema。
    ///
    /// # Returns
    ///
    /// 成功时返回包含查询结果的 `DataSet`。空结果集返回空 DataSet。
    ///
    /// # Errors
    ///
    /// * 参数数量与占位符不匹配时返回 sqlx 错误。
    /// * 参数类型与数据库不兼容时返回 sqlx 错误。
    pub async fn query_with_datavalues(
        &self,
        sql: &str,
        params: &[DataValue],
        dataset_id: &str,
    ) -> crate::Result<DataSet> {
        match self {
            DbPool::Postgres(pool) => {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                for param in params {
                    query = bind_data_value_postgres(query, param);
                }
                let rows = query.fetch_all(pool).await?;
                Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
            }
            DbPool::MySql(pool) => {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                for param in params {
                    query = bind_data_value_mysql(query, param);
                }
                let rows = query.fetch_all(pool).await?;
                Ok(ResultConverter::convert_mysql_rows(rows, dataset_id))
            }
            DbPool::Sqlite(pool) => {
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                for param in params {
                    query = bind_data_value_sqlite(query, param);
                }
                let rows = query.fetch_all(pool).await?;
                Ok(ResultConverter::convert_sqlite_rows(rows, dataset_id))
            }
        }
    }

    /// 在连接池上执行带 sea-query-binder SqlxValues 参数的 SQL 语句。
    ///
    /// 使用 sea-query 的参数绑定机制，适用于通过 sea-query 构建器生成的 SQL 和参数。
    /// 目前仅支持 PostgreSQL；MySQL 和 SQLite 返回不支持的错误。
    ///
    /// # Arguments
    ///
    /// * `sql` - 待执行的 SQL 语句。
    /// * `params` - sea-query-binder 的 `SqlxValues`，包含预绑定的参数。
    ///
    /// # Returns
    ///
    /// 成功时返回受影响的行数（`u64`）。
    ///
    /// # Errors
    ///
    /// * `Error::InvalidParams` - MySQL 或 SQLite 数据库不支持 sea-query 参数绑定。
    /// * 底层的 sqlx 执行错误。
    pub async fn execute_with_sqlxvalues(
        &self,
        sql: &str,
        params: SqlxValues,
    ) -> crate::Result<u64> {
        match self {
            DbPool::Postgres(pool) => {
                let query = sqlx::query_with(sqlx::AssertSqlSafe(sql), params);
                let result = query.execute(pool).await?;
                Ok(result.rows_affected())
            }
            DbPool::MySql(_) => Err(crate::Error::InvalidParams(
                "MySql not supported with sea-query yet".to_string(),
            )),
            DbPool::Sqlite(_) => Err(crate::Error::InvalidParams(
                "Sqlite not supported with sea-query yet".to_string(),
            )),
        }
    }

    /// 在连接池上执行带 sea-query-binder SqlxValues 参数的 SQL 查询并返回 DataSet。
    ///
    /// 使用 sea-query 的参数绑定机制，适用于通过 sea-query 构建器生成的 SQL 和参数。
    /// 目前仅支持 PostgreSQL；MySQL 和 SQLite 返回不支持的错误。
    ///
    /// # Arguments
    ///
    /// * `sql` - 待执行的 SQL 查询语句。
    /// * `params` - sea-query-binder 的 `SqlxValues`，包含预绑定的参数。
    /// * `dataset_id` - 查询结果的唯一标识，用于构建返回的 DataSet schema。
    ///
    /// # Returns
    ///
    /// 成功时返回包含查询结果的 `DataSet`。
    ///
    /// # Errors
    ///
    /// * `Error::InvalidParams` - MySQL 或 SQLite 数据库不支持 sea-query 参数绑定。
    /// * 底层的 sqlx 执行错误。
    pub async fn query_with_sqlxvalues(
        &self,
        sql: &str,
        params: SqlxValues,
        dataset_id: &str,
    ) -> crate::Result<DataSet> {
        match self {
            DbPool::Postgres(pool) => {
                let query = sqlx::query_with(sqlx::AssertSqlSafe(sql), params);
                let rows = query.fetch_all(pool).await?;
                Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
            }
            DbPool::MySql(_) => Err(crate::Error::InvalidParams(
                "MySql not supported with sea-query yet".to_string(),
            )),
            DbPool::Sqlite(_) => Err(crate::Error::InvalidParams(
                "Sqlite not supported with sea-query yet".to_string(),
            )),
        }
    }
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
    /// 活跃连接计数
    active_connections: Arc<std::sync::atomic::AtomicUsize>,
    /// 是否正在关闭
    is_closing: Arc<std::sync::atomic::AtomicBool>,
}

impl DatabasePoolImpl {
    /// 创建新的数据库连接池实现
    pub async fn new(config: DbConfig) -> crate::Result<Self> {
        let dbx = create_dbx(&config).await?;
        Ok(Self {
            dbx,
            config,
            active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            is_closing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 获取 Dbx（增加活跃计数）
    pub fn acquire(&self) -> Dbx {
        self.active_connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.dbx.clone()
    }

    /// 释放 Dbx（减少活跃计数）
    pub fn release(&self) {
        self.active_connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// 检查是否正在关闭
    pub fn is_closing(&self) -> bool {
        self.is_closing.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 标记为正在关闭
    pub fn mark_closing(&self) {
        self.is_closing.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 获取活跃连接数
    pub fn active_count(&self) -> usize {
        self.active_connections.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 等待所有活跃连接关闭
    pub async fn wait_for_idle(&self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while self.active_count() > 0 {
            if start.elapsed() > timeout {
                return false;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        true
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
pub(crate) struct DbRegistry {
    pools: RwLock<HashMap<String, DatabasePoolImpl>>,
}

impl DbRegistry {
    /// 创建新的注册器
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    /// 注册数据库连接池
    pub async fn register(&self, config: DbConfig) -> crate::Result<()> {
        let db_key = config.db_id.clone();
        let pool = DatabasePoolImpl::new(config).await?;
        let mut pools = self.pools.write().await;
        pools.insert(db_key, pool);
        Ok(())
    }

    /// 更新数据库连接池配置（优雅关闭旧池）
    #[allow(dead_code)]
    pub async fn update(&self, config: DbConfig) -> crate::Result<()> {
        let key = config.db_id.clone();

        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(&key) {
                pool.mark_closing();
            }
        }

        // 等待旧池中的活跃连接关闭
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(&key) {
                let timeout = std::time::Duration::from_secs(30);
                if !pool.wait_for_idle(timeout).await {
                    tracing::warn!("等待旧连接池关闭超时，仍有 {} 个活跃连接", pool.active_count());
                }
            }
        }

        // 创建新池并替换
        let pool = DatabasePoolImpl::new(config).await?;
        let mut pools = self.pools.write().await;
        pools.insert(key, pool);
        Ok(())
    }

    /// 注销数据库连接池（优雅关闭）
    pub async fn unregister(&self, key: &str) -> Option<DatabasePoolImpl> {
        // 标记为关闭
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(key) {
                pool.mark_closing();
            }
        }

        // 从注册表中移除
        let mut pools = self.pools.write().await;
        pools.remove(key)
    }

    /// 获取所有数据库连接池名称
    pub async fn list(&self) -> Vec<String> {
        let pools = self.pools.read().await;
        pools.keys().cloned().collect()
    }

    /// 获取数据库连接池
    pub async fn get(&self, key: &str) -> Option<(Dbx, DbConfig)> {
        let pools = self.pools.read().await;
        pools.get(key).map(|pool| (pool.get_dbx(), pool.get_config()))
    }

    /// 获取数据库访问对象
    pub async fn get_db_access(&self, key: &str) -> Option<Dbx> {
        self.get(key).await.map(|(dbx, _)| dbx)
    }

    /// 获取数据库配置
    pub async fn get_db_config(&self, key: &str) -> Option<DbConfig> {
        self.get(key).await.map(|(_, config)| config)
    }
}

use std::sync::OnceLock;

static GLOBAL_REGISTRY: OnceLock<Arc<DbRegistry>> = OnceLock::new();

pub(crate) fn get_global_registry() -> &'static Arc<DbRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| Arc::new(DbRegistry::new()))
}

// pub async fn register_db_pool(config: DbConfig) -> Result<()> {
//     get_global_registry().register(config).await
// }
//
// pub async fn remove_db_pool(key: &str) {
//     get_global_registry().unregister(key).await;
// }
//
// pub fn get_db_access(key: &str) -> Option<Dbx> {
//     get_global_registry().get_db_access(key)
// }
//
// pub fn list_db_pools() -> Vec<String> {
//     get_global_registry().list()
// }

// // 全局实例
// static GLOBAL_REGISTRY: OnceLock<Arc<DbRegistry>> = OnceLock::new();
//
// /// 获取全局注册器实例
// pub fn get_registry() -> &'static Arc<DbRegistry> {
//     GLOBAL_REGISTRY.get_or_init(|| Arc::new(DbRegistry::new()))
// }


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
 async fn new_db_pool(config: &DbConfig) -> sqlx::Result<DbPool> {
    let pool_config = &config.pool_config;

    match config.db_type {
        DbType::Postgres => {
            info!("创建 PostgreSQL 连接池，连接池配置：{:?}", pool_config);
            let db_schema = config.db_schema.clone();
            let db_url = config.db_url.clone();
            let pool = PgPoolOptions::new()
                .max_connections(pool_config.max_connections as u32)
                .min_connections(pool_config.min_connections as u32)
                .acquire_timeout(std::time::Duration::from_secs(pool_config.acquire_timeout))
                .idle_timeout(std::time::Duration::from_secs(pool_config.idle_timeout))
                .max_lifetime(std::time::Duration::from_secs(pool_config.max_lifetime))
                .after_connect(move |conn, _metadata| {
                    let schema = db_schema.clone();
                    Box::pin(async move {
                        // 每次新建连接时设置 schema
                        sqlx::query(sqlx::AssertSqlSafe(format!("SET search_path TO {}, public", schema
                            .unwrap_or("public".to_string()))))
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                })

                .connect(&db_url)

                .await?;
            Ok(DbPool::Postgres(pool))
        },
        DbType::MySql => {
            info!("创建 MySQL 连接池，连接池配置：{:?}", pool_config);
            let db_url = config.db_url.clone();
            let pool = MySqlPoolOptions::new()
                .max_connections(pool_config.max_connections as u32)
                .min_connections(pool_config.min_connections as u32)
                .acquire_timeout(std::time::Duration::from_secs(pool_config.acquire_timeout))
                .idle_timeout(std::time::Duration::from_secs(pool_config.idle_timeout))
                .max_lifetime(std::time::Duration::from_secs(pool_config.max_lifetime))
                .connect(&db_url)
                .await?;
            Ok(DbPool::MySql(pool))
        },
        DbType::Sqlite => {
            info!("创建 SQLite 连接池，连接池配置：{:?}", pool_config);
            let db_url = config.db_url.clone();
            let pool = SqlitePoolOptions::new()
                .max_connections(pool_config.max_connections as u32)
                .acquire_timeout(std::time::Duration::from_secs(pool_config.acquire_timeout))
                .connect(&db_url)
                .await?;
            Ok(DbPool::Sqlite(pool))
        },
    }
}

