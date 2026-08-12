/*
 * @Author: yqs
 * @Date: 2026-03-10 17:07:29
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-18 08:41:53
 */
//! cmx-common-api —— 通用 API 层（原 cmx-api 重命名）。
//!
//! 重构后：共享骨架（CmxAppState / ModuleRoutes / rest / middleware / CRUD 宏）已下沉到
//! `cmx-api-core`。本 crate 通过 re-export 保持对外 API 兼容（`cmx_common_api::CmxAppState`、
//! `cmx_common_api::ModuleRoutes`、`cmx_common_api::rest::handler::create` 等仍可用），并保留
//! 剩余 handler（debug/portal/service）、路由聚合（routes/routes_impl.rs）、OpenAPI 文档（openapi.rs）。
//!
//! 命名：cmx-domain-api 分组目录已改名 cmx-apis（含 5 个域 api crate）；本 crate 因不再
//! 是唯一的 "api" crate，改名 cmx-common-api 以区分（common = 跨域通用 + 装配中枢）。

// —— 从 cmx-api-core re-export 共享骨架（保持 crate::xxx 与 cmx_api::xxx 双路径兼容）——
pub use cmx_api_core::app_state;
pub use cmx_api_core::middleware;
pub use cmx_api_core::rest;
// CRUD 宏（#[macro_export] 于 cmx_api_core 根，re-export 以便本 crate 内 `crate::宏!` 调用）
pub use cmx_api_core::{
    declare_crud_handlers, register_crud_handlers_module, register_crud_routes, setup_crud_api,
};

pub use cmx_api_types::{ApiResp, ErrCode, Error, Pagination, Result, UnitResp};
pub use cmx_api_types::{
    DeletePayloadDoc, GetParamsDoc, ListParamsDoc, PageParamsDoc, UpdatePayloadDoc,
};
pub use cmx_api_types::{TreeNode, TreeNodeData};

/// 业务模型模块（自定义 HTTP Handler）
pub mod handlers;

/// 路由注册模块（crud_handlers 宏调用 + routes_impl 聚合；traits/macros 从 core re-export）
pub mod routes;

/// OpenAPI 文档模块
pub mod openapi;

// db_id/msgpack/actor 已下沉 cmx-api-core；validation_fail_resp 移至 cmx-biz::errcode。

pub use cmx_api_core::CmxAppState;
pub use openapi::ApiDoc;
pub use cmx_api_core::rest::handler::{create, create_many, delete, get_by_id, list, page, update, update_many};
pub use cmx_api_core::ModuleRoutes;

// 注意：register_crud_routes 等宏现由 cmx-api-core 定义并 re-export，外部仍可用
// cmx_api::register_crud_routes! 访问。
