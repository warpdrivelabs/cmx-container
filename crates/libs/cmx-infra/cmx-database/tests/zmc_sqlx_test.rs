//! sqlx 侧 ZmcDataSet 往返测试(需真实 PostgreSQL)。
//!
//! 验证:sqlx 查真实 PG → `ZmcDataSet<SqlxPgRowSource>` → `encode_columnar_binary` → msgpack 解回,
//! 结构与老 columnar 契约同构、值对齐老 DataValue 编码。证明 sqlx 侧零拷贝编码正确、与
//! tokio 侧(cmx-database-pg zmc_test)输出一致。
//!
//! 默认库 dev-local.toml primary(`127.0.0.1:5432/cmx`);可用 TEST_PG_URL 覆盖。
//! `#[ignore]` 门控:`cargo test -p cmx-database --test zmc_sqlx_test -- --ignored`

use cmx_database::{DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig};

const DEFAULT_DB_URL: &str = "postgresql://postgres:postgres@127.0.0.1:5432/cmx";
const TEST_DB_KEY: &str = "zmc_sqlx_test_db";

fn test_db_url() -> String {
    std::env::var("TEST_PG_URL").unwrap_or_else(|_| DEFAULT_DB_URL.to_string())
}

fn table_name() -> String {
    format!("zmc_sqlx_{}", uuid::Uuid::new_v4().simple())
}

async fn setup() -> DatabaseManager {
    let pool_config = PoolConfig {
        max_connections: 4,
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

fn decode_msgpack(bytes: &[u8]) -> serde_json::Value {
    rmp_serde::from_slice(bytes).expect("msgpack 解码失败")
}

fn base64_std(b: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    BASE64.encode(b)
}

#[tokio::test]
#[ignore]
async fn test_sqlx_zmc_roundtrip() -> cmx_database::Result<()> {
    let mm = setup().await;
    let table = table_name();

    mm.execute_sql(
        TEST_DB_KEY,
        None,
        &format!(
            "CREATE TABLE {table} (\
             id BIGINT PRIMARY KEY, name TEXT, amount NUMERIC(18,4), \
             flag BOOLEAN, uid UUID, blob BYTEA, meta JSONB, \
             created TIMESTAMPTZ, note TEXT)"
        ),
    )
    .await?;

    let uid = uuid::Uuid::new_v4();
    mm.execute_sql_with_datavalues(
        TEST_DB_KEY,
        None,
        &format!(
            "INSERT INTO {table} (id,name,amount,flag,uid,blob,meta,created,note) \
             VALUES ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9)"
        ),
        vec![
            cmx_core::model::cell::DataValue::Int(2001),
            cmx_core::model::cell::DataValue::String("héllo 世界".to_string()),
            cmx_core::model::cell::DataValue::Decimal("1130000.5000".parse().unwrap()),
            cmx_core::model::cell::DataValue::Bool(true),
            cmx_core::model::cell::DataValue::Uuid(uid),
            cmx_core::model::cell::DataValue::Binary(vec![1, 2, 3, 255]),
            cmx_core::model::cell::DataValue::Json(r#"{"k":"v","n":1}"#.to_string()),
            cmx_core::model::cell::DataValue::DateTime(chrono::Utc::now()),
            cmx_core::model::cell::DataValue::String("note-a".to_string()),
        ],
    )
    .await?;
    // 全 NULL 行
    mm.execute_sql(
        TEST_DB_KEY,
        None,
        &format!(
            "INSERT INTO {table} (id,name,amount,flag,uid,blob,meta,created,note) \
             VALUES (2002, 'second', NULL, false, NULL, NULL, NULL, NULL, NULL)"
        ),
    )
    .await?;

    // 走 sqlx 的 query_zmc(经 DbPool)
    let dbx = mm.get_dbx(TEST_DB_KEY).await?;
    let zmc = dbx
        .db()
        .query_zmc(
            &format!(
                "SELECT id,name,amount,flag,uid,blob,meta,created,note FROM {table} ORDER BY id"
            ),
            "zmc_sqlx",
        )
        .await?;
    assert_eq!(zmc.row_count(), 2);

    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    let v = decode_msgpack(&buf);

    assert_eq!(v["datasetId"], "zmc_sqlx");
    let cols = v["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 9);
    assert_eq!(cols[0], "id");

    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);

    // 行 1(有值)—— 与 tokio 侧 zmc_test 断言完全一致(证明两侧输出同构)
    let r0 = rows[0].as_array().unwrap();
    assert_eq!(r0[0], 2001); // Int → number
    assert_eq!(r0[1], "héllo 世界"); // 文本零拷贝
    assert_eq!(r0[2], "1130000.5000"); // Decimal → 字符串
    assert_eq!(r0[3], true);
    assert_eq!(r0[4], uid.to_string());
    assert_eq!(r0[5], format!("B64:{}", base64_std(&[1, 2, 3, 255]))); // Binary → B64:
    assert!(r0[6].as_str().unwrap().contains("\"k\"")); // JSONB → JSON 串
    assert!(r0[7].is_string()); // timestamptz → RFC3339
    assert_eq!(r0[8], "note-a");

    // 行 2(NULL → null)
    let r1 = rows[1].as_array().unwrap();
    assert_eq!(r1[0], 2002);
    assert_eq!(r1[1], "second");
    assert!(r1[2].is_null());
    assert_eq!(r1[3], false);
    assert!(r1[4].is_null());
    assert!(r1[5].is_null());
    assert!(r1[6].is_null());
    assert!(r1[7].is_null());
    assert!(r1[8].is_null());

    mm.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    mm.shutdown().await?;
    Ok(())
}

/// 带参 + 数组绑定路径:`query_sql_zmc_with_datavalues` + `WHERE id = ANY($1)`。
///
/// 覆盖第三套实现(Sqlx 二进制)子层装载的核心 SQL 形态:父 id 数组一条 SQL 取整层。
#[tokio::test]
#[ignore]
async fn test_sqlx_zmc_with_datavalues() -> cmx_database::Result<()> {
    use cmx_core::model::cell::DataValue;

    let mm = setup().await;
    let table = table_name();

    mm.execute_sql(
        TEST_DB_KEY,
        None,
        &format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY, name TEXT)"),
    )
    .await?;
    mm.execute_sql(
        TEST_DB_KEY,
        None,
        &format!("INSERT INTO {table} (id, name) VALUES (2001,'a'), (2002,'b'), (2003,'c')"),
    )
    .await?;

    // 带参查询:仅取 id ∈ {2001, 2003}(数组绑定 → PG bigint[] → ANY($1))
    let zmc = mm
        .query_sql_zmc_with_datavalues(
            TEST_DB_KEY,
            &format!("SELECT id, name FROM {table} WHERE id = ANY($1) ORDER BY id"),
            vec![DataValue::Array(vec![
                DataValue::Int(2001),
                DataValue::Int(2003),
            ])],
            "zmc_sqlx_param",
        )
        .await?;
    assert_eq!(zmc.row_count(), 2);

    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    let v = decode_msgpack(&buf);
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].as_array().unwrap()[0], 2001);
    assert_eq!(rows[0].as_array().unwrap()[1], "a");
    assert_eq!(rows[1].as_array().unwrap()[0], 2003);
    assert_eq!(rows[1].as_array().unwrap()[1], "c");

    mm.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    mm.shutdown().await?;
    Ok(())
}
