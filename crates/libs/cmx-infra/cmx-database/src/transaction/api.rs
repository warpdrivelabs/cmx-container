/// 事务API模块，提供WebAssembly兼容的接口
///
/// 该模块定义了通过数据库ID和事务ID执行SQL操作的接口，包括：
/// - 通过事务ID操作事务的函数
/// - 通过数据库ID和事务ID执行SQL操作的函数
/// - 通过数据库ID和事务ID执行SQL查询的函数

use futures::future::BoxFuture;
use crate::error::{Error, Result};
use crate::transaction::core::{Dbx, DbTransaction};
use crate::transaction::registry::get_txn_holder_registry;
use crate::transaction::metadata::TransactionStatus;

use cmx_core::model::data::dataset::DataSet;
use crate::executor::{ParamValue, ResultConverter};

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
    // 从全局TxnHolder注册表中获取事务
    let txn_holder_mutex = get_txn_holder_registry().read().unwrap().get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        let mut should_commit = false;
        let mut txn_to_commit = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = txn_holder_mutex.lock().unwrap();

            // 检查是否存在事务
            if let Some(txh) = txh_g.as_mut() {
                // 减少引用计数
                let counter = txh.dec();

                // 如果计数器为 0，则提交事务
                if counter == 0 {
                    // 从 Option 中取出事务
                    txn_to_commit = txh_g.take();
                    should_commit = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        // 如果需要提交，执行提交操作并更新事务状态
        if should_commit && txn_to_commit.is_some() {
            let txn = txn_to_commit.unwrap();

            // 执行提交操作
            txn.commit().await?;

            // 更新事务状态
            crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::Committed);
            // 从注册表中移除
            get_txn_holder_registry().write().unwrap().remove(txn_id);
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
    // 从全局TxnHolder注册表中获取事务
    let txn_holder_mutex = get_txn_holder_registry().read().unwrap().get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        let mut should_rollback = false;
        let mut txn_to_rollback = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = txn_holder_mutex.lock().unwrap();

            // 检查是否存在事务
            if let Some(mut txn_holder) = txh_g.take() {
                // 检查引用计数
                if txn_holder.counter > 1 {
                    // 如果不是最后一个引用，减少计数并放回
                    txn_holder.counter -= 1;
                    let _ = txh_g.replace(txn_holder);
                } else {
                    // 保存事务以便后续回滚
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        // 如果需要回滚，执行回滚操作并更新事务状态
        if should_rollback && txn_to_rollback.is_some() {
            let txn = txn_to_rollback.unwrap();

            // 执行回滚操作
            txn.rollback().await?;

            // 更新事务状态
            crate::transaction::metadata::update_txn_status(txn_id, TransactionStatus::RolledBack);
            // 从注册表中移除
            get_txn_holder_registry().write().unwrap().remove(txn_id);
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
pub fn get_dbx_by_db_id(db_id: &str) -> Option<Dbx> {
    // 直接从连接模块导入 get_db_access 函数
    crate::connection::get_db_access(db_id)
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
    // 从全局TxnHolder注册表中获取事务
    let txn_holder_mutex = get_txn_holder_registry().read().unwrap().get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        // 获取事务持有器的锁
        let mut txh_g = txn_holder_mutex.lock().unwrap();

        // 检查是否存在事务
        if let Some(txh) = txh_g.as_mut() {
            // 执行闭包
            let result = f(&mut txh.txn).await;
            result
        } else {
            Err(Error::NoTxn)
        }
    } else {
        Err(Error::NoTxn)
    }
}

/// 通过数据库ID和事务ID执行SQL操作
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
pub async fn execute_sql_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64> {
    let sql = sql.to_string();
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, move |txn| Box::pin(async move {
                let result = txn.execute(&sql).await?;
                Ok(result)
            })).await
        },
        None => {
            // 非事务方式执行
            if let Some(dbx) = get_dbx_by_db_id(db_id) {
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
/// * `params` - SQL参数，使用serde_json序列化的参数数组
///
/// # 返回值
/// * `Result<u64>` - 执行结果，返回受影响的行数
pub async fn execute_sql_with_params_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value) -> Result<u64> {
    let sql = sql.to_string();
    let params: Vec<ParamValue> = match params {
        serde_json::Value::Array(arr) => arr.into_iter().map(ParamValue::from_json).collect(),
        _ => return Err(Error::InvalidParams("params must be an array".to_string())),
    };

    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| Box::pin(async move {
                let result = txn.execute(&sql).await?;
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id) {
                match dbx.db() {
                    crate::connection::DbPool::Postgres(pool) => {
                        let mut query = sqlx::query(&sql);
                        for param in &params {
                            query = match param {
                                ParamValue::Null => query.bind(None::<String>),
                                ParamValue::Bool(v) => query.bind(*v),
                                ParamValue::Int(v) => query.bind(*v),
                                ParamValue::Float(v) => query.bind(*v),
                                ParamValue::String(v) => query.bind(v.as_str()),
                                ParamValue::Decimal(v) => query.bind(v.to_string()),
                                ParamValue::DateTime(v) => query.bind(v.to_string()),
                                ParamValue::Date(v) => query.bind(v.to_string()),
                                ParamValue::Json(v) => query.bind(v.to_string()),
                            };
                        }
                        let result = query.execute(pool).await?;
                        Ok(result.rows_affected())
                    },
                    crate::connection::DbPool::MySql(pool) => {
                        let mut query = sqlx::query(&sql);
                        for param in &params {
                            query = match param {
                                ParamValue::Null => query.bind(None::<String>),
                                ParamValue::Bool(v) => query.bind(*v),
                                ParamValue::Int(v) => query.bind(*v),
                                ParamValue::Float(v) => query.bind(*v),
                                ParamValue::String(v) => query.bind(v.as_str()),
                                ParamValue::Decimal(v) => query.bind(v.to_string()),
                                ParamValue::DateTime(v) => query.bind(v.to_string()),
                                ParamValue::Date(v) => query.bind(v.to_string()),
                                ParamValue::Json(v) => query.bind(v.to_string()),
                            };
                        }
                        let result = query.execute(pool).await?;
                        Ok(result.rows_affected())
                    },
                    crate::connection::DbPool::Sqlite(pool) => {
                        let mut query = sqlx::query(&sql);
                        for param in &params {
                            query = match param {
                                ParamValue::Null => query.bind(None::<String>),
                                ParamValue::Bool(v) => query.bind(*v),
                                ParamValue::Int(v) => query.bind(*v),
                                ParamValue::Float(v) => query.bind(*v),
                                ParamValue::String(v) => query.bind(v.as_str()),
                                ParamValue::Decimal(v) => query.bind(v.to_string()),
                                ParamValue::DateTime(v) => query.bind(v.to_string()),
                                ParamValue::Date(v) => query.bind(v.to_string()),
                                ParamValue::Json(v) => query.bind(v.to_string()),
                            };
                        }
                        let result = query.execute(pool).await?;
                        Ok(result.rows_affected())
                    },
                }
            } else {
                Err(Error::NoDb)
            }
        },
    }
}

/// 通过数据库ID和事务ID执行SQL查询并返回DataSet
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
pub async fn query_sql_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str, dataset_id: &str) -> Result<DataSet> {
    let sql = sql.to_string();
    let dataset_id = dataset_id.to_string();
    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| Box::pin(async move {
                let result = txn.query(&sql, &dataset_id).await?;
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id) {
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
/// * `params` - SQL参数，使用serde_json序列化的参数数组
/// * `dataset_id` - 数据集唯一标识
///
/// # 返回值
/// * `Result<DataSet>` - 查询结果转换为DataSet
pub async fn query_sql_with_params_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value, dataset_id: &str) -> Result<DataSet> {
    let sql = sql.to_string();
    let dataset_id = dataset_id.to_string();
    let params: Vec<ParamValue> = match params {
        serde_json::Value::Array(arr) => arr.into_iter().map(ParamValue::from_json).collect(),
        _ => return Err(Error::InvalidParams("params must be an array".to_string())),
    };

    match txn_id {
        Some(txn_id) => {
            with_transaction_by_id(txn_id, |txn| Box::pin(async move {
                let result = txn.query(&sql, &dataset_id).await?;
                Ok(result)
            })).await
        },
        None => {
            if let Some(dbx) = get_dbx_by_db_id(db_id) {
                match dbx.db() {
                    crate::connection::DbPool::Postgres(pool) => {
                        let mut query = sqlx::query(&sql);
                        for param in &params {
                            query = match param {
                                ParamValue::Null => query.bind(None::<String>),
                                ParamValue::Bool(v) => query.bind(*v),
                                ParamValue::Int(v) => query.bind(*v),
                                ParamValue::Float(v) => query.bind(*v),
                                ParamValue::String(v) => query.bind(v.as_str()),
                                ParamValue::Decimal(v) => query.bind(v.to_string()),
                                ParamValue::DateTime(v) => query.bind(v.to_string()),
                                ParamValue::Date(v) => query.bind(v.to_string()),
                                ParamValue::Json(v) => query.bind(v.to_string()),
                            };
                        }
                        let rows = query.fetch_all(pool).await?;
                        Ok(ResultConverter::convert_postgres_rows(rows, &dataset_id))
                    },
                    crate::connection::DbPool::MySql(pool) => {
                        let mut query = sqlx::query(&sql);
                        for param in &params {
                            query = match param {
                                ParamValue::Null => query.bind(None::<String>),
                                ParamValue::Bool(v) => query.bind(*v),
                                ParamValue::Int(v) => query.bind(*v),
                                ParamValue::Float(v) => query.bind(*v),
                                ParamValue::String(v) => query.bind(v.as_str()),
                                ParamValue::Decimal(v) => query.bind(v.to_string()),
                                ParamValue::DateTime(v) => query.bind(v.to_string()),
                                ParamValue::Date(v) => query.bind(v.to_string()),
                                ParamValue::Json(v) => query.bind(v.to_string()),
                            };
                        }
                        let rows = query.fetch_all(pool).await?;
                        Ok(ResultConverter::convert_mysql_rows(rows, &dataset_id))
                    },
                    crate::connection::DbPool::Sqlite(pool) => {
                        let mut query = sqlx::query(&sql);
                        for param in &params {
                            query = match param {
                                ParamValue::Null => query.bind(None::<String>),
                                ParamValue::Bool(v) => query.bind(*v),
                                ParamValue::Int(v) => query.bind(*v),
                                ParamValue::Float(v) => query.bind(*v),
                                ParamValue::String(v) => query.bind(v.as_str()),
                                ParamValue::Decimal(v) => query.bind(v.to_string()),
                                ParamValue::DateTime(v) => query.bind(v.to_string()),
                                ParamValue::Date(v) => query.bind(v.to_string()),
                                ParamValue::Json(v) => query.bind(v.to_string()),
                            };
                        }
                        let rows = query.fetch_all(pool).await?;
                        Ok(ResultConverter::convert_sqlite_rows(rows, &dataset_id))
                    },
                }
            } else {
                Err(Error::NoDb)
            }
        },
    }
}
