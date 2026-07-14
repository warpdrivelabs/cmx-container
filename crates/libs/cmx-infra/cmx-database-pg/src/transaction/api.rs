//! 事务 API 模块（tokio-postgres 版），提供 WebAssembly 兼容的数据库事务接口。
//!
//! 与 cmx-database（sqlx 版）逐行对齐，仅：
//! - `SqlParams::SqlxValues(SqlxValues)` → `SqlParams::SeaValues(sea_query::Values)`
//! - `execute_with_sqlxvalues` / `query_with_sqlxvalues` → `*_with_seavalues`
//! - `get_default_db_manager` → `get_default_pg_db_manager`
//!
//! `with_transaction_by_id` 的 take/放回手法、`TransactionGuard` RAII + Drop 异步回滚
//! （mpsc + OnceLock 通道）完全保留。
use crate::error::{Error, Result};
use crate::transaction::core::{DbTransaction, Dbx};
use crate::transaction::metadata::TransactionStatus;
use crate::transaction::registry::{get_txn_holder_by_id, get_txn_holder_registry};
use futures::future::BoxFuture;
use std::sync::OnceLock;
use tokio::sync::mpsc;

use crate::DataSet;
use crate::executor::json_to_data_values;
use crate::get_default_pg_db_manager;
use cmx_core::model::cell::DataValue;
use tracing::debug;

/// TransactionGuard 清理命令
#[derive(Debug)]
enum TxnCleanupCommand {
    Rollback(String),
}

/// TransactionGuard 的全局清理通道
static CLEANUP_CHAN: OnceLock<mpsc::Sender<TxnCleanupCommand>> = OnceLock::new();

fn get_cleanup_sender() -> &'static mpsc::Sender<TxnCleanupCommand> {
    CLEANUP_CHAN.get_or_init(|| {
        let (tx, mut rx) = mpsc::channel::<TxnCleanupCommand>(100);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    TxnCleanupCommand::Rollback(txn_id) => {
                        if let Err(e) = rollback_txn_by_id(&txn_id).await {
                            tracing::error!(
                                "TransactionGuard 自动回滚失败: txn_id={}, error={}",
                                txn_id,
                                e
                            );
                        } else {
                            tracing::debug!("TransactionGuard 自动回滚成功: txn_id={}", txn_id);
                        }
                    }
                }
            }
        });
        tx
    })
}

/// TransactionGuard - RAII 模式的事务守卫。
///
/// 确保事务在函数结束或发生 panic 时自动回滚（除非显式提交）。
#[derive(Debug)]
pub struct TransactionGuard {
    txn_id: String,
    db_id: String,
    committed: bool,
}

impl TransactionGuard {
    /// 构造事务守卫（初始未提交）。
    pub fn new(txn_id: String, db_id: String) -> Self {
        Self {
            txn_id,
            db_id,
            committed: false,
        }
    }

    /// 返回事务 ID。
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 返回数据库 ID。
    pub fn db_id(&self) -> &str {
        &self.db_id
    }

    /// 是否已提交。
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    /// 提交事务。先真提交成功才标记 committed；失败则尝试回滚兜底。
    pub async fn commit(mut self) -> Result<()> {
        match commit_txn_by_id(&self.txn_id).await {
            Ok(()) => {
                self.committed = true;
                Ok(())
            }
            Err(e) => {
                self.committed = true;
                if let Err(rb_err) = rollback_txn_by_id(&self.txn_id).await {
                    tracing::error!(
                        target: "cmx_db_txn",
                        txn_id = %self.txn_id,
                        commit_error = %e,
                        rollback_error = %rb_err,
                        "事务 commit 失败且 rollback 也失败，事务可能悬空"
                    );
                } else {
                    tracing::warn!(
                        target: "cmx_db_txn",
                        txn_id = %self.txn_id,
                        commit_error = %e,
                        "事务 commit 失败，已成功 rollback"
                    );
                }
                Err(e)
            }
        }
    }

    /// 回滚事务。
    pub async fn rollback(mut self) -> Result<()> {
        self.committed = true;
        rollback_txn_by_id(&self.txn_id).await?;
        Ok(())
    }

    /// 获取与该 Guard 关联的数据库访问对象。
    pub async fn get_dbx(&self) -> Option<Dbx> {
        get_dbx_by_db_id(&self.db_id).await
    }

    /// 在该事务的上下文中执行用户提供的闭包操作。
    pub async fn with_transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut DbTransaction) -> BoxFuture<'_, Result<T>> + Send,
    {
        with_transaction_by_id(&self.txn_id, f).await
    }
}

