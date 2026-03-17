/// 数据库管理器模块
///
/// 提供 DatabaseManager 结构体，将全局状态封装为实例级状态

use std::sync::Arc;
use tracing::info;

use crate::config::{DbConfig, PoolConfig};
use crate::error::{Error, Result};
use crate::transaction::Dbx;
use cmx_core::model::data::dataset::DataSet;

/// 数据库管理器配置
#[derive(Debug, Clone)]
pub struct DatabaseManagerConfig {
    pub default_pool_config: PoolConfig,
    pub health_check_interval: std::time::Duration,
    pub health_check_timeout: std::time::Duration,
}

impl Default for DatabaseManagerConfig {
    fn default() -> Self {
        Self {
            default_pool_config: PoolConfig::default(),
            health_check_interval: std::time::Duration::from_secs(60),
            health_check_timeout: std::time::Duration::from_secs(5),
        }
    }
}

/// 事务选项
#[derive(Debug, Clone)]
pub struct TransactionOptions {
    pub propagation: crate::transaction::Propagation,
}

impl Default for TransactionOptions {
    fn default() -> Self {
        Self {
            propagation: crate::transaction::Propagation::Required,
        }
    }
}

/// 数据库管理器
///
/// 统一入口，管理连接池、事务注册表等资源
#[allow(dead_code)]
#[derive(Clone)]
pub struct DatabaseManager {
    pool_manager: Arc<PoolManager>,
    config: DatabaseManagerConfig,
}

/// 连接池管理器
#[derive(Clone)]
pub struct PoolManager {
    registry: &'static crate::connection::DbRegistry,
}

impl DatabaseManager {
    /// 创建新的数据库管理器
    pub fn new(config: DatabaseManagerConfig) -> Self {
        Self {
            pool_manager: Arc::new(PoolManager::new()),
            config,
        }
    }

    /// 注册数据源
    pub async fn register_data_source(&self, db_config: DbConfig) -> Result<()> {
        self.pool_manager.register(db_config).await
    }

    /// 注销数据源
    pub async fn unregister_data_source(&self, db_id: &str) -> Result<()> {
        self.pool_manager.unregister(db_id).await
    }

    /// 获取数据库访问对象（非事务）
    pub fn get_dbx(&self, db_id: &str) -> Result<Dbx> {
        self.pool_manager.get_dbx(db_id)
    }

    /// 获取数据库访问对象（非事务）
    pub fn get_db_config(&self, db_id: &str) -> Result<DbConfig> {
        self.pool_manager.get_db_config(db_id)
    }

    pub fn get_db(&self, db_id: &str) -> Result<(Dbx, DbConfig)> {
        self.pool_manager.get_db(db_id)
    }

    /// 开始事务
    pub async fn begin_transaction(&self, db_id: &str, options: TransactionOptions) -> Result<String> {
        let dbx = self.get_dbx(db_id)?;
        let dbx_with_txn = dbx.with_transaction()?;
        dbx_with_txn.begin_txn(db_id, options.propagation).await
    }

    /// 获取事务上下文（用于声明式事务）
    pub fn get_transaction_context(&self) -> TransactionContext {
        TransactionContext {
            pool_manager: self.pool_manager.clone(),
        }
    }

    /// 列出所有数据源
    pub fn list_data_sources(&self) -> Vec<String> {
        self.pool_manager.list()
    }

    /// 健康检查
    pub async fn health_check(&self, db_id: &str) -> Result<bool> {
        self.pool_manager.health_check(db_id).await
    }

    /// 优雅关闭
    pub async fn shutdown(&self) -> Result<()> {
        info!("DatabaseManager 开始关闭");
        crate::transaction::cleanup_completed_transactions();
        info!("DatabaseManager 已关闭");
        Ok(())
    }

