//! `cmx_code_gap` 断号表读写（断号补偿，enable_gap=true 时启用）。
//!
//! 对应方案 §05。默认关闭——只有连号域（凭证号/发票号）才启用。

use cmx_code_model::error::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::get_default_pg_db_manager;
use cmx_utils::next_pk_id;

use super::rule_store::db_err;

/// 取最小断号（enable_gap=true 时优先填补断号）。
///
/// 用单条 `DELETE ... RETURNING` 原子取走最小断号（集群安全，避免两步查询+删除的并发竞争）。
/// 返回 None 表示无断号。必须在事务内调用（txn_id 非 None），使行锁与落库形成原子段。
pub async fn take_gap(
    prefix: &str,
    width: usize,
    db_id: &str,
    txn_id: Option<&str>,
) -> Result<Option<i64>> {
    let mm = get_default_pg_db_manager();

    // 原子取走最小断号：CTE 先 FOR UPDATE SKIP LOCKED 锁定最小断号行，外层 DELETE 取走。
    // 并发节点同时取时，SKIP LOCKED 跳过被锁行取下一个，不会重复取同一断号。
    let sql = r#"WITH candidate AS (
        SELECT id FROM cmx_code_gap
        WHERE prefix = $1
        ORDER BY serial_val ASC
        LIMIT 1
        FOR UPDATE SKIP LOCKED
    )
    DELETE FROM cmx_code_gap WHERE id IN (SELECT id FROM candidate)
    RETURNING serial_val"#;

    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            txn_id,
            sql,
            vec![DataValue::String(prefix.into())],
            "code_gap",
        )
        .await
        .map_err(|e| db_err("取走断号", e))?;

    let serial_val = ds
        .rows
        .first()
        .and_then(|r| r.get(0))
        .and_then(|dv| match dv {
            DataValue::Int(n) => Some(*n),
            _ => None,
        });

    if let Some(sv) = serial_val {
        tracing::debug!(
            target: "cmx_code::gap",
            prefix = %prefix, serial = sv, width,
            "取走断号填补"
        );
    }
    Ok(serial_val)
}

/// 记录断号（事务回滚时调，把未落库的号记为断号供后续填补）。
pub async fn record_gap(
    prefix: &str,
    serial_val: i64,
    width: usize,
    db_id: &str,
) -> Result<()> {
    let mm = get_default_pg_db_manager();
    let id = next_pk_id();
    let sql = r#"INSERT INTO cmx_code_gap (id, prefix, serial_val, width) VALUES ($1, $2, $3, $4)"#;
    mm.execute_sql_with_datavalues(
        db_id,
        None,
        sql,
        vec![
            DataValue::Int(id as i64),
            DataValue::String(prefix.into()),
            DataValue::Int(serial_val),
            DataValue::Int(width as i64),
        ],
    )
    .await
    .map_err(|e| db_err("记录断号", e))?;
    Ok(())
}

/// 查询断号列表（管理员查看用）。
pub async fn query_gaps(prefix: Option<&str>, db_id: &str) -> Result<Vec<serde_json::Value>> {
    let mm = get_default_pg_db_manager();
    let (sql, params) = match prefix {
        Some(p) => (
            r#"SELECT prefix, serial_val, width FROM cmx_code_gap WHERE prefix = $1 ORDER BY serial_val"#,
            vec![DataValue::String(p.into())],
        ),
        None => (
            r#"SELECT prefix, serial_val, width FROM cmx_code_gap ORDER BY prefix, serial_val"#,
            vec![],
        ),
    };
    let ds = mm
        .query_sql_with_datavalues(db_id, None, sql, params, "code_gaps")
        .await
        .map_err(|e| db_err("查询断号列表", e))?;

    let mut result = Vec::new();
    for row in &ds.rows {
        let prefix = match row.get(0) {
            Some(DataValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let serial_val = match row.get(1) {
            Some(DataValue::Int(n)) => *n,
            _ => 0,
        };
        let width = match row.get(2) {
            Some(DataValue::Int(n)) => *n as usize,
            _ => 0,
        };
        result.push(serde_json::json!({
            "prefix": prefix,
            "serialVal": serial_val,
            "width": width,
        }));
    }
    Ok(result)
}
