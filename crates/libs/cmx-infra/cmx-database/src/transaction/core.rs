//! 事务核心模块，包含核心事务结构和实现
//!
//! 该模块定义了事务的核心结构和基本操作，包括：
//! - Dbx：数据库访问对象，支持事务管理
//! - DbTransaction：统一的数据库事务类型，支持多种数据库
//! - TxnHolder：事务持有器，管理事务和引用计数

use crate::connection::DbPool;
use crate::error::{Error, Result};
use crate::executor::{bind_data_value_postgres, bind_data_value_mysql, bind_data_value_sqlite, ResultConverter};
use crate::transaction::metadata::{TransactionStatus, register_txn};
use crate::transaction::registry::get_txn_holder_registry;
use sqlx::{Executor, MySql, Postgres, Sqlite, Transaction as SqlxTransaction};
use sea_query_binder::SqlxValues;
use cmx_core::model::cell::DataValue;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc};
use tokio::sync::Mutex;
use uuid;

/// 数据库访问对象，支持事务管理
///
/// Dbx 是与数据库交互的主要入口点，提供了事务管理功能
#[derive(Debug, Clone)]
pub struct Dbx {
    /// 数据库连接池，用于获取数据库连接
    db_pool: DbPool,
    /// 事务持有器，使用互斥锁保护，确保线程安全
    txn_holder: Arc<Mutex<Option<TxnHolder>>>,
    /// 是否启用事务，为false时不能使用事务功能
    with_txn: bool,
    // /// 挂起的事务栈，用于保存被挂起的事务
    // suspended_txns: Arc<Mutex<Vec<TxnHolder>>>,
}

