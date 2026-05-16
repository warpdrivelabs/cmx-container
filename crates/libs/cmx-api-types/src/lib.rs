//! 通用 HTTP 类型库，统一 REST API 响应格式、错误处理、OpenAPI 文档参数和树形结构。
//!
//! 该 crate 提供了 REST API 开发中常用的类型定义，包括：
//! - [`ApiResp`] 统一响应格式，支持分页和无数据响应
//! - [`Error`] 和 [`ErrCode`] 统一错误码与 HTTP 状态码映射
//! - [`GetParamsDoc`]、[`ListParamsDoc`]、[`PageParamsDoc`] 等 OpenAPI 文档参数结构
//! - [`TreeNode`] 和 [`TreeNodeData`] 树形结构构建与序列化

pub mod api_response;
pub mod error;
pub mod param_doc;
pub mod tree;

pub use api_response::{ApiResp, Pagination, UnitResp};
pub use error::{ErrCode, Error, Result};
pub use param_doc::*;
pub use tree::{TreeNode, TreeNodeData};
