/// 事务API模块，提供WebAssembly兼容的接口
///
/// 该模块定义了通过数据库ID和事务ID执行SQL操作的接口，包括：
/// - 通过事务ID操作事务的函数
/// - 通过数据库ID和事务ID执行SQL操作的函数
/// - 通过数据库ID和事务ID执行SQL查询的函数

use futures::future::BoxFuture;
use std::sync::OnceLock;
use log::{info, warn};
use tokio::sync::mpsc;
use crate::error::{Error, Result};
use crate::transaction::core::{Dbx, DbTransaction};
use crate::transaction::registry::{get_txn_holder_registry, get_txn_holder_by_id};
use crate::transaction::metadata::TransactionStatus;

use cmx_core::model::data::dataset::DataSet;
use cmx_core::model::cell::DataValue;
use crate::executor::{ResultConverter, bind_data_value_postgres, bind_data_value_mysql, bind_data_value_sqlite, json_to_data_values};
use crate::get_default_db_manager;
use sea_query_binder::SqlxValues;
use tracing::debug;

/// TransactionGuard 清理命令
#[derive(Debug)]
enum TxnCleanupCommand {
    Rollback(String),
    // Commit(String),
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
                            tracing::error!("TransactionGuard 自动回滚失败: txn_id={}, error={}", txn_id, e);
                        } else {
                            tracing::debug!("TransactionGuard 自动回滚成功: txn_id={}", txn_id);
                        }
                    },
                    // TxnCleanupCommand::Commit(txn_id) => {
                    //     if let Err(e) = commit_txn_by_id(&txn_id).await {
                    //         tracing::error!("TransactionGuard 自动提交失败: txn_id={}, error={}", txn_id, e);
                    //     } else {
                    //         tracing::debug!("TransactionGuard 自动提交成功: txn_id={}", txn_id);
                    //     }
                    // }
                }
            }
        });
        tx
    })
}

/// TransactionGuard - RAII 模式的事务守卫
///
/// 确保事务在函数结束或发生panic时自动回滚（除非显式提交）
///
/// # 示例
///
/// ```ignore
/// async fn do_something() -> Result<()> {
///     let guard = begin_transaction_guard("db1").await?;
///     // 执行数据库操作...
///
///     // 显式提交事务
///     guard.commit().await?;
///     Ok(())
/// } // 如果没有调用 commit，guard 析构时会自动回滚
/// ```
#[derive(Debug)]
pub struct TransactionGuard {
    txn_id: String,
    db_id: String,
    committed: bool,
}

impl TransactionGuard {
    /// 创建新的 TransactionGuard
    pub fn new(txn_id: String, db_id: String) -> Self {
        Self {
            txn_id,
            db_id,
            committed: false,
        }
    }

    /// 获取事务ID
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 获取数据库ID
    pub fn db_id(&self) -> &str {
        &self.db_id
    }

    /// 检查事务是否已提交
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    /// 提交事务
    pub async fn commit(mut self) -> Result<()> {
        self.committed = true;
        commit_txn_by_id(&self.txn_id).await?;
        Ok(())
    }

    /// 回滚事务
    pub async fn rollback(mut self) -> Result<()> {
        self.committed = true;
        rollback_txn_by_id(&self.txn_id).await?;
        Ok(())
    }

    /// 获取数据库访问对象
    pub async fn get_dbx(&self) -> Option<Dbx> {
        get_dbx_by_db_id(&self.db_id).await
    }

    /// 在事务中执行操作
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
            warn!("TransactionGuard 析构，txnID: {}", self.txn_id);
            let txn_id = self.txn_id.clone();
            let sender = get_cleanup_sender();
            let _ = sender.try_send(TxnCleanupCommand::Rollback(txn_id));
        }
    }
}

/// SQL 查询参数，支持多种输入类型
pub enum SqlParams {
    /// serde_json::Value 数组，内部自动转换为 DataValue
    Json(serde_json::Value),
    /// DataValue 数组，直接使用
    DataValues(Vec<DataValue>),
    /// sea-query-binder 的 SqlxValues
    SqlxValues(SqlxValues),
}

/// 通过事务ID提交事务
///
/// 允许通过事务ID远程提交事务
///
/// # 参数
/// * `txn_id` - 事务ID
///
/// # 返回值
/// * `Result<()>` - 成功返回 Ok(())，失败返回错误
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

        if should_commit && txn_to_commit.is_some() {
            let txn = txn_to_commit.unwrap();
            txn.commit().await?;
            crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::Committed).await;
            get_txn_holder_registry().write().await.remove(txn_id);
        }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过事务ID回滚事务
