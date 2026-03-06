/*
 * @Author: yqs
 * @Date: 2026-03-05 13:31:23
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-05 19:07:50
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

#[cfg(test)]
mod tests {
    use super::*;

    // 测试配置结构
    #[test]
    fn test_config_structures() {
        // 测试 PoolConfig 默认值
        let pool_config = PoolConfig::default();
        // 在测试环境中，max_connections 会被设置为 1
        let expected_max_connections = if cfg!(test) { 1 } else { 10 };
        assert_eq!(pool_config.max_connections, expected_max_connections);
        assert_eq!(pool_config.min_connections, 2);
        assert_eq!(pool_config.connect_timeout, 30);
        assert_eq!(pool_config.idle_timeout, 600);
        assert_eq!(pool_config.max_lifetime, 1800);

        // 测试 DbConfig 默认值
        let db_config = DbConfig::default();
        assert_eq!(db_config.db_type, DbType::Postgres);
        assert_eq!(db_config.db_url, "postgresql://localhost/test");
        assert_eq!(db_config.health_check_interval, 60);
        assert_eq!(db_config.health_check_timeout, 5);
    }

    // 测试负载均衡策略
    #[test]
    fn test_load_balancing() {
        // 测试轮询负载均衡
        let round_robin = RoundRobinLoadBalancing::new(vec!["db1".to_string(), "db2".to_string(), "db3".to_string()]);
        assert_eq!(round_robin.next().unwrap(), "db1");
        assert_eq!(round_robin.next().unwrap(), "db2");
        assert_eq!(round_robin.next().unwrap(), "db3");
        assert_eq!(round_robin.next().unwrap(), "db1"); // 循环

        // 测试随机负载均衡
        let mut random = RandomLoadBalancing::new(vec!["db1".to_string(), "db2".to_string()]);
        let result1 = random.next().unwrap();
        let result2 = random.next().unwrap();
        // 验证结果是有效的数据库键
        assert!(result1 == "db1" || result1 == "db2");
        assert!(result2 == "db1" || result2 == "db2");
    }

    // 测试事务状态枚举
    #[test]
    fn test_transaction_status() {
        assert_eq!(TransactionStatus::Active, TransactionStatus::Active);
        assert_eq!(TransactionStatus::Committed, TransactionStatus::Committed);
        assert_eq!(TransactionStatus::RolledBack, TransactionStatus::RolledBack);
        assert_ne!(TransactionStatus::Active, TransactionStatus::Committed);
    }

    // 测试传播行为枚举
    #[test]
    fn test_propagation() {
        assert_eq!(Propagation::Required, Propagation::Required);
        assert_eq!(Propagation::RequiresNew, Propagation::RequiresNew);
        // assert_eq!(Propagation::Nested, Propagation::Nested);
        assert_eq!(Propagation::Supports, Propagation::Supports);
        assert_eq!(Propagation::NotSupported, Propagation::NotSupported);
        assert_eq!(Propagation::Mandatory, Propagation::Mandatory);
        assert_eq!(Propagation::Never, Propagation::Never);
    }

    // 测试隔离级别枚举
    #[test]
    fn test_isolation_level() {
        assert_eq!(IsolationLevel::ReadUncommitted, IsolationLevel::ReadUncommitted);
        assert_eq!(IsolationLevel::ReadCommitted, IsolationLevel::ReadCommitted);
        assert_eq!(IsolationLevel::RepeatableRead, IsolationLevel::RepeatableRead);
        assert_eq!(IsolationLevel::Serializable, IsolationLevel::Serializable);
    }
}
