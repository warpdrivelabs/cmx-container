//! [`HierService`] 适配 —— 让 DOC 业务单据成为 [`CmxMasterSlave`] 的可换后端服务。
//!
//! **依赖反转落地**：本模块 `impl cmx_master_slave::HierService for DocHierService`，把协调器接到
//! DOC 现成的加载（[`ZmcDocLoader::load`]）/ 保存（[`DocSaver::save`]）上——**零新存储逻辑**，全委托。
//!
//! DOC 是**形状 A**（异构 path-tree，childRows 嵌套）：装载返回嵌套 ZmcDataSet，协调器用
//! `from_zmc` 建树。写时上卷已由协调器在 `save_via` 里完成，本层只把中立 ChangeSet 翻成 DOC saver
//! 的 `changes` JSON 落库。DocMetaView 解析链统一复用 [`crate::resolve`]（读定义 + base + parse + 缓存），
//! 不依赖 cmx-api。

use async_trait::async_trait;
use cmx_database_pg::get_default_pg_db_manager;
use cmx_database_pg::zmcdataset::TokioPgRowSource;
use cmx_doc_model::DocQuery;
use cmx_master_slave::{ChangeSet, HierSchema, HierService, LoadQuery, SaveOutcome};
use cmx_rowsource::ZmcDataSet;
use serde_json::{json, Value};

use crate::saver::{SaveCtx, SaveMode};
use crate::{DocSaver, ZmcDocLoader};

/// DOC 业务单据的层级服务实现。持有定位坐标 + 数据源 id；每次调用解析 DocMetaView。
///
/// DAM 三段（domain/application/module）可选：缺失时由 [`resolve_doc_meta`] 按
/// `doc`(moduleCode) > `file` 全局反查补全（与 `/doc/*` HTTP 端点同一咽喉点）。
pub struct DocHierService {
    pub domain: Option<String>,
    pub application: Option<String>,
    pub module: Option<String>,
    pub file: Option<String>,
    /// 单据模块编码（moduleMeta.moduleCode）；前端走 code 定位时传，优先于 file。
    pub doc: Option<String>,
    pub db_id: String,
}

impl DocHierService {
    pub fn new(
        domain: Option<String>,
        application: Option<String>,
        module: Option<String>,
        db_id: impl Into<String>,
    ) -> Self {
        Self {
            domain,
            application,
            module,
            file: None,
            doc: None,
            db_id: db_id.into(),
        }
    }
}

#[async_trait]
impl HierService for DocHierService {
    type Row = TokioPgRowSource;

    async fn load(
        &self,
        _schema: &HierSchema,
        query_in: &LoadQuery,
    ) -> Result<ZmcDataSet<Self::Row>, String> {
        let (meta, _file) = crate::resolve::resolve_doc_meta(
            self.domain.as_deref(),
            self.application.as_deref(),
            self.module.as_deref(),
            self.file.as_deref(),
            self.doc.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let root_id = meta
            .root_layer()
            .map(|l| l.id.clone())
            .ok_or_else(|| "单据定义无根层".to_string())?;
        let mut dq = DocQuery::simple(&root_id, query_in.limit.map(|l| l as u64), query_in.depth);
        dq.count_total = query_in.count_total;
        let mm = get_default_pg_db_manager();
        ZmcDocLoader::load(mm, &self.db_id, &meta, &dq)
            .await
            .map_err(|e| e.to_string())
    }

    async fn expand(
        &self,
        _schema: &HierSchema,
        layer_path: &str,
        parent_ids: &[String],
    ) -> Result<ZmcDataSet<Self::Row>, String> {
        let (meta, _file) = crate::resolve::resolve_doc_meta(
            self.domain.as_deref(),
            self.application.as_deref(),
            self.module.as_deref(),
            self.file.as_deref(),
            self.doc.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        // layer_path 末段 = 目标层 id；懒下钻用 load_subtree。
        let layer_id = layer_path.rsplit('.').next().unwrap_or(layer_path);
        let pids: Vec<Value> = parent_ids.iter().map(|s| json!(s)).collect();
        let mm = get_default_pg_db_manager();
        let dq = DocQuery::simple(layer_id, None, None);
        ZmcDocLoader::load_subtree(mm, &self.db_id, &meta, layer_id, &pids, &dq)
            .await
            .map_err(|e| e.to_string())
    }

    async fn save(
        &self,
        _schema: &HierSchema,
        changes: &ChangeSet,
    ) -> Result<SaveOutcome, String> {
        let (meta, _file) = crate::resolve::resolve_doc_meta(
            self.domain.as_deref(),
            self.application.as_deref(),
            self.module.as_deref(),
            self.file.as_deref(),
            self.doc.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        // DocSaver::save 走 sqlx 管理器（与 doc-api 一致；装载走 tokio-pg，保存走 sqlx）。
        let mm = cmx_database::get_default_db_manager();
        // 中立 ChangeSet → DOC saver 的 changes JSON（协调器已写时上卷，承接字段就绪）。
        let changes_json = changes.to_json();
        let sctx = SaveCtx {
            actor_id: 0,
            actor_name: "系统".to_string(),
            doc_file: self.file.clone().unwrap_or_default(),
            op_override: None,
            code_rule_overrides: std::collections::HashMap::new(),
        };
        let res = DocSaver::save(mm, &self.db_id, &meta, SaveMode::Merge, &changes_json, &sctx)
            .await
            .map_err(|e| e.to_string())?;
        // SaveResult → 中立 SaveOutcome
        let updated_at = serde_json::to_value(&res.updated_at)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        Ok(SaveOutcome {
            affected: res.affected,
            id_map: res.id_map,
            updated_at,
        })
    }
}
