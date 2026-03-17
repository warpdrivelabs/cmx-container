//! cmx-api 模块
//!
//! 提供 Web API 开发所需的基础组件，包括错误处理、响应封装、中间件和通用 CRUD 框架。

pub mod middleware;
pub mod error;
pub mod response;
pub mod api;

/// 通用 CRUD 模块
pub mod crud;

/// REST 协议层模块
pub mod rest;

/// 业务模型模块
pub mod models;

// 重新导出常用类型
pub use crud::{DbBmc, GenericCrudService};
pub use rest::{PageParams, create, get_by_id, update, delete_by_id, list, page};
pub use error::{Error, Result};
pub use response::{ApiResp, Pagination};
