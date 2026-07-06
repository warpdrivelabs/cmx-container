//! ZmcDocLoader —— 业务单据零拷贝装载(tokio-postgres + ZmcDataSet 版)。
//!
//! 与老 [`DocLoader`](super::loader::DocLoader) 算法一致(BFS 逐层、父批量驱动子、
//! `childKey = ANY($ids)` 一条 SQL 取整层),但:
//! - 走 `cmx-database-pg` 的 `query_sql_zmc_*`,产 [`ZmcDataSet`](cmx_database_pg::ZmcDataSet)
//!   (持有原始 tokio-postgres Row,零拷贝);
//! - 子集挂载为 [`ZmcChildGroup`](cmx_database_pg::ZmcChildGroup):记录每个子行的父 id 字符串,
//!   由 ZmcDataSet 的列式二进制编码器在编码时按父 id 分桶(不复制行)。
//!
//! 老 DocLoader / 老 DataSet 完全不动;本装载器仅供二进制新链路。

use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_database_pg::{DatabaseManager as PgManager, ZmcChildGroup, ZmcColType, ZmcDataSet, ZmcRowSource, ZmcSchema};

use super::loader::LoadOptions;
use super::meta::{DocMetaView, LayerView};
use crate::{BizError, Result};

/// 零拷贝单据装载器。
pub struct ZmcDocLoader;

impl ZmcDocLoader {
    /// 按定义装载整棵单据树,返回根层 [`ZmcDataSet`](cmx_database_pg::ZmcDataSet)(含子集分组)。
    pub async fn load(
        mm: &PgManager,
        db_id: &str,
        meta: &DocMetaView,
        opts: &LoadOptions,
    ) -> Result<ZmcDataSet> {
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据定义无根层"))?;

        // 1. 根层查询
        let mut root_ds = Self::query_root(mm, db_id, root, opts).await?;

        // 2. 逐层下钻
        let max_depth = opts.depth.unwrap_or(usize::MAX);
        Self::descend(mm, db_id, meta, root, &mut root_ds, 1, max_depth).await?;

        Ok(root_ds)
    }

    /// 根层:SELECT 列 FROM 表 [WHERE filter] [ORDER BY id] [LIMIT n]。
    async fn query_root(
        mm: &PgManager,
        db_id: &str,
        layer: &LayerView,
        opts: &LoadOptions,
    ) -> Result<ZmcDataSet> {
        let cols = column_list(layer);
        let mut sql = format!("SELECT {cols} FROM {}", layer.table_name);
        let mut params: Vec<DataValue> = Vec::new();

        if !opts.root_filter.is_empty() {
            let mut conds = Vec::new();
            for (i, (col, val)) in opts.root_filter.iter().enumerate() {
                if layer.schema.get_index(col).is_none() {
                    return Err(BizError::business(format!(
                        "过滤列 {col} 不在层 {} 中",
                        layer.table_name
                    )));
                }
                conds.push(format!("{col} = ${}", i + 1));
                params.push(val.clone());
            }
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        if layer.schema.get_index("id").is_some() {
            sql.push_str(" ORDER BY id");
        }
        if let Some(n) = opts.root_limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }

        let ds = mm
            .query_sql_zmc_with_datavalues(db_id, &sql, params, &layer.table_name)
            .await
            .map_err(|e| BizError::internal(format!("装载根层 {} 失败: {e}", layer.table_name)))?;
        Ok(rebind_schema(ds, layer))
    }

    /// 递归下钻:按 layer_groups 推导父子层,**同父兄弟**并列装载所有子表(与老 DocLoader 同算法)。
    async fn descend(
        mm: &PgManager,
        db_id: &str,
        meta: &DocMetaView,
        parent_layer: &LayerView,
        parent_ds: &mut ZmcDataSet,
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

        let child_key = meta.child_key_for(&parent_layer.id);

        // 同父兄弟:下一层组全部子表,各查一次、各挂一个 ZmcChildGroup
        for child_layer in meta.child_layers(&parent_layer.id) {
            let is_primary = meta.is_primary_in_group(&child_layer.id);
            // 兄弟表容错:非主表装载失败(元数据定义了但物理表未部署)→ 跳过 + 警告,不中断整单装载。
            let mut child_ds =
                match Self::query_children_by_key(mm, db_id, child_layer, &child_key, &parent_ids)
                    .await
                {
                    Ok(ds) => ds,
                    Err(e) if !is_primary => {
                        tracing::warn!(
                            "跳过并列兄弟子表 {}(装载失败,可能未物理部署): {e}",
                            child_layer.table_name
                        );
                        continue;
                    }
                    Err(e) => return Err(e),
                };

            // 孙层:仅从该组主表继续下钻
            if is_primary {
                Box::pin(Self::descend(
                    mm,
                    db_id,
                    meta,
                    child_layer,
                    &mut child_ds,
                    cur_depth + 1,
                    max_depth,
                ))
                .await?;
            }

            // 计算每个子行的父 id 字符串(childKey 列),组成 ZmcChildGroup 挂到父层
            let group = build_child_group(child_ds, &child_key, &child_layer.id)?;
            parent_ds.add_child_group(group);
        }
        Ok(())
    }

