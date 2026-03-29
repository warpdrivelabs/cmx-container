//! REST 协议层
//!
//! 提供 REST API 的 Handler 和参数解析。

pub mod param_doc;
pub mod handler;
pub mod header_parse;
pub use param_doc::{ListParamsDoc, PageParamsDoc,GetParamsDoc,UpdatePayloadDoc,DeletePayloadDoc};
pub use handler::{create, create_many, get_by_id, update, update_many, delete, list, page };
