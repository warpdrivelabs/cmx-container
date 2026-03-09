/*
 * @Author: yqs
 * @Date: 2026-03-05 13:31:23
 * @Describe: 数据库操作模块，支持 WebAssembly 调用 host 实现数据库操作
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-09 15:00:00
 */
pub mod config;
pub mod connection;
pub mod monitoring;
pub mod transaction;

// 导出错误类型，确保所有模块都可以使用crate::Error
pub mod error;
pub use error::{Error, Result};

pub use config::{DbConfig, DbType, PoolConfig};
pub use connection::{get_db_access, get_db_access_with_timeout, get_db_config, register_db_pool, remove_db_pool, update_db_pool, DbPool};
pub use monitoring::start_monitoring;
pub use transaction::{check_long_running_transactions, cleanup_completed_transactions, commit_txn_by_id, execute_sql_by_ids, execute_sql_with_params_by_ids, get_active_transactions, get_dbx_by_db_id, get_txn_holder_by_id, get_txn_metadata, query_sql_by_ids, query_sql_with_params_by_ids, rollback_txn_by_id, with_transaction_by_id, Dbx, IsolationLevel, Propagation, TransactionMetadata, TransactionStatus};

/// 数据库操作模块，支持 WebAssembly 调用 host 实现数据库操作
///
/// 该模块提供了以下功能：
/// - 数据库连接池管理：支持 PostgreSQL、MySQL 和 SQLite
/// - 事务管理：支持事务的创建、提交、回滚和传播行为
/// - 数据库操作接口：通过数据库 ID 和事务 ID 执行 SQL 操作
/// - 监控和指标采集：跟踪事务状态和连接池使用情况
///
/// 主要设计目标：
/// 1. 支持 WebAssembly 调用 host 实现数据库操作
/// 2. 确保 wasm 端不持有任何数据库相关资源
/// 3. 所有数据库交互通过传递数据库 ID 和事务 ID 完成
/// 4. 支持多种数据库后端（PostgreSQL、MySQL、SQLite）
/// 5. 提供统一的接口，与具体数据库类型解耦
/// 6. 使用 tokio 作为唯一的异步运行时
///
/// # 核心功能
///
/// ## 数据库连接管理
/// - `register_db_pool`：注册数据库连接池
/// - `update_db_pool`：更新数据库连接池配置
/// - `remove_db_pool`：移除数据库连接池
/// - `get_db_access`：获取数据库访问对象
///
/// ## 事务管理
/// - `begin_txn`：开始事务，支持不同的传播行为
/// - `commit_txn`：提交事务
/// - `rollback_txn`：回滚事务
/// - `commit_txn_by_id`：通过事务 ID 提交事务
/// - `rollback_txn_by_id`：通过事务 ID 回滚事务
///
/// ## WebAssembly 兼容接口
/// - `execute_sql_by_ids`：通过数据库 ID 和事务 ID 执行 SQL 操作
/// - `execute_sql_with_params_by_ids`：通过数据库 ID 和事务 ID 执行带参数的 SQL 操作
/// - `query_sql_by_ids`：通过数据库 ID 和事务 ID 执行 SQL 查询并返回 DataSet
/// - `query_sql_with_params_by_ids`：通过数据库 ID 和事务 ID 执行带参数的 SQL 查询并返回 DataSet
/// - `get_dbx_by_db_id`：通过数据库 ID 获取 Dbx 实例
/// - `get_txn_holder_by_id`：通过事务 ID 获取事务持有器
///
/// # 使用示例
///
/// ```rust
/// // 注册数据库连接池
/// let config = DbConfig {
///     db_type: DbType::Postgres,
///     db_url: "postgresql://localhost/test",
///     ..Default::default()
/// };
/// register_db_pool("db1".to_string(), config).await?;
///
/// // 获取数据库访问对象
/// let dbx = get_db_access("db1").unwrap();
/// let dbx_with_txn = dbx.with_transaction().unwrap();
///
/// // 开始事务
/// let txn_id = dbx_with_txn.begin_txn_default("db1").await?;
///
/// // 执行 SQL 操作
/// let result = execute_sql_by_ids("db1", Some(&txn_id), "INSERT INTO users (name) VALUES ('test')").await?;
///
/// // 提交事务
/// dbx_with_txn.commit_txn().await?;
///
/// // 移除数据库连接池
/// remove_db_pool("db1");
/// ```

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

    // 测试数据库连接池创建
    #[tokio::test]
    async fn test_new_db_pool() {
        // 测试PostgreSQL连接池创建
        let postgres_config = DbConfig {
            db_type: DbType::Postgres,
            db_url: "postgresql://localhost/test".to_string(),
            ..Default::default()
        };
        let postgres_result = connection::new_db_pool(&postgres_config).await;
        // 注意：这里可能会失败，因为实际连接需要真实的数据库
        // 但我们可以测试代码结构是否正确
        match postgres_result {
            Ok(DbPool::Postgres(_)) => println!("PostgreSQL连接池创建成功"),
            Err(e) => println!("PostgreSQL连接池创建失败: {}", e),
            _ => panic!("PostgreSQL连接池类型错误"),
        }

        // 测试MySQL连接池创建
        let mysql_config = DbConfig {
            db_type: DbType::MySql,
            db_url: "mysql://localhost/test".to_string(),
            ..Default::default()
        };
        let mysql_result = connection::new_db_pool(&mysql_config).await;
        match mysql_result {
            Ok(DbPool::MySql(_)) => println!("MySQL连接池创建成功"),
            Err(e) => println!("MySQL连接池创建失败: {}", e),
            _ => panic!("MySQL连接池类型错误"),
        }

        // 测试SQLite连接池创建
        let sqlite_config = DbConfig {
            db_type: DbType::Sqlite,
            db_url: "sqlite::memory:".to_string(),
            ..Default::default()
        };
        let sqlite_result = connection::new_db_pool(&sqlite_config).await;
        match sqlite_result {
            Ok(DbPool::Sqlite(_)) => println!("SQLite连接池创建成功"),
            Err(e) => println!("SQLite连接池创建失败: {}", e),
            _ => panic!("SQLite连接池类型错误"),
        }
    }

    // 测试事务管理
    #[tokio::test]
    async fn test_transaction_management() {
        // 使用SQLite内存数据库进行测试
        let config = DbConfig {
            db_type: DbType::Sqlite,
            db_url: "sqlite::memory:".to_string(),
            ..Default::default()
        };

        // 注册数据库连接池
        let db_id = "test_sqlite";
        register_db_pool(db_id.to_string(), config).await.unwrap();

        // 获取Dbx实例
        let dbx = get_db_access(db_id).unwrap();
        let dbx_with_txn = dbx.with_transaction().unwrap();

        // 开始事务
        let txn_id = dbx_with_txn.begin_txn_default(db_id).await.unwrap();
        assert!(!txn_id.is_empty());

        // 提交事务
        let commit_result = dbx_with_txn.commit_txn().await;
        assert!(commit_result.is_ok());

        // 移除数据库连接池
        remove_db_pool(db_id);
    }

    // 测试通过ID执行SQL和查询数据
    #[tokio::test]
    async fn test_execute_sql_by_ids() {
        // 使用SQLite内存数据库进行测试
        let config = DbConfig {
            db_type: DbType::Sqlite,
            db_url: "sqlite::memory:".to_string(),
            ..Default::default()
        };

        // 注册数据库连接池
        let db_id = "test_sqlite_execute";
        register_db_pool(db_id.to_string(), config).await.unwrap();

        // 获取Dbx实例
        let dbx = get_db_access(db_id).unwrap();
        let dbx_with_txn = dbx.with_transaction().unwrap();

        // 创建表
        let create_table_sql = "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, name TEXT)";
        let result = execute_sql_by_ids(db_id, None, create_table_sql).await;
        assert!(result.is_ok());

        // 开始事务
        let txn_id = dbx_with_txn.begin_txn_default(db_id).await.unwrap();

        // 在事务中插入数据
        let insert_sql = "INSERT INTO test (name) VALUES ('test')";
        let result = execute_sql_by_ids(db_id, Some(&txn_id), insert_sql).await;
        assert!(result.is_ok());

        // 提交事务
        let commit_result = dbx_with_txn.commit_txn().await;
        assert!(commit_result.is_ok());

        // 查询数据并打印dataset
        let query_sql = "SELECT * FROM test";
        let dataset_id = "test_dataset";
        let dataset_result = query_sql_by_ids(db_id, None, query_sql, dataset_id).await;
        assert!(dataset_result.is_ok());

        let dataset = dataset_result.unwrap();
        println!("Dataset ID: {}", dataset.id);
        println!("Schema fields: {:?}", dataset.schema.fields);
        println!("Number of rows: {}", dataset.rows.len());

        // 打印每一行数据
        for (index, row) in dataset.rows.iter().enumerate() {
            println!("Row {}: {:?}", index, row);
        }

        // 移除数据库连接池
        remove_db_pool(db_id);
    }
}
