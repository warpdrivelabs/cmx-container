//! RuntimeInvoker 适配器
//!
//! 将 Arc<RwLock<WasmEngine>> 包装为 Arc<dyn RuntimeInvoker>，
//! 用于依赖注入场景。

use std::sync::Arc;
use tokio::sync::RwLock;

use async_trait::async_trait;
use cmx_traits::{CallerData, RuntimeInvoker, WasmInvokeResult, TraitError};

use crate::engine::WasmEngine;

/// WASM 引擎调用器适配器
///
/// 包装 `Arc<RwLock<WasmEngine>>` 以实现 `RuntimeInvoker` trait。
/// 用于需要将引擎作为 trait 对象注入的场景。
pub struct WasmEngineInvokerAdapter {
    /// WASM 引擎引用
    engine: Arc<RwLock<WasmEngine>>,
}

impl WasmEngineInvokerAdapter {
    /// 创建新的适配器
    pub fn new(engine: Arc<RwLock<WasmEngine>>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl RuntimeInvoker for WasmEngineInvokerAdapter {
    /// 调用 WASM 函数
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError> {
        let engine = self.engine.read().await;
        engine.invoke(plugin_id, function_name, input, caller_data).await
    }

    /// 加载 WASM 模块
    async fn load_module(
        &self,
        plugin_id: &str,
        wasm_path: &std::path::Path,
    ) -> Result<(), TraitError> {
        let engine = self.engine.read().await;
        engine.load_module(plugin_id, wasm_path).await
    }

    /// 卸载 WASM 模块
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        let engine = self.engine.write().await;
        engine.unload_module(plugin_id).await
    }

    /// 检查模块是否已加载
    async fn is_loaded(&self, plugin_id: &str) -> bool {
        let engine = self.engine.read().await;
        engine.is_loaded(plugin_id).await
    }
}
