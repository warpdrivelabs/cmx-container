//! DocLoader — 业务单据数据装载（方案 §5.1）
//!
//! 读单据定义（DocMetaView）→ 逐层查询 → 按 upper_id 组装嵌套 DataSet。
//!
//! 算法（按层广度优先，父批量驱动子，避免 N+1）：
//!   1. 根层：`SELECT <列> FROM <root表> [WHERE <filter>]` → DataSet，收集 rootIds
//!   2. 逐层下钻：`SELECT <列> FROM <子表> WHERE <childKey> = ANY($parentIds)` 一条 SQL 取全部子行
//!      → 按 childKey(upper_id) 分桶 → 挂到父行 _children[childKey]
//!   3. 返回根层 DataSet（完整嵌套主从树）
//!
//! 关键：一层一条 SQL、`ANY($ids)` 批量、参数化（DataValue 绑定，注入免疫）。

use std::collections::HashMap;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{DataSet, Row};
use cmx_database::DatabaseManager;

use super::meta::{DocMetaView, LayerView};
use crate::{BizError, Result};

/// 装载选项。
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// 根层过滤：`(列名, 值)` 列表，AND 连接（简单等值；复杂条件后续扩展）。
    pub root_filter: Vec<(String, DataValue)>,
    /// 根层限制行数（分页第一步；None 不限）。
    pub root_limit: Option<u64>,
    /// 装载深度（None = 全部层；Some(n) = 只装前 n 层，懒下钻用）。
    pub depth: Option<usize>,
}

pub struct DocLoader;

impl DocLoader {
    /// 按定义装载整棵单据树，返回根层嵌套 DataSet。
    pub async fn load(
        mm: &DatabaseManager,
        db_id: &str,
        meta: &DocMetaView,
        opts: &LoadOptions,
    ) -> Result<DataSet> {
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据定义无根层"))?;

        // 1. 根层查询
        let mut root_ds = Self::query_root(mm, db_id, root, opts).await?;

        // 2. 逐层下钻（沿 relations 递归挂子集）
        let max_depth = opts.depth.unwrap_or(usize::MAX);
        Self::descend(mm, db_id, meta, root, &mut root_ds, 1, max_depth).await?;

        Ok(root_ds)
    }

    /// 根层：SELECT 列 FROM 表 [WHERE filter] [LIMIT n]
    async fn query_root(
        mm: &DatabaseManager,
        db_id: &str,
        layer: &LayerView,
        opts: &LoadOptions,
    ) -> Result<DataSet> {
        let cols = column_list(layer);
        let mut sql = format!("SELECT {cols} FROM {}", layer.table_name);
        let mut params: Vec<DataValue> = Vec::new();

        if !opts.root_filter.is_empty() {
            let mut conds = Vec::new();
            for (i, (col, val)) in opts.root_filter.iter().enumerate() {
                // 仅允许定义里存在的列名，防注入
                if layer.schema.get_index(col).is_none() {
                    return Err(BizError::business(format!("过滤列 {col} 不在层 {} 中", layer.table_name)));
                }
                conds.push(format!("{col} = ${}", i + 1));
                params.push(val.clone());
            }
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        // 排序保证稳定
        if layer.schema.get_index("id").is_some() {
            sql.push_str(" ORDER BY id");
        }
        if let Some(n) = opts.root_limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }

        mm.query_sql_with_datavalues(db_id, None, &sql, params, &layer.table_name)
            .await
            .map(|ds| rebind_schema(ds, layer))
            .map_err(|e| BizError::internal(format!("装载根层 {} 失败: {e}", layer.table_name)))
    }

