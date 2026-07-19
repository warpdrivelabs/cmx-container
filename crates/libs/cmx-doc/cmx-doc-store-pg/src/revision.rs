//! DocRevision — 业务单据版本化（方案 §6A，落地 Phase 8）
//!
//! 在保存事务内记录整单快照（append-only），并提供查询/回滚。
//!   - `record`  保存事务内产生新版本（rev_no+1，旧版 is_current=0，快照=列式包 JSONB）
//!   - `list`    列某单全部版本（时间线）
//!   - `get`     取某历史版完整快照（列式包）
//!   - `restore` 把某历史版恢复为新当前版（op=restore，历史不丢）
//!
//! 快照复用 ColumnarCodec（与装载同一序列化器），存进 cmx_doc_revision.snapshot(JSONB)。

use serde_json::{json, Value};

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{ColumnarCodec, DataSet};
use cmx_database::DatabaseManager;
use cmx_utils::snowflake_id;

use cmx_biz::{BizError, Result};

const REV_TABLE: &str = "cmx_doc_revision";

/// 版本记录写入器。
pub struct DocRevision;

impl DocRevision {
    /// 在保存事务内记录新版本。root_ds 为「保存后重新装配的整单」。
    ///
    /// 步骤：读当前 rev_no → 旧当前版 is_current=0 → INSERT 新版(is_current=1)。
    pub async fn record(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        doc_file: &str,
        root_table: &str,
        root_id: &str,
        op: &str,
        root_ds: &DataSet,
        actor_id: Option<&str>,
        actor_name: Option<&str>,
        reason: Option<&str>,
        biz_status: Option<&str>,
    ) -> Result<i64> {
        // 1. 取当前最大 rev_no
        let next_rev = Self::next_rev_no(mm, db_id, Some(txn_id), doc_file, root_id).await?;

        // 2. 旧当前版翻 0
        let flip_sql = format!(
            "UPDATE {REV_TABLE} SET is_current = 0 WHERE doc_file = $1 AND root_id = $2 AND is_current = 1"
        );
        mm.execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            &flip_sql,
            vec![
                DataValue::String(doc_file.to_string()),
                DataValue::String(root_id.to_string()),
            ],
        )
        .await
        .map_err(|e| BizError::internal(format!("翻旧版本失败: {e}")))?;

        // 3. 列式包快照
        let snapshot = ColumnarCodec::encode(root_ds);
        let rev_id = snowflake_id();

