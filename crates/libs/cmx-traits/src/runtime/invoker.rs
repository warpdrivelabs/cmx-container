//! WASM 运行时调用 trait 定义。
//!
//! 定义跨模块的 WASM 调用接口，cmx-runtime 的 WasmEngine 将实现此 trait，
//! cmx-service 等模块通过此 trait 调用 WASM 执行而无需直接依赖 cmx-runtime。

use std::path::Path;
use async_trait::async_trait;

use crate::error::TraitError;
use super::invoke_context::InvokeOptions;

/// WASM 调用结果。
///
/// 封装 WASM 函数调用的返回数据和执行元信息。
#[derive(Debug, Clone)]
pub struct WasmInvokeResult {
    /// 返回数据（字节）。
    pub output: Vec<u8>,

    /// 执行耗时（微秒）。
    pub elapsed_us: u64,

    /// 消耗的燃料（可选，仅在启用燃料计量时有效）。
    pub fuel_consumed: Option<u64>,
}

/// WASM 运行时调用 trait。
///
/// 供 cmx-service 等模块使用，用于调用 WASM 执行引擎。
/// cmx-runtime 的 WasmEngine 实现此 trait，实现跨模块解耦。
#[async_trait]
pub trait RuntimeInvoker: Send + Sync {
    /// 调用 WASM 模块的指定导出函数（使用默认选项）。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 目标插件 ID。
    /// * `function_name` - WASM 导出函数名。
    /// * `input` - 输入数据（字节），通常为 `FunctionInput` 的 JSON 序列化。
    ///
    /// # Returns
    ///
    /// 返回 WASM 函数的执行结果。
    ///
    /// # Errors
    ///
    /// 模块未加载、函数不存在或执行异常时返回 [`TraitError`]。
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
    ) -> Result<WasmInvokeResult, TraitError> {
        self.invoke_with_options(plugin_id, function_name, input, &InvokeOptions::default()).await
    }

    /// 带选项的 WASM 调用。
    ///
    /// 支持自定义超时时间和调用深度限制。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 目标插件 ID。
    /// * `function_name` - WASM 导出函数名。
    /// * `input` - 输入数据（字节）。
    /// * `options` - 调用选项（超时、深度限制等）。
    ///
    /// # Returns
    ///
    /// 返回 WASM 函数的执行结果。
    ///
    /// # Errors
    ///
    /// 模块未加载、函数不存在、执行异常或超时/深度超限时返回 [`TraitError`]。
    async fn invoke_with_options(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError>;

    /// 加载 WASM 模块到运行时。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件 ID，作为模块标识。
    /// * `wasm_path` - WASM 文件路径。
    ///
    /// # Errors
    ///
    /// 文件不存在或 WASM 编译失败时返回 [`TraitError`]。
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError>;

    /// 从运行时卸载 WASM 模块。
    ///
    /// 释放模块占用的资源。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 要卸载的插件 ID。
    ///
    /// # Errors
    ///
    /// 卸载失败时返回 [`TraitError`]。
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError>;

    /// 检查模块是否已加载。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件 ID。
    ///
    /// # Returns
    ///
    /// 已加载返回 `true`，否则返回 `false`。
    async fn is_loaded(&self, plugin_id: &str) -> bool;
}
