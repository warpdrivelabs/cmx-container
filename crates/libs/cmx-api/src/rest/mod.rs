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

pub mod param_doc;
pub mod handler;
pub mod header_parse;
pub mod tree;
pub use param_doc::{ListParamsDoc, PageParamsDoc,GetParamsDoc,UpdatePayloadDoc,DeletePayloadDoc};
pub use handler::{create, create_many, get_by_id, update, update_many, delete, list, page };
pub use tree::{TreeNode, TreeNodeData};
