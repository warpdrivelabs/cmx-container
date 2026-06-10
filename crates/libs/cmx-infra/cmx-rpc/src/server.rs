//! gRPC 服务端实现
//!
//! 实现 CmxServiceOrchestrator trait，桥接 gRPC 请求到 ServiceInvoker 和 RuntimeInvoker。

use std::sync::Arc;

use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceInvoker};
use tracing::instrument;

/// CmxServiceOrchestrator 的 gRPC 服务端实现
pub struct CmxOrchestratorServiceImpl {
    /// 服务编排调用器
    service_invoker: Arc<dyn ServiceInvoker>,
    /// WASM 运行时调用器
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    /// 插件查询（检查安装状态、获取 WASM 路径）
    plugin_query: Arc<dyn PluginQuery>,
}

impl CmxOrchestratorServiceImpl {
    /// 创建新的服务实现
    pub fn new(
        service_invoker: Arc<dyn ServiceInvoker>,
        runtime_invoker: Arc<dyn RuntimeInvoker>,
        plugin_query: Arc<dyn PluginQuery>,
    ) -> Self {
        Self {
            service_invoker,
            runtime_invoker,
            plugin_query,
        }
    }
}

impl CmxServiceOrchestrator for CmxOrchestratorServiceImpl {
    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_execute_service")]
    fn execute_service(
        &self,
        req: volo_grpc::Request<ExecuteServiceRequest>,
    ) -> impl std::future::Future<
        Output = Result<volo_grpc::Response<ExecuteServiceResponse>, volo_grpc::Status>,
    > + Send {
        let service_invoker = self.service_invoker.clone();
        async move {
            let req = req.into_inner();

            let input: serde_json::Value = serde_json::from_str(&req.input).map_err(|e| {
                volo_grpc::Status::new(
                    volo_grpc::Code::InvalidArgument,
                    format!("输入 JSON 解析失败: {e}"),
                )
            })?;

            let options = cmx_traits::ServiceInvokeOptions {
                include_steps: req.include_steps,
                debug: req.debug,
                debug_node_id: req.debug_node_id.map(|s| s.to_string()),
                debug_params: if req.debug_params.is_empty() {
                    None
                } else {
                    Some(
                        req.debug_params
                            .into_iter()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    )
                },
            };

            match service_invoker
                .invoke_service(&req.service_key, input, options)
                .await
            {
                Ok(resp) => {
                    let mut pb_resp = ExecuteServiceResponse::default();
                    pb_resp.success = resp.success;
                    pb_resp.output = resp.output.map(|v| v.to_string().into());
                    pb_resp.steps = resp.steps.into_iter().map(execution_step_to_proto).collect();
                    pb_resp.total_elapsed_us = resp.total_elapsed_us.unwrap_or(0);
                    pb_resp.error = resp.error.map(|e| OrchestrationError {
                        message: e.message.into(),
                    });
                    Ok(volo_grpc::Response::new(pb_resp))
                }
                Err(e) => {
                    tracing::error!(
                        target: "cmx_rpc",
                        error = %e,
                        "服务编排执行失败"
                    );
                    let mut pb_resp = ExecuteServiceResponse::default();
                    pb_resp.success = false;
                    pb_resp.error = Some(OrchestrationError {
                        message: e.to_string().into(),
                    });
                    Ok(volo_grpc::Response::new(pb_resp))
                }
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_call_function")]
    fn call_function(
        &self,
        req: volo_grpc::Request<CallFunctionRequest>,
    ) -> impl std::future::Future<
        Output = Result<volo_grpc::Response<CallFunctionResponse>, volo_grpc::Status>,
    > + Send {
        let runtime_invoker = self.runtime_invoker.clone();
        let plugin_query = self.plugin_query.clone();
        async move {
            let req = req.into_inner();
            let plugin_id = req.plugin_id.to_string();
            let function_name = req.function_name.to_string();

            // ==================== 检查插件安装状态 ====================

            let is_installed = plugin_query.is_installed(&plugin_id).await
                .map_err(|e| volo_grpc::Status::new(
                    volo_grpc::Code::Internal,
                    format!("检查插件安装状态失败: {e}"),
                ))?;

            if !is_installed {
                return Err(volo_grpc::Status::new(
                    volo_grpc::Code::NotFound,
                    format!("插件 {} 未安装", plugin_id),
                ));
            }

            // ==================== 检查/加载 WASM 模块 ====================

            let is_loaded = runtime_invoker.is_loaded(&plugin_id).await;

            if !is_loaded {
                let wasm_path = plugin_query.get_wasm_path(&plugin_id).await
                    .map_err(|e| volo_grpc::Status::new(
                        volo_grpc::Code::Internal,
                        format!("获取 WASM 路径失败: {e}"),
                    ))?;

                runtime_invoker.load_module(&plugin_id, &wasm_path).await
                    .map_err(|e| volo_grpc::Status::new(
                        volo_grpc::Code::Internal,
                        format!("加载 WASM 模块失败: {e}"),
                    ))?;
            }

            // ==================== 调用 WASM 函数 ====================

            let input_bytes = req.input.as_bytes();

            match runtime_invoker
                .invoke(&plugin_id, &function_name, input_bytes)
                .await
            {
                Ok(result) => {
                    let mut pb_resp = CallFunctionResponse::default();
                    pb_resp.success = true;
                    pb_resp.result =
                        Some(String::from_utf8_lossy(&result.output).to_string().into());
                    pb_resp.elapsed_us = result.elapsed_us;
                    Ok(volo_grpc::Response::new(pb_resp))
                }
                Err(e) => {
                    tracing::error!(
                        target: "cmx_rpc",
                        plugin_id = %plugin_id,
                        function_name = %function_name,
                        error = %e,
                        "插件函数调用失败"
                    );
                    let mut pb_resp = CallFunctionResponse::default();
                    pb_resp.success = false;
                    pb_resp.error = Some(e.to_string().into());
                    Ok(volo_grpc::Response::new(pb_resp))
                }
            }
        }
    }
}

/// 将 cmx_core::ExecutionStep 转换为 protobuf ExecutionStep
fn execution_step_to_proto(step: cmx_core::ExecutionStep) -> ExecutionStep {
    let mut pb = ExecutionStep::default();
    pb.node_id = step.node_id.into();
    pb.node_name = step.node_name.into();
    pb.node_type = step.node_type.into();
    pb.status = step_status_to_str(&step.status).into();
    pb.output = step.output.map(|v| v.to_string().into());
    pb.elapsed_us = step.elapsed_us;
    pb.error = step.error.map(|s| s.into());
    pb.previous_output = step.previous_output.map(|v| v.to_string().into());
    pb
}

/// 将 StepStatus 转换为稳定的字符串表示，避免依赖 Debug 格式
fn step_status_to_str(status: &cmx_core::StepStatus) -> &'static str {
    match status {
        cmx_core::StepStatus::Success => "Success",
        cmx_core::StepStatus::Failed => "Failed",
        cmx_core::StepStatus::Skipped => "Skipped",
        cmx_core::StepStatus::DebugPaused => "DebugPaused",
    }
}
