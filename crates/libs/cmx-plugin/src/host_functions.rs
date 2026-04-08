//! WASM 宿主函数 — 插件间调用
//!
//! 为 WASM 插件提供插件间调用能力的宿主函数。
//! 通过 GlobalRuntime 访问 WASM 运行时，实现跨插件的服务调用。

use cmx_traits::{ExtismFunctionProvider, GlobalRuntime, HostFuncError, WasmInvokeResult};
use extism::{host_fn, Manifest, UserData, ValType};

const PTR: ValType = ValType::I64;

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

    /// 构建 CallerData（简化版本）
    fn build_caller_data() -> cmx_traits::CallerData {
        cmx_traits::CallerData::new("default", "default")
    }
}

impl Default for PluginHostFunctions {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtismFunctionProvider for PluginHostFunctions {
    /// 返回命名空间 "cmx:plugin"
    fn namespace(&self) -> &str {
        "cmx:plugin"
    }

    /// 注册插件操作宿主函数
    ///
    /// 注册以下函数：
    /// - `call_service` — 调用另一个插件的服务
    /// - `get_info` — 获取当前插件信息
    fn register_functions(&self, builder: &mut extism::PluginBuilder) -> Result<(), HostFuncError> {
        // call_service — 调用另一个插件的服务
        host_fn!(call_service(_user_data: (); request: String) -> String {
            let req: cmx_core::wasm_types::PluginCallRequest = match serde_json::from_str(&request) {
                Ok(r) => r,
                Err(e) => return Ok(PluginHostFunctions::err_response(format!("解析请求失败: {}", e))),
            };

            let runtime = GlobalRuntime::get();
            let input_bytes = req.input.as_bytes();
            let caller_data = PluginHostFunctions::build_caller_data();

            let rt = tokio::runtime::Handle::current();
            let result: Result<WasmInvokeResult, _> = rt.block_on(async {
                runtime.invoke(
                    &req.target_plugin_id,
                    &req.function_name,
                    input_bytes,
                    &caller_data,
                ).await
            });

            match result {
                Ok(invoke_result) => {
                    let output = if invoke_result.output.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8_lossy(&invoke_result.output).to_string())
                    };
                    Ok(PluginHostFunctions::ok_response(output, Some(invoke_result.elapsed_us)))
                }
                Err(e) => Ok(PluginHostFunctions::err_response(e.to_string())),
            }
        });

        // get_info — 获取当前插件信息
        host_fn!(get_info(_user_data: (); _input: ()) -> String {
            let info = cmx_core::wasm_types::PluginInfoResponse {
                plugin_id: "current_plugin".to_string(),
                db_id: "default".to_string(),
                txn_id: None,
                request_id: "default".to_string(),
                tenant_id: None,
            };
            Ok(serde_json::to_string(&info).unwrap_or_default())
        });

        // 使用 std::mem::replace 替换 builder
        let temp_manifest = Manifest::new([extism::Wasm::data(vec![])]);
        let temp_builder = extism::PluginBuilder::new(temp_manifest);
        let old_builder = std::mem::replace(builder, temp_builder);

        let new_builder = old_builder
            .with_function("call_service", [PTR], [PTR], UserData::new(()), call_service)
            .with_function("get_info", [], [PTR], UserData::new(()), get_info);

        *builder = new_builder;

        Ok(())
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec!["call_service", "get_info"]
    }
}
