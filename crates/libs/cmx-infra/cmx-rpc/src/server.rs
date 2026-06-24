//! gRPC 服务端实现
//!
//! 实现 CmxServiceOrchestrator trait，桥接 gRPC 请求到 ServiceInvoker 和 RuntimeInvoker。

use std::sync::Arc;

use cmx_core::model::service::SVRContext;
use cmx_rpc_gen::cmx::cmx_plugin_data_service::cmx_plugin_data_service::cmx as plugin_data_proto;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::plugin::{
    PluginDataCategory, PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImporter,
    PluginQuery,
};
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::ServiceInvoker;
use tracing::instrument;

/// CmxServiceOrchestrator 的 gRPC 服务端实现
///
/// 同时实现 `CmxServiceOrchestrator` 和 `CmxPluginDataService` 两个 trait，
/// 通过 `Clone` 共享给两个 volo service 注册。
#[derive(Clone)]
pub struct CmxOrchestratorServiceImpl {
    /// 服务编排调用器
    service_invoker: Arc<dyn ServiceInvoker>,
    /// WASM 运行时调用器
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    /// 插件查询（检查安装状态、获取 WASM 路径）
    plugin_query: Arc<dyn PluginQuery>,
    /// 插件数据导入器（可选，未配置时 import/cleanup 返回错误）
    data_importer: Option<Arc<dyn PluginDataImporter>>,
}

impl CmxOrchestratorServiceImpl {
    /// 创建新的服务实现
    pub fn new(
        service_invoker: Arc<dyn ServiceInvoker>,
        runtime_invoker: Arc<dyn RuntimeInvoker>,
        plugin_query: Arc<dyn PluginQuery>,
        data_importer: Option<Arc<dyn PluginDataImporter>>,
    ) -> Self {
        Self {
            service_invoker,
            runtime_invoker,
            plugin_query,
            data_importer,
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
                        steps: resp.steps.into_iter().map(execution_step_to_proto).collect(),
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

            // ==================== 参数解析 ====================

            let input_value: serde_json::Value = serde_json::from_str(&req.input).unwrap_or(serde_json::Value::Null);

            let initial_input = req.initial_input
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok());

            let svr_ctx = SVRContext::new(
                input_value.clone(),
                std::collections::HashMap::new(),
                chrono::Utc::now(),
                format!("rpc-{}", uuid::Uuid::new_v4()),
            );

            // ==================== 调用核心逻辑（cmx-biz） ====================

            match cmx_biz::function_invoker::invoke_plugin_function(
                &runtime_invoker,
                &plugin_query,
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
        }
    }
}

