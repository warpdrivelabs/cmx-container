//! 事务 API 模块，提供 WebAssembly 兼容的数据库事务接口。
//!
//! 该模块定义了通过数据库ID和事务ID执行 SQL 操作的接口，包括：
//! - 通过事务ID提交和回滚事务的函数
//! - 通过数据库ID和事务ID执行 SQL 操作的函数
//! - 通过数据库ID和事务ID执行 SQL 查询的函数
//! - RAII 模式的 TransactionGuard，用于自动管理事务生命周期
use futures::future::BoxFuture;
use std::sync::OnceLock;
use tokio::sync::mpsc;
use crate::error::{Error, Result};
use crate::transaction::core::{Dbx, DbTransaction};
use crate::transaction::registry::{get_txn_holder_registry, get_txn_holder_by_id};
use crate::transaction::metadata::TransactionStatus;

use crate::executor::json_to_data_values;
use crate::get_default_db_manager;
use crate::DataSet;
use cmx_core::model::cell::DataValue;
use sea_query_sqlx::SqlxValues;
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

/// TransactionGuard - RAII 模式的事务守卫。
///
/// 确保事务在函数结束或发生 panic 时自动回滚（除非显式提交）。
///
/// 该结构体实现了 Drop trait，在析构时检查事务是否已提交：
/// - 若已调用 `commit()` 或 `rollback()`，则无事可做。
/// - 若未提交而 Guard 被析构（如 panic、提前 return、作用域结束），则自动发送回滚命令。
///
/// # Examples
///
/// ```
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
    /// 事务的唯一标识。
    txn_id: String,
    /// 所属数据库的唯一标识。
    db_id: String,
    /// 是否已显式提交或回滚。
    committed: bool,
}

impl TransactionGuard {
    /// 创建新的 TransactionGuard 实例。
    ///
    /// # Arguments
    ///
    /// * `txn_id` - 事务的唯一标识。
    /// * `db_id` - 所属数据库的唯一标识。
    ///
    /// # Returns
    ///
    /// 返回一个新的 TransactionGuard 实例，初始状态为未提交。
    pub fn new(txn_id: String, db_id: String) -> Self {
        Self {
            txn_id,
            db_id,
            committed: false,
        }
    }

    /// 返回事务的唯一标识。
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 返回所属数据库的唯一标识。
    pub fn db_id(&self) -> &str {
        &self.db_id
    }

    /// 检查事务是否已显式提交或回滚。
    ///
    /// # Returns
    ///
    /// * `true` - 已调用 `commit()` 或 `rollback()`。
    /// * `false` - 尚未提交或回滚。
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    /// 提交事务。
    ///
    /// 将 `committed` 标记设为 `true`，然后执行实际的事务提交操作。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 返回底层事务提交的错误。
    pub async fn commit(mut self) -> Result<()> {
        self.committed = true;
        commit_txn_by_id(&self.txn_id).await?;
        Ok(())
    }

    /// 回滚事务。
    ///
    /// 将 `committed` 标记设为 `true`，然后执行实际的事务回滚操作。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 返回底层事务回滚的错误。
    pub async fn rollback(mut self) -> Result<()> {
        self.committed = true;
        rollback_txn_by_id(&self.txn_id).await?;
        Ok(())
    }

    /// 获取与该 Guard 关联的数据库访问对象。
    ///
    /// # Returns
    ///
    /// * `Some(Dbx)` - 数据库存在时返回对应的 Dbx 访问对象。
    /// * `None` - 数据库不存在时返回。
    pub async fn get_dbx(&self) -> Option<Dbx> {
        get_dbx_by_db_id(&self.db_id).await
    }

    /// 在该事务的上下文中执行用户提供的闭包操作。
    ///
    /// # Arguments
    ///
    /// * `f` - 接受可变事务引用的异步闭包，返回 `Result<T>`。
    ///
    /// # Returns
    ///
    /// 成功时返回闭包的返回值。
    ///
    /// # Errors
    ///
    /// * `Error::NoTxn` - 事务已被清理或不存在。
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
/// 该枚举封装了三种不同的参数输入方式，以适配不同的使用场景：
/// - **Json**：适用于从外部（如 WebAssembly）传入 JSON 数据的场景，内部自动转换为 DataValue。
/// - **DataValues**：适用于已在 Rust 代码中构建好参数的场景，直接使用 DataValue 数组。
/// - **SqlxValues**：适用于通过 sea-query 构建器的场景，使用预绑定的 SqlxValues。
pub enum SqlParams {
    /// serde_json::Value 数组，内部自动转换为 DataValue。
    Json(serde_json::Value),
    /// DataValue 数组，直接使用。
    DataValues(Vec<DataValue>),
    /// sea-query-binder 的 SqlxValues，用于 sea-query 构建的 SQL。
    SqlxValues(SqlxValues),
}

/// 通过事务ID提交事务。
///
/// 允许通过事务ID远程提交事务。当事务引用计数归零时执行实际的数据库提交操作。
///
/// # Arguments
///
/// * `txn_id` - 待提交事务的唯一标识。
///
/// # Returns
///
/// 成功时返回 `Ok(())`。
///
/// # Errors
///
/// * `Error::NoTxn` - 指定的事务ID不存在或事务已被清理。
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

