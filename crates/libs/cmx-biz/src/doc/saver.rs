//! DocSaver — 业务单据数据回存，双模式（方案 §6.4）
//!
//! 两种模式，由 `save_mode` 控制：
//!   - `merge`（默认）：按 changeset 精确 INSERT(UPSERT)/UPDATE/DELETE，保主键/审计、写入最小。
//!   - `replace`：对 rootId 子树先 DELETE 旧行、再全量 INSERT snapshot（前端免 diff）。
//!
//! 共性（两模式）：同一事务、按 relations 拓扑序（插/更父先、删子先）、参数化 DataValue 绑定。
//!
//! changeset 结构（merge，§6.3）：
//! ```json
//! { "cv_header": { "updated": [ { "id": "...", "fields": { "total_dr": 100 } } ],
//!                  "inserted": [ { "id": "...", "upper_id": "...", "fields": {...} } ],
//!                  "deleted": [ "id1", "id2" ] } }
//! ```

use serde_json::{Map, Value};

use cmx_core::model::cell::DataValue;
use cmx_database::DatabaseManager;

use super::meta::{DocMetaView, LayerView};
use crate::{BizError, Result};

/// 保存模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// 增量：按 changeset 精确写。
    Merge,
    /// 先删后插：按 rootId 子树覆盖。
    Replace,
}

impl SaveMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "replace" => SaveMode::Replace,
            _ => SaveMode::Merge,
        }
    }
}

/// 保存结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SaveResult {
    pub ok: bool,
    pub mode: String,
    pub affected: u64,
}

pub struct DocSaver;

impl DocSaver {
    /// 回存单据。`changes` 为 merge 模式 changeset；`snapshot` 为 replace 模式整树列式包。
    pub async fn save(
        mm: &DatabaseManager,
        db_id: &str,
        meta: &DocMetaView,
        mode: SaveMode,
        changes: &Value,
    ) -> Result<SaveResult> {
        let ctx = mm.get_transaction_context();
        let guard = ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {e}")))?;
        let txn_id = guard.txn_id().to_string();

        let affected = match mode {
            SaveMode::Merge => Self::apply_merge(mm, db_id, &txn_id, meta, changes).await,
            SaveMode::Replace => Self::apply_replace(mm, db_id, &txn_id, meta, changes).await,
        };

        match affected {
            Ok(n) => {
                guard
                    .commit()
                    .await
                    .map_err(|e| BizError::internal(format!("提交事务失败: {e}")))?;
                Ok(SaveResult {
                    ok: true,
                    mode: match mode {
                        SaveMode::Merge => "merge".into(),
                        SaveMode::Replace => "replace".into(),
                    },
                    affected: n,
                })
            }
            Err(e) => {
                // guard drop 自动回滚；显式 rollback 更清晰
                let _ = guard.rollback().await;
                Err(e)
            }
        }
    }

    // ─────────────────── merge 模式 ───────────────────

    async fn apply_merge(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        changes: &Value,
    ) -> Result<u64> {
        let mut affected: u64 = 0;
        let changes = changes
            .as_object()
            .ok_or_else(|| BizError::business("changes 必须是对象"))?;

        // 父先：按 layer_order 正序 UPSERT/UPDATE
        for (idx, layer_id) in meta.layer_order.iter().enumerate() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(layer_changes) = layer_changes_for(changes, meta, idx, layer)
            else {
                continue;
            };

            // inserted → UPSERT
            if let Some(rows) = layer_changes.get("inserted").and_then(|v| v.as_array()) {
                for row in rows {
                    affected += Self::upsert_row(mm, db_id, txn_id, layer, row).await?;
                }
            }
            // updated → UPDATE 变更列
            if let Some(rows) = layer_changes.get("updated").and_then(|v| v.as_array()) {
                for row in rows {
                    affected += Self::update_row(mm, db_id, txn_id, layer, row).await?;
                }
            }
        }

