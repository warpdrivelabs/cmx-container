//! DocLoader — 业务单据数据装载（方案 §5.1，元数据驱动）
//!
//! 读单据定义（DocMetaView）→ 逐层查询 → 按 childKey 组装嵌套 DataSet。
//!
//! 算法（按层广度优先，父批量驱动子，避免 N+1）：
//!   1. 根层：`build_layer_select`（富条件/排序/分页）→ DataSet，收集 rootIds
//!   2. 逐层下钻：子层 `childKey = ANY($parentIds)` **+ 该层富条件** 一条 SQL 取全部子行
//!      → 按 childKey 分桶 → 挂到父行 _children[childId]
//!   3. 返回根层 DataSet（完整嵌套主从树）
//!
//! **通用性铁律**：SQL 全部由 `sql_builder::build_layer_select` 按 `LayerView`/`LayerQuery`
//! 元数据生成，本装载器不含任何单据专属列名/表名。每层查询指令来自 `DocQuery`（缺省默认）。

use std::collections::HashMap;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{DataSet, Row};
use cmx_database::DatabaseManager;

use super::meta::{DocMetaView, LayerView};
use super::query::DocQuery;
use super::sql_builder::build_layer_select;
use crate::{BizError, Result};

pub struct DocLoader;

impl DocLoader {
    /// 按定义 + 查询指令装载整棵单据树，返回根层嵌套 DataSet。
    pub async fn load(
        mm: &DatabaseManager,
        db_id: &str,
        meta: &DocMetaView,
        query: &DocQuery,
    ) -> Result<DataSet> {
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据定义无根层"))?;

        // 1. 根层查询
        let mut root_ds = Self::query_root(mm, db_id, root, query).await?;

        // 2. 逐层下钻（沿 layer_groups 递归挂子集）
        let max_depth = query.depth.unwrap_or(usize::MAX);
        Self::descend(mm, db_id, meta, root, &mut root_ds, 1, max_depth, query).await?;

        Ok(root_ds)
    }

    /// 根层：`build_layer_select`（该层 filter/orderBy/limit/offset/cursor）。
    async fn query_root(
        mm: &DatabaseManager,
        db_id: &str,
        layer: &LayerView,
        query: &DocQuery,
    ) -> Result<DataSet> {
        let lq = query.layer(&layer.id);
        let (sql, params) = build_layer_select(layer, &lq, None)?;
        mm.query_sql_with_datavalues(db_id, None, &sql, params, &layer.table_name)
            .await
            .map(|ds| rebind_schema(ds, layer))
            .map_err(|e| BizError::internal(format!("装载根层 {} 失败: {e}", layer.table_name)))
    }

    /// 递归下钻：按 layer_groups 推导父子层，**同父兄弟**并列装载所有子表。
    ///
    /// 注意：定义里 relations 的 parent/child 用「逻辑名」(headers/account_lines)，
    /// 而 schema 节点与表用「物理名」(cv_header/cv_acc_line)，两者不一致。
    /// 故这里**不按 relations 名字匹配层**，而是 `meta.child_layers(parent.id)` 取下一层组
    /// 全部兄弟表（parentTable 声明则精确匹配，否则整组回退）；childKey 由 `child_key_for` 给。
    /// 孙层只从每组**主表**（`is_primary_in_group`）继续下钻。汇总表不装载（不遍历 summaries）。
    async fn descend(
        mm: &DatabaseManager,
        db_id: &str,
        meta: &DocMetaView,
        parent_layer: &LayerView,
        parent_ds: &mut DataSet,
        cur_depth: usize,
        max_depth: usize,
        query: &DocQuery,
    ) -> Result<()> {
        if cur_depth >= max_depth {
            return Ok(());
        }
        let parent_ids = collect_ids(parent_ds);
        if parent_ids.is_empty() {
            return Ok(());
        }

        let child_key = meta.child_key_for(&parent_layer.id);
        let child_layers = if query.include_siblings {
            meta.child_layers(&parent_layer.id)
        } else {
            // 不装兄弟：只取该组主表
            meta.child_layers(&parent_layer.id)
                .into_iter()
                .filter(|l| meta.is_primary_in_group(&l.id))
                .collect()
        };

        // 同父兄弟：下一层组全部子表，各查一次、各挂一次（各吃自己的 LayerQuery）
        for child_layer in child_layers {
            let is_primary = meta.is_primary_in_group(&child_layer.id);
            // 兄弟表容错：非主表装载失败（元数据定义了但物理表未部署）→ 跳过 + 警告，不中断整单。
            let mut child_ds =
                match Self::query_children_by_key(mm, db_id, child_layer, &child_key, &parent_ids, query)
                    .await
                {
                    Ok(ds) => ds,
                    Err(e) if !is_primary => {
                        tracing::warn!(
                            "跳过并列兄弟子表 {}（装载失败，可能未物理部署）: {e}",
                            child_layer.table_name
                        );
                        continue;
                    }
                    Err(e) => return Err(e),
                };

            // 孙层：仅从该组主表继续下钻
            if is_primary {
                Box::pin(Self::descend(
                    mm, db_id, meta, child_layer, &mut child_ds, cur_depth + 1, max_depth, query,
                ))
                .await?;
            }

            attach_children(parent_ds, child_ds, &child_key, &child_layer.id);
        }
        Ok(())
    }

