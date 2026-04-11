//! cmx-runtime — WASM 运行时引擎
//!
//! 基于 Extism 的 WASM 运行时引擎，负责 WASM 模块的加载、实例化和调用。
//! 实现 cmx_traits::RuntimeInvoker trait。
//!
//! # 核心功能
//!
//! - **ExtismEngine** - WASM 运行时引擎，支持高并发调用
//! - **ExtismEngineConfig** - 引擎配置（超时、内存限制、实例池大小等）
//! - **GlobalExtismEngine** - 全局引擎单例管理器
//!
//! # 高并发架构
//!
//! 使用 Extism 内置的 `CompiledPlugin` + `Pool` 实现高性能实例池：
//!
//! ```text
//! ExtismEngine
//!   └── plugin_pools: RwLock<HashMap<String, Pool>>
//!         │
//!         └── Pool (每个 plugin_id 一个)
//!               ├── CompiledPlugin (预编译 WASM，避免重复编译)
//!               ├── 工厂函数 (从 CompiledPlugin 快速创建实例)
//!               └── 内置 Condvar 等待机制
//! ```
//!
//! # 多层防护机制
//!
//! 1. **调用深度限制** — 防止无限递归（默认最大 8 层）
//! 2. **循环检测** — 检测同一插件函数的递归调用（A.a → B.b → A.a）
//! 3. **Extism 原生超时** — 单次 plugin.call() 超时自动中断
//!
//! # 依赖关系
//!
//! - 依赖 cmx-traits（trait 定义）
//! - 依赖 extism（WASM 运行时）
//! - 被 cmx-service 依赖（通过 RuntimeInvoker trait）

pub mod engine;
pub mod error;
pub mod global;
pub mod lifecycle_listener;

pub use engine::{ExtismEngine, ExtismEngineConfig};
pub use error::ExtismError;
pub use global::GlobalExtismEngine;
pub use lifecycle_listener::RuntimeLifecycleListener;