        // 子先：按 layer_order 逆序 DELETE
        for (idx, layer_id) in meta.layer_order.iter().enumerate().rev() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(layer_changes) = layer_changes_for(changes, meta, idx, layer)
            else {
                continue;
            };
            if let Some(ids) = layer_changes.get("deleted").and_then(|v| v.as_array()) {
                if !ids.is_empty() {
                    affected += Self::delete_ids(mm, db_id, txn_id, layer, ids).await?;
                }
            }
        }

        Ok(affected)
    }

    // ─────────────────── replace 模式 ───────────────────

    async fn apply_replace(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        snapshot: &Value,
    ) -> Result<u64> {
        // snapshot 结构同 merge 的按层 { table: { rows: [ {id,upper_id,fields} ] } }
        // 简化：replace 也按层给全量行；先删（子先）、再插（父先）。
        let obj = snapshot
            .as_object()
            .ok_or_else(|| BizError::business("snapshot 必须是对象"))?;
        let mut affected: u64 = 0;

        // 收集 rootId（根层 rows 的 id），界定删除范围
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据无根层"))?;
        let root_ids: Vec<Value> = obj
            .get(&root.table_name)
            .or_else(|| obj.get(&root.id))
            .and_then(|l| l.get("rows"))
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.get("id").cloned())
                    .collect()
            })
            .unwrap_or_default();

        if root_ids.is_empty() {
            return Err(BizError::business("replace 模式必须提供根层 rows 以界定覆盖范围"));
        }

        // 先删：子先父后，沿 upper_id 链圈定 rootId 子树
        // 简化实现：各层按其父键 = ANY(上层被删id)，逐层下推收集，再逆序删。
        // 此处按 rootId 直接删根层，子层按 upper_id 关联删（依赖各层 upper_id 指向直接父 id）。
        // 为保证安全，仅删「属于本次 rootId 树」的行。
        let mut layer_del_ids: std::collections::HashMap<String, Vec<Value>> =
            std::collections::HashMap::new();
        layer_del_ids.insert(root.id.clone(), root_ids.clone());
        // 下推：对每层求其子层待删 id（SELECT id WHERE upper_id = ANY(parent_del_ids)）
        for layer_id in &meta.layer_order {
            let parent_ids = layer_del_ids.get(layer_id).cloned().unwrap_or_default();
            if parent_ids.is_empty() {
                continue;
            }
            for rel in meta.child_relations(layer_id) {
                if let Some(child) = meta.layer(&rel.child) {
                    let child_ids =
                        Self::select_child_ids(mm, db_id, txn_id, child, &rel.child_key, &parent_ids)
                            .await?;
                    layer_del_ids
                        .entry(child.id.clone())
                        .or_default()
                        .extend(child_ids);
                }
            }
        }
        // 逆序删
        for layer_id in meta.layer_order.iter().rev() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            if let Some(ids) = layer_del_ids.get(layer_id) {
                if !ids.is_empty() {
                    affected += Self::delete_ids(mm, db_id, txn_id, layer, ids).await?;
                }
            }
        }

        // 再插：父先，按 snapshot 各层 rows 全量 INSERT
        for layer_id in &meta.layer_order {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(rows) = obj
                .get(&layer.table_name)
                .or_else(|| obj.get(layer_id))
                .and_then(|l| l.get("rows"))
                .and_then(|v| v.as_array())
            else {
                continue;
            };
            for row in rows {
                affected += Self::insert_row(mm, db_id, txn_id, layer, row).await?;
            }
        }

        Ok(affected)
    }

    // ─────────────────── 单行 SQL ───────────────────

    /// UPSERT：INSERT ... ON CONFLICT(id) DO UPDATE。row 形如 {id, upper_id, fields:{...}}。
    async fn upsert_row(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        row: &Value,
    ) -> Result<u64> {
        let (cols, vals) = Self::row_cols_vals(layer, row)?;
        if cols.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = (1..=cols.len()).map(|i| format!("${i}")).collect();
        let updates: Vec<String> = cols
            .iter()
            .filter(|c| c.as_str() != "id")
            .map(|c| format!("{c} = EXCLUDED.{c}"))
            .collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT (id) DO UPDATE SET {}",
            layer.table_name,
            cols.join(", "),
            placeholders.join(", "),
            updates.join(", ")
        );
        Self::exec(mm, db_id, txn_id, &sql, vals).await
    }

    /// 纯 INSERT（replace 模式用；子树已先删）。
    async fn insert_row(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        row: &Value,
    ) -> Result<u64> {
        let (cols, vals) = Self::row_cols_vals(layer, row)?;
        if cols.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = (1..=cols.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            layer.table_name,
            cols.join(", "),
            placeholders.join(", ")
        );
        Self::exec(mm, db_id, txn_id, &sql, vals).await
    }

    /// UPDATE 变更列 WHERE id=$last。
    async fn update_row(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        row: &Value,
    ) -> Result<u64> {
        let id = row
            .get("id")
            .ok_or_else(|| BizError::business("updated 行缺少 id"))?;
        let fields = row
            .get("fields")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if fields.is_empty() {
            return Ok(0);
        }
        let mut sets = Vec::new();
        let mut vals: Vec<DataValue> = Vec::new();
        let mut ph = 0; // 占位符计数（跳过非法列时保持连续）
        for (col, v) in fields.iter() {
            if layer.schema.get_index(col).is_none() {
                continue; // 只更定义里的列，防注入
            }
            ph += 1;
            sets.push(format!("{col} = ${ph}"));
            vals.push(dv_for_col(v, layer, col));
        }
        if sets.is_empty() {
            return Ok(0);
        }
        ph += 1;
        vals.push(dv_for_col(id, layer, "id"));
        let sql = format!(
            "UPDATE {} SET {} WHERE id = ${ph}",
            layer.table_name,
            sets.join(", "),
        );
        Self::exec(mm, db_id, txn_id, &sql, vals).await
    }

    /// DELETE WHERE id = ANY($1)。
    async fn delete_ids(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        ids: &[Value],
    ) -> Result<u64> {
        let dv_ids: Vec<DataValue> = ids.iter().map(|v| dv_for_col(v, layer, "id")).collect();
        let sql = format!("DELETE FROM {} WHERE id = ANY($1)", layer.table_name);
        Self::exec(mm, db_id, txn_id, &sql, vec![DataValue::Array(dv_ids)]).await
    }

    /// SELECT id FROM 子表 WHERE childKey = ANY($1)（replace 圈定子树用）。
    async fn select_child_ids(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        child_key: &str,
        parent_ids: &[Value],
    ) -> Result<Vec<Value>> {
        let dv_ids: Vec<DataValue> = parent_ids.iter().map(|v| dv_for_col(v, layer, child_key)).collect();
        let sql = format!(
            "SELECT id FROM {} WHERE {} = ANY($1)",
            layer.table_name, child_key
        );
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                Some(txn_id),
                &sql,
                vec![DataValue::Array(dv_ids)],
                &layer.table_name,
            )
            .await
            .map_err(|e| BizError::internal(format!("查子层 id 失败: {e}")))?;
        let id_idx = ds.schema.get_index("id");
        Ok(ds
            .rows
            .iter()
            .filter_map(|r| id_idx.and_then(|i| r.get(i)))
            .map(|dv| serde_json::to_value(dv).unwrap_or(Value::Null))
            .collect())
    }

    /// 从 row {id, upper_id, fields} 拼列名+值（只取定义里的列）。
    fn row_cols_vals(layer: &LayerView, row: &Value) -> Result<(Vec<String>, Vec<DataValue>)> {
        let mut cols = Vec::new();
        let mut vals = Vec::new();
        // 顶层 id / upper_id / line_no（若在 schema）
        for top in ["id", "upper_id", "line_no"] {
            if layer.schema.get_index(top).is_some() {
                if let Some(v) = row.get(top) {
                    cols.push(top.to_string());
                    vals.push(dv_for_col(v, layer, top));
                }
            }
        }
        // fields 里的业务列
        if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
            for (col, v) in fields {
                if layer.schema.get_index(col).is_some() && !cols.contains(col) {
                    cols.push(col.clone());
                    vals.push(dv_for_col(v, layer, col));
                }
            }
        }
        Ok((cols, vals))
    }

    async fn exec(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        sql: &str,
        params: Vec<DataValue>,
    ) -> Result<u64> {
        mm.execute_sql_with_datavalues(db_id, Some(txn_id), sql, params)
            .await
            .map_err(|e| BizError::internal(format!("执行回存 SQL 失败: {e} | {sql}")))
    }
}

