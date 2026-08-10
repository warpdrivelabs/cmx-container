/*
 * @Author: yqs
 * @Date: 2026-03-10 17:07:29
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-18 08:41:53
 */
//! cmx-api 模块
//!
//! 提供 Web API 开发所需的基础组件，包括错误处理、响应封装、中间件和通用 CRUD 框架。

pub mod middleware;
// pub mod error;
// pub mod api_response;

pub use cmx_api_types::{ApiResp, ErrCode, Error, Pagination, Result, UnitResp};
pub use cmx_api_types::{
    DeletePayloadDoc, GetParamsDoc, ListParamsDoc, PageParamsDoc, UpdatePayloadDoc,
};
pub use cmx_api_types::{TreeNode, TreeNodeData};

/// REST 协议层模块
pub mod rest;

/// 业务模型模块（自定义 HTTP Handler）
pub mod handlers;

/// 路由注册模块
pub mod routes;

/// 状态管理模块
pub mod app_state;

/// OpenAPI 文档模块
pub mod openapi;

/// 请求库路由：db_id 解析（doc/dct/code/mdm 等 API crate 共用）。
pub mod db_id;

/// msgpack 成功信封编码（doc/dct 列式二进制响应共用）。
pub mod msgpack;

/// 操作者身份提取（doc/mdm 等 API handler 共用）。
pub mod actor;

/// 校验失败响应构造（doc/dct 等 API handler 共用）。
pub mod validation;

pub use app_state::CmxAppState;
pub use openapi::ApiDoc;
pub use rest::handler::{create, create_many, delete, get_by_id, list, page, update, update_many};

// 注意：register_crud_routes 宏通过 #[macro_export] 自动导出到 crate 根目录
// 使用时直接通过 cmx_api::register_crud_routes! 访问
