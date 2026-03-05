/*
 * @Author: yqs
 * @Date: 2026-03-05 13:31:23
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-05 20:46:14
 */
pub mod config;
pub mod connection;
pub mod load_balancing;
pub mod metrics;
pub mod monitoring;
pub mod transaction;

// 重新导出错误类型，确保所有模块都可以使用crate::Error
pub use transaction::Error;
pub use transaction::Result;

pub use config::{DbConfig, DbType, PoolConfig};
pub use connection::{DbPool, register_db_pool, update_db_pool, remove_db_pool, get_db_access, get_db_config, get_db_access_with_timeout};
pub use load_balancing::{RoundRobinLoadBalancing, RandomLoadBalancing};
pub use metrics::{PoolMetrics, record_connection_acquire, increment_wait_queue, decrement_wait_queue, get_pool_metrics, get_all_pool_metrics};
pub use monitoring::start_monitoring;
pub use transaction::{Dbx, IsolationLevel, Propagation, TransactionMetadata, TransactionStatus, get_txn_metadata, get_active_transactions, cleanup_completed_transactions, check_long_running_transactions, commit_txn_by_id, rollback_txn_by_id};