impl plugin_data_proto::CmxPluginDataService for CmxOrchestratorServiceImpl {
    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_import_plugin_data")]
    fn import_plugin_data(
        &self,
        req: volo_grpc::Request<plugin_data_proto::ImportPluginDataRequest>,
    ) -> impl std::future::Future<
        Output = Result<volo_grpc::Response<plugin_data_proto::ImportPluginDataResponse>, volo_grpc::Status>,
    > + Send {
        let data_importer = self.data_importer.clone();
        async move {
            let req = req.into_inner();

            let Some(importer) = data_importer else {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: "data_importer 未配置".into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            };

            let Some(category) = PluginDataCategory::parse_from_str(&req.category) else {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: format!("无效的数据类别: {}（有效值: menu/perm/form/flow）", req.category).into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            };

            // 校验必填字段（与 HTTP 端点保持一致）
            if req.domain_code.is_empty()
                || req.application_code.is_empty()
                || req.module_code.is_empty()
            {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: "domain_code/application_code/module_code 不能为空".into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            }
            if req.plugin_id.is_empty() || req.app_id.is_empty() {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: "plugin_id/app_id 不能为空".into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            }
            if req.version.is_empty() {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: "version 不能为空".into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            }

            let request = PluginDataImportRequest {
                category,
                domain_code: req.domain_code.to_string(),
                application_code: req.application_code.to_string(),
                module_code: req.module_code.to_string(),
                plugin_id: req.plugin_id.to_string(),
                app_id: req.app_id.to_string(),
                version: req.version.to_string(),
                zip_data: req.zip_data.to_vec(),
            };

            match importer.import_data(request).await {
                Ok(result) => {
                    let response = plugin_data_proto::ImportPluginDataResponse {
                        success: result.success,
                        message: result.message.into(),
                        created_count: result.created_count,
                        updated_count: result.updated_count,
                        deleted_count: result.deleted_count,
                    };
                    Ok(volo_grpc::Response::new(response))
                }
                Err(e) => {
                    tracing::error!(
                        target: "cmx_rpc",
                        error = %e,
                        category = %req.category,
                        domain = %req.domain_code,
                        app = %req.application_code,
                        module = %req.module_code,
                        plugin_id = %req.plugin_id,
                        "插件数据导入失败"
                    );
                    let response = plugin_data_proto::ImportPluginDataResponse {
                        success: false,
                        message: e.to_string().into(),
                        created_count: 0,
                        updated_count: 0,
                        deleted_count: 0,
                    };
                    Ok(volo_grpc::Response::new(response))
                }
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_cleanup_plugin_data")]
    fn cleanup_plugin_data(
        &self,
        req: volo_grpc::Request<plugin_data_proto::CleanupPluginDataRequest>,
    ) -> impl std::future::Future<
        Output = Result<volo_grpc::Response<plugin_data_proto::ImportPluginDataResponse>, volo_grpc::Status>,
    > + Send {
        let data_importer = self.data_importer.clone();
        async move {
            let req = req.into_inner();

            let Some(importer) = data_importer else {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: "data_importer 未配置".into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            };

            let Some(category) = PluginDataCategory::parse_from_str(&req.category) else {
                let response = plugin_data_proto::ImportPluginDataResponse {
                    success: false,
                    message: format!("无效的数据类别: {}", req.category).into(),
                    created_count: 0,
                    updated_count: 0,
                    deleted_count: 0,
                };
                return Ok(volo_grpc::Response::new(response));
            };

            let request = PluginDataCleanupRequest {
                category,
                domain_code: req.domain_code.to_string(),
                application_code: req.application_code.to_string(),
                module_code: req.module_code.to_string(),
                plugin_id: req.plugin_id.to_string(),
                app_id: req.app_id.to_string(),
            };

            match importer.cleanup_data(request).await {
                Ok(result) => {
                    let response = plugin_data_proto::ImportPluginDataResponse {
                        success: result.success,
                        message: result.message.into(),
                        created_count: result.created_count,
                        updated_count: result.updated_count,
                        deleted_count: result.deleted_count,
                    };
                    Ok(volo_grpc::Response::new(response))
                }
                Err(e) => {
                    tracing::error!(
                        target: "cmx_rpc",
                        error = %e,
                        category = %req.category,
                        domain = %req.domain_code,
                        app = %req.application_code,
                        module = %req.module_code,
                        plugin_id = %req.plugin_id,
                        "插件数据清理失败"
                    );
                    let response = plugin_data_proto::ImportPluginDataResponse {
                        success: false,
                        message: e.to_string().into(),
                        created_count: 0,
                        updated_count: 0,
                        deleted_count: 0,
                    };
                    Ok(volo_grpc::Response::new(response))
                }
            }
        }
    }
}

/// 将 cmx_core::ExecutionStep 转换为 protobuf ExecutionStep
fn execution_step_to_proto(step: cmx_core::ExecutionStep) -> ExecutionStep {
    ExecutionStep {
        node_id: step.node_id.into(),
        node_name: step.node_name.into(),
        node_type: step.node_type.into(),
        status: cmx_biz::service_executor::step_status_to_str(&step.status).into(),
        output: step.output.map(|v| v.to_string().into()),
        elapsed_us: step.elapsed_us,
        error: step.error.map(|s| s.into()),
        previous_output: step.previous_output.map(|v| v.to_string().into()),
    }
}
