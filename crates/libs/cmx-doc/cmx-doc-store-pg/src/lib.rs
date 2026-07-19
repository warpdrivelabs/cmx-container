//! cmx-doc-store-pg —— 业务单据（DOC）模块的 PostgreSQL 持久化/服务层。
//!
//! - `loader`：DocLoader 装载（老 DataSet 全拷贝，sqlx 驱动）。
//! - `zmc_loader` / `zmc_loader_sqlx`：ZmcDataSet 零拷贝装载（tokio-postgres / sqlx）。
//! - `saver`：DocSaver 双模式回存（落库前列级校验 + 铸号 + 事务编排）。
//! - `revision`：DocRevision 单据版本化（快照/回滚）。
//! - `cache`：DocMetaView 进程内缓存。
//!
//! 强类型 meta 由 cmx-doc-model 提供；本层接 cmx-database(-pg) 全局 manager 执行。

pub mod cache;
pub mod loader;
pub mod revision;
pub mod saver;
pub mod zmc_loader;
pub mod zmc_loader_sqlx;

pub use loader::DocLoader;
pub use revision::DocRevision;
pub use saver::{BatchItem, BatchOutcome, DocSaver, SaveCtx, SaveMode, SaveResult};
pub use zmc_loader::ZmcDocLoader;
pub use zmc_loader_sqlx::ZmcDocLoaderSqlx;

// 便捷再导出：把 cmx-doc-model 的中立模型透传到本层命名空间，让 cmx-doc-api
// 只需 `use cmx_doc_store_pg::{...}` 即可拿到 meta/query/formula/rule 全套（对标
// 迁移前 `cmx_biz::doc::*` 单一入口）。
pub use cmx_doc_model::{
    build_layer_select, eval_bool, eval_formula, json_to_datavalue, scope_from_json, validate,
    ColumnView, Cond, Cursor, DocMetaView, DocQuery, FValue, Filter, LayerQuery, LayerView,
    LevelGroup, Op, OrderBy, RelationView, Scope, SummaryView, ValidateResult, Violation,
};
