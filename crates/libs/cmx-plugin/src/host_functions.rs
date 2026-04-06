//! WASM 宿主函数 — 插件间调用
//!
//! 为 WASM 插件提供插件间调用能力的宿主函数。
//! 通过 RuntimeInvoker trait 实现跨插件的服务调用。
//!
//! # 依赖关系
//!
//! PluginHostFunctions 通过 `Arc<dyn RuntimeInvoker>` 引用 WASM 运行时，
/// 不直接依赖 cmx-runtime crate，仅依赖 cmx-traits 中的 trait 定义。

use std::sync::Arc;

use cmx_traits::{CallerData, HostFuncError, HostFunctionProvider, HostFuncWrapper, RuntimeInvoker, WasmLinker};

/// 插件服务调用请求（JSON 反序列化）
#[derive(serde::Deserialize)]
struct ServiceCallRequest {
    /// 目标插件ID
    target_plugin_id: String,
    /// 目标函数名
    function_name: String,
    /// 输入数据（JSON 值）
    input: serde_json::Value,
}

/// 插件服务调用响应（JSON 序列化）
#[derive(serde::Serialize)]
struct ServiceCallResponse {
    /// 是否成功
    success: bool,
    /// 输出数据（JSON 值）
    output: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    elapsed_us: Option<u64>,
    /// 错误信息
    error: Option<String>,
}

/// 插件宿主函数提供者
///
/// 向 WASM 运行时注册插件间调用相关的宿主函数。
/// 通过 RuntimeInvoker trait 调用目标插件的 WASM 导出函数。
pub struct PluginHostFunctions {
    /// WASM 运行时调用器（trait 对象）
    runtime: Arc<dyn RuntimeInvoker>,
}

impl PluginHostFunctions {
    /// 创建插件宿主函数提供者
    ///
    /// # 参数
    ///
    /// * `runtime` - WASM 运行时调用器（trait 对象）
    pub fn new(runtime: Arc<dyn RuntimeInvoker>) -> Self {
        Self { runtime }
    }

    /// 构建成功响应
    fn ok_response(output: Option<serde_json::Value>, elapsed_us: Option<u64>) -> Vec<u8> {
        serde_json::to_vec(&ServiceCallResponse {
            success: true,
            output,
            elapsed_us,
            error: None,
        }).unwrap_or_default()
    }

    /// 构建错误响应
    fn err_response(msg: String) -> Vec<u8> {
        serde_json::to_vec(&ServiceCallResponse {
            success: false,
            output: None,
            elapsed_us: None,
            error: Some(msg),
        }).unwrap_or_default()
    }
}

impl HostFunctionProvider for PluginHostFunctions {
    /// 返回命名空间 "cmx:plugin"
    fn namespace(&self) -> &str {
        "cmx:plugin"
    }

    /// 注册插件操作宿主函数
    ///
    /// 注册以下函数：
    /// - `cmx:plugin/call_service` — 调用另一个插件的服务
    /// - `cmx:plugin/get_info` — 获取当前插件信息
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
        // cmx:plugin/call_service — 调用另一个插件的服务
        let runtime = self.runtime.clone();
        let call_fn: HostFuncWrapper = Box::new(move |caller, input| {
            let caller_data = caller.caller_data();

            let request = match serde_json::from_slice::<ServiceCallRequest>(input) {
                Ok(req) => req,
                Err(e) => return Ok(Self::err_response(format!("请求数据解析失败: {}", e))),
            };

            let mut target_caller_data = CallerData::new(&request.target_plugin_id, &caller_data.db_id)
                .with_request_id(&caller_data.request_id);
            if let Some(txn_id) = &caller_data.txn_id {
                target_caller_data = target_caller_data.with_txn_id(txn_id.as_str());
            }
            if let Some(tenant_id) = &caller_data.tenant_id {
                target_caller_data = target_caller_data.with_tenant_id(tenant_id.as_str());
            }

            let input_bytes = serde_json::to_vec(&request.input).unwrap_or_default();
            let runtime = runtime.clone();
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                runtime.invoke(
                    &request.target_plugin_id,
                    &request.function_name,
                    &input_bytes,
                    &target_caller_data,
                ).await
            });

            match result {
                Ok(invoke_result) => {
                    let output_json = if invoke_result.output.is_empty() {
                        None
                    } else {
                        serde_json::from_slice::<serde_json::Value>(&invoke_result.output).ok()
                    };
                    Ok(Self::ok_response(output_json, Some(invoke_result.elapsed_us)))
                }
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:plugin", "call_service", call_fn)?;

        // cmx:plugin/get_info — 获取当前插件信息
        let info_fn: HostFuncWrapper = Box::new(|caller, _input| {
            let caller_data = caller.caller_data();
            let info = serde_json::json!({
                "plugin_id": caller_data.plugin_id,
                "db_id": caller_data.db_id,
                "txn_id": caller_data.txn_id,
                "request_id": caller_data.request_id,
                "tenant_id": caller_data.tenant_id,
            });
            Ok(serde_json::to_vec(&info).unwrap_or_default())
        });
        linker.define("cmx:plugin", "get_info", info_fn)?;

        Ok(())
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec![
            "cmx:plugin/call_service",
            "cmx:plugin/get_info",
        ]
    }
}