impl Drop for TransactionGuard {
    fn drop(&mut self) {
        if !self.committed {
            tracing::warn!("TransactionGuard 析构，txnID: {}", self.txn_id);
            let txn_id = self.txn_id.clone();
            let sender = get_cleanup_sender();
            let _ = sender.try_send(TxnCleanupCommand::Rollback(txn_id));
        }
    }
}

/// SQL 查询参数，支持多种输入类型。
///
/// - **Json**：从外部（如 WebAssembly）传入 JSON 数据，内部转 DataValue。
/// - **DataValues**：Rust 侧已构建的 DataValue 数组。
/// - **SeaValues**：sea-query 构建器产出的 `sea_query::Values`（替代 sqlx 版的 SqlxValues）。
/// - **Typed**：强类型参数（带类型 NULL 支持），内部转 DataValue。
pub enum SqlParams {
    /// JSON 输入（内部转 DataValue）。
    Json(serde_json::Value),
    /// Rust 侧已构建的 DataValue 数组。
    DataValues(Vec<DataValue>),
    /// sea-query 构建器产出的 `sea_query::Values`。
    SeaValues(sea_query::Values),
    /// 强类型参数（带类型 NULL 支持，内部转 DataValue）。
    Typed(Vec<cmx_core::model::cell::SqlParam>),
}

/// 通过事务ID提交事务。
pub async fn commit_txn_by_id(txn_id: &str) -> Result<()> {
    let txn_holder_mutex = get_txn_holder_registry().read().await.get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        let mut should_commit = false;
        let mut txn_to_commit = None;

        let result = {
            let mut txh_g = txn_holder_mutex.lock().await;
            if let Some(txh) = txh_g.as_mut() {
                let counter = txh.dec();
                if counter == 0 {
                    txn_to_commit = txh_g.take();
                    should_commit = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        if should_commit && let Some(txn) = txn_to_commit {
            txn.commit().await?;
            crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::Committed)
                .await;
            get_txn_holder_registry().write().await.remove(txn_id);
        }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过事务ID回滚事务。
pub async fn rollback_txn_by_id(txn_id: &str) -> Result<()> {
    let txn_holder_mutex = get_txn_holder_registry().read().await.get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        let mut should_rollback = false;
        let mut txn_to_rollback = None;

        let result = {
            let mut txh_g = txn_holder_mutex.lock().await;
            if let Some(mut txn_holder) = txh_g.take() {
                if txn_holder.counter > 1 {
                    txn_holder.counter -= 1;
                    let _ = txh_g.replace(txn_holder);
                } else {
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        if should_rollback && let Some(txn) = txn_to_rollback {
            txn.rollback().await?;
            crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::RolledBack)
                .await;
            get_txn_holder_registry().write().await.remove(txn_id);
        }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过数据库ID获取 Dbx 实例。
pub async fn get_dbx_by_db_id(db_id: &str) -> Option<Dbx> {
    get_default_pg_db_manager().get_dbx(db_id).await.ok()
}

/// 在指定事务中执行用户提供的闭包操作（take/放回，不持锁执行闭包）。
pub async fn with_transaction_by_id<T, F>(txn_id: &str, f: F) -> Result<T>
where
    F: FnOnce(&mut DbTransaction) -> BoxFuture<'_, Result<T>> + Send,
{
    let holder = get_txn_holder_by_id(txn_id).await.ok_or(Error::NoTxn)?;

    let mut txn = {
        let mut guard = holder.lock().await;
        guard.take().ok_or(Error::NoTxn)?
    };

    let result = f(&mut txn).await;

    {
        let mut guard = holder.lock().await;
        *guard = Some(txn);
    }

    result
}

/// 通过数据库ID和事务ID执行无参数的 SQL 操作。
pub async fn execute_sql(db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64> {
    let sql = sql.to_string();
    debug!(
        "execute_sql: db_id: {}, txn_id: {:?}, sql: {}",
        db_id, txn_id, sql
    );
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, move |txn| {
                Box::pin(async move {
                    let result = txn.execute(&sql).await?;
                    Ok(result)
                })
            })
            .await
        }
        None => {
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            dbx.db().execute(&sql).await
        }
    }
}

/// 通过数据库ID和事务ID执行带参数的 SQL 操作。
pub async fn execute_sql_with_params(
    db_id: &str,
    txn_id: Option<&str>,
    sql: &str,
    params: SqlParams,
) -> Result<u64> {
    let sql = sql.to_string();
    debug!(
        "execute_sql_with_params: db_id: {}, txn_id: {:?}, sql: {}, ",
        db_id, txn_id, sql
    );
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, move |txn| {
                Box::pin(async move {
                    let result = match params {
                        SqlParams::Json(json) => {
                            let values = json_to_data_values(json).map_err(Error::InvalidParams)?;
                            txn.execute_with_datavalues(&sql, &values).await?
                        }
                        SqlParams::DataValues(values) => {
                            txn.execute_with_datavalues(&sql, &values).await?
                        }
                        SqlParams::SeaValues(sea_values) => {
                            txn.execute_with_seavalues(&sql, sea_values).await?
                        }
                        SqlParams::Typed(params) => {
                            let values: Vec<DataValue> =
                                params.into_iter().map(Into::into).collect();
                            txn.execute_with_datavalues(&sql, &values).await?
                        }
                    };
                    Ok(result)
                })
            })
            .await
        }
        None => {
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            let pool = dbx.db();
            match params {
                SqlParams::Json(json) => {
                    let values = json_to_data_values(json).map_err(Error::InvalidParams)?;
                    pool.execute_with_datavalues(&sql, &values).await
                }
                SqlParams::DataValues(values) => pool.execute_with_datavalues(&sql, &values).await,
                SqlParams::SeaValues(sea_values) => {
                    pool.execute_with_seavalues(&sql, sea_values).await
                }
                SqlParams::Typed(params) => {
                    let values: Vec<DataValue> = params.into_iter().map(Into::into).collect();
                    pool.execute_with_datavalues(&sql, &values).await
                }
            }
        }
    }
}

/// 通过数据库ID和事务ID执行无参数的 SQL 查询并返回 DataSet。
pub async fn query_sql(
    db_id: &str,
    txn_id: Option<&str>,
    sql: &str,
    dataset_id: &str,
) -> Result<DataSet> {
    let sql = sql.to_string();
    let dataset_id = dataset_id.to_string();
    debug!(
        "query_sql: db_id: {}, txn_id: {:?}, sql: {}, dataset_id: {}",
        db_id, txn_id, sql, dataset_id
    );

    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| {
                Box::pin(async move {
                    let result = txn.query(&sql, &dataset_id).await?;
                    Ok(result)
                })
            })
            .await
        }
        None => {
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            dbx.db().query(&sql, &dataset_id).await
        }
    }
}

