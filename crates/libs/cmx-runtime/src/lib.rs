//! cmx-runtime — WASM 运行时引擎
//!
//! 基于 Extism 的 WASM 运行时引擎，负责 WASM 模块的加载、实例化和调用。
//! 实现 cmx_traits::RuntimeInvoker trait。

pub mod engine;
pub mod error;
pub mod global;

pub use engine::{ExtismEngine, ExtismEngineConfig};
pub use error::ExtismError;
pub use global::GlobalExtismEngine;