impl Dbx {
    /// 创建新的数据库访问对象
    ///
    /// # 参数
    /// * `db_pool` - 数据库连接池
    /// * `with_txn` - 是否启用事务
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回 Dbx 实例，失败返回错误
    pub fn new(db_pool: DbPool, with_txn: bool) -> Result<Self> {
        Ok(Dbx {
            db_pool,
            // 初始化为空的事务持有者
            txn_holder: Arc::default(),
            with_txn,
            // // 初始化挂起事务栈
            // suspended_txns: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 开始事务
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    /// * `propagation` - 事务传播行为
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    pub async fn begin_txn(&self, db_id: &str, propagation: Propagation) -> Result<String> {
        // 检查是否启用了事务
        if !self.with_txn {
            return Err(Error::CannotBeginTxnWithTxnFalse);
        }

        // 根据传播行为执行不同的逻辑
        match propagation {
            Propagation::Required => {
                self.do_required(db_id).await
            },
            // Propagation::RequiresNew => {
            //     self.do_requires_new(db_id).await
            // },
            // Propagation::Supports => {
            //     self.do_supports(db_id).await
            // },
            // Propagation::NotSupported => {
            //     self.do_not_supported(db_id).await
            // },
            // Propagation::Mandatory => {
            //     self.do_mandatory(db_id).await
            // },
            // Propagation::Never => {
            //     self.do_never(db_id).await
            // },
            _ => Err(Error::TransactionNotAllowed),
        }
    }

    /// Required: 如果存在事务则加入，否则创建新事务
    async fn do_required(&self, db_id: &str) -> Result<String> {
        let mut txh_g = self.txn_holder.lock().await;
        if txh_g.is_some() {
            if let Some(txh) = txh_g.as_mut() {
                txh.inc();
                Ok(txh.txn_id().to_string())
            } else {
                Err(Error::NoTxn)
            }
        } else {
            drop(txh_g);
            self.create_new_txn(db_id).await
        }
    }

    // /// RequiresNew: 创建新事务，挂起当前事务
    // async fn do_requires_new(&self, db_id: &str) -> Result<String> {
    //     // 保存当前事务状态（挂起）
    //     let mut suspended_txn: Option<TxnHolder> = None;
    //
    //     {
    //         let mut txh_g = self.txn_holder.lock().unwrap();
    //         if let Some(txh) = txh_g.take() {
    //             suspended_txn = Some(txh);
    //         }
    //     }
    //
    //     // 创建新事务
    //     let txn_id = self.create_new_txn(db_id).await?;
    //
    //     // 将挂起的事务保存到栈中
    //     if let Some(suspended) = suspended_txn {
    //         self.suspended_txns.lock().unwrap().push(suspended);
    //     }
    //
    //     Ok(txn_id)
    // }
    //
    // /// Supports: 如果存在事务则加入，否则以非事务方式执行
    // async fn do_supports(&self, _db_id: &str) -> Result<String> {
    //     let mut txh_g = self.txn_holder.lock().unwrap();
    //     if txh_g.is_some() {
    //         if let Some(txh) = txh_g.as_mut() {
    //             txh.inc();
    //             Ok(txh.txn_id().to_string())
    //         } else {
    //             Err(Error::NoTxn)
    //         }
    //     } else {
    //         Ok("non-transactional".to_string())
    //     }
    // }
    //
    // /// NotSupported: 以非事务方式执行，挂起当前事务
    // async fn do_not_supported(&self, _db_id: &str) -> Result<String> {
    //     // 保存当前事务状态（挂起）
    //     let mut suspended_txn: Option<TxnHolder> = None;
    //
    //     {
    //         let mut txh_g = self.txn_holder.lock().unwrap();
    //         if let Some(txh) = txh_g.take() {
    //             suspended_txn = Some(txh);
    //         }
    //     }
    //
    //     // 将挂起的事务保存到栈中
    //     if let Some(suspended) = suspended_txn {
    //         self.suspended_txns.lock().unwrap().push(suspended);
    //     }
    //
    //     Ok("non-transactional".to_string())
    // }
    //
    // /// Mandatory: 必须在事务中执行
    // async fn do_mandatory(&self, _db_id: &str) -> Result<String> {
    //     let mut txh_g = self.txn_holder.lock().unwrap();
    //     if txh_g.is_some() {
    //         if let Some(txh) = txh_g.as_mut() {
    //             txh.inc();
    //             Ok(txh.txn_id().to_string())
    //         } else {
    //             Err(Error::NoTxn)
    //         }
    //     } else {
    //         Err(Error::TransactionRequired)
    //     }
    // }
    //
    // /// Never: 必须以非事务方式执行
    // async fn do_never(&self, _db_id: &str) -> Result<String> {
    //     let txh_g = self.txn_holder.lock().unwrap();
    //     if txh_g.is_some() {
    //         Err(Error::TransactionNotAllowed)
    //     } else {
    //         Ok("non-transactional".to_string())
    //     }
    // }
    //
    // /// 恢复被挂起的事务
    // pub fn resume_suspended_txn(&self) {
    //     let suspended = self.suspended_txns.lock().unwrap().pop();
    //     if let Some(suspended) = suspended {
    //         let mut txh_g = self.txn_holder.lock().unwrap();
    //         if txh_g.is_none() {
    //             *txh_g = Some(suspended);
    //         } else {
    //             drop(txh_g);
    //             self.suspended_txns.lock().unwrap().push(suspended);
    //         }
    //     }
    // }
    //
    // /// 获取挂起事务的数量
    // pub fn suspended_txn_count(&self) -> usize {
    //     self.suspended_txns.lock().unwrap().len()
    // }

    /// 开始事务（默认传播行为：Required）
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    pub async fn begin_txn_default(&self, db_id: &str) -> Result<String> {
        // 使用默认的传播行为：Required
        self.begin_txn(db_id, Propagation::Required).await
    }

    /// 创建新事务
    ///
    /// 内部方法，用于创建新的事务实例
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    async fn create_new_txn(&self, db_id: &str) -> Result<String> {
        // 根据数据库类型创建事务
        let txn = match &self.db_pool {
            DbPool::Postgres(pool) => {
                let transaction = pool.begin().await?;
                DbTransaction::Postgres(transaction)
            },
            DbPool::MySql(pool) => {
                let transaction = pool.begin().await?;
                DbTransaction::MySql(transaction)
            },
            DbPool::Sqlite(pool) => {
                let transaction = pool.begin().await?;
                DbTransaction::Sqlite(transaction)
            },
        };

        // 创建事务持有器
        let txh = TxnHolder::new(txn, db_id);
        let txn_id = txh.txn_id().to_string();

        // 获取事务持有器的锁并插入新事务
        let mut txh_g = self.txn_holder.lock().await;
        let _ = txh_g.insert(txh);

        // 注册事务到元数据注册表
        register_txn(txn_id.clone(), db_id.to_string()).await;

        // 注册TxnHolder到全局注册表，以便通过事务ID操作事务
        get_txn_holder_registry().write().await.insert(txn_id.clone(), self.txn_holder.clone());

        // 返回事务ID
        Ok(txn_id)
    }

    /// 回滚事务
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn rollback_txn(&self) -> Result<()> {
        // 检查是否启用了事务
        if !self.with_txn {
            return Err(Error::CannotBeginTxnWithTxnFalse);
        }

        let mut txn_id: Option<String> = None;
        let mut should_rollback = false;
        let mut txn_to_rollback = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = self.txn_holder.lock().await;

            // 检查是否存在事务
            if let Some(mut txn_holder) = txh_g.take() {
                txn_id = Some(txn_holder.txn_id().to_string());

                // 检查引用计数
                if txn_holder.counter > 1 {
                    // 如果不是最后一个引用，减少计数并放回
                    txn_holder.counter -= 1;
                    let _ = txh_g.replace(txn_holder);
                } else {
                    // 保存事务以便后续回滚
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                    // 不需要替换，因为我们希望将其留为 None
                }
                Ok(())
            } else {
                // 没有活跃事务，返回错误
                Err(Error::NoTxn)
            }
        };
        result?;

        // 如果需要回滚，执行回滚操作并更新事务状态
        if should_rollback {
            if let (Some(txn), Some(txn_id)) = (txn_to_rollback, txn_id) {
                // 执行回滚操作
                txn.rollback().await?;

                // 更新事务状态为已回滚
                crate::transaction::metadata::update_txn_status(&txn_id, TransactionStatus::RolledBack).await;
                // 从全局TxnHolder注册表中移除
                get_txn_holder_registry().write().await.remove(&txn_id);
            }
        }

        Ok(())
    }

    /// 提交事务
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn commit_txn(&self) -> Result<()> {
        // 检查是否启用了事务
        if !self.with_txn {
            return Err(Error::CannotCommitTxnWithTxnFalse);
        }

        let mut txn_id: Option<String> = None;
        let mut should_commit = false;
        let mut txn_to_commit = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = self.txn_holder.lock().await;

            // 检查是否存在事务
            if let Some(txh) = txh_g.as_mut() {
                txn_id = Some(txh.txn_id().to_string());
                // 减少引用计数
                let counter = txh.dec();

                // 如果计数器为 0，则应该提交事务
                if counter == 0 {
                    // 从 Option 中取出事务
                    txn_to_commit = txh_g.take();
                    should_commit = true;
                }

                Ok(())
            }
            // 否则，返回错误
            else {
                Err(Error::TxnCantCommitNoOpenTxn)
            }
        };
        result?;

        // 如果需要提交，执行提交操作并更新事务状态
        if should_commit {
            if let (Some(txn), Some(txn_id)) = (txn_to_commit, txn_id) {
                // 执行提交操作
                txn.commit().await?;

                // 更新事务状态为已提交
                crate::transaction::metadata::update_txn_status(&txn_id, TransactionStatus::Committed).await;
                // 从全局TxnHolder注册表中移除
                get_txn_holder_registry().write().await.remove(&txn_id);
            }
        }

        Ok(())
    }

    /// 获取数据库连接池
    ///
    /// # 返回值
    /// * `&DbPool` - 数据库连接池引用
    pub fn db(&self) -> &DbPool {
        &self.db_pool
    }

    /// 获取当前事务ID
    ///
    /// # 返回值
    /// * `Option<String>` - 事务ID，如果没有活跃事务则返回 None
    pub async fn get_txn_id(&self) -> Option<String> {
        let txh_g = self.txn_holder.lock().await;
        txh_g.as_ref().map(|txh| txh.txn_id().to_string())
    }

    /// 检查事务是否超时
    ///
    /// # 参数
    /// * `timeout` - 超时时间
    ///
    /// # 返回值
    /// * `bool` - 如果事务超时则返回 true
    pub async fn is_txn_timeout(&self, timeout: std::time::Duration) -> bool {
        let txh_g = self.txn_holder.lock().await;
        txh_g.as_ref().map(|txh| txh.elapsed() > timeout).unwrap_or(false)
    }

    /// 创建一个支持事务的 Dbx 实例
    ///
    /// 从当前 Dbx 实例克隆连接池，并创建一个新的支持事务的 Dbx 实例
    /// 该实例不需要注册到注册表中，适用于临时的事务操作
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回支持事务的 Dbx 实例，失败返回错误
    pub fn with_transaction(&self) -> Result<Self> {
        Ok(Dbx {
            db_pool: self.db_pool.clone(),
            txn_holder: Arc::default(),
            with_txn: true,
            // suspended_txns: Arc::new(Mutex::new(Vec::new())),
        })
    }

}

/// 统一的数据库事务类型
///
/// 支持多种数据库类型的事务，包括PostgreSQL、MySQL和SQLite
#[derive(Debug)]
pub enum DbTransaction {
    /// PostgreSQL事务
    Postgres(SqlxTransaction<'static, Postgres>),
    /// MySQL事务
    MySql(SqlxTransaction<'static, MySql>),
    /// SQLite事务
    Sqlite(SqlxTransaction<'static, Sqlite>),
}

impl DbTransaction {
    /// 获取PostgreSQL事务的可变引用
    ///
    /// # 返回值
    /// * `Option<&mut SqlxTransaction<'static, Postgres>>` - PostgreSQL事务的可变引用，如果不是PostgreSQL事务则返回None
    pub fn as_postgres_mut(&mut self) -> Option<&mut SqlxTransaction<'static, Postgres>> {
        match self {
            DbTransaction::Postgres(txn) => Some(txn),
            _ => None,
        }
    }

    /// 获取MySQL事务的可变引用
    ///
    /// # 返回值
    /// * `Option<&mut SqlxTransaction<'static, MySql>>` - MySQL事务的可变引用，如果不是MySQL事务则返回None
    pub fn as_mysql_mut(&mut self) -> Option<&mut SqlxTransaction<'static, MySql>> {
        match self {
            DbTransaction::MySql(txn) => Some(txn),
            _ => None,
        }
    }

    /// 获取SQLite事务的可变引用
    ///
    /// # 返回值
    /// * `Option<&mut SqlxTransaction<'static, Sqlite>>` - SQLite事务的可变引用，如果不是SQLite事务则返回None
    pub fn as_sqlite_mut(&mut self) -> Option<&mut SqlxTransaction<'static, Sqlite>> {
        match self {
            DbTransaction::Sqlite(txn) => Some(txn),
            _ => None,
        }
    }

