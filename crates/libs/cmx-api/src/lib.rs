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
pub mod error;
pub mod response;

/// 通用 CRUD 模块
pub mod crud;

/// REST 协议层模块
pub mod rest;

/// 业务模型模块
pub mod models;

/// 路由注册模块
pub mod routes;

/// 状态管理模块
pub mod state;

// 重新导出常用类型
pub use crud::{DbBmc, GenericCrudService, UpdateItem};
pub use rest::{PageParams, handler::{create, create_many, get_by_id, update, update_many, delete, list, page, UpdatePayload, DeletePayload}};
pub use error::{Error, Result};
pub use response::{ApiResp, Pagination};
pub use state::{CmxAppState, AppStateInner};

// 注意：register_crud_routes 宏通过 #[macro_export] 自动导出到 crate 根目录
// 使用时直接通过 cmx_api::register_crud_routes! 访问