        if should_commit
            && let Some(txn) = txn_to_commit {
                txn.commit().await?;
                crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::Committed).await;
                get_txn_holder_registry().write().await.remove(txn_id);
            }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过事务ID回滚事务。
///
/// 允许通过事务ID远程回滚事务。当事务引用计数大于1时仅递减计数，
/// 当计数归零时执行实际的事务回滚操作。
///
/// # Arguments
///
/// * `txn_id` - 待回滚事务的唯一标识。
///
/// # Returns
///
/// 成功时返回 `Ok(())`。
///
/// # Errors
///
/// * `Error::NoTxn` - 指定的事务ID不存在或事务已被清理。
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

        if should_rollback
            && let Some(txn) = txn_to_rollback {
                txn.rollback().await?;
                crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::RolledBack).await;
                get_txn_holder_registry().write().await.remove(txn_id);
            }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过数据库ID获取 Dbx 实例。
///
/// # Arguments
///
/// * `db_id` - 数据库的唯一标识。
///
/// # Returns
///
/// * `Some(Dbx)` - 存在指定数据库时返回对应的 Dbx 访问对象。
/// * `None` - 数据库不存在时返回。
pub async fn get_dbx_by_db_id(db_id: &str) -> Option<Dbx> {
    get_default_db_manager().get_dbx(db_id).await.ok()
}

/// 在指定事务中执行用户提供的闭包操作。
///
/// 通过事务ID获取事务锁，执行用户闭包，完成后恢复事务锁的状态。
///
/// # Arguments
///
/// * `txn_id` - 事务的唯一标识。
/// * `f` - 接受可变事务引用的异步闭包，返回 `Result<T>`。
///
/// # Returns
///
/// 成功时返回闭包的返回值。
///
/// # Errors
///
/// * `Error::NoTxn` - 指定的事务ID不存在。
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
///
/// 当 `txn_id` 为 `Some` 时，在指定事务中执行 SQL；当 `txn_id` 为 `None` 时，
/// 直接从连接池获取连接执行。适用于 WebAssembly 调用 Host 的场景。
///
/// # Arguments
///
/// * `db_id` - 数据库的唯一标识。
/// * `txn_id` - 事务ID。`Some(txn_id)` 表示在事务中执行；`None` 表示使用非事务连接。
/// * `sql` - 待执行的 SQL 语句，不包含参数占位符。
///
/// # Returns
///
/// 成功时返回受影响的行数（`u64`）。SELECT 类语句返回 0。
///
/// # Errors
///
/// * `Error::NoDb` - 指定的数据库ID不存在。
/// * `Error::NoTxn` - 指定的事务ID不存在。
/// * 底层的 sqlx 执行错误。
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
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            dbx.db().execute(&sql).await
        },
    }
}

