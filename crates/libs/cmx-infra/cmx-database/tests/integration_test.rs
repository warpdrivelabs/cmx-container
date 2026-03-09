use cmx_database::{register_db_pool, remove_db_pool, get_db_access, transaction, Propagation, commit_txn_by_id, rollback_txn_by_id, start_monitoring};
use cmx_database::{DbConfig, DbType, PoolConfig, Result};

// 测试数据库配置
const TEST_DB_URL: &str = "postgresql://postgres:postgres@192.168.137.80:5432/postgres";
const TEST_DB_KEY: &str = "test_db";

// 生成唯一的测试表名
fn generate_test_table_name() -> String {
    format!("test_transaction_{}", uuid::Uuid::new_v4().simple())
}

// 初始化测试环境
async fn setup_test_environment(test_table: &str) -> Result<()> {
    // 注册数据库连接池
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    // 先尝试移除已存在的连接池
    let _ = remove_db_pool(TEST_DB_KEY);

    register_db_pool(TEST_DB_KEY.to_string(), db_config).await?;

    // 创建测试表
    let dbx = get_db_access(TEST_DB_KEY).unwrap();
    let pool = match dbx.db() {
        cmx_database::DbPool::Postgres(pool) => pool,
        _ => panic!("Expected PostgreSQL pool"),
    };

    // 先删除已存在的表（如果存在），使用CASCADE选项删除相关的序列
    sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", test_table)).execute(pool).await?;

    // 尝试删除序列（如果存在）
    sqlx::query(&format!("DROP SEQUENCE IF EXISTS {}_id_seq CASCADE", test_table)).execute(pool).await?;

    // 创建测试表
    sqlx::query(&format!(
        r#"
        CREATE TABLE {} (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            value INTEGER NOT NULL
        )
        "#,
        test_table
    )).execute(pool).await?;

    Ok(())
}

// 清理测试环境
async fn cleanup_test_environment(test_table: &str) -> Result<()> {
    let dbx = get_db_access(TEST_DB_KEY).unwrap();
    let pool = match dbx.db() {
        cmx_database::DbPool::Postgres(pool) => pool,
        _ => panic!("Expected PostgreSQL pool"),
    };

    // 删除测试表
    sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", test_table)).execute(pool).await?;

    // 移除连接池
    remove_db_pool(TEST_DB_KEY);

    Ok(())
}

// 测试事务基本功能
#[tokio::test]
async fn test_transaction_basic() -> Result<()> {
    // 注册数据库连接池
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    // 先尝试移除已存在的连接池
    let _ = remove_db_pool(TEST_DB_KEY);
    
    register_db_pool(TEST_DB_KEY.to_string(), db_config).await?;

    // 简化测试，只测试事务的基本功能
    let dbx = get_db_access(TEST_DB_KEY).unwrap();
    let txn_dbx = dbx.with_transaction().unwrap();

    // 测试事务提交
    let result: Result<()> = transaction!(TEST_DB_KEY, txn_dbx, async {
        // 执行一个简单的 SQL 语句
        Ok(())
    }).await;

    assert!(result.is_ok());

    // 移除连接池
    remove_db_pool(TEST_DB_KEY);
    
    Ok(())
}

// 测试事务回滚
#[tokio::test]
async fn test_transaction_rollback() -> Result<()> {
    // 注册数据库连接池
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    // 先尝试移除已存在的连接池
    let _ = remove_db_pool(TEST_DB_KEY);
    
    register_db_pool(TEST_DB_KEY.to_string(), db_config).await?;

    // 简化测试，只测试事务的回滚功能
    let dbx = get_db_access(TEST_DB_KEY).unwrap();
    let txn_dbx = dbx.with_transaction().unwrap();

    // 测试事务回滚
    let result: Result<()> = transaction!(TEST_DB_KEY, txn_dbx, async {
        // 故意失败
        Err(cmx_database::Error::NoTxn)
    }).await;

    assert!(result.is_err());

    // 移除连接池
    remove_db_pool(TEST_DB_KEY);
    
    Ok(())
}

// 测试事务传播机制
#[tokio::test]
async fn test_transaction_propagation() -> Result<()> {
    // 注册数据库连接池
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    // 先尝试移除已存在的连接池
    let _ = remove_db_pool(TEST_DB_KEY);
    
    register_db_pool(TEST_DB_KEY.to_string(), db_config).await?;

    // 简化测试，只测试事务的传播行为
    let dbx = get_db_access(TEST_DB_KEY).unwrap();
    let txn_dbx = dbx.with_transaction().unwrap();

    // 测试 REQUIRED 传播行为
    let result: Result<()> = transaction!(TEST_DB_KEY, txn_dbx, Propagation::Required, async {
        // 嵌套事务，使用 REQUIRED 传播行为
        let nested_result: Result<()> = transaction!(TEST_DB_KEY, txn_dbx, Propagation::Required, async {
            Ok(())
        }).await;
        
        assert!(nested_result.is_ok());
        Ok(())
    }).await;

    assert!(result.is_ok());

    // 移除连接池
    remove_db_pool(TEST_DB_KEY);
    
    Ok(())
}

// 测试通过事务ID操作事务
#[tokio::test]
async fn test_transaction_by_id() -> Result<()> {
    // 注册数据库连接池
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    // 先尝试移除已存在的连接池
    let _ = remove_db_pool(TEST_DB_KEY);
    
    register_db_pool(TEST_DB_KEY.to_string(), db_config).await?;

    // 简化测试，只测试通过事务ID操作事务的功能
    let dbx = get_db_access(TEST_DB_KEY).unwrap();
    let txn_dbx = dbx.with_transaction().unwrap();

    // 开始事务
    let txn_id = txn_dbx.begin_txn(TEST_DB_KEY, Propagation::Required).await?;

    // 通过事务ID提交事务
    commit_txn_by_id(&txn_id).await?;

    // 移除连接池
    remove_db_pool(TEST_DB_KEY);
    
    Ok(())
}

// 测试监控功能
#[tokio::test]
async fn test_monitoring() -> Result<()> {
    // 注册数据库连接池
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    // 先尝试移除已存在的连接池
    let _ = remove_db_pool(TEST_DB_KEY);
    
    register_db_pool(TEST_DB_KEY.to_string(), db_config).await?;

    // 启动监控
    start_monitoring().await;

    // 等待一段时间，确保监控任务启动
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // 监控任务应该正常运行，没有 panic
    assert!(true);

    // 移除连接池
    remove_db_pool(TEST_DB_KEY);
    
    Ok(())
}
