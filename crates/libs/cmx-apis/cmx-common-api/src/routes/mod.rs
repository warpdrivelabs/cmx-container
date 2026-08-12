//! 通用 route 注册模块。
//!
//! 重构后：`traits`（ModuleRoutes）与 `macros`（CRUD 宏）已下沉到 cmx-api-core，
//! 此处 re-export 以保持 `crate::routes::traits::ModuleRoutes` 路径兼容。
//! `crud_handlers`（具体实体的宏调用）与 `routes_impl`（api_routes 聚合）仍留本 crate。

pub mod routes_impl;

// 从 cmx-api-core re-export 已迁出的通用部分（保持 crate::routes::traits 路径兼容）
pub use cmx_api_core::routes::{macros, traits};

