//! WASM 宿主函数 — 插件间调用
//!
//! 为 WASM 插件提供插件间调用能力的宿主函数。
//! 通过 GlobalRuntime 访问 WASM 运行时，实现跨插件的服务调用。
//!
//! # 安全机制
//!
//! 插件间调用自动受到三层防护：
//! 1. 调用深度限制（默认 8 层）
//! 2. 循环检测（检测 A→B→A 循环调用）
//! 3. Extism 原生超时（默认 30 秒）

use tracing::{info, warn};
use cmx_traits::{HostFunctionProvider, HostFuncError, HostFunctionDef, ValType, GlobalRuntime, WasmInvokeResult, InvokeOptions};

/// 插件宿主函数提供者
///
/// 向 WASM 运行时注册插件间调用相关的宿主函数。
/// 通过 GlobalRuntime 访问 WASM 运行时，调用目标插件的 WASM 导出函数。
pub struct PluginHostFunctions;

impl PluginHostFunctions {
    /// 创建插件宿主函数提供者
    pub fn new() -> Self {
        Self
    }

    /// 构建 CallerData（简化版本）
    fn build_caller_data() -> cmx_traits::CallerData {
        cmx_traits::CallerData::new("default", "default")
    }

    /// 执行插件间调用
    ///
    /// 通过 GlobalRuntime 调用目标插件的 WASM 导出函数。
    /// 自动受到深度限制、循环检测和超时控制的三层防护。
    ///
    /// 注意：此函数在 spawn_blocking 线程中被调用（因为宿主函数回调
    /// 在 plugin.call() 的执行线程中），所以可以直接使用 block_on。
    fn do_call_service(&self, input: String) -> Result<String, HostFuncError> {
        let req: cmx_core::wasm_types::PluginCallRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response(format!("解析请求失败: {}", e))),
        };
        info!("插件间调用: target={}, function={}", req.target_plugin_id, req.function_name);

        let runtime = GlobalRuntime::get();
        let input_bytes = req.input.as_bytes();
        let caller_data = Self::build_caller_data();
        let options = InvokeOptions::default();

        // 当前已在 spawn_blocking 线程中，直接使用 block_on 调用 async 方法
        // invoke_with_options 内部会再次 spawn_blocking 执行 plugin.call()
        let rt = tokio::runtime::Handle::current();
        let result: Result<WasmInvokeResult, _> = rt.block_on(async {
            runtime.invoke_with_options(
                &req.target_plugin_id,
                &req.function_name,
                input_bytes,
                &caller_data,
                &options,
            ).await
        });

        match result {
            Ok(invoke_result) => {
                let output = if invoke_result.output.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&invoke_result.output).to_string())
                };
                Ok(Self::ok_response(output, Some(invoke_result.elapsed_us)))
            }
            Err(e) => {
                warn!("插件间调用失败: target={}, function={}, error={}",
                    req.target_plugin_id, req.function_name, e);
                Ok(Self::err_response(e.to_string()))
            }
        }
    }

    /// 获取插件信息
    fn do_get_info(&self, _input: String) -> Result<String, HostFuncError> {
        let info = cmx_core::wasm_types::PluginInfoResponse {
            plugin_id: "current_plugin".to_string(),
            db_id: "default".to_string(),
            txn_id: None,
            request_id: "default".to_string(),
            tenant_id: None,
        };
        Ok(serde_json::to_string(&info).unwrap_or_default())
    }

    /// 构建成功响应
    fn ok_response(output: Option<String>, elapsed_us: Option<u64>) -> String {
        serde_json::to_string(&cmx_core::wasm_types::PluginCallResponse {
            success: true,
            output,
            elapsed_us,
            error: None,
        })
        .unwrap_or_default()
    }

    /// 构建错误响应
    fn err_response(msg: String) -> String {
        serde_json::to_string(&cmx_core::wasm_types::PluginCallResponse {
            success: false,
            output: None,
            elapsed_us: None,
            error: Some(msg),
        })
        .unwrap_or_default()
    }
}

impl Default for PluginHostFunctions {
    fn default() -> Self {
        Self::new()
    }
}

impl HostFunctionProvider for PluginHostFunctions {
    /// 返回命名空间 "cmx:plugin"
    fn namespace(&self) -> &str {
        "cmx:plugin"
    }

    /// 返回提供的宿主函数列表
    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![
            HostFunctionDef::json_fn("call_service", "cmx:plugin"),
            HostFunctionDef::no_input("get_info", "cmx:plugin", &[ValType::Ptr]),
        ]
    }

    /// 调用宿主函数
    fn call(&self, name: &str, input: String) -> Result<String, HostFuncError> {
        match name {
            "call_service" => self.do_call_service(input),
            "get_info" => self.do_get_info(input),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec!["call_service", "get_info"]
    }
}
