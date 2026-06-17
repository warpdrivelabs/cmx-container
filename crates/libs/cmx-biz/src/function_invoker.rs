//! 插件函数调用核心逻辑
//!
//! 提取 cmx-api（HTTP）和 cmx-rpc（gRPC）中重复的插件函数调用核心链路，
//! 实现协议无关的统一调用入口。包含完整调用链：
//! 检查安装 → 加载 WASM → 构建 FunctionInput → 序列化 → 调用 → 反序列化。
//!
//! # 设计说明
//!
//! 本模块只负责"执行"环节，不涉及：
//! - 参数提取（由协议层从 HTTP/protobuf 中解析）
//! - SVRContext 构建（由协议层从 middleware 或请求中组装）
//! - 响应封装（由协议层转换为 JSON/protobuf 响应）
//!
//! # 调用结果处理
//!
//! - 基础设施错误（插件未安装、WASM 加载失败、序列化失败等）通过 `Err(BizError)` 返回
//! - WASM 函数执行失败通过 `Ok(FunctionInvokeResult { success: false, ... })` 返回，
//!   由调用方决定如何映射为协议级错误或业务级失败响应

use std::sync::Arc;

use cmx_core::model::service::{FunctionInput, FunctionOutput, SVRContext};
use cmx_traits::plugin::PluginQuery;
use cmx_traits::runtime::{InvokeOptions, RuntimeInvoker};
use serde_json::Value;

use crate::BizError;

/// 插件函数调用的核心结果（协议无关）
///
/// 封装 WASM 函数调用的执行结果，供 cmx-api / cmx-rpc 等协议层
/// 转换为各自的响应格式（HTTP JSON / protobuf）。
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

/// 插件函数调用的核心逻辑（协议无关）
///
/// 包含完整调用链：
/// 1. 检查插件安装状态（plugin_query.is_installed）
/// 2. 加载 WASM 模块（runtime.load_module if !runtime.is_loaded）
/// 3. 构建 FunctionInput（initial_input 选择 + SVRContext 设置）
/// 4. rmp_serde 序列化
/// 5. 构建 InvokeOptions 并调用 runtime.invoke_with_options
/// 6. 解析 FunctionOutput（rmp_serde 反序列化，带 JSON fallback）
///
/// # 参数
/// - `runtime`: WASM 运行时调用器
/// - `plugin_query`: 插件查询器
/// - `plugin_id`: 目标插件ID
/// - `function_name`: 目标函数名
/// - `input`: 当前步骤输入数据（封装到 FunctionInput.input）
/// - `initial_input`: 初始输入数据（可选，调试场景传递服务最开始的入参；
///   为 None 时使用 input 作为 initial_input）
/// - `svr_ctx`: 服务调用上下文（函数内部会设置其 initial_input 字段）
/// - `debug`: 是否调试模式
///
/// # 返回值
/// - `Err(BizError)`: 基础设施错误（插件未安装、WASM 加载失败、序列化失败等）
/// - `Ok(FunctionInvokeResult { success: false, ... })`: WASM 函数调用失败
/// - `Ok(FunctionInvokeResult { success: true, ... })`: 调用成功
pub async fn invoke_plugin_function(
    runtime: &Arc<dyn RuntimeInvoker>,
    plugin_query: &Arc<dyn PluginQuery>,
    plugin_id: &str,
    function_name: &str,
    input: Value,
    initial_input: Option<Value>,
    mut svr_ctx: SVRContext,
    debug: bool,
) -> Result<FunctionInvokeResult, BizError> {
    // ==================== 1. 检查插件安装状态 ====================

    let is_installed = plugin_query
        .is_installed(plugin_id)
        .await
        .map_err(|e| BizError::business(format!("检查插件安装状态失败: {}", e)))?;

    if !is_installed {
        return Err(BizError::not_found(format!("插件 {} 未安装", plugin_id)));
    }

    // ==================== 2. 加载 WASM 模块 ====================

    let is_loaded = runtime.is_loaded(plugin_id).await;

    if !is_loaded {
        let wasm_path = plugin_query
            .get_wasm_path(plugin_id)
            .await
            .map_err(|e| BizError::business(format!("获取 WASM 路径失败: {}", e)))?;

        runtime
            .load_module(plugin_id, &wasm_path)
            .await
            .map_err(|e| BizError::business(format!("加载 WASM 模块失败: {}", e)))?;
    }

    // ==================== 3. 构建 FunctionInput ====================

    // 调试时 initial_input 是服务最开始的入参；未提供时使用 input
    if let Some(init_input) = initial_input {
        svr_ctx.initial_input = init_input;
    } else {
        svr_ctx.initial_input = input.clone();
    }

    let func_input = FunctionInput::from_value(input, svr_ctx);

    // ==================== 4. rmp_serde 序列化 ====================

    let input_bytes = rmp_serde::to_vec(&func_input)
        .map_err(|e| BizError::business(format!("输入数据序列化失败: {}", e)))?;

    // ==================== 5. 调用 WASM 函数 ====================

    let invoke_options = InvokeOptions {
        debug,
        ..Default::default()
    };

    let invoke_result = runtime
        .invoke_with_options(plugin_id, function_name, &input_bytes, &invoke_options)
        .await;

    // ==================== 6. 解析调用结果 ====================

    match invoke_result {
        Ok(result) => {
            // rmp_serde 反序列化，带 JSON fallback
            let output: FunctionOutput = if result.output.is_empty() {
                FunctionOutput::new(Value::Null)
            } else {
                match rmp_serde::from_slice(&result.output) {
                    Ok(o) => o,
                    Err(e) => {
                        tracing::warn!(
                            target: "cmx_biz",
                            error = %e,
                            "rmp_serde 反序列化失败，尝试直接解析为 JSON"
                        );
                        FunctionOutput::new(
                            serde_json::from_slice(&result.output).unwrap_or(Value::Null),
                        )
                    }
                }
            };

            Ok(FunctionInvokeResult {
                success: true,
                result: output.result,
                elapsed_us: result.elapsed_us,
                error: None,
                debug: None,
            })
        }
        Err(e) => {
            tracing::error!(
                target: "cmx_biz",
                plugin_id = %plugin_id,
                function_name = %function_name,
                error = %e,
                "插件函数调用失败"
            );
            Ok(FunctionInvokeResult {
                success: false,
                result: Value::Null,
                elapsed_us: 0,
                error: Some(format!("WASM 调用失败: {}", e)),
                debug: None,
            })
        }
    }
}
