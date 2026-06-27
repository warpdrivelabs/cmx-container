use cmx_core::model::data::dataset::DataSet;
use cmx_database::{DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig};

const TEST_DB_URL: &str = "postgresql://postgres:postgres@192.168.137.80:5432/postgres";
const TEST_DB_KEY: &str = "test_db";

fn generate_test_table_name() -> String {
    format!("test_transaction_{}", uuid::Uuid::new_v4().simple())
}

async fn setup_db_manager() -> DatabaseManager {
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        acquire_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        db_id: TEST_DB_KEY.to_string(),
        db_schema: Some("public".to_string()),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
        default:true
    };

    let config = DatabaseManagerConfig::default();
    let manager = DatabaseManager::new(config);
    manager.register_data_source(db_config).await.unwrap();
    manager
}

#[tokio::test]
async fn test_database_manager_basic() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let data_sources = manager.list_data_sources().await;
    assert!(data_sources.contains(&TEST_DB_KEY.to_string()));

    let health = manager.health_check(TEST_DB_KEY).await?;
    assert!(health);

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_begin_transaction() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;
    assert!(!txn_id.is_empty());
    manager.commit_transaction(&txn_id).await?;

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_transaction_commit() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let table_name = generate_test_table_name();

    manager.execute_sql(TEST_DB_KEY, None, &format!("CREATE TABLE {} (id SERIAL PRIMARY KEY, name VARCHAR(100))", table_name)).await?;

    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;

    let insert_sql = format!("INSERT INTO {} (name) VALUES ('test')", table_name);
    manager.execute_sql(TEST_DB_KEY, Some(&txn_id), &insert_sql).await?;

    manager.commit_transaction(&txn_id).await?;

    let query_sql = format!("SELECT * FROM {} WHERE name = 'test'", table_name);
    let dataset: DataSet = manager.query_sql(TEST_DB_KEY, None, &query_sql, "test_commit").await?;

    assert_eq!(dataset.rows.len(), 1, "事务提交后应该能查询到插入的数据");

    manager.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {} CASCADE", table_name)).await?;

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_transaction_rollback() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let table_name = generate_test_table_name();

    manager.execute_sql(TEST_DB_KEY, None, &format!("CREATE TABLE {} (id SERIAL PRIMARY KEY, name VARCHAR(100))", table_name)).await?;

    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;

    let insert_sql = format!("INSERT INTO {} (name) VALUES ('should_be_rolled_back')", table_name);
    manager.execute_sql(TEST_DB_KEY, Some(&txn_id), &insert_sql).await?;

    manager.rollback_transaction(&txn_id).await?;

    let query_sql = format!("SELECT * FROM {} WHERE name = 'should_be_rolled_back'", table_name);
    let dataset: DataSet = manager.query_sql(TEST_DB_KEY, None, &query_sql, "test_rollback").await?;

    assert_eq!(dataset.rows.len(), 0, "事务回滚后应该查询不到插入的数据");

    manager.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {} CASCADE", table_name)).await?;

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_query_sql() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let table_name = generate_test_table_name();

    manager.execute_sql(TEST_DB_KEY, None, &format!("CREATE TABLE {} (id SERIAL PRIMARY KEY, name VARCHAR(100))", table_name)).await?;
    manager.execute_sql(TEST_DB_KEY, None, &format!("INSERT INTO {} (name) VALUES ('test1'), ('test2')", table_name)).await?;

    let query_sql = format!("SELECT * FROM {} ORDER BY id", table_name);
    let dataset: DataSet = manager.query_sql(TEST_DB_KEY, None, &query_sql, "test_query").await?;

    assert_eq!(dataset.rows.len(), 2);

    manager.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {} CASCADE", table_name)).await?;

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_transaction_with_propagation() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;

    let table_name = generate_test_table_name();

    let create_sql = format!("CREATE TABLE {} (id SERIAL PRIMARY KEY, value INTEGER)", table_name);
    manager.execute_sql(TEST_DB_KEY, Some(&txn_id), &create_sql).await?;

    let insert_sql = format!("INSERT INTO {} (value) VALUES (100)", table_name);
    manager.execute_sql(TEST_DB_KEY, Some(&txn_id), &insert_sql).await?;

    manager.commit_transaction(&txn_id).await?;

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_get_db_config() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let config = manager.get_db_config(TEST_DB_KEY).await?;
    assert_eq!(config.db_id, TEST_DB_KEY);
    assert_eq!(config.db_type, DbType::Postgres);

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_health_check() -> cmx_database::Result<()> {
    let manager = setup_db_manager().await;

    let is_healthy = manager.health_check(TEST_DB_KEY).await?;
    assert!(is_healthy);

    manager.shutdown().await?;
    Ok(())
}
