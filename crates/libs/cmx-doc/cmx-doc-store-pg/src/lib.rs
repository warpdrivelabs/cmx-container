//! cmx-doc-store-pg —— 业务单据（DOC）模块的 PostgreSQL 持久化/服务层。
//!
//! - `loader`：DocLoader 装载（老 DataSet 全拷贝，sqlx 驱动）。
//! - `zmc_loader`：ZmcDataSet 零拷贝装载（双驱动泛型，tokio-postgres / sqlx 共一份算法）。
//! - `zmc_util`：ZmcExecutor trait + 共享辅助函数（field_type_to_zmc/rebind_schema/collect_ids 等）。
//! - `saver`：DocSaver 双模式回存（落库前列级校验 + 铸号 + 事务编排）。
//! - `revision`：DocRevision 单据版本化（快照/回滚，FOR UPDATE 防并发）。
//! - `cache`：DocMetaView 进程内缓存（DashMap + TTL 兜底）。
//!
//! 强类型 meta 由 cmx-doc-model 提供；本层接 cmx-database(-pg) 全局 manager 执行。

pub mod cache;
pub mod hier_service;
pub mod loader;
pub mod resolve;
pub mod revision;
pub mod saver;
pub mod zmc_loader;
pub mod zmc_util;

pub use hier_service::DocHierService;
pub use loader::DocLoader;
pub use resolve::{load_base, resolve_doc_file_smart, resolve_doc_meta};
pub use revision::{DocRevision, RevisionRecord};
pub use saver::{BatchItem, BatchOutcome, DocSaver, SaveCtx, SaveMode, SaveResult};
pub use zmc_loader::ZmcDocLoader;
// 老导出名保留为 type alias:ZmcDocLoaderSqlx 与 ZmcDocLoader 是同一 struct,
// 调用 ZmcDocLoaderSqlx::load(sqlx_mm, ...) 时编译器按 mm 类型推断 E = sqlx 驱动。
pub type ZmcDocLoaderSqlx = ZmcDocLoader;
// 同时提供 Pg 别名,语义对称(老代码用 ZmcDocLoader 指 tokio 版,新代码可用 ZmcDocLoaderPg 显式)。
pub type ZmcDocLoaderPg = ZmcDocLoader;

// 便捷再导出：把 cmx-doc-model 的中立模型透传到本层命名空间，让 cmx-doc-api
// 只需 `use cmx_doc_store_pg::{...}` 即可拿到 meta/query/formula/rule 全套（对标
// 迁移前 `cmx_biz::doc::*` 单一入口）。
pub use cmx_doc_model::{
    ColumnView, Cond, Cursor, DocMetaView, DocQuery, FValue, Filter, LayerQuery, LayerView,
    LevelGroup, Op, OrderBy, RelationView, Scope, SummaryView, ValidateResult, Violation,
    build_layer_select, eval_bool, eval_formula, json_to_datavalue, scope_from_json, validate,
};
