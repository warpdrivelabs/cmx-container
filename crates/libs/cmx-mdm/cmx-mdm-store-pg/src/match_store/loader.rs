//! cm_* 主数据 published 行装载（查重 / 合并事务内读主数据）。
//!
//! 表名 / 列名经 [`validate_ident`] 白名单校验防注入。

use cmx_core::model::cell::DataValue;
use cmx_database_pg::DatabaseManager;
use cmx_mdm_model::match_algo::MatchRecord;

use crate::dct_accessor::validate_ident;
use crate::error::api_err_db;

/// 读字典全量 published 行（cm_*）。
///
/// `columns` = id + 比较 / 存活字段 + update_time。表名 / 列名经 [`validate_ident`] 白名单校验防注入。
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
