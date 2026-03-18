//! REST 协议层
//!
//! 提供 REST API 的 Handler 和参数解析。

pub mod params;
pub mod handler;

pub use params::{PageParams};
pub use handler::{create, get_by_id, update, delete_by_id, list, page};
