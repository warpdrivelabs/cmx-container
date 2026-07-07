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

        // 保存 + 对账，任一失败即回滚。
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
        // 对账分两类：
        //   write_expected/write_affected —— INSERT(UPSERT)+UPDATE，须严格相等（每行必落地 1 行）。
        //   delete 允许幂等空删（前端可能删已不存在的行），不纳入严格对账，仅累加实际数。
        let mut write_expected: u64 = 0;
        let mut write_affected: u64 = 0;
        let changes = changes
            .as_object()
            .ok_or_else(|| BizError::business("changes 必须是对象"))?;

        // 静默零写防护（H1）：changes 里每个 key 都必须能对上某一层，否则报错而非静默丢弃。
        Self::assert_all_keys_matched(changes, meta)?;

        // 父先：按 layer_order 正序 批量 UPSERT / UPDATE
        for (idx, layer_id) in meta.layer_order.iter().enumerate() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(layer_changes) = layer_changes_for(changes, meta, idx, layer)
            else {
                continue;
            };

            if let Some(rows) = layer_changes.get("inserted").and_then(|v| v.as_array()) {
                if !rows.is_empty() {
                    write_expected += rows.len() as u64;
                    write_affected += Self::upsert_rows(mm, db_id, txn_id, layer, rows).await?;
                }
            }
            if let Some(rows) = layer_changes.get("updated").and_then(|v| v.as_array()) {
                if !rows.is_empty() {
                    write_expected += rows.len() as u64;
                    write_affected += Self::update_rows(mm, db_id, txn_id, layer, rows).await?;
                }
            }
        }
        affected += write_affected;

        // 子先：按 layer_order 逆序 批量 DELETE（幂等，不纳入严格对账）
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

        // 对账（H1/H2）：INSERT+UPDATE 每行必须精确落地。实际 < 期望 = 有行未写
        // （UPDATE 命中 0 行：id 不存在/被并发删；UPSERT 本应恒等），报错回滚，杜绝「假成功」。
        if write_affected < write_expected {
            return Err(BizError::business(format!(
                "回存对账失败：写入期望 {write_expected} 行，实际 {write_affected} 行（有行 id 不存在或被并发修改）"
            )));
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

        // 先删：子先父后，沿 upper_id 链圈定 rootId 子树（方案 E）。
        // 旧实现逐层 SELECT id 收集再删（O(层×关系) 次往返）；
        // 新实现用「子查询链」直接 DELETE —— 每层一条 DELETE，WHERE 用嵌套子查询上溯到根层，
        // 零预 SELECT、无 id 物化（避免并发漂移），往返数 = 层数。
        for layer_id in meta.layer_order.iter().rev() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            affected += Self::delete_subtree_layer(mm, db_id, txn_id, meta, layer, &root_ids).await?;
        }

        // 再插：父先，按 snapshot 各层 rows 批量 INSERT
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
            if !rows.is_empty() {
                affected += Self::insert_rows(mm, db_id, txn_id, layer, rows).await?;
            }
        }

        Ok(affected)
    }

    // ─────────────────── 批量 SQL（方案 A：消除逐行 N+1） ───────────────────

    /// sqlx/PG 单条语句参数上限 65535，留余量取 60000。按 列数 折算每批最大行数。
    const MAX_PARAMS: usize = 60000;

    /// 批量 UPSERT：同层多行合并为多值 INSERT ... ON CONFLICT(id) DO UPDATE。
    /// 各行列集可能不同（fields 稀疏），按「列集合」分组，每组一条多值语句。
    /// 走单值 DataValue 绑定（覆盖 Decimal/Date/Float 全类型；数组绑定不支持这些，故不用 UNNEST）。
    async fn upsert_rows(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
    ) -> Result<u64> {
        Self::batch_insert_grouped(mm, db_id, txn_id, layer, rows, true).await
    }

    /// 批量纯 INSERT（replace 模式；子树已先删）。
    async fn insert_rows(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
    ) -> Result<u64> {
        Self::batch_insert_grouped(mm, db_id, txn_id, layer, rows, false).await
    }

    /// 批量 INSERT 内核：按列集合分组 → 每组多值 INSERT（可选 ON CONFLICT UPSERT），按参数上限分批。
    async fn batch_insert_grouped(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
        upsert: bool,
    ) -> Result<u64> {
        use std::collections::BTreeMap;
        // 按「列名序列」分组：同列集的行走同一条多值 INSERT
        let mut groups: BTreeMap<Vec<String>, Vec<Vec<DataValue>>> = BTreeMap::new();
        for row in rows {
            let (cols, vals) = Self::row_cols_vals(layer, row)?;
            if cols.is_empty() {
                continue;
            }
            groups.entry(cols).or_default().push(vals);
        }

        let mut affected: u64 = 0;
        for (cols, value_rows) in groups {
            let ncol = cols.len();
            if ncol == 0 {
                continue;
            }
            let rows_per_batch = (Self::MAX_PARAMS / ncol).max(1);
            for chunk in value_rows.chunks(rows_per_batch) {
                let sql = build_multi_insert_sql(&layer.table_name, &cols, chunk.len(), upsert);
                let flat: Vec<DataValue> = chunk.iter().flatten().cloned().collect();
                affected += Self::exec(mm, db_id, txn_id, &sql, flat).await?;
            }
        }
        Ok(affected)
    }

    /// 批量 UPDATE：按「变更列集合」分组，每组一条 `UPDATE ... SET c=v.c FROM (VALUES ...) AS v(id,cols) WHERE t.id=v.id`。
    /// 各行变更列不同（updated.fields 稀疏），故先分组。
    async fn update_rows(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
    ) -> Result<u64> {
        use std::collections::BTreeMap;
        // 分组键 = 排序后的变更列名；值 = 每行 (id_dv, [col_dv...])
        let mut groups: BTreeMap<Vec<String>, Vec<(DataValue, Vec<DataValue>)>> = BTreeMap::new();
        for row in rows {
            let Some(id) = row.get("id") else {
                return Err(BizError::business("updated 行缺少 id"));
            };
            let fields = row
                .get("fields")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            // 只取定义里的列（防注入），排序保证同列集分到一组
            let mut cols: Vec<String> = fields
                .keys()
                .filter(|c| layer.schema.get_index(c).is_some())
                .cloned()
                .collect();
            cols.sort();
            if cols.is_empty() {
                continue;
            }
            let id_dv = dv_for_col(id, layer, "id");
            let col_vals: Vec<DataValue> = cols
                .iter()
                .map(|c| dv_for_col(&fields[c], layer, c))
                .collect();
            groups.entry(cols).or_default().push((id_dv, col_vals));
        }

        let mut affected: u64 = 0;
        for (cols, id_rows) in groups {
            let ncol = cols.len() + 1; // +1 for id
            let rows_per_batch = (Self::MAX_PARAMS / ncol).max(1);
            for chunk in id_rows.chunks(rows_per_batch) {
                let sql = build_multi_update_sql(&layer.table_name, &cols, chunk.len());
                // 展平参数：每行 (id, col1, col2, ...)
                let mut flat: Vec<DataValue> = Vec::with_capacity(chunk.len() * ncol);
                for (id_dv, col_vals) in chunk {
                    flat.push(id_dv.clone());
                    flat.extend(col_vals.iter().cloned());
                }
                affected += Self::exec(mm, db_id, txn_id, &sql, flat).await?;
            }
        }
        Ok(affected)
    }

    // ─────────────────── 批量 DELETE / 子层圈定 ───────────────────

    /// 删除某层「属于本次 rootId 子树」的行（方案 E：子查询链，零预 SELECT）。
    ///
    /// 对 layer_order 中第 i 层，构造 WHERE 子查询链上溯到根层（第 0 层）：
    ///   根层：  `DELETE FROM L0 WHERE id = ANY($1)`
    ///   第 i 层：`DELETE FROM Li WHERE {ck_i} IN (SELECT id FROM L(i-1) WHERE {ck_(i-1)} IN (... WHERE id = ANY($1)))`
    /// 其中 ck_k = 第 k 层相对其父的 childKey（按 layer_order 相邻推导，独立于 relation 命名）。
    async fn delete_subtree_layer(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        layer: &LayerView,
        root_ids: &[Value],
    ) -> Result<u64> {
        let Some(depth) = meta.layer_order.iter().position(|id| id == &layer.id) else {
            return Ok(0);
        };
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据无根层"))?;

        // 自内向外构造子查询链：最内层是根层 `id = ANY($1)`
        // sql_inner 初始 = 根层选择条件的表 + WHERE
        let mut inner = format!(
            "SELECT id FROM {} WHERE id = ANY($1)",
            quote_ident(&root.table_name)
        );
        // 从第 1 层到第 depth 层，逐层包裹（第 depth 层是目标层，其 childKey 用于最外 WHERE）
        // 目标层自身的 WHERE 由外层 DELETE 提供，故这里只需包到 depth-1 层的 SELECT。
        for k in 1..depth {
            let Some(mid) = meta.layer(&meta.layer_order[k]) else {
                return Ok(0);
            };
            let ck = meta
                .child_key_for_child(&mid.id)
                .unwrap_or_else(|| "upper_id".to_string());
            inner = format!(
                "SELECT id FROM {} WHERE {} IN ({})",
                quote_ident(&mid.table_name),
                quote_ident(&ck),
                inner
            );
        }

        let sql = if depth == 0 {
            // 根层：直接按 id 删
            format!(
                "DELETE FROM {} WHERE id = ANY($1)",
                quote_ident(&root.table_name)
            )
        } else {
            let ck = meta
                .child_key_for_child(&layer.id)
                .unwrap_or_else(|| "upper_id".to_string());
            format!(
                "DELETE FROM {} WHERE {} IN ({})",
                quote_ident(&layer.table_name),
                quote_ident(&ck),
                inner
            )
        };

        let dv_ids: Vec<DataValue> = root_ids.iter().map(|v| dv_for_col(v, root, "id")).collect();
        Self::exec(mm, db_id, txn_id, &sql, vec![DataValue::Array(dv_ids)]).await
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
        let sql = format!("DELETE FROM {} WHERE id = ANY($1)", quote_ident(&layer.table_name));
        Self::exec(mm, db_id, txn_id, &sql, vec![DataValue::Array(dv_ids)]).await
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

    /// 静默零写防护（H1）：changes 里每个 key 必须能对上某一层，否则报错。
    /// 防前端 path 约定漂移（表名 vs 嵌套路径）导致「保存成功却一行没写」。
    fn assert_all_keys_matched(
        changes: &Map<String, Value>,
        meta: &DocMetaView,
    ) -> Result<()> {
        // 构造所有合法 key：每层的 表名 / 层 id / 嵌套全路径
        let mut valid: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (idx, layer_id) in meta.layer_order.iter().enumerate() {
            if let Some(layer) = meta.layer(layer_id) {
                valid.insert(layer.table_name.clone());
                valid.insert(layer.id.clone());
            }
            valid.insert(meta.layer_order[..=idx].join("."));
        }
        for key in changes.keys() {
            if !valid.contains(key) {
                return Err(BizError::business(format!(
                    "changeset 含未知层 key「{key}」，无法对应任何层（防静默零写）"
                )));
            }
        }
        Ok(())
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

/// PG 标识符双引号包裹（防列名/表名撞关键字，与 sql_builder.rs 的 SELECT 侧风格对齐）。
/// 内部双引号转义为两个双引号。列名已过 schema 白名单，这里只防关键字冲突。
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// 构造多值 INSERT（方案 A）：`INSERT INTO t (c...) VALUES ($1..$k),($k+1..) [ON CONFLICT (id) DO ...]`。
/// nrows 行 × cols.len() 列，占位符自 $1 连续编号。upsert=true 时加 ON CONFLICT 子句。
/// 纯函数（无 IO），便于单测占位符/列数/冲突子句正确性。
fn build_multi_insert_sql(table: &str, cols: &[String], nrows: usize, upsert: bool) -> String {
    let ncol = cols.len();
    let cols_sql = cols
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let mut p = 0usize;
    let value_groups: Vec<String> = (0..nrows)
        .map(|_| {
            let group: Vec<String> = (0..ncol)
                .map(|_| {
                    p += 1;
                    format!("${p}")
                })
                .collect();
            format!("({})", group.join(", "))
        })
        .collect();
    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        quote_ident(table),
        cols_sql,
        value_groups.join(", ")
    );
    if upsert {
        let updates: Vec<String> = cols
            .iter()
            .filter(|c| c.as_str() != "id")
            .map(|c| format!("{q} = EXCLUDED.{q}", q = quote_ident(c)))
            .collect();
        if updates.is_empty() {
            sql.push_str(" ON CONFLICT (id) DO NOTHING");
        } else {
            sql.push_str(&format!(" ON CONFLICT (id) DO UPDATE SET {}", updates.join(", ")));
        }
    }
    sql
}

/// 构造多值 UPDATE（方案 A）：`UPDATE t SET c=v.c FROM (VALUES (id,c..),...) AS v(id,c..) WHERE t.id=v.id`。
/// 每行参数序 = (id, col1, col2, ...)，占位符自 $1 连续。纯函数，便于单测。
fn build_multi_update_sql(table: &str, cols: &[String], nrows: usize) -> String {
    let ncol = cols.len() + 1; // +id
    let t = quote_ident(table);
    let set_sql = cols
        .iter()
        .map(|c| format!("{q} = v.{q}", q = quote_ident(c)))
        .collect::<Vec<_>>()
        .join(", ");
    let alias_cols = std::iter::once("id".to_string())
        .chain(cols.iter().map(|c| quote_ident(c)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut p = 0usize;
    let value_groups: Vec<String> = (0..nrows)
        .map(|_| {
            let group: Vec<String> = (0..ncol)
                .map(|_| {
                    p += 1;
                    format!("${p}")
                })
                .collect();
            format!("({})", group.join(", "))
        })
        .collect();
    format!(
        "UPDATE {t} SET {set} FROM (VALUES {vals}) AS v({alias}) WHERE {t}.id = v.id",
        t = t,
        set = set_sql,
        vals = value_groups.join(", "),
        alias = alias_cols,
    )
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
        // 目标是日期时间列（TIMESTAMP/TIMESTAMPTZ→DateTime）的非空字符串 → DateTime。
        // 兼容 RFC3339（"2026-07-07T09:00:00Z"）与无时区的 "2026-07-07T09:00:00" / "2026-07-07 09:00:00"（按 UTC）。
        (Value::String(s), Some(FieldType::DateTime)) => parse_datetime(s.trim())
            .map(DataValue::DateTime)
            .unwrap_or_else(|| DataValue::String(s.clone())),
        // 目标是 DATE 列的非空字符串 → Date
        (Value::String(s), Some(FieldType::Date)) => s
            .trim()
            .parse::<chrono::NaiveDate>()
            .map(DataValue::Date)
            .unwrap_or_else(|_| DataValue::String(s.clone())),
        // 其余走通用转换
        _ => json_to_dv(v),
    }
}

/// 解析日期时间字符串为 UTC DateTime，兼容 RFC3339 与无时区两种常见格式。
fn parse_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, NaiveDateTime, Utc};
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
        }
    }
    None
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

    #[test]
    fn quote_ident_wraps_and_escapes() {
        assert_eq!(quote_ident("id"), "\"id\"");
        assert_eq!(quote_ident("user"), "\"user\""); // 关键字也安全
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\""); // 内部引号转义
    }

    #[test]
    fn multi_insert_single_row_upsert() {
        let cols = vec!["id".to_string(), "amount".to_string()];
        let sql = build_multi_insert_sql("cv_acc_line", &cols, 1, true);
        assert_eq!(
            sql,
            "INSERT INTO \"cv_acc_line\" (\"id\", \"amount\") VALUES ($1, $2) \
             ON CONFLICT (id) DO UPDATE SET \"amount\" = EXCLUDED.\"amount\""
        );
    }

    #[test]
    fn multi_insert_three_rows_placeholders_continuous() {
        let cols = vec!["id".to_string(), "a".to_string()];
        let sql = build_multi_insert_sql("t", &cols, 3, false);
        // 3 行 × 2 列 = $1..$6，连续编号
        assert_eq!(
            sql,
            "INSERT INTO \"t\" (\"id\", \"a\") VALUES ($1, $2), ($3, $4), ($5, $6)"
        );
    }

    #[test]
    fn multi_insert_id_only_do_nothing() {
        let cols = vec!["id".to_string()];
        let sql = build_multi_insert_sql("t", &cols, 1, true);
        // 只有 id 列时冲突不更新
        assert!(sql.ends_with("ON CONFLICT (id) DO NOTHING"));
    }

    #[test]
    fn multi_update_from_values() {
        let cols = vec!["a".to_string(), "b".to_string()];
        let sql = build_multi_update_sql("cv_header", &cols, 2);
        // 每行 (id,a,b) = 3 参，2 行 = $1..$6
        assert_eq!(
            sql,
            "UPDATE \"cv_header\" SET \"a\" = v.\"a\", \"b\" = v.\"b\" \
             FROM (VALUES ($1, $2, $3), ($4, $5, $6)) AS v(id, \"a\", \"b\") \
             WHERE \"cv_header\".id = v.id"
        );
    }

    #[test]
    fn param_count_matches_placeholders() {
        // 批量插入的参数展平数应 == 占位符数（nrows × ncol）
        let cols = vec!["id".to_string(), "x".to_string(), "y".to_string()];
        let sql = build_multi_insert_sql("t", &cols, 4, false);
        let max_ph = (1..=100)
            .rev()
            .find(|i| sql.contains(&format!("${i}")))
            .unwrap();
        assert_eq!(max_ph, 4 * 3); // 12 个占位符
    }
}