    /// 执行 SQL 语句
    pub async fn execute_sql(&self, db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64> {
        crate::transaction::execute_sql_by_ids(db_id, txn_id, sql).await
    }

    /// 执行带参数的 SQL 语句
    pub async fn execute_sql_with_params(&self, db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value) -> Result<u64> {
        crate::transaction::execute_sql_with_params_by_ids(db_id, txn_id, sql, params).await
    }

    /// 查询 SQL 语句
    pub async fn query_sql(&self, db_id: &str, txn_id: Option<&str>, sql: &str, dataset_id: &str) -> Result<DataSet> {
        crate::transaction::query_sql_by_ids(db_id, txn_id, sql, dataset_id).await
    }

    /// 查询带参数的 SQL 语句
    pub async fn query_sql_with_params(&self, db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value, dataset_id: &str) -> Result<DataSet> {
        crate::transaction::query_sql_with_params_by_ids(db_id, txn_id, sql, params, dataset_id).await
    }

    /// 执行带 sea-query-binder SqlxValues 的 SQL 语句
    pub async fn execute_sql_with_sqlxvalues(&self, db_id: &str, txn_id: Option<&str>, sql: &str, params: sea_query_binder::SqlxValues) -> Result<u64> {
        crate::transaction::execute_sql_with_sqlxvalues_by_ids(db_id, txn_id, sql, params).await
    }

    /// 查询带 sea-query-binder SqlxValues 的 SQL 语句
    pub async fn query_sql_with_sqlxvalues(&self, db_id: &str, txn_id: Option<&str>, sql: &str, params: sea_query_binder::SqlxValues, dataset_id: &str) -> Result<DataSet> {
        crate::transaction::query_sql_with_sqlxvalues_by_ids(db_id, txn_id, sql, params, dataset_id).await
    }

    /// 提交事务
    pub async fn commit_transaction(&self, txn_id: &str) -> Result<()> {
        crate::transaction::commit_txn_by_id(txn_id).await
    }

    /// 回滚事务
    pub async fn rollback_transaction(&self, txn_id: &str) -> Result<()> {
        crate::transaction::rollback_txn_by_id(txn_id).await
    }
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            registry: crate::connection::get_global_registry(),
        }
    }

    pub async fn register(&self,  config: DbConfig) -> Result<()> {
        self.registry.register( config).await
    }

    pub async fn unregister(&self, key: &str) -> Result<()> {
        self.registry.unregister(key).await;
        Ok(())
    }

    pub fn get_dbx(&self, key: &str) -> Result<Dbx> {
        self.registry.get_db_access(key).ok_or(Error::NoDb)
    }

    pub  fn get_db_config(&self, key: &str) -> Result<DbConfig> {
        self.registry.get_db_config(key).ok_or(Error::NoDb)
    }
    pub fn  get_db(&self, db_id: &str)-> Result<(Dbx,DbConfig)> {
        self.registry.get(db_id).ok_or(Error::NoDb)
    }

    pub fn list(&self) -> Vec<String> {
        self.registry.list()
    }

    pub async fn health_check(&self, db_id: &str) -> Result<bool> {
        let result = crate::transaction::query_sql_by_ids(db_id, None, "SELECT 1", "health_check").await;
        match result {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// 事务上下文
///
/// 用于声明式事务管理
pub struct TransactionContext {
    pool_manager: Arc<PoolManager>,
}

impl TransactionContext {
    /// 开始事务
    pub async fn begin(&self, db_id: &str, options: TransactionOptions) -> Result<String> {
        let dbx = self.pool_manager.get_dbx(db_id)?;
        let dbx_with_txn = dbx.with_transaction()?;
        dbx_with_txn.begin_txn(db_id, options.propagation).await
    }

    /// 提交事务
    pub async fn commit(&self, txn_id: &str) -> Result<()> {
        crate::transaction::commit_txn_by_id(txn_id).await
    }

    /// 回滚事务
    pub async fn rollback(&self, txn_id: &str) -> Result<()> {
        crate::transaction::rollback_txn_by_id(txn_id).await
    }
}

/// 默认数据库管理器实例
static DEFAULT_MANAGER: std::sync::OnceLock<Arc<DatabaseManager>> = std::sync::OnceLock::new();

/// 获取默认数据库管理器实例
pub fn get_default_db_manager() -> &'static Arc<DatabaseManager> {
    DEFAULT_MANAGER.get_or_init(|| {
        Arc::new(DatabaseManager::new(DatabaseManagerConfig::default()))
    })
}

// impl Default for DatabaseManager {
//     fn default() -> Self {
//         Self::new(DatabaseManagerConfig::default())
//     }
// }