        // 4. 插新版
        let ins_sql = format!(
            "INSERT INTO {REV_TABLE} \
             (id, doc_file, root_table, root_id, rev_no, is_current, op, snapshot, reason, actor_id, actor_name, biz_status) \
             VALUES ($1,$2,$3,$4,$5,1,$6,$7::jsonb,$8,$9,$10,$11)"
        );
        mm.execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            &ins_sql,
            vec![
                DataValue::Int(rev_id),
                DataValue::String(doc_file.to_string()),
                DataValue::String(root_table.to_string()),
                DataValue::String(root_id.to_string()),
                DataValue::Int(next_rev as i64),
                DataValue::String(op.to_string()),
                DataValue::String(snapshot.to_string()),
                opt_str(reason),
                opt_str(actor_id),
                opt_str(actor_name),
                opt_str(biz_status),
            ],
        )
        .await
        .map_err(|e| BizError::internal(format!("写版本快照失败: {e}")))?;

        Ok(rev_id)
    }

    /// 下一个版本号（当前 max+1，无则 1）。
    async fn next_rev_no(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        doc_file: &str,
        root_id: &str,
    ) -> Result<i32> {
        let sql = format!(
            "SELECT COALESCE(MAX(rev_no), 0) AS m FROM {REV_TABLE} WHERE doc_file = $1 AND root_id = $2"
        );
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                txn_id,
                &sql,
                vec![
                    DataValue::String(doc_file.to_string()),
                    DataValue::String(root_id.to_string()),
                ],
                "doc_rev_max",
            )
            .await
            .map_err(|e| BizError::internal(format!("查版本号失败: {e}")))?;
        let cur = ds
            .rows
            .first()
            .and_then(|r| ds.schema.get_index("m").and_then(|i| r.get(i)))
            .and_then(|dv| match dv {
                DataValue::Int(n) => Some(*n as i32),
                _ => None,
            })
            .unwrap_or(0);
        Ok(cur + 1)
    }

    /// 列某单全部版本（倒序时间线）。返回 rows 数组。
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        doc_file: &str,
        root_id: &str,
    ) -> Result<Value> {
        let sql = format!(
            "SELECT id, rev_no, is_current, op, reason, actor_id, actor_name, biz_status, created_at \
             FROM {REV_TABLE} WHERE doc_file = $1 AND root_id = $2 ORDER BY rev_no DESC"
        );
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                &sql,
                vec![
                    DataValue::String(doc_file.to_string()),
                    DataValue::String(root_id.to_string()),
                ],
                "doc_rev_list",
            )
            .await
            .map_err(|e| BizError::internal(format!("列版本失败: {e}")))?;
        Ok(dataset_to_json_rows(&ds))
    }

    /// 取某版快照（rev=None 取当前版）。返回列式包 Value（前端 fromJSON 直接用）。
    pub async fn get_snapshot(
        mm: &DatabaseManager,
        db_id: &str,
        doc_file: &str,
        root_id: &str,
        rev: Option<i32>,
    ) -> Result<Value> {
        let (sql, params) = match rev {
            Some(n) => (
                format!(
                    "SELECT snapshot FROM {REV_TABLE} WHERE doc_file = $1 AND root_id = $2 AND rev_no = $3"
                ),
                vec![
                    DataValue::String(doc_file.to_string()),
                    DataValue::String(root_id.to_string()),
                    DataValue::Int(n as i64),
                ],
            ),
            None => (
                format!(
                    "SELECT snapshot FROM {REV_TABLE} WHERE doc_file = $1 AND root_id = $2 AND is_current = 1"
                ),
                vec![
                    DataValue::String(doc_file.to_string()),
                    DataValue::String(root_id.to_string()),
                ],
            ),
        };
        let ds = mm
            .query_sql_with_datavalues(db_id, None, &sql, params, "doc_rev_snap")
            .await
            .map_err(|e| BizError::internal(format!("取版本快照失败: {e}")))?;
        let snap_idx = ds.schema.get_index("snapshot");
        let val = ds
            .rows
            .first()
            .and_then(|r| snap_idx.and_then(|i| r.get(i)))
            .map(datavalue_to_json)
            .unwrap_or(Value::Null);
        // JSONB 列取回可能是 Json(String)，解析成对象
        Ok(normalize_jsonb(val))
    }
}

// ─────────────────────── 小工具 ───────────────────────

fn opt_str(s: Option<&str>) -> DataValue {
    match s {
        Some(v) => DataValue::String(v.to_string()),
        None => DataValue::Null,
    }
}

fn datavalue_to_json(dv: &DataValue) -> Value {
    serde_json::to_value(dv).unwrap_or(Value::Null)
}

/// JSONB 列取回若是字符串则 parse 成 JSON 对象。
fn normalize_jsonb(v: Value) -> Value {
    match v {
        Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        other => other,
    }
}

/// DataSet → JSON 行对象数组（列表接口用）。
fn dataset_to_json_rows(ds: &DataSet) -> Value {
    let cols: Vec<&str> = ds.schema.fields.iter().map(|f| f.name.as_str()).collect();
    let rows: Vec<Value> = ds
        .rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v = row.get(i).map(datavalue_to_json).unwrap_or(Value::Null);
                obj.insert((*c).to_string(), v);
            }
            Value::Object(obj)
        })
        .collect();
    json!({ "rows": rows, "total": rows.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_jsonb_parses_string() {
        let v = normalize_jsonb(Value::String("{\"a\":1}".into()));
        assert_eq!(v["a"], json!(1));
    }

    #[test]
    fn normalize_jsonb_passthrough_object() {
        let v = normalize_jsonb(json!({ "b": 2 }));
        assert_eq!(v["b"], json!(2));
    }

    #[test]
    fn opt_str_maps_none_to_null() {
        assert!(matches!(opt_str(None), DataValue::Null));
        assert!(matches!(opt_str(Some("x")), DataValue::String(_)));
    }
}
