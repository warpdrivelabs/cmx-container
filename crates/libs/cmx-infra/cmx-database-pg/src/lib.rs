//! 基于 tokio-postgres 的数据库操作模块（PG-only），与 cmx-database（sqlx）并行。
//!
//! 独立门面：入口 [`get_default_pg_db_manager`]，与 cmx-database 完全隔离（独立全局单例
//! 与注册表）。模块名对齐 cmx-database 以便逐一对照。
pub mod config;
pub mod connection;
pub mod executor;
pub mod host_functions;
pub mod manager;
pub mod monitoring;
pub mod transaction;
pub mod types;

pub mod crud;
pub mod error;
pub mod migration;
pub mod zmcdataset;

pub use error::{pg_detail, Error, Result};

pub use cmx_core::model::data::dataset::DataSet;
pub use config::{DbConfig, DbType, PoolConfig, PoolStatus};
pub use connection::DbPool;
pub use executor::{ParamValue, PgResultConverter};
pub use zmcdataset::{ZmcChildGroup, ZmcDataSet, ZmcSchema};
// 驱动无关的中立类型与编码器从 cmx-rowsource 重导出(对外统一入口)
pub use cmx_rowsource::{ZmcColType, ZmcRowSource};
pub use manager::{
    DatabaseManager, DatabaseManagerConfig, TransactionContext, TransactionOptions,
    get_default_pg_db_manager,
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