/// 通过数据库ID和事务ID执行带参数的 SQL 操作。
///
/// 当 `txn_id` 为 `Some` 时，在指定事务中执行 SQL；当 `txn_id` 为 `None` 时，
/// 直接从连接池获取连接执行。`SqlParams` 支持三种参数类型：Json、DataValues 和 SqlxValues。
///
/// # Arguments
///
/// * `db_id` - 数据库的唯一标识。
/// * `txn_id` - 事务ID。`Some(txn_id)` 表示在事务中执行；`None` 表示使用非事务连接。
/// * `sql` - 待执行的 SQL 语句，包含 `?` 占位符。
/// * `params` - SQL 参数，支持三种类型：`Json`（serde_json::Value 数组）、
///   `DataValues`（DataValue 数组）和 `SqlxValues`（sea-query-binder）。
///
/// # Returns
///
/// 成功时返回受影响的行数（`u64`）。
///
/// # Errors
///
/// * `Error::NoDb` - 指定的数据库ID不存在。
/// * `Error::NoTxn` - 指定的事务ID不存在。
/// * `Error::InvalidParams` - 参数类型与数据库不兼容，或参数数量与占位符不匹配。
/// * 底层的 sqlx 执行错误。
pub async fn execute_sql_with_params(db_id: &str, txn_id: Option<&str>, sql: &str, params: SqlParams) -> Result<u64> {
    let sql = sql.to_string();
    debug!("execute_sql_with_params: db_id: {}, txn_id: {:?}, sql: {}, ", db_id, txn_id, sql);
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, move |txn| Box::pin(async move {
                let result = match params {
                    SqlParams::Json(json) => {
                        let values = json_to_data_values(json)
                            .map_err(Error::InvalidParams)?;
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
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            let pool = dbx.db();
            match params {
                SqlParams::Json(json) => {
                    let values = json_to_data_values(json)
                        .map_err(Error::InvalidParams)?;
                    pool.execute_with_datavalues(&sql, &values).await
                },
                SqlParams::DataValues(values) => {
                    pool.execute_with_datavalues(&sql, &values).await
                },
                SqlParams::SqlxValues(sqlx_values) => {
                    pool.execute_with_sqlxvalues(&sql, sqlx_values).await
                },
            }
        },
    }
}

/// 通过数据库ID和事务ID执行无参数的 SQL 查询并返回 DataSet。
///
/// 当 `txn_id` 为 `Some` 时，在指定事务中执行查询；当 `txn_id` 为 `None` 时，
/// 直接从连接池获取连接执行。适用于 WebAssembly 调用 Host 的场景。
///
/// # Arguments
///
/// * `db_id` - 数据库的唯一标识。
/// * `txn_id` - 事务ID。`Some(txn_id)` 表示在事务中执行；`None` 表示使用非事务连接。
/// * `sql` - 待执行的 SQL 查询语句，不包含参数占位符。
/// * `dataset_id` - 查询结果的唯一标识，用于构建返回的 DataSet schema。
///
/// # Returns
///
/// 成功时返回包含查询结果的 `DataSet`。空结果集返回空 DataSet。
///
/// # Errors
///
/// * `Error::NoDb` - 指定的数据库ID不存在。
/// * `Error::NoTxn` - 指定的事务ID不存在。
/// * 底层的 sqlx 执行错误。
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
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            dbx.db().query(&sql, &dataset_id).await
        },
    }
}

/// 通过数据库ID和事务ID执行带参数的 SQL 查询并返回 DataSet。
///
/// 当 `txn_id` 为 `Some` 时，在指定事务中执行查询；当 `txn_id` 为 `None` 时，
/// 直接从连接池获取连接执行。`SqlParams` 支持三种参数类型：Json、DataValues 和 SqlxValues。
///
/// # Arguments
///
/// * `db_id` - 数据库的唯一标识。
/// * `txn_id` - 事务ID。`Some(txn_id)` 表示在事务中执行；`None` 表示使用非事务连接。
/// * `sql` - 待执行的 SQL 查询语句，包含 `?` 占位符。
/// * `params` - SQL 参数，支持三种类型：`Json`（serde_json::Value 数组）、
///   `DataValues`（DataValue 数组）和 `SqlxValues`（sea-query-binder）。
/// * `dataset_id` - 查询结果的唯一标识，用于构建返回的 DataSet schema。
///
/// # Returns
///
/// 成功时返回包含查询结果的 `DataSet`。空结果集返回空 DataSet。
///
/// # Errors
///
/// * `Error::NoDb` - 指定的数据库ID不存在。
/// * `Error::NoTxn` - 指定的事务ID不存在。
/// * `Error::InvalidParams` - 参数类型与数据库不兼容，或参数数量与占位符不匹配。
/// * 底层的 sqlx 执行错误。
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
                            .map_err(Error::InvalidParams)?;
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
            let dbx = get_dbx_by_db_id(db_id).await.ok_or(Error::NoDb)?;
            let pool = dbx.db();
            match params {
                SqlParams::Json(json) => {
                    let values = json_to_data_values(json)
                        .map_err(Error::InvalidParams)?;
                    pool.query_with_datavalues(&sql, &values, &dataset_id).await
                },
                SqlParams::DataValues(values) => {
                    pool.query_with_datavalues(&sql, &values, &dataset_id).await
                },
                SqlParams::SqlxValues(sqlx_values) => {
                    pool.query_with_sqlxvalues(&sql, sqlx_values, &dataset_id).await
                },
            }
        },
    }
}

/// 开始事务并返回 TransactionGuard（RAII 模式）。
///
/// 通过数据库ID创建事务，返回 TransactionGuard 来自动管理事务生命周期。
/// 当 TransactionGuard 超出作用域且未被显式提交时，会自动触发回滚。
///
/// # Arguments
///
/// * `db_id` - 数据库的唯一标识。
/// * `options` - 事务选项，包括传播行为等配置。
///
/// # Returns
///
/// 成功时返回 TransactionGuard。
///
/// # Errors
///
/// * `Error::NoDb` - 指定的数据库ID不存在。
///
/// # Examples
///
/// ```
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