/// 通过数据库ID和事务ID执行带参数的 SQL 查询并返回 DataSet。
pub async fn query_sql_with_params(
    db_id: &str,
    txn_id: Option<&str>,
    sql: &str,
    params: SqlParams,
    dataset_id: &str,
) -> Result<DataSet> {
    let sql = sql.to_string();
    let dataset_id = dataset_id.to_string();
    debug!(
        "query_sql_with_params: db_id: {}, txn_id: {:?}, sql: {}, dataset_id: {}",
        db_id, txn_id, sql, dataset_id
    );
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| {
                Box::pin(async move {
                    let result = match params {
                        SqlParams::Json(json) => {
                            let values = json_to_data_values(json).map_err(Error::InvalidParams)?;
                            txn.query_with_datavalues(&sql, &values, &dataset_id).await?
                        }
                        SqlParams::DataValues(values) => {
                            txn.query_with_datavalues(&sql, &values, &dataset_id).await?
                        }
                        SqlParams::SeaValues(sea_values) => {
                            txn.query_with_seavalues(&sql, sea_values, &dataset_id).await?
                        }
                        SqlParams::Typed(params) => {
                            let values: Vec<DataValue> =
                                params.into_iter().map(Into::into).collect();
                            txn.query_with_datavalues(&sql, &values, &dataset_id).await?
                        }
                    };
                    Ok(result)
                })
            })
            .await
        }
        None => {
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            let pool = dbx.db();
            match params {
                SqlParams::Json(json) => {
                    let values = json_to_data_values(json).map_err(Error::InvalidParams)?;
                    pool.query_with_datavalues(&sql, &values, &dataset_id).await
                }
                SqlParams::DataValues(values) => {
                    pool.query_with_datavalues(&sql, &values, &dataset_id).await
                }
                SqlParams::SeaValues(sea_values) => {
                    pool.query_with_seavalues(&sql, sea_values, &dataset_id).await
                }
                SqlParams::Typed(params) => {
                    let values: Vec<DataValue> = params.into_iter().map(Into::into).collect();
                    pool.query_with_datavalues(&sql, &values, &dataset_id).await
                }
            }
        }
    }
}

/// 开始事务并返回 TransactionGuard（RAII 模式）。
pub async fn begin_transaction_guard_by_db_id(
    db_id: &str,
    options: crate::manager::TransactionOptions,
) -> Result<TransactionGuard> {
    let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
    let dbx_with_txn = dbx.with_transaction()?;
    let txn_id = dbx_with_txn.begin_txn(db_id, options.propagation).await?;
    Ok(TransactionGuard::new(txn_id, db_id.to_string()))
}