    /// 执行SQL语句
    ///
    /// # 参数
    /// * `sql` - SQL语句
    ///
    /// # 返回值
    /// * `sqlx::Result<u64>` - 执行结果，返回受影响的行数
    pub async fn execute(&mut self, sql: &str) -> sqlx::Result<u64> {
        match self {
            DbTransaction::Postgres(txn) => Ok(txn.execute(sql).await?.rows_affected()),
            DbTransaction::MySql(txn) => Ok(txn.execute(sql).await?.rows_affected()),
            DbTransaction::Sqlite(txn) => Ok(txn.execute(sql).await?.rows_affected()),
        }
    }

    /// 执行带 DataValue 参数的 SQL 语句
    ///
    /// # 参数
    /// * `sql` - SQL语句
    /// * `params` - DataValue 数组
    ///
    /// # 返回值
    /// * `sqlx::Result<u64>` - 执行结果，返回受影响的行数
    pub async fn execute_with_datavalues(&mut self, sql: &str, params: &[DataValue]) -> sqlx::Result<u64> {
        if params.is_empty() {
            return self.execute(sql).await;
        }

        match self {
            DbTransaction::Postgres(txn) => {
                let mut query = sqlx::query(sql);
                for param in params {
                    query = bind_data_value_postgres(query, param);
                }
                let result = query.execute(txn.as_mut()).await?;
                Ok(result.rows_affected())
            },
            DbTransaction::MySql(txn) => {
                let mut query = sqlx::query(sql);
                for param in params {
                    query = bind_data_value_mysql(query, param);
                }
                let result = query.execute(txn.as_mut()).await?;
                Ok(result.rows_affected())
            },
            DbTransaction::Sqlite(txn) => {
                let mut query = sqlx::query(sql);
                for param in params {
                    query = bind_data_value_sqlite(query, param);
                }
                let result = query.execute(txn.as_mut()).await?;
                Ok(result.rows_affected())
            },
        }
    }

