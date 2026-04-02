//! cmx-runtime — WASM 运行时引擎
//!
//! 基于 wasmtime 的 WASM 运行时引擎，负责：
//! - 加载、编译和实例化 WASM 模块
//! - 管理宿主函数注册（通过 HostFunctionProvider trait）
//! - 调用 WASM 导出函数
//! - 管理模块生命周期
//!
//! # 模块结构
//!
//! - `engine` — WASM 引擎核心（WasmEngine）
//! - `instance` — WASM 实例包装（WasmInstance, WasmStoreData）
//! - `linker_adapter` — Linker 适配器（实现 cmx_traits::WasmLinker）
//! - `error` — 运行时错误类型
//!
//! # 关于 WasmCallerAccess
//!
//! WasmCallerAccess 的具体实现（InlineCallerAccess）位于 linker_adapter 模块内部，
//! 因为 wasmtime::Caller 的生命周期与 func_wrap 闭包绑定，无法创建独立的结构体。
//!
//! # 依赖约束
//!
//! cmx-runtime 仅依赖 cmx-core, cmx-traits, cmx-utils,
//! **不依赖** cmx-database, cmx-metadata, cmx-plugin, cmx-buffer, cmx-service.

pub mod engine;
pub mod error;
pub mod instance;
pub mod invoker_adapter;
pub mod linker_adapter;

// 导出核心类型
pub use engine::{WasmEngine, WasmEngineConfig};
pub use instance::{WasmInstance, WasmStoreData};
pub use error::RuntimeError;
pub use invoker_adapter::WasmEngineInvokerAdapter;

use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

/// 全局 WASM 引擎单例
///
/// 提供应用级别的单例访问，确保整个应用共享同一个 WasmEngine 实例。
pub struct GlobalWasmEngine;

static GLOBAL_WASM_ENGINE: OnceLock<Arc<RwLock<WasmEngine>>> = OnceLock::new();

impl GlobalWasmEngine {
    /// 初始化全局 WASM 引擎
    ///
    /// # 参数
    ///
    /// * `config` - 引擎配置
    ///
    /// # 错误
    ///
    /// 如果已经初始化或引擎创建失败，返回错误。
    pub fn initialize(config: WasmEngineConfig) -> Result<(), RuntimeError> {
        let engine = WasmEngine::new(config)?;

        GLOBAL_WASM_ENGINE
            .set(Arc::new(RwLock::new(engine)))
            .map_err(|_| RuntimeError::Internal("全局 WASM 引擎已初始化".to_string()))?;

        tracing::info!("全局 WASM 引擎初始化完成");
        Ok(())
    }

    /// 获取全局 WASM 引擎读锁
    ///
    /// # Panics
    ///
    /// 如果未初始化则 panic。
    pub async fn get() -> tokio::sync::RwLockReadGuard<'static, WasmEngine> {
        let arc = GLOBAL_WASM_ENGINE
            .get()
            .expect("WASM 引擎未初始化，请先调用 GlobalWasmEngine::initialize()");
        arc.read().await
    }

    /// 获取全局 WASM 引擎写锁
    ///
    /// # Panics
    ///
    /// 如果未初始化则 panic。
    pub async fn get_mut() -> tokio::sync::RwLockWriteGuard<'static, WasmEngine> {
        let arc = GLOBAL_WASM_ENGINE
            .get()
            .expect("WASM 引擎未初始化，请先调用 GlobalWasmEngine::initialize()");
        arc.write().await
    }

    /// 获取全局 WASM 引擎 Arc 引用
    ///
    /// # Panics
    ///
    /// 如果未初始化则 panic。
    pub fn get_arc() -> Arc<RwLock<WasmEngine>> {
        GLOBAL_WASM_ENGINE
            .get()
            .expect("WASM 引擎未初始化，请先调用 GlobalWasmEngine::initialize()")
            .clone()
    }

    /// 获取全局 WASM 引擎作为 RuntimeInvoker trait 对象
    ///
    /// 返回 `Arc<dyn RuntimeInvoker>`，可直接用于依赖注入。
    pub fn get_as_invoker() -> Arc<dyn cmx_traits::RuntimeInvoker> {
        let arc = GLOBAL_WASM_ENGINE
            .get()
            .expect("WASM 引擎未初始化，请先调用 GlobalWasmEngine::initialize()")
            .clone();
        Arc::new(WasmEngineInvokerAdapter::new(arc))
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_WASM_ENGINE.get().is_some()
    }

    /// 尝试获取全局 WASM 引擎写锁（非异步）
    ///
    /// # 返回值
    ///
    /// 如果已初始化返回 Some，否则返回 None。
    pub fn try_get_mut() -> Option<tokio::sync::RwLockWriteGuard<'static, WasmEngine>> {
        let arc = GLOBAL_WASM_ENGINE.get()?;
        // 使用 try_write 避免阻塞
        arc.try_write().ok()
    }
}
