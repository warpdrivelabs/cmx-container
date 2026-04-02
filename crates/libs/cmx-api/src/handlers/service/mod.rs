//! 服务调用 Handler
//!
//! 提供 WASM 插件服务调用的 HTTP 接口。

pub mod handler;

pub use handler::{service_call, execute_orchestration};