    /// 执行带 serde_json::Value 参数的 SQL 语句
    ///
    /// # 参数
    /// * `sql` - SQL语句
    /// * `params` - serde_json::Value 数组
    ///
    /// # 返回值
    /// * `sqlx::Result<u64>` - 执行结果，返回受影响的行数
    pub async fn execute_with_json(&mut self, sql: &str, params: serde_json::Value) -> sqlx::Result<u64> {
        let values: Vec<DataValue> = serde_json::from_value(params)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        self.execute_with_datavalues(sql, &values).await
    }

    /// 执行SQL查询并返回DataSet
    ///
    /// # 参数
    /// * `sql` - SQL查询语句
    /// * `dataset_id` - 数据集唯一标识
    ///
    /// # 返回值
    /// * `sqlx::Result<DataSet>` - 查询结果转换为DataSet
    pub async fn query(&mut self, sql: &str, dataset_id: &str) -> sqlx::Result<cmx_core::model::data::dataset::DataSet> {
        match self {
            DbTransaction::Postgres(txn) => {
                let rows = txn.fetch_all(sqlx::query(sql)).await?;
                Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
            },
            DbTransaction::MySql(txn) => {
                let rows = txn.fetch_all(sqlx::query(sql)).await?;
                Ok(ResultConverter::convert_mysql_rows(rows, dataset_id))
            },
            DbTransaction::Sqlite(txn) => {
                let rows = txn.fetch_all(sqlx::query(sql)).await?;
                Ok(ResultConverter::convert_sqlite_rows(rows, dataset_id))
            },
        }
    }

