//! ZmcDocLoaderSqlx —— 业务单据零拷贝装载(**sqlx PostgreSQL** + ZmcDataSet 版)。
//!
//! 与 tokio 版 [`ZmcDocLoader`](super::zmc_loader::ZmcDocLoader) 算法完全一致(BFS 逐层、父批量
//! 驱动子、`childKey = ANY($ids)` 一条 SQL 取整层),**唯一差异是走 sqlx 驱动**:
//! - 走 `cmx-database` 的 `query_sql_zmc_*`,产 [`ZmcDataSet`](cmx_database::ZmcDataSet)
//!   (`ZmcDataSet<SqlxPgRowSource>`,持有原始 sqlx PgRow,零拷贝);
//! - 子集挂载为 [`ZmcChildGroup`](cmx_database::ZmcChildGroup):记录每个子行的父 id 字符串,
//!   由 ZmcDataSet 的列式二进制编码器在编码时按父 id 分桶(不复制行)。
//!
//! 老 DocLoader / 老 DataSet、以及 tokio 版 ZmcDocLoader 完全不动;本装载器供 sqlx 二进制新链路
//! (`/api/doc/data/sqlx-zmc-msgpack`)。辅助函数本就泛型于 `ZmcDataSet<R>`,此处 `R = SqlxPgRowSource`。

use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_database::{
    DatabaseManager as SqlxManager, ZmcChildGroup, ZmcColType, ZmcDataSet, ZmcRowSource, ZmcSchema,
};

use cmx_doc_model::meta::{DocMetaView, LayerView};
use cmx_doc_model::query::DocQuery;
use cmx_doc_model::sql_builder::build_layer_select;
use cmx_biz::{BizError, Result};

/// 零拷贝单据装载器(sqlx 侧)。SQL 全部由 `build_layer_select` 元数据驱动。
pub struct ZmcDocLoaderSqlx;

impl ZmcDocLoaderSqlx {
    /// 按定义 + 查询指令装载整棵单据树,返回根层 [`ZmcDataSet`](cmx_database::ZmcDataSet)。
    pub async fn load(
        mm: &SqlxManager,
        db_id: &str,
        meta: &DocMetaView,
        query: &DocQuery,
    ) -> Result<ZmcDataSet> {
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据定义无根层"))?;

        let mut root_ds = Self::query_root(mm, db_id, root, query).await?;

        let max_depth = query.depth.unwrap_or(usize::MAX);
        Self::descend(mm, db_id, meta, root, &mut root_ds, 1, max_depth, query).await?;

        Ok(root_ds)
    }

    /// 懒下钻：以 `layer_id` 为根，装载它在 `parent_ids`（JSON，按 childKey 列类型化）下的子树。
    pub async fn load_subtree(
        mm: &SqlxManager,
        db_id: &str,
        meta: &DocMetaView,
        layer_id: &str,
        parent_ids_json: &[serde_json::Value],
        query: &DocQuery,
    ) -> Result<ZmcDataSet> {
        let layer = meta
            .layer(layer_id)
            .ok_or_else(|| BizError::business(format!("层 {layer_id} 不在定义中")))?;
        let child_key = meta
            .child_key_for_child(layer_id)
            .ok_or_else(|| BizError::business(format!("层 {layer_id} 是根层，无父，不能懒下钻")))?;
        let parent_ids = typecast_ids(layer, &child_key, parent_ids_json)?;

        let mut ds = Self::query_children_by_key(mm, db_id, layer, &child_key, &parent_ids, query).await?;
        let max_depth = query.depth.unwrap_or(usize::MAX);
        if max_depth > 0 {
            Self::descend(mm, db_id, meta, layer, &mut ds, 1, max_depth, query).await?;
        }
        Ok(ds)
    }

    /// 根层:`build_layer_select`(该层 filter/orderBy/limit/offset/cursor)。
    async fn query_root(
        mm: &SqlxManager,
        db_id: &str,
        layer: &LayerView,
        query: &DocQuery,
    ) -> Result<ZmcDataSet> {
        let lq = query.layer(&layer.id);
        let (sql, params) = build_layer_select(layer, &lq, None)?;
        let ds = mm
            .query_sql_zmc_with_datavalues(db_id, &sql, params, &layer.table_name)
            .await
            .map_err(|e| BizError::internal(format!("装载根层 {} 失败: {e}", layer.table_name)))?;
        Ok(rebind_schema(ds, layer))
    }

    /// 递归下钻:按 layer_groups 推导父子层,**同父兄弟**并列装载所有子表(与老 DocLoader 同算法)。
    #[allow(clippy::too_many_arguments)]
    async fn descend(
        mm: &SqlxManager,
        db_id: &str,
        meta: &DocMetaView,
        parent_layer: &LayerView,
        parent_ds: &mut ZmcDataSet,
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
        let child_layers: Vec<&LayerView> = if query.include_siblings {
            meta.child_layers(&parent_layer.id)
        } else {
            meta.child_layers(&parent_layer.id)
                .into_iter()
                .filter(|l| meta.is_primary_in_group(&l.id))
                .collect()
        };

        // 同父兄弟:下一层组全部子表,各查一次、各挂一个 ZmcChildGroup(各吃自己的 LayerQuery)
        for child_layer in child_layers {
            let is_primary = meta.is_primary_in_group(&child_layer.id);
            let mut child_ds =
                match Self::query_children_by_key(mm, db_id, child_layer, &child_key, &parent_ids, query)
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

            if is_primary {
                Box::pin(Self::descend(
                    mm,
                    db_id,
                    meta,
                    child_layer,
                    &mut child_ds,
                    cur_depth + 1,
                    max_depth,
                    query,
                ))
                .await?;
            }

            let group = build_child_group(child_ds, &child_key, &child_layer.id)?;
            parent_ds.add_child_group(group);
        }
        Ok(())
    }

    /// 子层:`build_layer_select`(parent_scope = childKey ANY + 该层 filter/orderBy/分页)。
    async fn query_children_by_key(
        mm: &SqlxManager,
        db_id: &str,
        layer: &LayerView,
        child_key: &str,
        parent_ids: &[DataValue],
        query: &DocQuery,
    ) -> Result<ZmcDataSet> {
        let lq = query.layer(&layer.id);
        let (sql, params) = build_layer_select(layer, &lq, Some((child_key, parent_ids)))?;
        let ds = mm
            .query_sql_zmc_with_datavalues(db_id, &sql, params, &layer.table_name)
            .await
            .map_err(|e| BizError::internal(format!("装载子层 {} 失败: {e}", layer.table_name)))?;
        Ok(rebind_schema(ds, layer))
    }
}

// ─────────────────────── 组装辅助 ───────────────────────


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

/// 把父 id JSON 值按 childKey 列类型化成 DataValue（懒下钻 parent_ids 用）。
fn typecast_ids(
    layer: &LayerView,
    child_key: &str,
    ids: &[serde_json::Value],
) -> Result<Vec<DataValue>> {
    let col = layer
        .column(child_key)
        .ok_or_else(|| BizError::business(format!("子键 {child_key} 不在层 {}", layer.table_name)))?;
    ids.iter()
        .map(|v| cmx_doc_model::query::json_to_datavalue(col, v))
        .collect()
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