/// 从 changes 里取某层的变更桶，兼容三种 key 形态：
///   - 表名（如 `cv_acc_line`）
///   - 层 id（同表名）
///   - **嵌套全路径**（前端 collector 用，如 `cv_batch.cv_header.cv_acc_line`）
///
/// 前端 ChangeSetCollector 的 path 是 schema 嵌套路径（rootId + 各层 child id 累积），
/// 故子层 key 是嵌套路径而非表名 —— 这里补上嵌套路径匹配，避免子层保存匹配不到（affected 0）。
fn layer_changes_for<'a>(
    changes: &'a serde_json::Map<String, Value>,
    meta: &DocMetaView,
    layer_idx: usize,
    layer: &LayerView,
) -> Option<&'a Value> {
    // ① 表名 / 层 id
    if let Some(v) = changes.get(&layer.table_name).or_else(|| changes.get(&layer.id)) {
        return Some(v);
    }
    // ② 嵌套全路径：layer_order[0..=layer_idx].join(".")
    let nested = meta.layer_order[..=layer_idx].join(".");
    changes.get(&nested)
}

/// JSON 值 → DataValue（回存参数绑定）。
fn json_to_dv(v: &Value) -> DataValue {    match v {
        Value::Null => DataValue::Null,
        Value::Bool(b) => DataValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DataValue::Int(i)
            } else {
                DataValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => DataValue::String(s.clone()),
        other => DataValue::Json(other.to_string()),
    }
}

