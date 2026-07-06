//! ZMCDataSet 集成测试（需真实 PostgreSQL）。
//!
//! 验证阶段 1–3:query_zmc → ZmcDataSet → encode_columnar_binary → msgpack 解回,
//! 结构与老 columnar 契约(`{datasetId, columns, rows, childRows}`)同构、值对齐老 DataValue 编码。
//!
//! 默认连接串取 dev-local.toml 的 primary 库(`127.0.0.1:5432/cmx`);可用 TEST_PG_URL 覆盖。
//! 用例 `#[ignore]` 门控:`cargo test -p cmx-database-pg --test zmc_test -- --ignored`

use cmx_core::model::cell::{DataValue, SqlParam, SqlTypeMarker};
use cmx_database_pg::{
    DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig,
};

const DEFAULT_DB_URL: &str = "postgresql://postgres:postgres@127.0.0.1:5432/cmx";
const TEST_DB_KEY: &str = "zmc_test_db";

fn test_db_url() -> String {
    std::env::var("TEST_PG_URL").unwrap_or_else(|_| DEFAULT_DB_URL.to_string())
}

fn table_name() -> String {
    format!("zmc_bench_{}", uuid::Uuid::new_v4().simple())
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

/// 用 rmp-serde 把 msgpack 字节解成 serde_json::Value（模拟前端 msgpack.decode → JS 对象）。
fn decode_msgpack(bytes: &[u8]) -> serde_json::Value {
    rmp_serde::from_slice(bytes).expect("msgpack 解码失败")
}

/// 阶段 1+2+3:平表 → ZmcDataSet → 列式二进制 → 解回校验结构与值。
#[tokio::test]
#[ignore]
async fn test_zmc_columnar_roundtrip() -> cmx_database_pg::Result<()> {
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
    // 行 1:全有值
    mm.execute_sql_with_datavalues(
        TEST_DB_KEY,
        None,
        &format!(
            "INSERT INTO {table} (id,name,amount,flag,uid,blob,meta,created,note) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        ),
        vec![
            DataValue::Int(1001),
            DataValue::String("héllo 世界".to_string()),
            DataValue::Decimal("1130000.5000".parse().unwrap()),
            DataValue::Bool(true),
            DataValue::Uuid(uid),
            DataValue::Binary(vec![1, 2, 3, 255]),
            DataValue::Json(r#"{"k":"v","n":1}"#.to_string()),
            DataValue::DateTime(chrono::Utc::now()),
            DataValue::String("note-a".to_string()),
        ],
    )
    .await?;
    // 行 2:含 NULL（note 为 typed NULL）
    mm.execute_sql_typed(
        TEST_DB_KEY,
        None,
        &format!(
            "INSERT INTO {table} (id,name,amount,flag,uid,blob,meta,created,note) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        ),
        vec![
            SqlParam::Int(1002),
            SqlParam::Text("second".to_string()),
            SqlParam::Null(SqlTypeMarker::Decimal),
            SqlParam::Bool(false),
            SqlParam::Null(SqlTypeMarker::Uuid),
            SqlParam::Null(SqlTypeMarker::Binary),
            SqlParam::Null(SqlTypeMarker::Json),
            SqlParam::Null(SqlTypeMarker::Timestamp),
            SqlParam::Null(SqlTypeMarker::Text),
        ],
    )
    .await?;

    // 查 ZmcDataSet + 编码
    let zmc = mm
        .query_sql_zmc(
            TEST_DB_KEY,
            &format!("SELECT id,name,amount,flag,uid,blob,meta,created,note FROM {table} ORDER BY id"),
            "zmc_test",
        )
        .await?;
    assert_eq!(zmc.row_count(), 2);

    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    let v = decode_msgpack(&buf);

    // 结构:datasetId / columns / rows
    assert_eq!(v["datasetId"], "zmc_test");
    let cols = v["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 9);
    assert_eq!(cols[0], "id");
    assert_eq!(cols[1], "name");

    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);

    // 行 1 值(按列序:id,name,amount,flag,uid,blob,meta,created,note)
    let r0 = rows[0].as_array().unwrap();
    assert_eq!(r0[0], 1001); // Int → msgpack int → JSON number
    assert_eq!(r0[1], "héllo 世界"); // 文本零拷贝
    assert_eq!(r0[2], "1130000.5000"); // Decimal → 字符串(保精度)
    assert_eq!(r0[3], true); // Bool
    assert_eq!(r0[4], uid.to_string()); // Uuid → 字符串
    assert_eq!(r0[5], format!("B64:{}", base64_std(&[1, 2, 3, 255]))); // Binary → B64: 前缀
    // meta JSONB:老契约保持 JSON 字符串;PG 存 jsonb 会规范化,只校验含 k/v
    assert!(r0[6].as_str().unwrap().contains("\"k\""));
    assert!(r0[7].is_string()); // created → RFC3339 字符串
    assert_eq!(r0[8], "note-a");

    // 行 2:NULL 列为 msgpack nil → JSON null
    let r1 = rows[1].as_array().unwrap();
    assert_eq!(r1[0], 1002);
    assert_eq!(r1[1], "second");
    assert!(r1[2].is_null(), "amount NULL 应为 null,实际 {:?}", r1[2]);
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

/// 空表:仍返回正确 columns（若走查询推断则空 schema，需 loader 覆盖——此处仅验证不 panic）。
#[tokio::test]
#[ignore]
async fn test_zmc_empty() -> cmx_database_pg::Result<()> {
    let mm = setup().await;
    let table = table_name();
    mm.execute_sql(
        TEST_DB_KEY,
        None,
        &format!("CREATE TABLE {table} (id BIGINT, name TEXT)"),
    )
    .await?;

    let zmc = mm
        .query_sql_zmc(TEST_DB_KEY, &format!("SELECT id,name FROM {table}"), "empty")
        .await?;
    assert_eq!(zmc.row_count(), 0);
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    let v = decode_msgpack(&buf);
    assert_eq!(v["datasetId"], "empty");
    assert_eq!(v["rows"].as_array().unwrap().len(), 0);

    mm.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    mm.shutdown().await?;
    Ok(())
}

fn base64_std(b: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    BASE64.encode(b)
}

/// 把真实 PG 查出的列式二进制包 dump 到 /tmp/zmc_real.bin,供前端 JS 解码器跨语言互通校验。
#[tokio::test]
#[ignore]
async fn dump_real_columnar_binary() -> cmx_database_pg::Result<()> {
    let mm = setup().await;
    let table = table_name();
    mm.execute_sql(
        TEST_DB_KEY,
        None,
        &format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY, name TEXT, amount NUMERIC(18,4), flag BOOLEAN)"),
    )
    .await?;
    mm.execute_sql_with_datavalues(
        TEST_DB_KEY,
        None,
        &format!("INSERT INTO {table} (id,name,amount,flag) VALUES ($1,$2,$3,$4)"),
        vec![
            DataValue::Int(1001),
            DataValue::String("héllo 世界".to_string()),
            DataValue::Decimal("1130000.5000".parse().unwrap()),
            DataValue::Bool(true),
        ],
    )
    .await?;
    let zmc = mm
        .query_sql_zmc(
            TEST_DB_KEY,
            &format!("SELECT id,name,amount,flag FROM {table} ORDER BY id"),
            "zmc_real",
        )
        .await?;
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    std::fs::write("/tmp/zmc_real.bin", &buf).unwrap();
    eprintln!("dumped {} bytes to /tmp/zmc_real.bin", buf.len());
    mm.execute_sql(TEST_DB_KEY, None, &format!("DROP TABLE {table} CASCADE"))
        .await?;
    mm.shutdown().await?;
    Ok(())
}

