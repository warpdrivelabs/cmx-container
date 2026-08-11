//! cm_* 主数据写入闸口（激活器唯一入口，强制 lifecycle_status='published'）。
//!
//! 自己拼 SQL + DatabaseManager 事务执行（不复用 cmx-dct-store-pg：要纳入激活器单事务 + 强制 published）。
//! INSERT/UPDATE SQL 由 [`crate::sql_builder`] 的 `build_insert_sql` / `build_update_sql` 构造；
//! 列值经 `to_dv` 转 DataValue。

use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_database_pg::DatabaseManager;
use cmx_utils::next_pk_id;
use serde_json::{Map, Value};

use crate::error::{api_err, api_err_db};
use crate::sql_builder::{build_insert_sql, build_update_sql};

/// 新建主数据行（INSERT，头表/明细表共用）。返回新 id。
///
/// row 已含 lifecycle_status='published'（由 plan_create/plan_lines 强制）。
/// 补 id（next_pk_id）+ backfill 公共 NOT NULL 列（sort_no/status/create_time/...，
/// 对齐 cmx-dct-store-pg 的 backfill 语义，避免 NOT NULL 列缺失导致插入失败）。
pub async fn insert_header(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    table: &str,
    row: &Map<String, Value>,
    operated_by: i64,
) -> Result<i64, cmx_api_types::Error> {
    validate_ident(table)?;
    let id = next_pk_id();
    // backfill:对缺失的公共 NOT NULL 列补默认值(不覆盖 row 已有值)
    // 注:create_time/update_time 由 build_insert_sql 用 SQL now() 填充,这里只占位
    let mut full = row.clone();
    // code/name 是 dictionaryCommonFields 的 NOT NULL 列;头表已由 plan_create 填 code,
    // 明细表若缺则补基于 id 的占位码(避免 NOT NULL 约束失败)
    let id_str = id.to_string();
    backfill(&mut full, "code", serde_json::json!(format!("MDM-{id_str}")));
    backfill(&mut full, "name", serde_json::json!(format!("MDM-{id_str}")));
    backfill(&mut full, "published_version", serde_json::json!(1));
    backfill(&mut full, "sort_no", serde_json::json!(0));
    backfill(&mut full, "status", serde_json::json!(1));
    backfill(&mut full, "create_by", serde_json::json!(operated_by));
    backfill(&mut full, "update_by", serde_json::json!(operated_by));
    backfill(&mut full, "create_time", serde_json::Value::Null);
    backfill(&mut full, "update_time", serde_json::Value::Null);
    let (sql, params) = build_insert_sql(table, &full, id);
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
        .await
        .map_err(|e| {
            tracing::error!(target: "cmx_mdm::db", table=table, sql=%sql, error=%e, "INSERT 失败");
            api_err_db(&format!("INSERT {table} 失败"))
        })?;
    Ok(id)
}

/// 若 row 无该列则补默认值（不覆盖已有）。
fn backfill(row: &mut Map<String, Value>, col: &str, default: Value) {
    if !row.contains_key(col) {
        row.insert(col.to_string(), default);
    }
}

/// 变更主数据头（UPDATE by id + 乐观锁 CAS）。返回受影响行数（0=版本冲突）。
pub async fn update_header(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    table: &str,
    record_id: i64,
    row: &Map<String, Value>,
    expected_version: i64,
) -> Result<u64, cmx_api_types::Error> {
    validate_ident(table)?;
    let (sql, params) = build_update_sql(table, record_id, row, expected_version);
    let n = mm
        .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
        .await
        .map_err(|e| api_err_db(&format!("UPDATE {table} 失败: {e}")))?;
    Ok(n)
}

/// 查当前 published_version（乐观锁快照用）。无记录返回 None。
pub async fn get_version(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    table: &str,
    record_id: i64,
) -> Result<Option<i64>, cmx_api_types::Error> {
    validate_ident(table)?;
    let sql = format!("SELECT published_version FROM {table} WHERE id = $1");
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            txn_id,
            &sql,
            vec![DataValue::Int(record_id)],
            "mdm_get_version",
        )
        .await
        .map_err(|e| api_err_db(&format!("查 {table} 版本失败: {e}")))?;
    let Some(row) = ds.rows.first() else {
        return Ok(None);
    };
    Ok(row.get_by_name_as::<i64>(ds.schema.as_ref(), "published_version"))
}

