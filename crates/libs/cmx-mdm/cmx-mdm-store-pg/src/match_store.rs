//! 匹配组 / 交叉引用 store（M3）：md_match_group + md_xref 读写 + cm_* published 装载。
//!
//! 绑定口径（审查重要-2）：可空 BIGINT 用 `DataValue::from(Option<i64>)`（NullTyped(Int)），
//! 可空 JSONB 用 `NullTyped(SqlTypeMarker::Json)`——裸 `DataValue::Null` 绑成 VARCHAR NULL 会被
//! BIGINT/JSONB 列拒收（executor/mod.rs:280）。
//! 时间戳：md_match_group 仅 created_at（DEFAULT now()）、md_xref 无时间戳列——update SQL **不 SET 时间戳**（审查建议-1）。

use cmx_core::dv;
use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_database_pg::DatabaseManager;
use cmx_mdm_model::match_algo::MatchRecord;
use cmx_utils::next_pk_id;
use serde_json::Value;

use crate::dct_accessor::validate_ident;
use crate::error::api_err_db;

/// 读字典全量 published 行（cm_*）。columns = id + 比较/存活字段 + update_time。
///
/// 表名/列名经 [`validate_ident`] 白名单校验防注入。
pub async fn load_published(
    mm: &DatabaseManager,
    db_id: &str,
    table: &str,
    columns: &[&str],
) -> Result<Vec<MatchRecord>, cmx_api_types::Error> {
    validate_ident(table)?;
    for c in columns {
        validate_ident(c)?;
    }
    // cm_* 治理列无 delete_flag（DCT 字典表），仅按 lifecycle_status 过滤
    let sql = format!(
        "SELECT {} FROM {table} WHERE lifecycle_status = 'published'",
        columns.join(", ")
    );
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, vec![], "mdm_load_published")
        .await
        .map_err(|e| api_err_db(&format!("装载 {table} published 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    let mut out = Vec::with_capacity(ds.rows.len());
    for row in ds.rows.iter() {
        let v = row.to_json_value(schema);
        let obj = v.as_object().cloned().unwrap_or_default();
        let id = obj.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        out.push(MatchRecord { id, fields: obj });
    }
    Ok(out)
}

/// 按 id 批量读 cm_* 行（merge 事务内读 master/victims）。columns 同 [`load_published`]。
pub async fn load_by_ids(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    table: &str,
    columns: &[&str],
    ids: &[i64],
) -> Result<Vec<MatchRecord>, cmx_api_types::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    validate_ident(table)?;
    for c in columns {
        validate_ident(c)?;
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${i}")).collect();
    let sql = format!(
        "SELECT {} FROM {table} WHERE id IN ({})",
        columns.join(", "),
        placeholders.join(", ")
    );
    let params: Vec<DataValue> = ids.iter().map(|i| DataValue::Int(*i)).collect();
    let ds = mm
        .query_sql_with_datavalues(db_id, txn_id, &sql, params, "mdm_load_by_ids")
        .await
        .map_err(|e| api_err_db(&format!("按 id 读 {table} 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    let mut out = Vec::with_capacity(ds.rows.len());
    for row in ds.rows.iter() {
        let v = row.to_json_value(schema);
        let obj = v.as_object().cloned().unwrap_or_default();
        let id = obj.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        out.push(MatchRecord { id, fields: obj });
    }
    Ok(out)
}

/// 写 md_match_group 一条。返回 id。
#[allow(clippy::too_many_arguments)]
pub async fn insert_match_group(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    dict_code: &str,
    group_key: &str,
    member_ids: &Value,
    master_id: Option<i64>,
    score: i64,
    decision: &str,
    status: &str,
) -> Result<i64, cmx_api_types::Error> {
    let id = next_pk_id();
    // score 列 SMALLINT：DataValue::Int 走 PgInt 宽度自适应 INT2/4/8，可直绑
    let sql = r#"INSERT INTO md_match_group
        (id, dict_code, group_key, member_ids, master_id, score, decision, survivorship_log, status, created_at)
      VALUES ($1,$2,$3,$4,$5,$6,$7,NULL,$8,now())"#;
    mm.execute_sql_with_datavalues(
        db_id,
        txn_id,
        sql,
        dv![
            DataValue::Int(id),
            DataValue::String(dict_code.into()),
            DataValue::String(group_key.into()),
            DataValue::Json(member_ids.to_string()),
            DataValue::from(master_id),
            DataValue::Int(score),
            DataValue::String(decision.into()),
            DataValue::String(status.into()),
        ],
    )
        .await
        .map_err(|e| api_err_db(&format!("写 md_match_group 失败: {e}")))?;
    Ok(id)
}

/// 更新 md_match_group（status / survivorship_log / master_id）。不 SET 时间戳。
pub async fn update_match_group(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    id: i64,
    status: &str,
    survivorship_log: Option<&Value>,
    master_id: Option<i64>,
) -> Result<u64, cmx_api_types::Error> {
    let sql = "UPDATE md_match_group SET status = $1, survivorship_log = $2, master_id = $3 WHERE id = $4";
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            txn_id,
            sql,
            dv![
                DataValue::String(status.into()),
                survivorship_log
                    .map(|v| DataValue::Json(v.to_string()))
                    .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
                DataValue::from(master_id),
                DataValue::Int(id),
            ],
        )
        .await
        .map_err(|e| api_err_db(&format!("更新 md_match_group 失败: {e}")))?;
    Ok(n)
}

/// 状态 CAS 转换（M4 审查 C3/C6）：仅当当前 status=from 才改 to。返回行数（0=冲突）。
/// 不 SET 时间戳。用于 reject(pending→rejected) / merge 占位(pending→reviewed)。
pub async fn transition_match_group(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    id: i64,
    from: &str,
    to: &str,
) -> Result<u64, cmx_api_types::Error> {
    let sql = "UPDATE md_match_group SET status = $1 WHERE id = $2 AND status = $3";
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            txn_id,
            sql,
            dv![
                DataValue::String(to.into()),
                DataValue::Int(id),
                DataValue::String(from.into()),
            ],
        )
        .await
        .map_err(|e| api_err_db(&format!("转换 md_match_group 状态失败: {e}")))?;
    Ok(n)
}

/// 匹配组列表（dictCode + status 双过滤，吃 (dict_code,status) 索引）。
pub async fn list_match_groups(
    mm: &DatabaseManager,
    db_id: &str,
    dict_code: Option<&str>,
    status: Option<&str>,
    page: i64,
    page_size: i64,
) -> Result<(Vec<Value>, i64), cmx_api_types::Error> {
    let mut clauses = vec!["1=1".to_string()];
    let mut params: Vec<DataValue> = Vec::new();
    if let Some(d) = dict_code {
        clauses.push(format!("dict_code = ${}", params.len() + 1));
        params.push(DataValue::String(d.into()));
    }
    if let Some(s) = status {
        clauses.push(format!("status = ${}", params.len() + 1));
        params.push(DataValue::String(s.into()));
    }
    let where_sql = clauses.join(" AND ");
    // 总数
    let cnt_sql = format!("SELECT COUNT(*) AS c FROM md_match_group WHERE {where_sql}");
    let cds = mm
        .query_sql_with_datavalues(db_id, None, &cnt_sql, params.clone(), "mdm_match_count")
        .await
        .map_err(|e| api_err_db(&format!("查 md_match_group 总数失败: {e}")))?;
    let total = cds.rows.first()
        .and_then(|r| r.get_by_name_as::<i64>(cds.schema.as_ref(), "c"))
        .unwrap_or(0);
    let ps = if page_size > 0 { page_size } else { 20 };
    let pg = if page > 0 { page } else { 1 };
    let off = (pg - 1) * ps;
    let n = params.len() as i64;
    params.push(DataValue::Int(ps));
    params.push(DataValue::Int(off));
    let sql = format!(
        "SELECT id, dict_code, group_key, member_ids, master_id, score, decision, status, created_at \
         FROM md_match_group WHERE {where_sql} ORDER BY created_at DESC, id DESC \
         LIMIT ${} OFFSET ${}", n + 1, n + 2);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_match_list")
        .await
        .map_err(|e| api_err_db(&format!("列表 md_match_group 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((ds.rows.iter().map(|r| r.to_json_value(schema)).collect(), total))
}

/// 变更历史/版本留痕（md_audit，分页）。可选 dictCode/recordId 过滤。
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
    let total = cds.rows.first()
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
         LIMIT ${} OFFSET ${}", n + 1, n + 2);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_audit_list")
        .await
        .map_err(|e| api_err_db(&format!("列表 md_audit 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((ds.rows.iter().map(|r| r.to_json_value(schema)).collect(), total))
}

/// 事件查询（md_event_log，delta 拉取，分页）。可选 dictCode/since(seq)。
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
    let total = cds.rows.first()
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
         LIMIT ${} OFFSET ${}", n + 1, n + 2);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_event_list")
        .await
        .map_err(|e| api_err_db(&format!("列表 md_event_log 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((ds.rows.iter().map(|r| r.to_json_value(schema)).collect(), total))
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
    let total = cds.rows.first()
        .and_then(|r| r.get_by_name_as::<i64>(cds.schema.as_ref(), "c"))
        .unwrap_or(0);
    let ps = if page_size > 0 { page_size } else { 20 };
    let pg = if page > 0 { page } else { 1 };
    let off = (pg - 1) * ps;
    let sql = format!(
        "SELECT id, target_sys, dict_code, filter, field_map, channel, active, created_at \
         FROM md_subscription ORDER BY created_at DESC, id DESC LIMIT ${} OFFSET ${}", 1, 2);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, dv![DataValue::Int(ps), DataValue::Int(off)], "mdm_sub_list")
        .await
        .map_err(|e| api_err_db(&format!("列表 md_subscription 失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((ds.rows.iter().map(|r| r.to_json_value(schema)).collect(), total))
}

/// 订阅配置 upsert（按 id；id 缺省生成）。
pub async fn upsert_subscription(
    mm: &DatabaseManager,
    db_id: &str,
    body: &Value,
) -> Result<i64, cmx_api_types::Error> {
    let id = body.get("id").and_then(|v| v.as_i64()).unwrap_or_else(cmx_utils::next_pk_id);
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
        db_id, None, sql,
        dv![
            DataValue::Int(id),
            DataValue::String(target.into()),
            DataValue::String(dict.into()),
            filter.map(|v| DataValue::Json(v)).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
            field_map.map(|v| DataValue::Json(v)).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Json)),
            DataValue::String(channel.into()),
            DataValue::Bool(active),
        ],
    )
        .await
        .map_err(|e| api_err_db(&format!("写 md_subscription 失败: {e}")))?;
    Ok(id)
}

/// 按 id 查 md_match_group。
pub async fn get_match_group(
    mm: &DatabaseManager,
    db_id: &str,
    id: i64,
) -> Result<Option<Value>, cmx_api_types::Error> {
    let sql = "SELECT id, dict_code, group_key, member_ids, master_id, score, decision, status, survivorship_log \
               FROM md_match_group WHERE id = $1";
    let ds = mm
        .query_sql_with_datavalues(db_id, None, sql, dv![DataValue::Int(id)], "mdm_match_get")
        .await
        .map_err(|e| api_err_db(&format!("查 md_match_group 失败: {e}")))?;
    let Some(row) = ds.rows.first() else {
        return Ok(None);
    };
    Ok(Some(row.to_json_value(ds.schema.as_ref())))
}

/// md_xref 置 inactive（merge 后 victim 引用失效）。不 SET 时间戳。
pub async fn deactivate_xref(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    dict_code: &str,
    record_id: i64,
) -> Result<u64, cmx_api_types::Error> {
    set_xref_status(mm, db_id, txn_id, dict_code, record_id, "inactive").await
}

/// md_xref 恢复 active（unmerge）。
pub async fn activate_xref(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    dict_code: &str,
    record_id: i64,
) -> Result<u64, cmx_api_types::Error> {
    set_xref_status(mm, db_id, txn_id, dict_code, record_id, "active").await
}

async fn set_xref_status(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    dict_code: &str,
    record_id: i64,
    status: &str,
) -> Result<u64, cmx_api_types::Error> {
    let sql = "UPDATE md_xref SET xref_status = $1 WHERE dict_code = $2 AND record_id = $3";
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            txn_id,
            sql,
            dv![
                DataValue::String(status.into()),
                DataValue::String(dict_code.into()),
                DataValue::Int(record_id),
            ],
        )
        .await
        .map_err(|e| api_err_db(&format!("改 md_xref 状态失败: {e}")))?;
    Ok(n)
}

