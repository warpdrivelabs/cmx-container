//! ZmcDocLoader —— 业务单据零拷贝装载(双驱动泛型版)。
//!
//! 与老 [`DocLoader`](super::loader::DocLoader) 算法一致(BFS 逐层、父批量驱动子、
//! `childKey = ANY($ids)` 一条 SQL 取整层),但走零拷贝 [`ZmcDataSet`] 持有原始 Row:
//! - **tokio-postgres** 路径:`ZmcDocLoader::load::<cmx_database_pg::DatabaseManager>(...)`
//! - **sqlx** 路径:`ZmcDocLoader::load::<cmx_database::DatabaseManager>(...)`
//!
//! 调用方传入 `mm: &E`(`E: ZmcExecutor`),编译器按 mm 类型推断 E,无需显式指定。
//! 子集挂载为 [`ZmcChildGroup`]:记录每个子行的父 id 字符串,由 ZmcDataSet 的列式
//! 二进制编码器在编码时按父 id 分桶(不复制行)。
//!
//! SQL 由 `cmx-doc-model::sql_builder` 生成;辅助函数见 [`super::zmc_util`]。

use cmx_biz::{BizError, Result};
use cmx_doc_model::meta::{DocMetaView, LayerView};
use cmx_doc_model::query::DocQuery;
use cmx_doc_model::sql_builder::{build_layer_count, build_layer_select};
use cmx_rowsource::{ZmcDataSet, ZmcRowSource};

use super::zmc_util::{
    ZmcExecutor, build_child_group, collect_ids, rebind_schema, typecast_ids,
};

/// 零拷贝单据装载器。泛型于驱动 `E: ZmcExecutor`(tokio-postgres 或 sqlx)。
///
/// 调用方式:外部传 `mm: &E`,编译器推断 E。对老 API 兼容:
/// - `ZmcDocLoader::load(tokio_mm, ...)` → E = cmx-database-pg::DatabaseManager
/// - `ZmcDocLoaderSqlx::load(sqlx_mm, ...)` → 通过 type alias,同样推断 E = cmx-database::DatabaseManager
pub struct ZmcDocLoader;

impl ZmcDocLoader {
    /// 按定义 + 查询指令装载整棵单据树,返回根层 [`ZmcDataSet`]。
    pub async fn load<E: ZmcExecutor>(
        mm: &E,
        db_id: &str,
        meta: &DocMetaView,
        query: &DocQuery,
    ) -> Result<ZmcDataSet<E::Row>> {
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据定义无根层"))?;

        let mut root_ds = Self::query_root::<E>(mm, db_id, root, query).await?;

        // 可选:根层 COUNT(*) —— 当 count_total=true 时多跑一条 COUNT,结果挂到 root_ds.total。
        if query.count_total {
            let (csql, cparams) = build_layer_count(root, &query.layer(&root.id))?;
            let count_ds = mm
                .query_sql_zmc_with_datavalues(db_id, &csql, cparams, "doc_count")
                .await
                .map_err(|e| {
                    BizError::internal(format!("装载根层 {} COUNT 失败: {e}", root.table_name))
                })?;
            if let Some(row0) = count_ds.rows.first()
                && let Some(n) = row0.get_i64(0)
            {
                root_ds.total = Some(n);
            }
        }

        let max_depth = query.depth.unwrap_or(usize::MAX);
        Self::descend::<E>(mm, db_id, meta, root, &mut root_ds, 1, max_depth, query).await?;

        Ok(root_ds)
    }

