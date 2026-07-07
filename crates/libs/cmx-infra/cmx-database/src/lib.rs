/*
 * @Author: yqs
 * @Date: 2026-03-05 13:31:23
 * @Describe: 数据库操作模块，支持 WebAssembly 调用 host 实现数据库操作
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-09 15:00:00
 */
pub mod config;
pub mod connection;
pub mod executor;
pub mod host_functions;
pub mod manager;
pub mod monitoring;
pub mod transaction;
pub mod types;

// 导出错误类型，确保所有模块都可以使用crate::Error
pub mod crud;
pub mod error;
pub mod migration;
// sqlx 侧零拷贝行来源(ZmcDataSet<SqlxPgRowSource>),纯新增,不动老 DataSet 路径
pub mod zmc;

pub use error::{Error, Result};

pub use cmx_core::model::data::dataset::DataSet;
pub use config::{DbConfig, DbType, PoolConfig};
pub use connection::DbPool;
pub use zmc::{SqlxPgRowSource, ZmcChildGroup, ZmcDataSet, ZmcSchema};
// 驱动无关的中立类型与 trait 从 cmx-rowsource 重导出(对外统一入口,与 cmx-database-pg 对称)
pub use cmx_rowsource::{ZmcColType, ZmcRowSource};
pub use executor::{ParamValue, ResultConverter};
pub use manager::{
    DatabaseManager, DatabaseManagerConfig, TransactionContext, TransactionOptions,
    get_default_db_manager,
};
pub use monitoring::start_monitoring;
pub use transaction::{
    Dbx, Propagation, SqlParams, TransactionMetadata, TransactionStatus,
    check_long_running_transactions, cleanup_completed_transactions, commit_txn_by_id, execute_sql,
    execute_sql_with_params, get_active_transactions, get_dbx_by_db_id, get_txn_holder_by_id,
    get_txn_metadata, query_sql, query_sql_with_params, rollback_txn_by_id, with_transaction_by_id,
};
pub use types::{CompareOp, OrderDirection, QueryBuilder, TypedResult, TypedRow};

pub use host_functions::DatabaseHostFunctions;

pub use migration::{
    MigrationError, MigrationLoader, MigrationRecord, MigrationResult, MigrationRunner,
    MigrationStatus as DbMigrationStatus, MigrationSummary, PendingMigration,
};