    /// 执行带参数的SQL查询并返回DataSet
    pub async fn query_with_datavalues(&mut self, sql: &str, params: &[DataValue], dataset_id: &str) -> sqlx::Result<cmx_core::model::data::dataset::DataSet> {
        if params.is_empty() {
            return self.query(sql, dataset_id).await;
        }

        match self {
            DbTransaction::Postgres(txn) => {
                let mut query = sqlx::query(sql);
                for param in params {
                    query = bind_data_value_postgres(query, param);
                }
                let rows = txn.fetch_all(query).await?;
                Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
            },
            DbTransaction::MySql(txn) => {
                let mut query = sqlx::query(sql);
                for param in params {
                    query = bind_data_value_mysql(query, param);
                }
                let rows = txn.fetch_all(query).await?;
                Ok(ResultConverter::convert_mysql_rows(rows, dataset_id))
            },
            DbTransaction::Sqlite(txn) => {
                let mut query = sqlx::query(sql);
                for param in params {
                    query = bind_data_value_sqlite(query, param);
                }
                let rows = txn.fetch_all(query).await?;
                Ok(ResultConverter::convert_sqlite_rows(rows, dataset_id))
            },
        }
    }

    /// 执行带 serde_json::Value 参数的 SQL 查询并返回 DataSet
    pub async fn query_with_json(&mut self, sql: &str, params: serde_json::Value, dataset_id: &str) -> sqlx::Result<cmx_core::model::data::dataset::DataSet> {
        let values: Vec<DataValue> = serde_json::from_value(params)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        self.query_with_datavalues(sql, &values, dataset_id).await
    }

