//! cmx-runtime — WASM 运行时引擎
//!
//! 基于 Extism 的 WASM 运行时引擎，负责 WASM 模块的加载、实例化和调用。
//! 实现 cmx_traits::RuntimeInvoker trait。
//!
//! # 模块结构
//!
//! ```text
//! lib.rs              ← 本文件：crate 入口，模块声明和导出
//! engine.rs           ← 核心引擎：ExtismEngine + RuntimeInvoker 实现
//! config.rs           ← 引擎配置：ExtismEngineConfig + 缓存/Fuel 初始化
//! metrics.rs          ← 运行时指标：EngineMetrics（无锁原子计数器）
//! host_function.rs    ← 宿主函数桥接：HostFunctionContext + Extism 回调
//! error.rs            ← 错误类型：ExtismError
//! global.rs           ← 全局单例：GlobalExtismEngine
//! lifecycle_listener.rs ← 生命周期监听器
//! ```
//!
//! # 依赖关系
//!
//! - 依赖 cmx-traits（trait 定义）
//! - 依赖 cmx-utils（ConfigManager 配置读取）
//! - 依赖 extism（WASM 运行时）
//! - 被 cmx-service 依赖（通过 RuntimeInvoker trait）

pub mod config;
pub mod engine;
pub mod error;
pub mod global;
pub mod host_function;
pub mod lifecycle_listener;
pub mod metrics;

pub use config::ExtismEngineConfig;
pub use engine::{ExtismEngine};
pub use error::ExtismError;
pub use global::GlobalExtismEngine;
pub use lifecycle_listener::RuntimeLifecycleListener;
pub use metrics::EngineMetrics;