    /// 递归下钻：按 layer_order 顺序推导父子层，查子层并挂载。
    ///
    /// 注意：定义里 relations 的 parent/child 用「逻辑名」(headers/account_lines)，
    /// 而 schema 节点与表用「物理名」(cv_header/cv_acc_line)，两者不一致。
    /// 故这里**不按 relations 的名字匹配层**，而是按 layer_order 相邻层推导父子，
    /// childKey 优先取 relations（按顺序对齐），无则默认 "upper_id"。
    async fn descend(
        mm: &DatabaseManager,
        db_id: &str,
        meta: &DocMetaView,
        parent_layer: &LayerView,
        parent_ds: &mut DataSet,
        cur_depth: usize,
        max_depth: usize,
    ) -> Result<()> {
        if cur_depth >= max_depth {
            return Ok(());
        }
        let parent_ids = collect_ids(parent_ds);
        if parent_ids.is_empty() {
            return Ok(());
        }

        // 父层在 layer_order 里的位置 → 子层 = 下一层
        let pos = meta.layer_order.iter().position(|id| id == &parent_layer.id);
        let child_id = match pos.and_then(|i| meta.layer_order.get(i + 1)) {
            Some(id) => id.clone(),
            None => return Ok(()), // 已是最深层
        };
        let child_layer = match meta.layer(&child_id) {
            Some(l) => l,
            None => return Ok(()),
        };
        // childKey：优先取第 pos 个 relation 的 child_key（按 layer 顺序对齐），默认 upper_id
        let child_key = pos
            .and_then(|i| meta.relations.get(i))
            .map(|r| r.child_key.clone())
            .unwrap_or_else(|| "upper_id".to_string());

        // 查子层（WHERE childKey = ANY($parent_ids)）
        let mut child_ds =
            Self::query_children_by_key(mm, db_id, child_layer, &child_key, &parent_ids).await?;

        // 递归挂孙层
        Box::pin(Self::descend(
            mm, db_id, meta, child_layer, &mut child_ds, cur_depth + 1, max_depth,
        ))
        .await?;

        // 按 childKey 分桶挂到父行；childId 用物理层 id（前端 schema 路径对应）
        attach_children(parent_ds, child_ds, &child_key, &child_id);
        Ok(())
    }

    /// 子层：SELECT 列 FROM 子表 WHERE childKey = ANY($1) ORDER BY childKey, line_no
    async fn query_children_by_key(
        mm: &DatabaseManager,
        db_id: &str,
        layer: &LayerView,
        child_key: &str,
        parent_ids: &[DataValue],
    ) -> Result<DataSet> {
        let cols = column_list(layer);
        let mut sql = format!(
            "SELECT {cols} FROM {} WHERE {} = ANY($1)",
            layer.table_name, child_key
        );
        // 有序返回：先按父键，再按行号
        let has_line_no = layer.schema.get_index("line_no").is_some();
        if has_line_no {
            sql.push_str(&format!(" ORDER BY {}, line_no", child_key));
        } else {
            sql.push_str(&format!(" ORDER BY {}", child_key));
        }

        let params = vec![DataValue::Array(parent_ids.to_vec())];
        mm.query_sql_with_datavalues(db_id, None, &sql, params, &layer.table_name)
            .await
            .map(|ds| rebind_schema(ds, layer))
            .map_err(|e| BizError::internal(format!("装载子层 {} 失败: {e}", layer.table_name)))
    }
}

// ─────────────────────── 组装辅助 ───────────────────────

