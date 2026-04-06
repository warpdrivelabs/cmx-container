//! WASM 运行时调用 trait 定义
//!
//! 定义跨模块的 WASM 调用接口，cmx-runtime 的 WasmEngine 将实现此 trait，
//! cmx-service 等模块通过此 trait 调用 WASM 执行而无需直接依赖 cmx-runtime。

use std::path::Path;
use async_trait::async_trait;

use crate::caller_data::CallerData;
use crate::error::TraitError;

/// WASM 调用结果
///
/// 封装 WASM 函数调用的返回数据和执行元信息。
#[derive(Debug, Clone)]
pub struct WasmInvokeResult {
    /// 返回数据（字节）
    pub output: Vec<u8>,

    /// 执行耗时（微秒）
    pub elapsed_us: u64,

    /// 消耗的燃料（可选，仅在启用燃料计量时有效）
    pub fuel_consumed: Option<u64>,
}

/// WASM 运行时调用 trait
///
/// 供 cmx-service 等模块使用，用于调用 WASM 执行引擎。
/// cmx-runtime 的 WasmEngine 实现此 trait，实现跨模块解耦。
#[async_trait]
pub trait RuntimeInvoker: Send + Sync {
    /// 调用 WASM 模块的指定导出函数
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 目标插件ID
    /// * `function_name` - WASM 导出函数名
    /// * `input` - 输入数据（字节）
    /// * `caller_data` - 调用者上下文
    ///
    /// # 返回值
    ///
    /// 返回 WASM 函数的执行结果。
    ///
    /// # 错误
    ///
    /// 模块未加载、函数不存在或执行异常时返回错误。
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError>;

    /// 加载 WASM 模块到运行时
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件ID，作为模块标识
    /// * `wasm_path` - WASM 文件路径
    ///
    /// # 错误
    ///
    /// 文件不存在或 WASM 编译失败时返回错误。
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError>;

    /// 从运行时卸载 WASM 模块
    ///
    /// 释放模块占用的资源。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 要卸载的插件ID
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError>;

    /// 检查模块是否已加载
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件ID
    async fn is_loaded(&self, plugin_id: &str) -> bool;
}
