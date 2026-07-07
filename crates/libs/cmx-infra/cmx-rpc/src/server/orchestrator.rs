//! 服务编排 gRPC 服务端实现。
//!
//! 实现 [`CmxServiceOrchestrator`] trait，桥接 gRPC 请求到
//! [`ServiceInvoker`]（编排执行）和 [`FunctionInvoker`]（插件函数调用）。

use std::sync::Arc;

use cmx_core::model::service::SVRContext;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::function_invoker::FunctionInvoker;
use cmx_traits::service::ServiceInvoker;
use tracing::instrument;

use super::auth_layer::{AuthVerifier, VerifiedAuth, verify_request};

/// [`CmxServiceOrchestrator`] 的 gRPC 服务端实现。
#[derive(Clone)]
pub struct CmxOrchestratorServerImpl {
    /// 服务编排调用器
    service_invoker: Arc<dyn ServiceInvoker>,
    /// 插件函数调用器（封装 RuntimeInvoker + PluginQuery 的完整调用链）
    function_invoker: Arc<dyn FunctionInvoker>,
    /// 鉴权器（`None` 表示不启用 gRPC 鉴权）。
    auth_verifier: Option<AuthVerifier>,
}

impl CmxOrchestratorServerImpl {
    /// 创建新的服务编排 gRPC 服务端。
    pub fn new(
        service_invoker: Arc<dyn ServiceInvoker>,
        function_invoker: Arc<dyn FunctionInvoker>,
    ) -> Self {
        Self {
            service_invoker,
            function_invoker,
            auth_verifier: None,
        }
    }

    /// 设置鉴权器（由 Bundle 在 `build_server` 时按需注入）。
    pub fn with_auth_verifier(mut self, verifier: AuthVerifier) -> Self {
        self.auth_verifier = Some(verifier);
        self
    }

    /// 统一鉴权入口。未配置 verifier 时直接返回 None（兼容无鉴权场景）。
    async fn auth(
        &self,
        meta: &volo_grpc::metadata::MetadataMap,
    ) -> Result<Option<VerifiedAuth>, volo_grpc::Status> {
        match &self.auth_verifier {
            Some(v) => verify_request(meta, v).await.map(Some),
            None => Ok(None),
        }
    }
}

impl CmxServiceOrchestrator for CmxOrchestratorServerImpl {
    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_execute_service")]
    fn execute_service(
        &self,
        req: volo_grpc::Request<ExecuteServiceRequest>,
    ) -> impl std::future::Future<
        Output = Result<volo_grpc::Response<ExecuteServiceResponse>, volo_grpc::Status>,
    > + Send {
        let service_invoker = self.service_invoker.clone();
        let auth_verifier = self.clone();
        async move {
            // 鉴权（在 into_inner 前从 metadata 读取）
            let verified = auth_verifier.auth(req.metadata()).await?;
            let req = req.into_inner();

            // 建立 task_local scope（用 scope_full 透传委托用户 token + request_id，
            // 使链式跨服务调用可继续 on-behalf-of）。
            let (auth_ctx, user_token, request_id) = match verified {
                Some(v) => (Some(v.context), v.original_user_token, v.request_id),
                None => (None, None, None),
            };
            cmx_traits::auth::context_scope::scope_full(
                auth_ctx,
                user_token,
                request_id.unwrap_or_default(),
                None,
                async {
                let input: serde_json::Value = serde_json::from_str(&req.input).map_err(|e| {
                    volo_grpc::Status::new(
                        volo_grpc::Code::InvalidArgument,
                        format!("输入 JSON 解析失败: {e}"),
                    )
                })?;

                let options = cmx_traits::service::ServiceInvokeOptions {
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
                        let pb_resp = ExecuteServiceResponse {
                            success: resp.success,
                            output: resp.output.map(|v| v.to_string().into()),
                            steps: resp
                                .steps
                                .into_iter()
                                .map(execution_step_to_proto)
                                .collect(),
                            total_elapsed_us: resp.total_elapsed_us.unwrap_or(0),
                            error: resp.error.map(|e| OrchestrationError {
                                message: e.message.into(),
                            }),
                        };
                        Ok(volo_grpc::Response::new(pb_resp))
                    }
                    Err(e) => {
                        tracing::error!(
                            target: "cmx_rpc",
                            error = %e,
                            "服务编排执行失败"
                        );
                        let pb_resp = ExecuteServiceResponse {
                            success: false,
                            output: None,
                            steps: Vec::new(),
                            total_elapsed_us: 0,
                            error: Some(OrchestrationError {
                                message: e.to_string().into(),
                            }),
                        };
                        Ok(volo_grpc::Response::new(pb_resp))
                    }
                }
            })
            .await
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_call_function")]
    fn call_function(
        &self,
        req: volo_grpc::Request<CallFunctionRequest>,
    ) -> impl std::future::Future<
        Output = Result<volo_grpc::Response<CallFunctionResponse>, volo_grpc::Status>,
    > + Send {
        let function_invoker = self.function_invoker.clone();
        let auth_self = self.clone();
        async move {
            // 鉴权（在 into_inner 前从 metadata 读取）
            let verified = auth_self.auth(req.metadata()).await?;
            let req = req.into_inner();
            let plugin_id = req.plugin_id.to_string();
            let function_name = req.function_name.to_string();

            // 建立 task_local scope（同 execute_service，透传委托用户 + request_id）
            let (auth_ctx, user_token, request_id) = match verified {
                Some(v) => (Some(v.context), v.original_user_token, v.request_id),
                None => (None, None, None),
            };
            cmx_traits::auth::context_scope::scope_full(
                auth_ctx,
                user_token,
                request_id.unwrap_or_default(),
                None,
                async {
                // ==================== 参数解析 ====================

                let input_value: serde_json::Value =
                    serde_json::from_str(&req.input).unwrap_or(serde_json::Value::Null);

                let initial_input = req
                    .initial_input
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok());

                let svr_ctx = SVRContext::new(
                    input_value.clone(),
                    std::collections::HashMap::new(),
                    chrono::Utc::now(),
                    format!("rpc-{}", uuid::Uuid::new_v4()),
                );

                // ==================== 调用核心逻辑（通过 FunctionInvoker trait） ====================

                match function_invoker
                    .invoke_plugin_function(
                        &plugin_id,
                        &function_name,
                        input_value,
                        initial_input,
                        svr_ctx,
                        req.debug,
                    )
                    .await
                {
                Ok(result) => {
                    let pb_resp = CallFunctionResponse {
                        success: result.success,
                        result: if result.success {
                            Some(result.result.to_string().into())
                        } else {
                            None
                        },
                        elapsed_us: result.elapsed_us,
                        error: result.error.map(|s| s.into()),
                    };
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
                    let pb_resp = CallFunctionResponse {
                        success: false,
                        result: None,
                        elapsed_us: 0,
                        error: Some(e.to_string().into()),
                    };
                    Ok(volo_grpc::Response::new(pb_resp))
                }
                }
            })
            .await
        }
    }
}

/// 将 [`cmx_core::ExecutionStep`] 转换为 protobuf [`ExecutionStep`]。
fn execution_step_to_proto(step: cmx_core::ExecutionStep) -> ExecutionStep {
    ExecutionStep {
        node_id: step.node_id.into(),
        node_name: step.node_name.into(),
        node_type: step.node_type.into(),
        status: cmx_traits::step_status::step_status_to_str(&step.status).into(),
        output: step.output.map(|v| v.to_string().into()),
        elapsed_us: step.elapsed_us,
        error: step.error.map(|s| s.into()),
        previous_output: step.previous_output.map(|v| v.to_string().into()),
    }
}
