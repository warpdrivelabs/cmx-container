//! 治理表分页查询（md_audit / md_event_log / md_subscription）。
//!
//! 与 [`crate::md_accessor`] 的写入函数对偶：`write_audit` → [`list_audit`]，
//! `write_event` → [`list_events`]，`upsert_subscription` → [`list_subscriptions`]。

use cmx_core::dv;
use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_database_pg::DatabaseManager;
use serde_json::Value;

use crate::error::api_err_db;

/// 变更历史 / 版本留痕（md_audit，分页）。可选 dictCode / recordId 过滤。
pub async fn list_audit(
    mm: &DatabaseManager,
    db_id: &str,
    dict_code: Option<&str>,
    record_id: Option<i64>,
    page: i64,
    page_size: i64,
) -> Result<(Vec<Value>, i64), cmx_api_types::Error> {
    let mut clauses = vec!["1=1".to_string()];
    let mut params: Vec<DataValue> = Vec::new();
    if let Some(d) = dict_code {
        clauses.push(format!("dict_code = ${}", params.len() + 1));
        params.push(DataValue::String(d.into()));
    }
    if let Some(r) = record_id {
        clauses.push(format!("record_id = ${}", params.len() + 1));
        params.push(DataValue::Int(r));
    }
    let where_sql = clauses.join(" AND ");
    let cnt_sql = format!("SELECT COUNT(*) AS c FROM md_audit WHERE {where_sql}");
    let cds = mm
        .query_sql_with_datavalues(db_id, None, &cnt_sql, params.clone(), "mdm_audit_count")
        .await
        .map_err(|e| api_err_db(&format!("查 md_audit 总数失败: {e}")))?;
    let total = cds
        .rows
        .first()
        .and_then(|r| r.get_by_name_as::<i64>(cds.schema.as_ref(), "c"))
        .unwrap_or(0);
    let ps = if page_size > 0 { page_size } else { 20 };
    let pg = if page > 0 { page } else { 1 };
    let off = (pg - 1) * ps;
    let n = params.len() as i64;
    params.push(DataValue::Int(ps));
    params.push(DataValue::Int(off));
    let sql = format!(
        "SELECT id, dict_code, record_id, version, action, source_cr_id, field, old_value, new_value, operated_by, operated_at \
         FROM md_audit WHERE {where_sql} ORDER BY operated_at DESC, id DESC \
         LIMIT ${} OFFSET ${}",
        n + 1,
        n + 2
    );
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_audit_list")
        .await
        .map_err(|e| api_err_db(&format!("列表 md_audit 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((
        ds.rows.iter().map(|r| r.to_json_value(schema)).collect(),
        total,
    ))
}

/// 事件查询（md_event_log，delta 拉取，分页）。可选 dictCode / since(seq)。
pub async fn list_events(
    mm: &DatabaseManager,
    db_id: &str,
    dict_code: Option<&str>,
    since: Option<i64>,
    page: i64,
    page_size: i64,
) -> Result<(Vec<Value>, i64), cmx_api_types::Error> {
    let mut clauses = vec!["1=1".to_string()];
    let mut params: Vec<DataValue> = Vec::new();
    if let Some(d) = dict_code {
        clauses.push(format!("dict_code = ${}", params.len() + 1));
        params.push(DataValue::String(d.into()));
    }
    if let Some(s) = since {
        clauses.push(format!("seq > ${}", params.len() + 1));
        params.push(DataValue::Int(s));
    }
    let where_sql = clauses.join(" AND ");
    let cnt_sql = format!("SELECT COUNT(*) AS c FROM md_event_log WHERE {where_sql}");
    let cds = mm
        .query_sql_with_datavalues(db_id, None, &cnt_sql, params.clone(), "mdm_event_count")
        .await
        .map_err(|e| api_err_db(&format!("查 md_event_log 总数失败: {e}")))?;
    let total = cds
        .rows
        .first()
        .and_then(|r| r.get_by_name_as::<i64>(cds.schema.as_ref(), "c"))
        .unwrap_or(0);
    let ps = if page_size > 0 { page_size } else { 20 };
    let pg = if page > 0 { page } else { 1 };
    let off = (pg - 1) * ps;
    let n = params.len() as i64;
    params.push(DataValue::Int(ps));
    params.push(DataValue::Int(off));
    let sql = format!(
        "SELECT id, seq, dict_code, record_id, event_type, payload, emitted_at \
         FROM md_event_log WHERE {where_sql} ORDER BY seq ASC \
         LIMIT ${} OFFSET ${}",
        n + 1,
        n + 2
    );
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_event_list")
        .await
        .map_err(|e| api_err_db(&format!("列表 md_event_log 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((
        ds.rows.iter().map(|r| r.to_json_value(schema)).collect(),
        total,
    ))
}

/// 订阅配置列表（md_subscription，分页）。
pub async fn list_subscriptions(
    mm: &DatabaseManager,
    db_id: &str,
    page: i64,
    page_size: i64,
) -> Result<(Vec<Value>, i64), cmx_api_types::Error> {
    let cnt_sql = "SELECT COUNT(*) AS c FROM md_subscription";
    let cds = mm
        .query_sql_with_datavalues(db_id, None, cnt_sql, vec![], "mdm_sub_count")
        .await
        .map_err(|e| api_err_db(&format!("查 md_subscription 总数失败: {e}")))?;
    let total = cds
        .rows
        .first()
        .and_then(|r| r.get_by_name_as::<i64>(cds.schema.as_ref(), "c"))
        .unwrap_or(0);
    let ps = if page_size > 0 { page_size } else { 20 };
    let pg = if page > 0 { page } else { 1 };
    let off = (pg - 1) * ps;
    let sql = format!(
        "SELECT id, target_sys, dict_code, filter, field_map, channel, active, created_at \
         FROM md_subscription ORDER BY created_at DESC, id DESC LIMIT ${} OFFSET ${}",
        1, 2
    );
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            &sql,
            dv![DataValue::Int(ps), DataValue::Int(off)],
            "mdm_sub_list",
        )
        .await
        .map_err(|e| api_err_db(&format!("列表 md_subscription 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((
        ds.rows.iter().map(|r| r.to_json_value(schema)).collect(),
        total,
    ))
}

/// 订阅配置 upsert（按 id；id 缺省生成）。
pub async fn upsert_subscription(
    mm: &DatabaseManager,
    db_id: &str,
    body: &Value,
) -> Result<i64, cmx_api_types::Error> {
    let id = body
        .get("id")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(cmx_utils::next_pk_id);
    let target = body.get("target_sys").and_then(|v| v.as_str()).unwrap_or("");
    let dict = body.get("dict_code").and_then(|v| v.as_str()).unwrap_or("");
    let channel = body.get("channel").and_then(|v| v.as_str()).unwrap_or("rest");
    let active = body.get("active").and_then(|v| v.as_bool()).unwrap_or(true);
    let filter = body.get("filter").map(|v| v.to_string());
    let field_map = body.get("field_map").map(|v| v.to_string());
    let sql = r#"INSERT INTO md_subscription (id, target_sys, dict_code, filter, field_map, channel, active, created_at)
      VALUES ($1,$2,$3,$4,$5,$6,$7,now())
      ON CONFLICT (id) DO UPDATE SET target_sys=EXCLUDED.target_sys, dict_code=EXCLUDED.dict_code,
        filter=EXCLUDED.filter, field_map=EXCLUDED.field_map, channel=EXCLUDED.channel, active=EXCLUDED.active"#;
    mm.execute_sql_with_datavalues(
        db_id,
        None,
        sql,
        dv![
            DataValue::Int(id),
            DataValue::String(target.into()),
            DataValue::String(dict.into()),
            filter
                .map(DataValue::Json)
                .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
            field_map
                .map(DataValue::Json)
                .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
            DataValue::String(channel.into()),
            DataValue::Bool(active),
        ],
    )
    .await
    .map_err(|e| api_err_db(&format!("写 md_subscription 失败: {e}")))?;
    Ok(id)
}
