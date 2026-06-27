//! 插件函数调用 trait 定义。
//!
//! 定义跨模块的插件函数调用接口，抽象出 `invoke_plugin_function` 的协议无关调用链。
//! cmx-biz 的 `BizFunctionInvoker` 实现此 trait，cmx-rpc 等基础设施层通过此 trait
//! 调用插件函数而无需直接依赖业务层 cmx-biz，实现依赖倒置（infra → traits ← business）。

use async_trait::async_trait;
use cmx_core::model::service::SVRContext;
use serde_json::Value;

use crate::error::TraitError;

/// 插件函数调用的核心结果（协议无关）。
///
/// 封装 WASM 函数调用的执行结果，供 cmx-rpc / cmx-api 等协议层
/// 转换为各自的响应格式（protobuf / HTTP JSON）。
#[derive(Debug, Clone)]
pub struct FunctionInvokeResult {
    /// 是否执行成功
    pub success: bool,
    /// 函数执行结果（来自 FunctionOutput.result）
    pub result: Value,
    /// 执行耗时（微秒），来自 WasmInvokeResult.elapsed_us
    pub elapsed_us: u64,
    /// 错误信息（WASM 调用失败时包含）
    pub error: Option<String>,
    /// 调试信息（预留，调试模式下可能包含额外数据）
    pub debug: Option<Value>,
}

/// 插件函数调用器 trait。
///
/// 抽象出"检查安装 → 加载 WASM → 构建 FunctionInput → 序列化 → 调用 → 反序列化"
/// 的完整调用链，供 cmx-rpc 等基础设施层调用。
///
/// 实现方（如 cmx-biz 的 `BizFunctionInvoker`）持有 `RuntimeInvoker` 和 `PluginQuery`
/// 等运行时依赖，调用方仅通过此 trait 即可完成插件函数调用，无需感知底层依赖装配。
#[async_trait]
pub trait FunctionInvoker: Send + Sync {
    /// 调用插件函数。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 目标插件 ID。
    /// * `function_name` - 目标函数名。
    /// * `input` - 当前步骤输入数据（封装到 FunctionInput.input）。
    /// * `initial_input` - 初始输入数据（可选，调试场景传递服务最开始的入参）。
    /// * `svr_ctx` - 服务调用上下文（函数内部会设置其 initial_input 字段）。
    /// * `debug` - 是否调试模式。
    ///
    /// # Returns
    ///
    /// - `Err(TraitError)`：基础设施错误（插件未安装、WASM 加载失败、序列化失败等）。
    /// - `Ok(FunctionInvokeResult { success: false, ... })`：WASM 函数执行失败。
    /// - `Ok(FunctionInvokeResult { success: true, ... })`：调用成功。
    async fn invoke_plugin_function(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: Value,
        initial_input: Option<Value>,
        svr_ctx: SVRContext,
        debug: bool,
    ) -> Result<FunctionInvokeResult, TraitError>;
}