///
/// 允许通过事务ID远程回滚事务
///
/// # 参数
/// * `txn_id` - 事务ID
///
/// # 返回值
/// * `Result<()>` - 成功返回 Ok(())，失败返回错误
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

        if should_rollback && txn_to_rollback.is_some() {
            let txn = txn_to_rollback.unwrap();
            txn.rollback().await?;
            crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::RolledBack).await;
            get_txn_holder_registry().write().await.remove(txn_id);
        }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过数据库ID获取Dbx实例
///
/// 允许通过数据库ID获取到Dbx实例，以便执行数据库操作
///
/// # 参数
/// * `db_id` - 数据库ID
///
/// # 返回值
/// * `Option<Dbx>` - Dbx实例，如果数据库不存在则返回None
pub async fn get_dbx_by_db_id(db_id: &str) -> Option<Dbx> {
    get_default_db_manager().get_dbx(db_id).await.ok()
}

/// 通过事务ID获取TxnHolder的可变引用
///
/// 允许通过事务ID获取到TxnHolder的可变引用，以便操作事务
///
/// # 参数
/// * `txn_id` - 事务ID
/// * `f` - 闭包，用于操作事务
///
/// # 返回值
/// * `Result<T>` - 成功返回闭包的返回值，失败返回错误
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

/// 通过数据库ID和事务ID执行SQL操作（无参数）
///
/// 允许通过数据库ID和事务ID执行SQL操作，适用于wasm调用host的场景
///
/// # 参数
/// * `db_id` - 数据库ID
/// * `txn_id` - 事务ID，None表示使用非事务方式执行
/// * `sql` - SQL语句
///
/// # 返回值
/// * `Result<u64>` - 执行结果，返回受影响的行数
pub async fn execute_sql(db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64> {
    let sql = sql.to_string();
    debug!("execute_sql: db_id: {}, txn_id: {:?}, sql: {}", db_id, txn_id, sql);
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, move |txn| Box::pin(async move {
                let result = txn.execute(&sql).await?;
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id).await {
                match dbx.db() {
                    crate::connection::DbPool::Postgres(pool) => {
                        let result = sqlx::query(&sql).execute(pool).await?;
                        Ok(result.rows_affected())
                    },
                    crate::connection::DbPool::MySql(pool) => {
                        let result = sqlx::query(&sql).execute(pool).await?;
                        Ok(result.rows_affected())
                    },
                    crate::connection::DbPool::Sqlite(pool) => {
                        let result = sqlx::query(&sql).execute(pool).await?;
                        Ok(result.rows_affected())
                    },
                }
            } else {
                Err(Error::NoDb)
            }
        },
    }
}

/// 通过数据库ID和事务ID执行带参数的SQL操作
///
/// 允许通过数据库ID和事务ID执行带参数的SQL操作，适用于wasm调用host的场景
///
/// # 参数
/// * `db_id` - 数据库ID
/// * `txn_id` - 事务ID，None表示使用非事务方式执行
/// * `sql` - SQL语句
/// * `params` - SQL参数，支持 Json、DataValues、SqlxValues
///
/// # 返回值
/// * `Result<u64>` - 执行结果，返回受影响的行数
pub async fn execute_sql_with_params(db_id: &str, txn_id: Option<&str>, sql: &str, params: SqlParams) -> Result<u64> {
    let sql = sql.to_string();
    debug!("execute_sql_with_params: db_id: {}, txn_id: {:?}, sql: {}, ", db_id, txn_id, sql);
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, move |txn| Box::pin(async move {
                let result = match params {
                    SqlParams::Json(json) => {
                        let values = json_to_data_values(json)
                            .map_err(|e| Error::InvalidParams(e))?;
                        txn.execute_with_datavalues(&sql, &values).await?
                    },
                    SqlParams::DataValues(values) => {
                        txn.execute_with_datavalues(&sql, &values).await?
                    },
                    SqlParams::SqlxValues(sqlx_values) => {
                        txn.execute_with_sqlxvalues(&sql, sqlx_values).await?
                    },
                };
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id).await {
                match params {
                    SqlParams::Json(json) => {
                        let values = json_to_data_values(json)
                            .map_err(|e| Error::InvalidParams(e))?;
                        execute_with_datavalues_no_txn(&dbx, &sql, &values).await
                    },
                    SqlParams::DataValues(values) => {
                        execute_with_datavalues_no_txn(&dbx, &sql, &values).await
                    },
                    SqlParams::SqlxValues(sqlx_values) => {
                        match dbx.db() {
                            crate::connection::DbPool::Postgres(pool) => {
                                let query = sqlx::query_with(&sql, sqlx_values);
                                let result = query.execute(pool).await?;
                                Ok(result.rows_affected())
                            },
                            crate::connection::DbPool::MySql(_) => {
                                Err(Error::InvalidParams("MySql not supported with sea-query yet".to_string()))
                            },
                            crate::connection::DbPool::Sqlite(_) => {
                                Err(Error::InvalidParams("Sqlite not supported with sea-query yet".to_string()))
                            },
                        }
                    },
                }
            } else {
                Err(Error::NoDb)
            }
        },
    }
}