/// 改 lifecycle_status（merge→merged / unmerge→published / freeze 等，M3）。
///
/// **双保险**（审查重要-5）：① CAS `expected→next` 防双 merge / 双 unmerge；
/// ② SET 同时 `published_version+1`，使并发持旧版本的 update-CR 写入者 CAS 失配回滚。
/// 返回受影响行数（0 = 状态冲突）。
pub async fn set_lifecycle(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    table: &str,
    record_id: i64,
    expected: &str,
    next: &str,
) -> Result<u64, cmx_api_types::Error> {
    validate_ident(table)?;
    let sql = format!(
        "UPDATE {table} SET lifecycle_status = $1, published_version = published_version + 1, \
         update_time = now() WHERE id = $2 AND lifecycle_status = $3"
    );
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            &sql,
            vec![
                DataValue::String(next.into()),
                DataValue::Int(record_id),
                DataValue::String(expected.into()),
            ],
        )
        .await
        .map_err(|e| api_err_db(&format!("改 {table} 生命周期失败: {e}")))?;
    Ok(n)
}

/// 明细 re-parent（M3 merge）：detail 表里 parent_field=from_id 的行改指 to_id。返回行数。
pub async fn reparent_lines(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    detail_table: &str,
    parent_field: &str,
    from_id: i64,
    to_id: i64,
) -> Result<u64, cmx_api_types::Error> {
    validate_ident(detail_table)?;
    validate_ident(parent_field)?;
    let sql = format!(
        "UPDATE {detail_table} SET {parent_field} = $1, update_time = now() \
         WHERE {parent_field} = $2"
    );
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            &sql,
            vec![DataValue::Int(to_id), DataValue::Int(from_id)],
        )
        .await
        .map_err(|e| api_err_db(&format!("re-parent {detail_table} 失败: {e}")))?;
    Ok(n)
}

/// 查明细行 id 清单（M3）：detail 表里 parent_field=parent_id 的行 id（merge 前快照，供 unmerge 逆操作）。
pub async fn select_line_ids(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    detail_table: &str,
    parent_field: &str,
    parent_id: i64,
) -> Result<Vec<i64>, cmx_api_types::Error> {
    validate_ident(detail_table)?;
    validate_ident(parent_field)?;
    let sql = format!("SELECT id FROM {detail_table} WHERE {parent_field} = $1 ORDER BY id");
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            Some(txn_id),
            &sql,
            vec![DataValue::Int(parent_id)],
            "mdm_line_ids",
        )
        .await
        .map_err(|e| api_err_db(&format!("查 {detail_table} 行 id 失败: {e}")))?;
    Ok(ds
        .rows
        .iter()
        .filter_map(|r| r.get_by_name_as::<i64>(ds.schema.as_ref(), "id"))
        .collect())
}

/// 按 id 精确 re-parent（M3 unmerge 逆操作）：把 ids 行改指 to_id。返回行数。
pub async fn reparent_lines_by_ids(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    detail_table: &str,
    parent_field: &str,
    ids: &[i64],
    to_id: i64,
) -> Result<u64, cmx_api_types::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    validate_ident(detail_table)?;
    validate_ident(parent_field)?;
    // IN 列表用 $2..$N+1 参数化（防注入）
    let placeholders: Vec<String> = (2..=ids.len() + 1).map(|i| format!("${i}")).collect();
    let sql = format!(
        "UPDATE {detail_table} SET {parent_field} = $1, update_time = now() \
         WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut params = vec![DataValue::Int(to_id)];
    params.extend(ids.iter().map(|i| DataValue::Int(*i)));
    let n = mm
        .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
        .await
        .map_err(|e| api_err_db(&format!("按 id re-parent {detail_table} 失败: {e}")))?;
    Ok(n)
}

/// 锁行（M3 merge，审查重要-4）：事务内 `SELECT ... FOR UPDATE` 占行锁，
/// 串行化交叉 merge（X⇄Y 互并）。返回行是否存在。
pub async fn lock_record(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    table: &str,
    record_id: i64,
) -> Result<bool, cmx_api_types::Error> {
    validate_ident(table)?;
    let sql = format!("SELECT id FROM {table} WHERE id = $1 FOR UPDATE");
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            Some(txn_id),
            &sql,
            vec![DataValue::Int(record_id)],
            "mdm_lock_record",
        )
        .await
        .map_err(|e| api_err_db(&format!("锁 {table} 行失败: {e}")))?;
    Ok(!ds.rows.is_empty())
}

/// 标识符（表名/列名）白名单校验：仅允许 [a-zA-Z0-9_]，防 SQL 注入。
/// pub(crate)：match_store 复用（审查建议-6，不复制第二份）。
pub(crate) fn validate_ident(name: &str) -> Result<(), cmx_api_types::Error> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(api_err(&format!("非法标识符: {name}")));
    }
    Ok(())
}

/// 把 Option<DataValue> 的 None 转成带类型的 Null（供外部用）。
#[allow(dead_code)]
fn typed_null(marker: SqlTypeMarker) -> DataValue {
    DataValue::NullTyped(marker)
}
