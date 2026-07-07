//! cmx-database-pg 集成测试（需真实 PostgreSQL）。
//!
//! 默认连接串沿用旧 crate 测试；可用环境变量 `TEST_PG_URL` 覆盖：
//!   TEST_PG_URL=postgresql://user:pass@host:5432/db cargo test -p cmx-database-pg
//!
//! 无可用 PG 时这些用例会在 `register_data_source`/首次连接处失败——它们标记为
//! `#[ignore]`，默认 `cargo test` 跳过；显式 `cargo test -- --ignored` 运行。

use cmx_core::model::cell::{DataValue, SqlParam, SqlTypeMarker};
use cmx_core::model::data::dataset::DataSet;
use cmx_database_pg::{
    DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig,
    transaction::begin_transaction_guard_by_db_id,
};

const DEFAULT_DB_URL: &str = "postgresql://postgres:postgres@192.168.137.80:5432/postgres";
const TEST_DB_KEY: &str = "test_pg_db";

fn test_db_url() -> String {
    std::env::var("TEST_PG_URL").unwrap_or_else(|_| DEFAULT_DB_URL.to_string())
}

fn generate_test_table_name() -> String {
    format!("test_pg_{}", uuid::Uuid::new_v4().simple())
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
        db_url: test_db_url(),
        db_id: TEST_DB_KEY.to_string(),
        db_schema: Some("public".to_string()),
        db_name: None,
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
        domain_code: None,
        application_code: None,
        module_code: None,
        default: true,
        source_type: None,
    };

    let manager = DatabaseManager::new(DatabaseManagerConfig::default());
    manager.register_data_source(db_config).await.unwrap();
    manager
}

// ---------------------------------------------------------------------------
// 1:1 移植旧 crate 的 8 个基础用例
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_database_manager_basic() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let data_sources = manager.list_data_sources().await;
    assert!(data_sources.contains(&TEST_DB_KEY.to_string()));
    let health = manager.health_check(TEST_DB_KEY).await?;
    assert!(health);
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_begin_transaction() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;
    assert!(!txn_id.is_empty());
    manager.commit_transaction(&txn_id).await?;
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_transaction_commit() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let table = generate_test_table_name();

    manager
        .execute_sql(
            TEST_DB_KEY,
            None,
            &format!("CREATE TABLE {table} (id SERIAL PRIMARY KEY, name VARCHAR(100))"),
        )
        .await?;

    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;
    manager
        .execute_sql(
            TEST_DB_KEY,
            Some(&txn_id),
            &format!("INSERT INTO {table} (name) VALUES ('test')"),
        )
        .await?;
    manager.commit_transaction(&txn_id).await?;

    let dataset: DataSet = manager
        .query_sql(
            TEST_DB_KEY,
            None,
            &format!("SELECT * FROM {table} WHERE name = 'test'"),
            "test_commit",
        )
        .await?;
    assert_eq!(dataset.rows.len(), 1, "事务提交后应该能查询到插入的数据");

    manager
        .execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_transaction_rollback() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let table = generate_test_table_name();

    manager
        .execute_sql(
            TEST_DB_KEY,
            None,
            &format!("CREATE TABLE {table} (id SERIAL PRIMARY KEY, name VARCHAR(100))"),
        )
        .await?;

    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;
    manager
        .execute_sql(
            TEST_DB_KEY,
            Some(&txn_id),
            &format!("INSERT INTO {table} (name) VALUES ('should_be_rolled_back')"),
        )
        .await?;
    manager.rollback_transaction(&txn_id).await?;

    let dataset: DataSet = manager
        .query_sql(
            TEST_DB_KEY,
            None,
            &format!("SELECT * FROM {table} WHERE name = 'should_be_rolled_back'"),
            "test_rollback",
        )
        .await?;
    assert_eq!(dataset.rows.len(), 0, "事务回滚后应该查询不到插入的数据");

    manager
        .execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_query_sql() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let table = generate_test_table_name();

    manager
        .execute_sql(
            TEST_DB_KEY,
            None,
            &format!("CREATE TABLE {table} (id SERIAL PRIMARY KEY, name VARCHAR(100))"),
        )
        .await?;
    manager
        .execute_sql(
            TEST_DB_KEY,
            None,
            &format!("INSERT INTO {table} (name) VALUES ('test1'), ('test2')"),
        )
        .await?;

    let dataset: DataSet = manager
        .query_sql(
            TEST_DB_KEY,
            None,
            &format!("SELECT * FROM {table} ORDER BY id"),
            "test_query",
        )
        .await?;
    assert_eq!(dataset.rows.len(), 2);

    manager
        .execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_transaction_with_propagation() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let txn_id = manager.get_transaction_context().begin(TEST_DB_KEY).await?;
    let table = generate_test_table_name();

    manager
        .execute_sql(
            TEST_DB_KEY,
            Some(&txn_id),
            &format!("CREATE TABLE {table} (id SERIAL PRIMARY KEY, value INTEGER)"),
        )
        .await?;
    manager
        .execute_sql(
            TEST_DB_KEY,
            Some(&txn_id),
            &format!("INSERT INTO {table} (value) VALUES (100)"),
        )
        .await?;
    manager.commit_transaction(&txn_id).await?;
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_get_db_config() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let config = manager.get_db_config(TEST_DB_KEY).await?;
    assert_eq!(config.db_id, TEST_DB_KEY);
    assert_eq!(config.db_type, DbType::Postgres);
    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_health_check() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    assert!(manager.health_check(TEST_DB_KEY).await?);
    manager.shutdown().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 类型往返（最高优先级）：验证各 PG 类型 → DataValue 映射正确、numeric 是 Decimal、
