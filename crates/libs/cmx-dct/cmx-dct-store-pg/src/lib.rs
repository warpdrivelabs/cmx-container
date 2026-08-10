//! cmx-dct-store-pg —— 数据字典（DCT）模块的 PostgreSQL 持久化/服务层。
//!
//! 模块结构：
//! - [`resolve`]：从定义 JSON 解析目标字典表 → 强类型 `DictView`（合并 base 字段集 +
//!   构建/缓存落库校验规范 TableSpec）+ db_id 路由。
//! - [`query`]：`search`（分页读）/ `search_zmc`（零拷贝列式二进制）。
//! - [`write`]：`upsert`（merge 回存 + 铸号 + 列校验）/ `delete` / `save`（changeset 事务回存）。
//! - [`hierarchy`]：分级字典三字段（level_no / full_path / is_leaf）级联维护（内部）。
//! - [`error`]：错误助手（api_err / api_err_db / map_db_err）。
//! - [`import_export`]：流式导入导出服务。
//!
//! 每个服务返回纯数据 / 语义化结果枚举，HTTP 信封由 cmx-dct-api 薄 handler 包装。
//! SQL 全部来自 cmx-dct-model；本层接 cmx-database-pg 全局 manager 执行 + 事务编排。

mod error;
mod hier_service;
mod hierarchy;
mod import_export;
mod query;
mod resolve;
mod write;

pub use error::{api_err, api_err_db};
pub use hier_service::DctHierService;
pub use import_export::*;
pub use query::{search, search_zmc};
pub use resolve::resolve_dict;
pub use write::{UpsertOutcome, SaveOutcome, delete, save, upsert};
