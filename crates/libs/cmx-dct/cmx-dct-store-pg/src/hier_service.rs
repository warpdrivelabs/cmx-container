//! [`HierService`] 适配 —— 让 DCT 字典成为 [`CmxMasterSlave`] 的可换后端服务。
//!
//! **依赖反转落地**：本模块 `impl cmx_master_slave::HierService for DctHierService`，把协调器接到
//! DCT 现成的加载（[`query::search_zmc`] 的底层 SQL）/ 保存（[`write::save`]）上——**零新存储逻辑**，
//! 全委托。协调器换到 DCT = 用这个实现；换回别的服务 = 换实现。
//!
//! DCT 是**形状 B**（自引用单表 `parent_id`）：装载返回扁平 ZmcDataSet，协调器用
//! `from_zmc_self_ref` 建树。写时上卷已由协调器在 `save_via` 里完成，本层只把中立 ChangeSet
//! 翻成 DCT saver 的 `{saveMode, changes}` body 落库。

use async_trait::async_trait;
use cmx_database_pg::get_default_pg_db_manager;
use cmx_database_pg::zmcdataset::TokioPgRowSource;
use cmx_dct_model::DctQuery;
use cmx_master_slave::{ChangeSet, HierSchema, HierService, LoadQuery, SaveOutcome};
use cmx_rowsource::ZmcDataSet;
use serde_json::{json, Value};

use crate::query;
use crate::resolve::resolve_dict;
use crate::write::{self, SaveOutcome as DctSaveOutcome, Txn};

/// DCT 字典的层级服务实现。持有定位坐标 + 数据源 id；每次调用解析 DictView。
pub struct DctHierService {
    pub domain: String,
    pub application: String,
    pub module: String,
    pub file: Option<String>,
    pub db_id: String,
}

impl DctHierService {
    pub fn new(
        domain: impl Into<String>,
        application: impl Into<String>,
        module: impl Into<String>,
        db_id: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            application: application.into(),
            module: module.into(),
            file: None,
            db_id: db_id.into(),
        }
    }

    /// 由中立 schema 的根层表名解析出 DCT 的 DictView（dictCode = 根层 pk 所在表的 dictCode）。
    /// 这里以根层 `table` 反查 dictCode：DCT 定义里 dictMeta.tableName 即物理表，dict 即 dictCode。
    /// 约定 schema 根层的 `table` 存 dictCode（前端/接入方按此填），最贴 DCT 的 `dict` 参数。
    fn dict_query(&self, schema: &HierSchema) -> Result<DctQuery, String> {
        let dict = schema
            .roots()
            .into_iter()
            .next()
            .map(|l| l.table.clone())
            .ok_or_else(|| "schema 无根层".to_string())?;
        Ok(DctQuery {
            domain: Some(self.domain.clone()),
            application: Some(self.application.clone()),
            module: Some(self.module.clone()),
            file: self.file.clone(),
            dict,
            with_props: false,
        })
    }
}

#[async_trait]
impl HierService for DctHierService {
    type Row = TokioPgRowSource;

    async fn load(
        &self,
        schema: &HierSchema,
        query_in: &LoadQuery,
    ) -> Result<ZmcDataSet<Self::Row>, String> {
        let q = self.dict_query(schema)?;
        let view = resolve_dict(&q, false).await.map_err(|e| e.to_string())?;

        // 复用 DCT 的 SQL 构造（build_search_sql），走 manager 直接取 ZmcDataSet（不落 msgpack 编码）。
        let raw = load_query_to_raw(query_in);
        let (sql, _count_sql, params) = cmx_dct_model::build_search_sql(&view, &raw);
        let mm = get_default_pg_db_manager();
        mm.query_sql_zmc_with_datavalues(&self.db_id, &sql, params, &view.dict_code)
            .await
            .map_err(|e| e.to_string())
    }

    async fn expand(
        &self,
        schema: &HierSchema,
        _layer_path: &str,
        parent_ids: &[String],
    ) -> Result<ZmcDataSet<Self::Row>, String> {
        // 形状 B 下钻：build_search_sql 支持 parentId 过滤（raw.parentId）。
        let q = self.dict_query(schema)?;
        let view = resolve_dict(&q, false).await.map_err(|e| e.to_string())?;
        let raw = expand_raw(parent_ids);
        let (sql, _c, params) = cmx_dct_model::build_search_sql(&view, &raw);
        let mm = get_default_pg_db_manager();
        mm.query_sql_zmc_with_datavalues(&self.db_id, &sql, params, &view.dict_code)
            .await
            .map_err(|e| e.to_string())
    }

    async fn save(
        &self,
        schema: &HierSchema,
        changes: &ChangeSet,
    ) -> Result<SaveOutcome, String> {
        let q = self.dict_query(schema)?;
        let view = resolve_dict(&q, false).await.map_err(|e| e.to_string())?;

        // 中立 ChangeSet → DCT saver 的 body：{ saveMode:"merge", changes:{ <dictCode>: {...} } }
        // 协调器已做写时上卷，承接字段在 changes 里就绪。
        let body = json!({ "saveMode": "merge", "changes": changes.to_json() });
        let outcome = write::save(&view, &body, &self.db_id, Txn::Auto)
            .await
            .map_err(|e| e.to_string())?;

        match outcome {
            DctSaveOutcome::Ok {
                affected,
                updated_at,
                id_map,
            } => Ok(SaveOutcome {
                affected,
                id_map,
                updated_at,
            }),
            DctSaveOutcome::Conflict => Err("乐观锁冲突（409）".to_string()),
            DctSaveOutcome::Invalid(violations) => {
                Err(format!("校验未通过：{} 项", violations.len()))
            }
        }
    }
}

/// LoadQuery → DCT search 的 raw JSON（filters/limit/offset/q）。
fn load_query_to_raw(q: &LoadQuery) -> Value {
    let mut raw = serde_json::Map::new();
    if !q.root_filter.is_empty() {
        raw.insert("filters".into(), Value::Object(q.root_filter.clone()));
    }
    if let Some(l) = q.limit {
        raw.insert("limit".into(), json!(l));
    }
    if let Some(o) = q.offset {
        raw.insert("offset".into(), json!(o));
    }
    Value::Object(raw)
}

/// 构造 expand（懒下钻）喂 `build_search_sql` 的 raw JSON。
///
/// 键名必须是驼峰 `parentId`（`build_search_sql` 读 `raw.get("parentId")`）——
/// 此前误写成下划线 `parent_id`，过滤被静默忽略导致下钻返回全表。
/// 只取首个父 id：自分级下钻按单父展开；多 id 批量待接线时再扩展为 `IN (...)`。
fn expand_raw(parent_ids: &[String]) -> Value {
    let mut raw = serde_json::Map::new();
    if let Some(first) = parent_ids.first() {
        raw.insert("parentId".into(), json!(first));
    }
    Value::Object(raw)
}

// 保留对 query 模块的引用（避免 unused：expand/load 走 build_search_sql，query::search_zmc 是
// handler 侧编码入口，本适配不经它，但保持 use 以示同源）。
#[allow(unused_imports)]
use query as _dct_query;

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁键名：expand 的 raw 必须带驼峰 parentId，否则 build_search_sql 静默忽略过滤。
    #[test]
    fn expand_raw_uses_camel_case_parent_id_key() {
        let raw = expand_raw(&["42".into()]);
        assert_eq!(raw.get("parentId").unwrap().as_str(), Some("42"));

        // 无父 id → 整键省略（不过滤）
        let raw = expand_raw(&[]);
        assert!(raw.get("parentId").is_none());
    }
}