    /// 子层：`build_layer_select`（parent_scope = childKey ANY + 该层 filter/orderBy/分页）。
    async fn query_children_by_key(
        mm: &DatabaseManager,
        db_id: &str,
        layer: &LayerView,
        child_key: &str,
        parent_ids: &[DataValue],
        query: &DocQuery,
    ) -> Result<DataSet> {
        let lq = query.layer(&layer.id);
        let (sql, params) = build_layer_select(layer, &lq, Some((child_key, parent_ids)))?;
        mm.query_sql_with_datavalues(db_id, None, &sql, params, &layer.table_name)
            .await
            .map(|ds| rebind_schema(ds, layer))
            .map_err(|e| BizError::internal(format!("装载子层 {} 失败: {e}", layer.table_name)))
    }
}

// ─────────────────────── 组装辅助 ───────────────────────

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

    // 同父兄弟：两张子表各自 attach 到同一父，父行下两个子键都在
    #[test]
    fn attach_multiple_sibling_children() {
        let ps = schema("cv_acc_line", &[("id", FieldType::Int)]);
        let mut parent = DataSet::empty("cv_acc_line", ps);
        parent.add_row(Row::new(vec![DataValue::Int(1)]));
        parent.add_row(Row::new(vec![DataValue::Int(2)]));

        // 兄弟表 A：cv_aux_line
        let cs_a = schema(
            "cv_aux_line",
            &[("id", FieldType::Int), ("upper_id", FieldType::Int)],
        );
        let mut aux = DataSet::empty("cv_aux_line", cs_a);
        aux.add_row(Row::new(vec![DataValue::Int(101), DataValue::Int(1)]));
        aux.add_row(Row::new(vec![DataValue::Int(102), DataValue::Int(2)]));

        // 兄弟表 B：cv_cyzb_line
        let cs_b = schema(
            "cv_cyzb_line",
            &[("id", FieldType::Int), ("upper_id", FieldType::Int)],
        );
        let mut cyzb = DataSet::empty("cv_cyzb_line", cs_b);
        cyzb.add_row(Row::new(vec![DataValue::Int(201), DataValue::Int(1)]));

        // 各自 attach 到同一父（模拟 descend 里的兄弟循环）
        attach_children(&mut parent, aux, "upper_id", "cv_aux_line");
        attach_children(&mut parent, cyzb, "upper_id", "cv_cyzb_line");

        // 父行1 下同时有两个兄弟子键
        let p1 = &parent.rows[0];
        assert_eq!(p1.get_child("cv_aux_line").unwrap().row_count(), 1);
        assert_eq!(p1.get_child("cv_cyzb_line").unwrap().row_count(), 1);
        // 父行2 只有 cv_aux_line 有数据（cyzb 无 upper_id=2 的行）
        let p2 = &parent.rows[1];
        assert_eq!(p2.get_child("cv_aux_line").unwrap().row_count(), 1);
        assert!(p2.get_child("cv_cyzb_line").is_none());
    }
}
