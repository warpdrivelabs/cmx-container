//! 治理表写入：md_audit（版本留痕）+ md_event_log（分发事件）+ CR 状态归档。

use cmx_core::dv;
use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_database_pg::DatabaseManager;
use cmx_utils::{next_pk_id, snowflake_id_str};
use serde_json::Value;

use crate::error::api_err_db;

/// 写 md_audit 一条（create/update 留痕）。返回审计 id。
///
/// 参数多但语义清晰（审计字段：字典/记录/版本/动作/来源CR/字段/新旧值/操作人），不拆。
#[allow(clippy::too_many_arguments)]
pub async fn write_audit(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    dict_code: &str,
    record_id: i64,
    version: i64,
    action: &str,
    source_cr_id: Option<i64>,
    field: Option<&str>,
    old_value: Option<Value>,
    new_value: Option<Value>,
    operated_by: i64,
) -> Result<i64, cmx_api_types::Error> {
    let id = next_pk_id();
    let sql = r#"INSERT INTO md_audit (id, dict_code, record_id, version, action, source_cr_id,
                                       field, old_value, new_value, operated_by, operated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())"#;
    // 可选列走 NullTyped（B5 口径）：Option<i64>→NullTyped(Int)；JSONB 的 None→NullTyped(Json)
    let params = dv![
        DataValue::Int(id),
        DataValue::String(dict_code.into()),
        DataValue::Int(record_id),
        DataValue::Int(version),
        DataValue::String(action.into()),
        // Option<i64> → NullTyped(Int)，BIGINT 列安全（显式 DataValue::from 消除推断歧义）
        DataValue::from(source_cr_id),
        // Option<String> → Null，VARCHAR 列安全
        DataValue::from(field.map(|s| s.to_string())),
        old_value
            .map(|v| DataValue::Json(v.to_string()))
            .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
        new_value
            .map(|v| DataValue::Json(v.to_string()))
            .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
        DataValue::Int(operated_by),
    ];
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), sql, params)
        .await
        .map_err(|e| api_err_db(&format!("写 md_audit 失败: {e}")))?;
    Ok(id)
}

/// 写 md_event_log 一条（分发事件）。seq 由 DB 自增，不填。返回事件 id。
pub async fn write_event(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    dict_code: &str,
    record_id: i64,
    event_type: &str,
    payload: Value,
) -> Result<String, cmx_api_types::Error> {
    let id = snowflake_id_str();
    let sql = r#"INSERT INTO md_event_log (id, dict_code, record_id, event_type, payload, emitted_at)
                 VALUES ($1,$2,$3,$4,$5,now())"#;
    let params = dv![
        DataValue::String(id.clone()),
        DataValue::String(dict_code.into()),
        DataValue::Int(record_id),
        DataValue::String(event_type.into()),
        DataValue::Json(payload.to_string()),
    ];
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), sql, params)
        .await
        .map_err(|e| api_err_db(&format!("写 md_event_log 失败: {e}")))?;
    Ok(id)
}

/// 改 CR 状态（归档：activated / approved / aborted 等）。返回受影响行数。
///
/// `txn_id`:激活器传 Some(走事务);CR 审批服务传 None(自动提交)。
pub async fn set_cr_status(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    cr_id: i64,
    status: &str,
) -> Result<u64, cmx_api_types::Error> {
    let sql = "UPDATE cv_mdm_apply SET doc_status = $1 WHERE id = $2";
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            txn_id,
            sql,
            dv![DataValue::String(status.into()), DataValue::Int(cr_id)],
        )
        .await
        .map_err(|e| api_err_db(&format!("改 CR {cr_id} 状态失败: {e}")))?;
    Ok(n)
}
