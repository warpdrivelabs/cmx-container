//! cmx-api-core —— Web API 共享骨架层。
//!
//! 从 cmx-api 抽出的稳定骨架，供各域 `*-api` crate 依赖以暴露自己的 handler：
//! - `app_state`：`CmxAppState` / `IamState`（应用共享状态）
//! - `routes::traits`：`ModuleRoutes` trait（各模块路由注册契约）
//! - `rest`：通用 CRUD handler（create/list/page/get/update/delete）+ header 解析
//! - `middleware`：mw_auth / mw_context / mw_permission / mw_cors / mw_trace 等
//! - CRUD 宏：`declare_crud_handlers!` / `register_crud_routes!`
//!
//! ## 依赖方向（无环）
//! 各域 `*-api` crate（如 cmx-biz-api）→ 本 crate + 对应服务 crate；
//! 服务 crate（cmx-biz/cmx-iam/...）不反向依赖本 crate，故过渡期本 crate 持有
//! cmx-iam（IamState）/ cmx-storage（storage_service）也不成环。
//!
//! ## ApiResp/Result/Error re-export
//! 这三者定义在 `cmx-api-types`，本 crate re-export 之，使 CRUD 宏里的
//! `$crate::Error` / `$crate::ApiResp` / `$crate::Result`（`$crate` 解析到本 crate）
//! 自动生效，宏零改动；也使迁入的 rest/middleware 模块（原 `use crate::ApiResp`）
//! 零改动解析。

// —— 从 cmx-api 迁入的骨架模块 ——
pub mod actor;
pub mod app_state;
pub mod db_id;
pub mod middleware;
pub mod msgpack;
pub mod rest;
pub mod routes;

// re-export cmx-api-types 的响应/错误类型（宏 $crate::Error/ApiResp/Result 与
// 迁入模块 use crate::ApiResp 均据此零改动解析）。
pub use cmx_api_types::{
    ApiResp, ErrCode, Error, Pagination, Result, UnitResp,
};
pub use cmx_api_types::{
    DeletePayloadDoc, GetParamsDoc, ListParamsDoc, PageParamsDoc, UpdatePayloadDoc,
};
pub use cmx_api_types::{TreeNode, TreeNodeData};

// 顶层便捷 re-export
pub use app_state::CmxAppState;
pub use routes::traits::ModuleRoutes;

// 注意：register_crud_routes / declare_crud_handlers 宏通过 #[macro_export] 自动
// 导出到本 crate 根目录，外部用 cmx_api_core::declare_crud_routes! / declare_crud_handlers! 访问。