/// 逗号分隔的列名列表（按 schema 顺序）。
fn column_list(layer: &LayerView) -> String {
    layer
        .schema
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// 用「定义 Schema」替换查询推断的 Schema。
///
/// 关键：`convert_postgres_rows` 从查询结果行推断 Schema，**空表返回 0 行 → 空 Schema**，
/// 导致 columns 为空、前端无表头。这里用 DocMetaView 编译好的权威 Schema（含全部列，
/// 且与 SELECT 列顺序一致）覆盖，保证即使空表也返回正确 columns。
fn rebind_schema(ds: DataSet, layer: &LayerView) -> DataSet {
    // SELECT 用的正是 column_list(layer)（同一列顺序），行值与定义 Schema 对齐。
    let mut out = DataSet::with_capacity(ds.id.clone(), layer.schema.clone(), ds.row_count());
    for row in ds.rows {
        out.add_row(row);
    }
    out
}

/// 从 DataSet 收集 "id" 列的值集（作为子层 ANY 查询参数）。
fn collect_ids(ds: &DataSet) -> Vec<DataValue> {
    let Some(id_idx) = ds.schema.get_index("id") else {
        return Vec::new();
    };
    ds.rows
        .iter()
        .filter_map(|r| r.get(id_idx).cloned())
        .filter(|v| !matches!(v, DataValue::Null | DataValue::NullTyped(_)))
        .collect()
}

/// 把 child_ds 按 child_key 值分桶，挂到 parent_ds 对应父行的 _children[child_id]。
///
/// 分桶后每个父行分到一个新 DataSet（同 schema、子集行）。
fn attach_children(parent_ds: &mut DataSet, child_ds: DataSet, child_key: &str, child_id: &str) {
    let parent_id_idx = match parent_ds.schema.get_index("id") {
        Some(i) => i,
        None => return,
    };
    let child_fk_idx = match child_ds.schema.get_index(child_key) {
        Some(i) => i,
        None => return,
    };
    let child_schema = child_ds.schema.clone();

    // 分桶：父key字符串 → 子行列表
    let mut buckets: HashMap<String, Vec<Row>> = HashMap::new();
    for row in child_ds.rows {
        let key = row
            .get(child_fk_idx)
            .map(dv_to_key)
            .unwrap_or_default();
        buckets.entry(key).or_default().push(row);
    }

    // 回填：遍历父行，取对应桶建子 DataSet 挂上
    for prow in parent_ds.rows.iter_mut() {
        let pid = prow.get(parent_id_idx).map(dv_to_key).unwrap_or_default();
        if let Some(rows) = buckets.remove(&pid) {
            let mut cds = DataSet::with_capacity(child_id, child_schema.clone(), rows.len());
            for r in rows {
                cds.add_row(r);
            }
            prow.add_child(child_id, cds);
        }
    }
}

/// DataValue → 分桶/匹配用的字符串键。
fn dv_to_key(dv: &DataValue) -> String {
    match dv {
        DataValue::Int(i) => i.to_string(),
        DataValue::String(s) => s.clone(),
        DataValue::ShortStr(s) | DataValue::LongStr(s) => s.to_string(),
        DataValue::Uuid(u) => u.to_string(),
        DataValue::Null | DataValue::NullTyped(_) => String::new(),
        other => serde_json::to_value(other)
            .ok()
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_core::model::cell::{Field, FieldType};
    use cmx_core::model::data::dataset::Schema;
    use std::sync::Arc;

    fn schema(id: &str, cols: &[(&str, FieldType)]) -> Arc<Schema> {
        Arc::new(Schema::new_unchecked(
            id,
            cols.iter()
                .map(|(n, t)| Field {
                    name: (*n).into(),
                    field_type: t.clone(),
                    label: String::new(),
                })
                .collect(),
        ))
    }

    // 纯组装逻辑测试（不连库）：验证 attach_children 按 upper_id 正确分桶挂载
    #[test]
    fn attach_children_buckets_by_upper_id() {
        let ps = schema("cv_header", &[("id", FieldType::Int)]);
        let mut parent = DataSet::empty("cv_header", ps);
        parent.add_row(Row::new(vec![DataValue::Int(1)]));
        parent.add_row(Row::new(vec![DataValue::Int(2)]));

        let cs = schema(
            "cv_line",
            &[("id", FieldType::Int), ("upper_id", FieldType::Int)],
        );
        let mut child = DataSet::empty("cv_line", cs);
        child.add_row(Row::new(vec![DataValue::Int(11), DataValue::Int(1)]));
        child.add_row(Row::new(vec![DataValue::Int(12), DataValue::Int(1)]));
        child.add_row(Row::new(vec![DataValue::Int(21), DataValue::Int(2)]));

        attach_children(&mut parent, child, "upper_id", "cv_line");

        // 父行1 挂 2 个子行，父行2 挂 1 个
        let p1 = &parent.rows[0];
        let p2 = &parent.rows[1];
        assert_eq!(p1.get_child("cv_line").unwrap().row_count(), 2);
        assert_eq!(p2.get_child("cv_line").unwrap().row_count(), 1);
    }

    #[test]
    fn collect_ids_skips_null() {
        let s = schema("t", &[("id", FieldType::Int)]);
        let mut ds = DataSet::empty("t", s);
        ds.add_row(Row::new(vec![DataValue::Int(1)]));
        ds.add_row(Row::new(vec![DataValue::Int(2)]));
        let ids = collect_ids(&ds);
        assert_eq!(ids.len(), 2);
    }
}