/// 按列的定义类型把 JSON 值转成匹配的 DataValue（解决 PG `bigint = text` 类型不匹配）。
///
/// 前端 changeset 里 id/数值常是 JSON 字符串（如 "1000000001"），而列可能是 BIGINT。
/// 这里读 layer.schema 该列的 FieldType，把数字字符串强转为 Int/Float，避免绑定类型错。
fn dv_for_col(v: &Value, layer: &LayerView, col: &str) -> DataValue {
    use cmx_core::model::cell::FieldType;
    let ft = layer
        .schema
        .get_index(col)
        .and_then(|i| layer.schema.fields.get(i))
        .map(|f| f.field_type.clone());

    match (v, ft) {
        // 目标是整数列：数字字符串/数字 → Int
        (Value::String(s), Some(FieldType::Int)) => s
            .trim()
            .parse::<i64>()
            .map(DataValue::Int)
            .unwrap_or_else(|_| if s.is_empty() { DataValue::Null } else { DataValue::String(s.clone()) }),
        (Value::String(s), Some(FieldType::Float)) => s
            .trim()
            .parse::<f64>()
            .map(DataValue::Float)
            .unwrap_or_else(|_| if s.is_empty() { DataValue::Null } else { DataValue::String(s.clone()) }),
        // 目标是 Decimal/日期列的空字符串 → NULL
        (Value::String(s), Some(FieldType::Decimal | FieldType::Date | FieldType::DateTime))
            if s.trim().is_empty() =>
        {
            DataValue::Null
        }
        // 目标是 Decimal 列的非空数字字符串 → Decimal（避免 text 绑 numeric 报错）
        (Value::String(s), Some(FieldType::Decimal)) => s
            .trim()
            .parse::<rust_decimal::Decimal>()
            .map(DataValue::Decimal)
            .unwrap_or_else(|_| DataValue::String(s.clone())),
        // 目标是 Decimal 列的 JSON 数字 → Decimal
        (Value::Number(n), Some(FieldType::Decimal)) => {
            use std::str::FromStr;
            rust_decimal::Decimal::from_str(&n.to_string())
                .map(DataValue::Decimal)
                .unwrap_or_else(|_| json_to_dv(v))
        }
        // 其余走通用转换
        _ => json_to_dv(v),
    }
}

/// 从请求 body 提取 saveMode / changes（handler 用）。
pub fn parse_save_body(body: &Value) -> (SaveMode, Value) {
    let mode = body
        .get("saveMode")
        .and_then(|v| v.as_str())
        .map(SaveMode::from_str)
        .unwrap_or(SaveMode::Merge);
    let changes = match mode {
        SaveMode::Merge => body.get("changes").cloned().unwrap_or(Value::Null),
        SaveMode::Replace => body
            .get("snapshot")
            .cloned()
            .or_else(|| body.get("changes").cloned())
            .unwrap_or(Value::Null),
    };
    (mode, changes)
}

#[allow(dead_code)]
fn _map_unused(_: &Map<String, Value>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_save_body_defaults_merge() {
        let (mode, changes) = parse_save_body(&json!({ "changes": { "t": {} } }));
        assert_eq!(mode, SaveMode::Merge);
        assert!(changes.is_object());
    }

    #[test]
    fn parse_save_body_replace_takes_snapshot() {
        let (mode, changes) =
            parse_save_body(&json!({ "saveMode": "replace", "snapshot": { "t": { "rows": [] } } }));
        assert_eq!(mode, SaveMode::Replace);
        assert!(changes.get("t").is_some());
    }

    #[test]
    fn save_mode_from_str() {
        assert_eq!(SaveMode::from_str("replace"), SaveMode::Replace);
        assert_eq!(SaveMode::from_str("merge"), SaveMode::Merge);
        assert_eq!(SaveMode::from_str("xyz"), SaveMode::Merge);
    }

    #[test]
    fn json_to_dv_types() {
        assert!(matches!(json_to_dv(&json!(5)), DataValue::Int(5)));
        assert!(matches!(json_to_dv(&json!("a")), DataValue::String(_)));
        assert!(matches!(json_to_dv(&json!(null)), DataValue::Null));
        assert!(matches!(json_to_dv(&json!(true)), DataValue::Bool(true)));
    }
}
