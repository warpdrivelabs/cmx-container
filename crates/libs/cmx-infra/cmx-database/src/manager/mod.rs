use log::{error, warn};
use sea_query::SqlWriter;
/// 数据库管理器模块
///
/// 提供 DatabaseManager 结构体，将全局状态封装为实例级状态
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::info;

use crate::config::{DbConfig, PoolConfig};
use crate::error::{Error, Result};
use crate::transaction::{Dbx, check_long_running_transactions};
use cmx_core::model::data::dataset::DataSet;

/// 数据库管理器配置
#[derive(Debug, Clone)]
pub struct DatabaseManagerConfig {
    pub default_pool_config: PoolConfig,
    pub health_check_interval: std::time::Duration,
    pub health_check_timeout: std::time::Duration,
    // pub txn_timeout: std::time::Duration,
    // pub cleanup_interval: std::time::Duration,
    // pub enable_txn_cleanup: bool,
}

impl Default for DatabaseManagerConfig {
    fn default() -> Self {
        Self {
            default_pool_config: PoolConfig::default(),
            health_check_interval: std::time::Duration::from_secs(60),
            health_check_timeout: std::time::Duration::from_secs(5),
            // txn_timeout: std::time::Duration::from_secs(300),
            // cleanup_interval: std::time::Duration::from_secs(10),
            // enable_txn_cleanup: true,
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
pub struct DatabaseManager {
    pool_manager: Arc<PoolManager>,
    config: DatabaseManagerConfig,
    default_db_id: RwLock<String>,
    // cleanup_shutdown_tx: RwLock<Option<mpsc::Sender<()>>>,
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
            default_db_id: RwLock::new("default".to_string()),
            // cleanup_shutdown_tx: RwLock::new(None),
        }
    }

    pub async fn get_default_db_id(&self) -> String {
        self.default_db_id.read().await.clone().into()
    }

    /// 注册数据源
    pub async fn register_data_source(&self, db_config: DbConfig) -> Result<()> {
        if (db_config.clone().default) {
            let mut write_guard = self.default_db_id.write().await;
            *write_guard = db_config.db_id.clone();
        }

        self.pool_manager.register(db_config).await
    }

    /// 注销数据源
    pub async fn unregister_data_source(&self, db_id: &str) -> Result<()> {
        if db_id == self.default_db_id.read().await.as_str() {
            warn!("默认数据源不能删除");
            return Err(Error::DefaultDbSourceCantDelete(
                "默认数据源不能删除".into(),
            ));
        }
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

    // /// 开始事务
    // pub async fn begin_transaction(
    //     &self,
    //     db_id: &str,
    //     options: TransactionOptions,
    // ) -> Result<String> {
    //     let dbx = self.get_dbx(db_id)?;
    //     let dbx_with_txn = dbx.with_transaction()?;
    //     dbx_with_txn.begin_txn(db_id, options.propagation).await
    // }
    //
    // /// 开始事务并返回 TransactionGuard（RAII 模式）
    // ///
    // /// TransactionGuard 在函数结束或发生 panic 时会自动回滚未提交的事务
    // ///
    //
    // pub async fn begin_transaction_guard(&self, db_id: &str, options: TransactionOptions) -> Result<crate::transaction::TransactionGuard> {
    //     let txn_ctx = self.get_transaction_context();
    //     txn_ctx.begin_with_guard(db_id, options).await
    // }

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
        // self.stop_cleanup_task().await;
        crate::transaction::cleanup_completed_transactions();
        info!("DatabaseManager 已关闭");
        Ok(())
    }

    // /// 启动事务超时清理任务
    // pub async fn start_cleanup_task(self: &Arc<Self>) {
    //     if !self.config.enable_txn_cleanup {
    //         info!("事务清理任务已禁用");
    //         return;
    //     }
    //
    //     let mut shutdown_rx = {
    //         let mut shutdown_tx_guard = self.cleanup_shutdown_tx.write().await;
    //         if shutdown_tx_guard.is_some() {
    //             info!("事务清理任务已在运行");
    //             return;
    //         }
    //         let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    //         *shutdown_tx_guard = Some(shutdown_tx);
    //         shutdown_rx
    //     };
    //
    //     let timeout = self.config.txn_timeout;
    //     let interval = self.config.cleanup_interval;
    //     let manager = self.clone();
    //
    //     info!("启动事务超时清理任务，超时: {:?}, 间隔: {:?}", timeout, interval);
    //
    //     tokio::spawn(async move {
    //         let mut interval_timer = tokio::time::interval(interval);
    //         loop {
    //             tokio::select! {
    //                 _ = interval_timer.tick() => {
    //                     manager.cleanup_stale_transactions(timeout).await;
    //                 }
    //                 _ = shutdown_rx.recv() => {
    //                     info!("接收到清理任务关闭信号");
    //                     break;
    //                 }
    //             }
    //         }
    //         info!("事务超时清理任务已停止");
    //     });
    // }
    //
    // /// 停止事务超时清理任务
    // async fn stop_cleanup_task(&self) {
    //     let mut shutdown_tx_guard = self.cleanup_shutdown_tx.write().await;
    //     if let Some(shutdown_tx) = shutdown_tx_guard.take() {
    //         let _ = shutdown_tx.send(()).await;
    //     }
    // }

    /// 清理超时的事务
    async fn cleanup_stale_transactions(&self, timeout: std::time::Duration) {
        let stale = check_long_running_transactions(timeout);
        if stale.is_empty() {
            return;
        }

        warn!("发现 {} 个超时事务待清理", stale.len());
        for meta in stale {
            warn!(
                "清理超时事务: txn_id={}, db_id={}, elapsed={:?}",
                meta.txn_id,
                meta.db_id,
                meta.create_time.elapsed()
            );
            match self.rollback_transaction(&meta.txn_id).await {
                Ok(_) => info!("超时事务已回滚: {}", meta.txn_id),
                Err(e) => error!("清理事务失败: txn_id={}, error={}", meta.txn_id, e),
            }
        }
    }

    /// 执行 SQL 语句
    pub async fn execute_sql(&self, db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64> {
        crate::transaction::execute_sql(db_id, txn_id, sql).await
    }

    /// 执行带 serde_json::Value 参数的 SQL 语句
    pub async fn execute_sql_with_json(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        params: serde_json::Value,
    ) -> Result<u64> {
        crate::transaction::execute_sql_with_params(
            db_id,
            txn_id,
            sql,
            crate::transaction::SqlParams::Json(params),
        )
        .await
    }

    /// 执行带 DataValue 参数的 SQL 语句
    pub async fn execute_sql_with_datavalues(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        params: Vec<cmx_core::model::cell::DataValue>,
    ) -> Result<u64> {
        crate::transaction::execute_sql_with_params(
            db_id,
            txn_id,
            sql,
            crate::transaction::SqlParams::DataValues(params),
        )
        .await
    }

    /// 执行带 sea-query-binder SqlxValues 的 SQL 语句
    pub async fn execute_sql_with_sqlxvalues(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        params: sea_query_binder::SqlxValues,
    ) -> Result<u64> {
        crate::transaction::execute_sql_with_params(
            db_id,
            txn_id,
            sql,
            crate::transaction::SqlParams::SqlxValues(params),
        )
        .await
    }

    /// 查询 SQL 语句
    pub async fn query_sql(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        dataset_id: &str,
    ) -> Result<DataSet> {
        crate::transaction::query_sql(db_id, txn_id, sql, dataset_id).await
    }

    /// 查询带 serde_json::Value 参数的 SQL 语句
    pub async fn query_sql_with_json(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        params: serde_json::Value,
        dataset_id: &str,
    ) -> Result<DataSet> {
        crate::transaction::query_sql_with_params(
            db_id,
            txn_id,
            sql,
            crate::transaction::SqlParams::Json(params),
            dataset_id,
        )
        .await
    }

    /// 查询带 DataValue 参数的 SQL 语句
    pub async fn query_sql_with_datavalues(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        params: Vec<cmx_core::model::cell::DataValue>,
        dataset_id: &str,
    ) -> Result<DataSet> {
        crate::transaction::query_sql_with_params(
            db_id,
            txn_id,
            sql,
            crate::transaction::SqlParams::DataValues(params),
            dataset_id,
        )
        .await
    }

    /// 查询带 sea-query-binder SqlxValues 的 SQL 语句
    pub async fn query_sql_with_sqlxvalues(
        &self,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        params: sea_query_binder::SqlxValues,
        dataset_id: &str,
    ) -> Result<DataSet> {
        crate::transaction::query_sql_with_params(
            db_id,
            txn_id,
            sql,
            crate::transaction::SqlParams::SqlxValues(params),
            dataset_id,
        )
        .await
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

    pub async fn register(&self, config: DbConfig) -> Result<()> {
        self.registry.register(config).await
    }

    pub async fn unregister(&self, key: &str) -> Result<()> {
        self.registry.unregister(key).await;
        Ok(())
    }

    pub fn get_dbx(&self, key: &str) -> Result<Dbx> {
        self.registry.get_db_access(key).ok_or(Error::NoDb)
    }

    pub fn get_db_config(&self, key: &str) -> Result<DbConfig> {
        self.registry.get_db_config(key).ok_or(Error::NoDb)
    }
    pub fn get_db(&self, db_id: &str) -> Result<(Dbx, DbConfig)> {
        self.registry.get(db_id).ok_or(Error::NoDb)
    }

    pub fn list(&self) -> Vec<String> {
        self.registry.list()
    }

    pub async fn health_check(&self, db_id: &str) -> Result<bool> {
        let result = crate::transaction::query_sql(db_id, None, "SELECT 1", "health_check").await;
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
    pub async fn begin(&self, db_id: &str) -> Result<String> {
        let dbx = self.pool_manager.get_dbx(db_id)?;
        let dbx_with_txn = dbx.with_transaction()?;
        dbx_with_txn.begin_txn_default(db_id).await
    }

    /// 提交事务
    pub async fn commit(&self, txn_id: &str) -> Result<()> {
        crate::transaction::commit_txn_by_id(txn_id).await
    }

    /// 回滚事务
    pub async fn rollback(&self, txn_id: &str) -> Result<()> {
        crate::transaction::rollback_txn_by_id(txn_id).await
    }

    /// 开始事务并返回 TransactionGuard
    pub async fn begin_with_guard(
        &self,
        db_id: &str,
    ) -> Result<crate::transaction::TransactionGuard> {
        let dbx = self.pool_manager.get_dbx(db_id)?;
        let dbx_with_txn = dbx.with_transaction()?;
        let txn_id = dbx_with_txn.begin_txn_default(db_id).await?;
        Ok(crate::transaction::TransactionGuard::new(
            txn_id,
            db_id.to_string(),
        ))
    }
}

/// 默认数据库管理器实例
static DEFAULT_MANAGER: std::sync::OnceLock<Arc<DatabaseManager>> = std::sync::OnceLock::new();

/// 获取默认数据库管理器实例
pub fn get_default_db_manager() -> &'static Arc<DatabaseManager> {
    DEFAULT_MANAGER.get_or_init(|| Arc::new(DatabaseManager::new(DatabaseManagerConfig::default())))
}

// impl Default for DatabaseManager {
//     fn default() -> Self {
//         Self::new(DatabaseManagerConfig::default())
//     }
// }
