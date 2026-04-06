/*
 * @Author: yqs
 * @Date: 2026-03-05 13:31:23
 * @Describe: 数据库操作模块，支持 WebAssembly 调用 host 实现数据库操作
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-09 15:00:00
 */
pub mod config;
pub mod connection;
pub mod host_functions;
pub mod monitoring;
pub mod transaction;
pub mod executor;
pub mod manager;
pub mod types;

// 导出错误类型，确保所有模块都可以使用crate::Error
pub mod error;
pub mod crud;

pub use error::{Error, Result};

pub use config::{DbConfig, DbType, PoolConfig};
pub use monitoring::start_monitoring;
pub use executor::{ParamValue, ResultConverter};
pub use cmx_core::model::data::dataset::DataSet;
pub use manager::{DatabaseManager, DatabaseManagerConfig, TransactionContext, TransactionOptions, get_default_db_manager};
pub use types::{QueryBuilder, TypedRow, TypedResult, CompareOp, OrderDirection};
pub use transaction::{check_long_running_transactions, cleanup_completed_transactions, commit_txn_by_id, execute_sql, execute_sql_with_params, get_active_transactions, get_dbx_by_db_id, get_txn_holder_by_id, get_txn_metadata, query_sql, query_sql_with_params, rollback_txn_by_id, with_transaction_by_id, Dbx, Propagation, TransactionMetadata, TransactionStatus, SqlParams};
pub use connection::{DbPool};