/// 通过数据库ID和事务ID执行SQL查询（无参数）
///
/// 允许通过数据库ID和事务ID执行SQL查询并返回DataSet，适用于wasm调用host的场景
///
/// # 参数
/// * `db_id` - 数据库ID
/// * `txn_id` - 事务ID，None表示使用非事务方式执行
/// * `sql` - SQL查询语句
/// * `dataset_id` - 数据集唯一标识
///
/// # 返回值
/// * `Result<DataSet>` - 查询结果转换为DataSet
pub async fn query_sql(db_id: &str, txn_id: Option<&str>, sql: &str, dataset_id: &str) -> Result<DataSet> {
    let sql = sql.to_string();
    let dataset_id = dataset_id.to_string();
    debug!("query_sql: db_id: {}, txn_id: {:?}, sql: {}, dataset_id: {}",  db_id, txn_id, sql, dataset_id);

    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| Box::pin(async move {
                let result = txn.query(&sql, &dataset_id).await?;
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id).await {
                match dbx.db() {
                    crate::connection::DbPool::Postgres(pool) => {
                        let rows = sqlx::query(&sql).fetch_all(pool).await?;
                        Ok(ResultConverter::convert_postgres_rows(rows, &dataset_id))
                    },
                    crate::connection::DbPool::MySql(pool) => {
                        let rows = sqlx::query(&sql).fetch_all(pool).await?;
                        Ok(ResultConverter::convert_mysql_rows(rows, &dataset_id))
                    },
                    crate::connection::DbPool::Sqlite(pool) => {
                        let rows = sqlx::query(&sql).fetch_all(pool).await?;
                        Ok(ResultConverter::convert_sqlite_rows(rows, &dataset_id))
                    },
                }
            } else {
                Err(Error::NoDb)
            }
        },
    }
}

/// 通过数据库ID和事务ID执行带参数的SQL查询并返回DataSet
///
/// 允许通过数据库ID和事务ID执行带参数的SQL查询并返回DataSet，适用于wasm调用host的场景
///
/// # 参数
/// * `db_id` - 数据库ID
/// * `txn_id` - 事务ID，None表示使用非事务方式执行
/// * `sql` - SQL查询语句
/// * `params` - SQL参数，支持 Json、DataValues、SqlxValues
/// * `dataset_id` - 数据集唯一标识
///
/// # 返回值
/// * `Result<DataSet>` - 查询结果转换为DataSet
pub async fn query_sql_with_params(db_id: &str, txn_id: Option<&str>, sql: &str, params: SqlParams, dataset_id: &str) -> Result<DataSet> {
    let sql = sql.to_string();
    let dataset_id = dataset_id.to_string();
    debug!("query_sql_with_params: db_id: {}, txn_id: {:?}, sql: {}, dataset_id: {}", db_id, txn_id, sql, dataset_id);
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| Box::pin(async move {
                let result = match params {
                    SqlParams::Json(json) => {
                        let values = json_to_data_values(json)
                            .map_err(|e| Error::InvalidParams(e))?;
                        txn.query_with_datavalues(&sql, &values, &dataset_id).await?
                    },
                    SqlParams::DataValues(values) => {
                        txn.query_with_datavalues(&sql, &values, &dataset_id).await?
                    },
                    SqlParams::SqlxValues(sqlx_values) => {
                        txn.query_with_sqlxvalues(&sql, sqlx_values, &dataset_id).await?
                    },
                };
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id).await {
                match params {
                    SqlParams::Json(json) => {
                        let values = json_to_data_values(json)
                            .map_err(|e| Error::InvalidParams(e))?;
                        query_with_datavalues_no_txn(&dbx, &sql, &values, &dataset_id).await
                    },
                    SqlParams::DataValues(values) => {
                        query_with_datavalues_no_txn(&dbx, &sql, &values, &dataset_id).await
                    },
                    SqlParams::SqlxValues(sqlx_values) => {
                        match dbx.db() {
                            crate::connection::DbPool::Postgres(pool) => {
                                let query = sqlx::query_with(&sql, sqlx_values);
                                let rows = query.fetch_all(pool).await?;
                                Ok(ResultConverter::convert_postgres_rows(rows, &dataset_id))
                            },
                            crate::connection::DbPool::MySql(_) => {
                                Err(Error::InvalidParams("MySql not supported with sea-query yet".to_string()))
                            },
                            crate::connection::DbPool::Sqlite(_) => {
                                Err(Error::InvalidParams("Sqlite not supported with sea-query yet".to_string()))
                            },
                        }
                    },
                }
            } else {
                Err(Error::NoDb)
            }
        },
    }
}