    /// 执行带 sea-query-binder Values 的 SQL 语句
    ///
    /// # 参数
    /// * `sql` - SQL语句
    /// * `params` - sea-query-binder 的 SqlxValues
    ///
    /// # 返回值
    /// * `sqlx::Result<u64>` - 执行结果，返回受影响的行数
    pub async fn execute_with_sqlxvalues(&mut self, sql: &str, params: SqlxValues) -> sqlx::Result<u64> {
        match self {
            DbTransaction::Postgres(txn) => {
                let query = sqlx::query_with(sql, params);
                let result = query.execute(txn.as_mut()).await?;
                Ok(result.rows_affected())
            },
            DbTransaction::MySql(_txn) => {
                Err(sqlx::Error::Protocol("MySql not supported with sea-query yet".to_string()))
            },
            DbTransaction::Sqlite(_txn) => {
                Err(sqlx::Error::Protocol("Sqlite not supported with sea-query yet".to_string()))
            },
        }
    }

    /// 执行带 sea-query-binder Values 的 SQL 查询并返回 DataSet
    ///
    /// # 参数
    /// * `sql` - SQL查询语句
    /// * `params` - sea-query-binder 的 SqlxValues
    /// * `dataset_id` - 数据集唯一标识
    ///
    /// # 返回值
    /// * `sqlx::Result<DataSet>` - 查询结果转换为DataSet
    pub async fn query_with_sqlxvalues(&mut self, sql: &str, params: SqlxValues, dataset_id: &str) -> sqlx::Result<cmx_core::model::data::dataset::DataSet> {
        match self {
            DbTransaction::Postgres(txn) => {
                let query = sqlx::query_with(sql, params);
                let rows = txn.fetch_all(query).await?;
                Ok(ResultConverter::convert_postgres_rows(rows, dataset_id))
            },
            DbTransaction::MySql(_txn) => {
                Err(sqlx::Error::Protocol("MySql not supported with sea-query yet".to_string()))
            },
            DbTransaction::Sqlite(_txn) => {
                Err(sqlx::Error::Protocol("Sqlite not supported with sea-query yet".to_string()))
            },
        }
    }
}

/// 事务持有器，管理事务和引用计数
///
/// 用于跟踪事务的状态和引用计数，支持事务的嵌套使用
#[derive(Debug)]
pub struct TxnHolder {
    /// 数据库事务实例
    pub txn: DbTransaction,
    /// 引用计数器，用于跟踪事务的嵌套次数
    pub counter: i32,
    /// 事务ID，全局唯一
    txn_id: String,
    /// 数据库ID，用于标识事务所属的数据库
    db_id: String,
    /// 创建时间，用于计算事务运行时间
    create_time: std::time::Instant,
}

impl TxnHolder {
    /// 创建新的事务持有器
    ///
    /// # 参数
    /// * `txn` - 数据库事务
    /// * `db_id` - 数据库ID
    ///
    /// # 返回值
    /// * `Self` - 事务持有器实例
    pub fn new(txn: DbTransaction, db_id: &str) -> Self {
        TxnHolder {
            txn,
            counter: 1,  // 初始引用计数为1
            txn_id: uuid::Uuid::new_v4().to_string(),  // 生成唯一事务ID
            db_id: db_id.to_string(),
            create_time: std::time::Instant::now(),  // 记录创建时间
        }
    }

