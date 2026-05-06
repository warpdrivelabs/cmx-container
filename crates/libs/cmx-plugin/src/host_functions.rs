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

use std::collections::HashMap;

use chrono::Utc;
use tracing::{info, warn};
use cmx_traits::{HostFunctionProvider, HostFuncError, HostFunctionDef, GlobalRuntime, WasmInvokeResult, InvokeOptions};
use cmx_core::{PluginFunRequest, CallServiceRequest, CallServiceResponse};
use cmx_core::model::service::{FunctionInput, SVRContext};

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


    /// 执行调用指定插件的指定函数
    ///
    /// # 功能说明
    /// 接收 WASM 插件的调用请求，通过 GlobalRuntime 加载目标插件并执行指定函数。
    /// 输入输出均使用 MsgPack 编码。
    ///
    /// # 参数
    /// - `self`: 宿主函数提供者实例
    /// - `input`: MsgPack 编码的 PluginFunRequest 请求
    ///
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 包含 CallServiceResponse 的 MsgPack 编码
    /// - `Err(HostFuncError)`: 函数执行失败
    fn do_call_plugin(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let req: PluginFunRequest = match rmp_serde::from_slice(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response_msgpack(format!("解析请求失败: {}", e))),
        };
        info!("[call_plugin] 目标插件: {}, 函数: {}", req.plugin_id, req.function_name);

        let runtime = GlobalRuntime::get();

        let svr_ctx = SVRContext::new(
            req.initial_input.clone().unwrap_or_else(|| req.input.clone()),
            HashMap::new(),
            Utc::now(),
            generate_request_id(),
        );
        let func_input = FunctionInput::from_value(req.input.clone(), svr_ctx);

        let input_bytes = match rmp_serde::to_vec(&func_input) {
            Ok(b) => b,
            Err(e) => return Ok(Self::err_response_msgpack(format!("序列化输入失败: {}", e))),
        };

        let invoke_options = InvokeOptions {
            timeout: std::time::Duration::from_secs(30),
            max_depth: 8,
            debug: req.debug.unwrap_or(false),
        };

        let rt = tokio::runtime::Handle::current();
        let result: Result<WasmInvokeResult, _> = rt.block_on(async {
            runtime.invoke_with_options(&req.plugin_id, &req.function_name, &input_bytes, &invoke_options).await
        });

        match result {
            Ok(invoke_result) => {
                let output = if invoke_result.output.is_empty() {
                    serde_json::Value::Null
                } else {
                    rmp_serde::from_slice(&invoke_result.output)
                        .unwrap_or(serde_json::Value::Null)
                };
                Ok(Self::ok_response_msgpack(Some(output), Some(invoke_result.elapsed_us)))
            }
            Err(e) => {
                warn!("[call_plugin] 调用失败: {}", e);
                Ok(Self::err_response_msgpack(e.to_string()))
            }
        }
    }

    /// 执行调用指定服务编排
    ///
    /// # 功能说明
    /// 接收 WASM 插件的服务调用请求，通过 GlobalRuntime 执行完整的服务编排。
    /// 输入输出均使用 MsgPack 编码。
    ///
    /// 注意：当前实现需要 PluginQuery 和 ServiceQuery 访问能力，
    /// 这些在宿主函数上下文中不可用。后续版本将通过 GlobalState 提供。
    ///
    /// # 参数
    /// - `self`: 宿主函数提供者实例
    /// - `input`: MsgPack 编码的 CallServiceRequest 请求
    ///
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 包含 CallServiceResponse 的 MsgPack 编码
    /// - `Err(HostFuncError)`: 函数执行失败
    fn do_call_service_by_key(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let req: CallServiceRequest = match rmp_serde::from_slice(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response_msgpack(format!("解析请求失败: {}", e))),
        };
        warn!("[call_service_by_key] 服务: {} - 功能暂未实现，需要扩展 GlobalRuntime 支持", req.service_key);
        //todo yqs 未实现服务调用

        Ok(Self::err_response_msgpack(
            "call_service_by_key 功能暂未实现，宿主函数上下文缺少 PluginQuery 和 ServiceQuery 访问能力".to_string()
        ))
    }

    // /// 获取插件信息
    // ///
    // /// 注意：当前宿主函数回调运行在 spawn_blocking 线程中，
    // /// 无法获取当前插件的运行时上下文信息（plugin_id、request_id 等）。
    // /// 需要通过 Extism SDK 的 identity 机制或上下文传递来实现，
    // /// 暂返回未实现提示。
    // fn do_get_info(&self, _input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
    //     let info = PluginInfoResponse {
    //         plugin_id: String::new(),
    //         db_id: String::new(),
    //         txn_id: None,
    //         request_id: String::new(),
    //         tenant_id: None,
    //     };
    //     Ok(rmp_serde::to_vec(&info).unwrap_or_default())
    // }



    /// 构建成功响应（MsgPack 编码）- 新版本，返回 serde_json::Value
    fn ok_response_msgpack(output: Option<serde_json::Value>, elapsed_us: Option<u64>) -> Vec<u8> {
        rmp_serde::to_vec(&CallServiceResponse {
            success: true,
            output,
            elapsed_us,
            error: None,
        })
        .unwrap_or_default()
    }

    /// 构建错误响应（MsgPack 编码）- 新版本
    fn err_response_msgpack(msg: String) -> Vec<u8> {
        rmp_serde::to_vec(&CallServiceResponse {
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
            HostFunctionDef::msgpack_fn("call_plugin", "cmx:plugin"),
            HostFunctionDef::msgpack_fn("call_service_by_key", "cmx:plugin"),
        ]
    }

    /// 调用宿主函数
    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        match name {
            "call_plugin" => self.do_call_plugin(input),
            "call_service_by_key" => self.do_call_service_by_key(input),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec!["call_plugin", "call_service_by_key"]
    }
}

/// 生成请求ID
fn generate_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