/// 非事务方式执行带 DataValue 参数的 SQL
async fn execute_with_datavalues_no_txn(dbx: &Dbx, sql: &str, params: &[DataValue]) -> Result<u64> {
    match dbx.db() {
        crate::connection::DbPool::Postgres(pool) => {
            let mut query = sqlx::query(sql);
            for param in params {
                query = bind_data_value_postgres(query, param);
            }
            let result = query.execute(pool).await?;
            Ok(result.rows_affected())
        },
        crate::connection::DbPool::MySql(pool) => {
            let mut query = sqlx::query(sql);
            for param in params {
                query = bind_data_value_mysql(query, param);
            }
            let result = query.execute(pool).await?;
            Ok(result.rows_affected())
        },
        crate::connection::DbPool::Sqlite(pool) => {
            let mut query = sqlx::query(sql);
            for param in params {
                query = bind_data_value_sqlite(query, param);
            }
            let result = query.execute(pool).await?;
            Ok(result.rows_affected())
        },
    }
}

/// 非事务方式查询带 DataValue 参数的 SQL
async fn query_with_datavalues_no_txn(dbx: &Dbx, sql: &str, params: &[DataValue], dataset_id: &str) -> Result<DataSet> {
    match dbx.db() {
        crate::connection::DbPool::Postgres(pool) => {
            let mut query = sqlx::query(sql);
            for param in params {
                query = bind_data_value_postgres(query, param);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
        },
        crate::connection::DbPool::MySql(pool) => {
            let mut query = sqlx::query(sql);
            for param in params {
                query = bind_data_value_mysql(query, param);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(ResultConverter::convert_mysql_rows(rows, dataset_id))
        },
        crate::connection::DbPool::Sqlite(pool) => {
            let mut query = sqlx::query(sql);
            for param in params {
                query = bind_data_value_sqlite(query, param);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(ResultConverter::convert_sqlite_rows(rows, dataset_id))
        },
    }
}

/// 开始事务并返回 TransactionGuard（RAII 模式）
///
/// 通过数据库ID创建事务，返回 TransactionGuard 来自动管理事务生命周期
///
/// # 参数
/// * `db_id` - 数据库ID
/// * `options` - 事务选项
///
/// # 返回值
/// * `Result<TransactionGuard>` - 成功返回 TransactionGuard，失败返回错误
///
/// # 示例
///
/// ```ignore
/// async fn do_something() -> Result<()> {
///     let guard = begin_transaction_guard_by_db_id("db1", TransactionOptions::default()).await?;
///     // 执行数据库操作...
///     guard.commit().await?; // 显式提交
///     Ok(())
/// } // 如果没有调用 commit，guard 析构时会自动回滚
/// ```
pub async fn begin_transaction_guard_by_db_id(
    db_id: &str,
    options: crate::manager::TransactionOptions,
) -> Result<TransactionGuard> {
    let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
    let dbx_with_txn = dbx.with_transaction()?;
    let txn_id = dbx_with_txn.begin_txn(db_id, options.propagation).await?;
    Ok(TransactionGuard::new(txn_id, db_id.to_string()))
}
