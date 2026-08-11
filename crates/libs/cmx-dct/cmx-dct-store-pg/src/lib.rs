//! cmx-dct-store-pg —— 数据字典（DCT）模块的 PostgreSQL 持久化/服务层。
//!
//! 对外面（场景入口，一步到位，内部 `resolve_dict` 解析视图）：
//! - [`dict_meta`]：字典元数据文档（投影已下沉，可直接下发）。
//! - [`dict_search`] / [`dict_search_zmc`]：分页读 / 零拷贝列式读。
//! - [`dict_upsert`] / [`dict_delete`] / [`dict_save`]：回存 / 删除 / changeset 事务回存。
//!
//! 辅助类型：[`DctQuery`]（定位器，re-export 自 cmx-dct-model）/ [`DictMeta`] / [`SearchQuery`] /
//! [`SearchResult`] / [`Txn`] / [`UpsertOutcome`] / [`SaveOutcome`]。
//!
//! 层级服务适配：[`DctHierService`]（impl `cmx_master_slave::HierService`）。
//!
//! 流式导入导出：[`export_stream`] / [`import_stream`] / [`ImportFormat`] / [`ImportSummary`]。
//!
//! 内部模块（`resolve_dict` / `upsert` / `delete` / `save` / `DictView` 等）为 `pub(crate)`，
//! 不对外暴露——调用方走上面的场景函数，无需手动 resolve 或构建 DictView。
//! HTTP 信封由 cmx-dct-api 薄 handler 包装；SQL 全部来自 cmx-dct-model。

mod error;
mod hier_service;
mod hierarchy;
mod import_export;
mod meta;
mod query;
mod resolve;
mod write;

// —— 对外类型 ——
pub use cmx_dct_model::{BatchConflictMode, DctQuery};
pub use meta::{DictMeta, SearchQuery, SearchResult, Sort};
pub use write::{SaveOutcome, Txn, UpsertOutcome};

// —— 场景入口（一步到位）——
pub use query::{dict_search, dict_search_zmc};
pub use resolve::dict_meta;
pub use write::{dict_delete, dict_save, dict_upsert};

// —— 层级服务适配（保留，协调器挂载点）——
pub use hier_service::DctHierService;

// —— 错误助手 ——
pub use error::{api_err, api_err_db};

// —— 导入导出（对外）——
pub use import_export::{ImportError, ImportFormat, ImportSummary, export_stream, import_stream};