    /// 子层:SELECT 列 FROM 子表 WHERE childKey = ANY($1) ORDER BY childKey[, line_no]。
    async fn query_children_by_key(
        mm: &PgManager,
        db_id: &str,
        layer: &LayerView,
        child_key: &str,
        parent_ids: &[DataValue],
    ) -> Result<ZmcDataSet> {
        let cols = column_list(layer);
        let mut sql = format!(
            "SELECT {cols} FROM {} WHERE {} = ANY($1)",
            layer.table_name, child_key
        );
        let has_line_no = layer.schema.get_index("line_no").is_some();
        if has_line_no {
            sql.push_str(&format!(" ORDER BY {}, line_no", child_key));
        } else {
            sql.push_str(&format!(" ORDER BY {}", child_key));
        }

        let params = vec![DataValue::Array(parent_ids.to_vec())];
        let ds = mm
            .query_sql_zmc_with_datavalues(db_id, &sql, params, &layer.table_name)
            .await
            .map_err(|e| BizError::internal(format!("装载子层 {} 失败: {e}", layer.table_name)))?;
        Ok(rebind_schema(ds, layer))
    }
}

// ─────────────────────── 组装辅助 ───────────────────────

/// 逗号分隔列名(按定义 schema 顺序)。
fn column_list(layer: &LayerView) -> String {
    layer
        .schema
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// 空表兜底:查询推断的 schema 若列数不足(空结果集 → 0 列),用定义 schema 的列名 + 推断
/// PG 类型覆盖,保证前端拿到正确表头。
///
/// 非空结果集直接返回(schema 已由首行的真实列类型推导,精度更好)。
fn rebind_schema(ds: ZmcDataSet, layer: &LayerView) -> ZmcDataSet {
    if ds.schema.col_count() == layer.schema.fields.len() && !ds.is_empty() {
        return ds; // 列数吻合且有行,用真实类型
    }
    // 空表或列数不符:用定义 schema 的列名 + FieldType→ZmcColType 覆盖
    let columns: Vec<String> = layer.schema.fields.iter().map(|f| f.name.clone()).collect();
    let types: Vec<ZmcColType> = layer
        .schema
        .fields
        .iter()
        .map(|f| field_type_to_zmc(&f.field_type))
        .collect();
    let schema = Arc::new(ZmcSchema::from_parts(columns, types));
    ZmcDataSet::with_schema(ds.id.clone(), schema, ds.rows)
}

/// 老 FieldType → 中立 `ZmcColType`(空表兜底 schema 用;有行时不走此路)。
fn field_type_to_zmc(ft: &cmx_core::model::cell::FieldType) -> ZmcColType {
    use cmx_core::model::cell::FieldType as F;
    match ft {
        F::Int => ZmcColType::Int8,
        F::Float => ZmcColType::Float8,
        F::Decimal => ZmcColType::Numeric,
        F::Bool => ZmcColType::Bool,
        F::DateTime => ZmcColType::Timestamptz,
        F::Date => ZmcColType::Date,
        F::Uuid => ZmcColType::Uuid,
        F::Binary => ZmcColType::Bytea,
        F::Json => ZmcColType::Jsonb,
        // String/Text/Array/Unknown 一律当文本(空表无实际值,仅占位表头)
        _ => ZmcColType::Text,
    }
}

/// 从根/父 ZmcDataSet 收集 "id" 列值(作为子层 ANY 参数)。
fn collect_ids(ds: &ZmcDataSet) -> Vec<DataValue> {
    let Some(id_idx) = ds.schema.col_index("id") else {
        return Vec::new();
    };
    let ty = ds.schema.types[id_idx];
    let mut out = Vec::with_capacity(ds.row_count());
    for row in 0..ds.row_count() {
        if let Some(dv) = id_datavalue(ds, row, id_idx, ty) {
            out.push(dv);
        }
    }
    out
}

/// 取 id 列的 DataValue(用于 ANY 参数绑定;仅取常见 id 类型 int/text/uuid)。
fn id_datavalue(ds: &ZmcDataSet, row: usize, col: usize, ty: ZmcColType) -> Option<DataValue> {
    let r = ds.rows.get(row)?;
    match ty {
        ZmcColType::Int2 => r.get_i16(col).map(|v| DataValue::Int(v as i64)),
        ZmcColType::Int4 => r.get_i32(col).map(|v| DataValue::Int(v as i64)),
        ZmcColType::Int8 => r.get_i64(col).map(DataValue::Int),
        ZmcColType::Uuid => r.get_uuid(col).map(DataValue::Uuid),
        _ => r.get_str(col).map(|s| DataValue::String(s.to_string())),
    }
}

/// 把子 ZmcDataSet 组装成 ZmcChildGroup:逐子行取 childKey 列的父 id 字符串。
fn build_child_group(child: ZmcDataSet, child_key: &str, child_id: &str) -> Result<ZmcChildGroup> {
    let key_idx = child.schema.col_index(child_key);
    let mut parent_ids = Vec::with_capacity(child.row_count());
    for row in 0..child.row_count() {
        let pid = match key_idx {
            Some(ci) => child.row_key_string(row, ci).unwrap_or_default(),
            None => String::new(),
        };
        parent_ids.push(pid);
    }
    Ok(ZmcChildGroup {
        child_key: child_id.to_string(),
        child,
        parent_ids,
    })
}
