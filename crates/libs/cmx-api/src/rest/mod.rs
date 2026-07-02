/*
 * @Author: yqs
 * @Date: 2026-03-17 19:39:45
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-31 10:12:55
 */
//! REST 协议层
//!
//! 提供 REST API 的 Handler、参数解析和通用工具。

pub mod handler;
pub mod header_parse;
pub use cmx_api_types::{
    DeletePayloadDoc, GetParamsDoc, ListParamsDoc, PageParamsDoc, UpdatePayloadDoc,
};
pub use cmx_api_types::{TreeNode, TreeNodeData};
pub use handler::{create, create_many, delete, get_by_id, list, page, update, update_many};