    /// 懒下钻:以 `layer_id` 为根,装载它在 `parent_ids`(JSON,按 childKey 列类型化)下的子树。
    ///
    /// 通用(元数据驱动):childKey 由 `child_key_for_child` 推导;该层查询取自
    /// `query.layer(layer_id)`;depth 从该层起算(`query.depth`)。返回以该层为根的
    /// ZmcDataSet(含 childRows)。
    pub async fn load_subtree<E: ZmcExecutor>(
        mm: &E,
        db_id: &str,
        meta: &DocMetaView,
        layer_id: &str,
        parent_ids_json: &[serde_json::Value],
        query: &DocQuery,
    ) -> Result<ZmcDataSet<E::Row>> {
        let layer = meta
            .layer(layer_id)
            .ok_or_else(|| BizError::business(format!("层 {layer_id} 不在定义中")))?;
        let child_key = meta
            .child_key_for_child(layer_id)
            .ok_or_else(|| BizError::business(format!("层 {layer_id} 是根层,无父,不能懒下钻")))?;
        let parent_ids = typecast_ids(layer, &child_key, parent_ids_json)?;

        let mut ds =
            Self::query_children_by_key::<E>(mm, db_id, layer, &child_key, &parent_ids, query)
                .await?;
        let max_depth = query.depth.unwrap_or(usize::MAX);
        if max_depth > 0 {
            Self::descend::<E>(mm, db_id, meta, layer, &mut ds, 1, max_depth, query).await?;
        }
        Ok(ds)
    }

    /// 根层:`build_layer_select`(该层 filter/orderBy/limit/offset/cursor)。
    async fn query_root<E: ZmcExecutor>(
        mm: &E,
        db_id: &str,
        layer: &LayerView,
        query: &DocQuery,
    ) -> Result<ZmcDataSet<E::Row>> {
        let lq = query.layer(&layer.id);
        let (sql, params) = build_layer_select(layer, &lq, None)?;
        let ds = mm
            .query_sql_zmc_with_datavalues(db_id, &sql, params, &layer.table_name)
            .await
            .map_err(|e| BizError::internal(format!("装载根层 {} 失败: {e}", layer.table_name)))?;
        Ok(rebind_schema(ds, layer))
    }

    /// 递归下钻:按 layer_groups 推导父子层,**同父兄弟**并列装载所有子表(与老 DocLoader 同算法)。
    ///
    /// 并列兄弟表(非 primary)装载失败时跳过(可能未物理部署),不阻断整棵树;
    /// primary 子表失败则上抛。日志按 AGENTS 第二章结构化字段规范。
    #[allow(clippy::too_many_arguments)]
    async fn descend<E: ZmcExecutor>(
        mm: &E,
        db_id: &str,
        meta: &DocMetaView,
        parent_layer: &LayerView,
        parent_ds: &mut ZmcDataSet<E::Row>,
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
            let mut child_ds = match Self::query_children_by_key::<E>(
                mm,
                db_id,
                child_layer,
                &child_key,
                &parent_ids,
                query,
            )
            .await
            {
                Ok(ds) => ds,
                Err(e) if !is_primary => {
                    // 并列兄弟表装载失败:可能该层未物理部署,跳过不阻断整棵树
                    tracing::warn!(
                        target: "cmx_doc::load",
                        layer_table = %child_layer.table_name,
                        error = %e,
                        "skip sibling layer (load failed, maybe not deployed)"
                    );
                    continue;
                }
                Err(e) => return Err(e),
            };

            // 仅主子表递归下钻下一层;并列兄弟表只挂载不递归
            if is_primary {
                Box::pin(Self::descend::<E>(
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
    async fn query_children_by_key<E: ZmcExecutor>(
        mm: &E,
        db_id: &str,
        layer: &LayerView,
        child_key: &str,
        parent_ids: &[cmx_core::model::cell::DataValue],
        query: &DocQuery,
    ) -> Result<ZmcDataSet<E::Row>> {
        let lq = query.layer(&layer.id);
        let (sql, params) = build_layer_select(layer, &lq, Some((child_key, parent_ids)))?;
        let ds = mm
            .query_sql_zmc_with_datavalues(db_id, &sql, params, &layer.table_name)
            .await
            .map_err(|e| BizError::internal(format!("装载子层 {} 失败: {e}", layer.table_name)))?;
        Ok(rebind_schema(ds, layer))
    }
}
