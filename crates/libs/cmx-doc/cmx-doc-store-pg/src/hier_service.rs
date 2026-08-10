//! [`HierService`] 适配 —— 让 DOC 业务单据成为 [`CmxMasterSlave`] 的可换后端服务。
//!
//! **依赖反转落地**：本模块 `impl cmx_master_slave::HierService for DocHierService`，把协调器接到
//! DOC 现成的加载（[`ZmcDocLoader::load`]）/ 保存（[`DocSaver::save`]）上——**零新存储逻辑**，全委托。
//!
//! DOC 是**形状 A**（异构 path-tree，childRows 嵌套）：装载返回嵌套 ZmcDataSet，协调器用
//! `from_zmc` 建树。写时上卷已由协调器在 `save_via` 里完成，本层只把中立 ChangeSet 翻成 DOC saver
//! 的 `changes` JSON 落库。DocMetaView 解析链复刻自 doc-api（经 cmx-model-meta 读定义 + parse），
//! 不依赖 cmx-api。

use async_trait::async_trait;
use cmx_database_pg::get_default_pg_db_manager;
use cmx_database_pg::zmcdataset::TokioPgRowSource;
use cmx_doc_model::{DocMetaView, DocQuery};
use cmx_master_slave::{ChangeSet, HierSchema, HierService, LoadQuery, SaveOutcome};
use cmx_rowsource::ZmcDataSet;
use serde_json::{json, Value};

use crate::saver::{SaveCtx, SaveMode};
use crate::{DocSaver, ZmcDocLoader};

/// DOC 业务单据的层级服务实现。持有定位坐标 + 数据源 id；每次调用解析 DocMetaView。
pub struct DocHierService {
    pub domain: String,
    pub application: String,
    pub module: String,
    pub file: Option<String>,
    /// 单据模块编码（moduleMeta.moduleCode）；前端走 code 定位时传，优先于 file。
    pub doc: Option<String>,
    pub db_id: String,
}

impl DocHierService {
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
            doc: None,
            db_id: db_id.into(),
        }
    }

    /// 解析 DocMetaView（复刻 doc-api resolve_doc_meta：读定义 + base + parse；不带缓存）。
    /// 定位优先级：doc(moduleCode) 精确定位 > file 显式指定 > 盲选默认。
    async fn resolve_meta(&self) -> Result<DocMetaView, String> {
        use cmx_model_meta::definitions::{resolve::resolve_doc_file, store};
        // 脏值视为缺失
        let clean_file = self.file.as_deref().filter(|v| !v.is_empty() && *v != "undefined" && *v != "null");
        let clean_doc = self.doc.as_deref().filter(|v| !v.is_empty() && *v != "undefined" && *v != "null");
        let file = match clean_doc {
            Some(d) => resolve_doc_file(&self.domain, &self.application, &self.module, Some(d))
                .await
                .map_err(|e| e.to_string())?,
            _ => match clean_file {
                Some(f) => f.to_string(),
                None => resolve_doc_file(&self.domain, &self.application, &self.module, None)
                    .await
                    .map_err(|e| e.to_string())?,
            },
        };
        let doc_ref = store::DefRef {
            domain: Some(self.domain.clone()),
            application: Some(self.application.clone()),
            app: Some(self.application.clone()),
            module: Some(self.module.clone()),
            file: Some(file),
            id: None,
            kind: None,
        };
        let doc = store::get_definition(&doc_ref).await.map_err(|e| e.to_string())?;
        let base = load_base(&doc).await;
        DocMetaView::parse(&doc, &base).map_err(|e| e.to_string())
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
        let meta = self.resolve_meta().await?;
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
        let meta = self.resolve_meta().await?;
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
        let meta = self.resolve_meta().await?;
        // DocSaver::save 走 sqlx 管理器（与 doc-api 一致；装载走 tokio-pg，保存走 sqlx）。
        let mm = cmx_database::get_default_db_manager();
        // 中立 ChangeSet → DOC saver 的 changes JSON（协调器已写时上卷，承接字段就绪）。
        let changes_json = changes.to_json();
        let sctx = SaveCtx {
            actor_id: 0,
            actor_name: "系统".to_string(),
            doc_file: self.file.clone().unwrap_or_default(),
            op_override: None,
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

/// 读 base 字段集（复刻 doc-api load_base：从 baseDocMetaRef.file 读；无则 Null）。
async fn load_base(doc: &Value) -> Value {
    use cmx_model_meta::definitions::store;
    let base_file = doc
        .get("baseDocMetaRef")
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str());
    let Some(base_file) = base_file else {
        return Value::Null;
    };
    let base_ref = store::DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(base_file.to_string()),
        id: None,
        kind: None,
    };
    store::get_definition(&base_ref).await.unwrap_or(Value::Null)
}