    /// 增加引用计数
    ///
    /// 用于支持事务的嵌套使用
    pub fn inc(&mut self) {
        self.counter += 1;
    }

    /// 减少引用计数并返回当前值
    ///
    /// # 返回值
    /// * `i32` - 当前引用计数
    pub fn dec(&mut self) -> i32 {
        self.counter -= 1;
        self.counter
    }

    /// 回滚事务
    ///
    /// 执行实际的事务回滚操作
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn rollback(self) -> Result<()> {
        match self.txn {
            DbTransaction::Postgres(dbtnx) => dbtnx.rollback().await?,
            DbTransaction::MySql(dbtnx) => dbtnx.rollback().await?,
            DbTransaction::Sqlite(dbtnx) => dbtnx.rollback().await?,
        }
        Ok(())
    }

    /// 提交事务
    ///
    /// 执行实际的事务提交操作
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn commit(self) -> Result<()> {
        match self.txn {
            DbTransaction::Postgres(dbtnx) => dbtnx.commit().await?,
            DbTransaction::MySql(dbtnx) => dbtnx.commit().await?,
            DbTransaction::Sqlite(dbtnx) => dbtnx.commit().await?,
        }
        Ok(())
    }

    /// 获取事务ID
    ///
    /// # 返回值
    /// * `&str` - 事务ID
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 获取数据库ID
    ///
    /// # 返回值
    /// * `&str` - 数据库ID
    pub fn db_id(&self) -> &str {
        &self.db_id
    }

    /// 获取事务运行时间
    ///
    /// # 返回值
    /// * `std::time::Duration` - 事务运行时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.create_time.elapsed()
    }
}

/// 实现Deref trait，允许直接访问内部事务
impl Deref for TxnHolder {
    type Target = DbTransaction;

    /// 解引用获取事务引用
    ///
    /// # 返回值
    /// * `&Self::Target` - 事务引用
    fn deref(&self) -> &Self::Target {
        &self.txn
    }
}

/// 实现DerefMut trait，允许直接修改内部事务
impl DerefMut for TxnHolder {
    /// 可变解引用获取事务可变引用
    ///
    /// # 返回值
    /// * `&mut Self::Target` - 事务可变引用
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.txn
    }
}

// /// 事务隔离级别
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum IsolationLevel {
//     /// 读取未提交：允许读取未提交的数据
//     ReadUncommitted,
//     /// 读取已提交：只能读取已提交的数据
//     ReadCommitted,
//     /// 可重复读：保证在同一个事务中多次读取同一数据时结果一致
//     RepeatableRead,
//     /// 串行化：最高隔离级别，完全锁定数据
//     Serializable,
// }

/// 事务传播行为
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Propagation {
    /// REQUIRED (默认)：如果当前存在事务，则加入该事务；如果当前不存在事务，则创建一个新事务
    Required,
    /// REQUIRES_NEW：创建一个新事务，如果当前存在事务，则将当前事务挂起
    RequiresNew,
    /// SUPPORTS：如果当前存在事务，则加入该事务；如果当前不存在事务，则以非事务方式执行
    Supports,
    /// NOT_SUPPORTED：以非事务方式执行，如果当前存在事务，则将当前事务挂起
    NotSupported,
    /// MANDATORY：必须在事务中执行，如果当前不存在事务，则抛出异常
    Mandatory,
    /// NEVER：必须以非事务方式执行，如果当前存在事务，则抛出异常
    Never,
}