// 全程不 panic；NullTyped 各 marker 正确绑定 NULL。
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_type_roundtrip() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let table = generate_test_table_name();

    manager
        .execute_sql(
            TEST_DB_KEY,
            None,
            &format!(
                "CREATE TABLE {table} (\
                 c_int8 BIGINT, c_int4 INTEGER, c_bool BOOLEAN, c_num NUMERIC(18,4), \
                 c_text TEXT, c_uuid UUID, c_bytea BYTEA, c_jsonb JSONB, \
                 c_ts TIMESTAMPTZ, c_tsn TIMESTAMP, c_date DATE)"
            ),
        )
        .await?;

    // 用带类型参数插入一行有值 + 一行全 NULL（NullTyped 各 marker）
    let uuid_val = uuid::Uuid::new_v4();
    let params_row1 = vec![
        DataValue::Int(9_000_000_000),
        DataValue::Int(42),
        DataValue::Bool(true),
        DataValue::Decimal("1130000.0000".parse().unwrap()),
        DataValue::String("héllo 世界".to_string()),
        DataValue::Uuid(uuid_val),
        DataValue::Binary(vec![1, 2, 3, 255]),
        DataValue::Json(r#"{"k":"v","n":1}"#.to_string()),
        DataValue::DateTime(chrono::Utc::now()),
        DataValue::DateTime(chrono::Utc::now()),
        DataValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 7, 5).unwrap()),
    ];
    manager
        .execute_sql_with_datavalues(
            TEST_DB_KEY,
            None,
            &format!(
                "INSERT INTO {table} \
                 (c_int8,c_int4,c_bool,c_num,c_text,c_uuid,c_bytea,c_jsonb,c_ts,c_tsn,c_date) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"
            ),
            params_row1,
        )
        .await?;

    // 全 NULL 行：用 SqlParam typed NULL（每列对应 marker）
    let null_row = vec![
        SqlParam::Null(SqlTypeMarker::Int),
        SqlParam::Null(SqlTypeMarker::Int),
        SqlParam::Null(SqlTypeMarker::Bool),
        SqlParam::Null(SqlTypeMarker::Decimal),
        SqlParam::Null(SqlTypeMarker::Text),
        SqlParam::Null(SqlTypeMarker::Uuid),
        SqlParam::Null(SqlTypeMarker::Binary),
        SqlParam::Null(SqlTypeMarker::Json),
        SqlParam::Null(SqlTypeMarker::Timestamp),
        SqlParam::Null(SqlTypeMarker::Timestamp),
        SqlParam::Null(SqlTypeMarker::Date),
    ];
    manager
        .execute_sql_typed(
            TEST_DB_KEY,
            None,
            &format!(
                "INSERT INTO {table} \
                 (c_int8,c_int4,c_bool,c_num,c_text,c_uuid,c_bytea,c_jsonb,c_ts,c_tsn,c_date) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"
            ),
            null_row,
        )
        .await?;

    // 回查有值行
    let ds: DataSet = manager
        .query_sql(
            TEST_DB_KEY,
            None,
            &format!("SELECT * FROM {table} WHERE c_int4 = 42"),
            "roundtrip",
        )
        .await?;
    assert_eq!(ds.rows.len(), 1);
    let row = &ds.rows[0];
    let get = |name: &str| row.get_by_name(&ds.schema, name).unwrap();

    assert!(matches!(get("c_int8"), DataValue::Int(9_000_000_000)));
    assert!(matches!(get("c_int4"), DataValue::Int(42)));
    assert!(matches!(get("c_bool"), DataValue::Bool(true)));
    // numeric 必须解成 Decimal 而非 Null
    assert!(
        matches!(get("c_num"), DataValue::Decimal(_)),
        "NUMERIC 应解码为 Decimal，实际: {:?}",
        get("c_num")
    );
    assert!(matches!(get("c_text"), DataValue::String(s) if s == "héllo 世界"));
    assert!(matches!(get("c_uuid"), DataValue::Uuid(u) if *u == uuid_val));
    assert!(matches!(get("c_bytea"), DataValue::Binary(b) if b == &[1,2,3,255]));
    assert!(matches!(get("c_jsonb"), DataValue::Json(_)));
    assert!(matches!(get("c_ts"), DataValue::DateTime(_)));
    assert!(matches!(get("c_tsn"), DataValue::DateTime(_)));
    assert!(matches!(get("c_date"), DataValue::Date(_)));

    // 回查全 NULL 行：全程不 panic，且各列为 Null
    let ds_null: DataSet = manager
        .query_sql(
            TEST_DB_KEY,
            None,
            &format!("SELECT * FROM {table} WHERE c_int4 IS NULL"),
            "roundtrip_null",
        )
        .await?;
    assert_eq!(ds_null.rows.len(), 1);
    let null_row = &ds_null.rows[0];
    for i in 0..ds_null.schema.field_count() {
        let v = null_row.get(i).unwrap();
        assert!(matches!(v, DataValue::Null), "NULL 列应为 DataValue::Null，实际 {:?}", v);
    }

    manager
        .execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    manager.shutdown().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// TransactionGuard：未 commit 时 Drop 自动回滚。
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_guard_drop_auto_rollback() -> cmx_database_pg::Result<()> {
    let manager = setup_db_manager().await;
    let table = generate_test_table_name();

    manager
        .execute_sql(
            TEST_DB_KEY,
            None,
            &format!("CREATE TABLE {table} (id SERIAL PRIMARY KEY, name TEXT)"),
        )
        .await?;

    {
        let guard =
            begin_transaction_guard_by_db_id(TEST_DB_KEY, Default::default()).await?;
        let txn_id = guard.txn_id().to_string();
        manager
            .execute_sql(
                TEST_DB_KEY,
                Some(&txn_id),
                &format!("INSERT INTO {table} (name) VALUES ('leak')"),
            )
            .await?;
        // 不 commit，作用域结束触发 Drop → mpsc 异步回滚
    }

    // 给后台清理任务一点时间完成回滚
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let ds: DataSet = manager
        .query_sql(
            TEST_DB_KEY,
            None,
            &format!("SELECT * FROM {table} WHERE name = 'leak'"),
            "guard_drop",
        )
        .await?;
    assert_eq!(ds.rows.len(), 0, "Guard 未提交，Drop 应自动回滚");

    manager
        .execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    manager.shutdown().await?;
    Ok(())
}
