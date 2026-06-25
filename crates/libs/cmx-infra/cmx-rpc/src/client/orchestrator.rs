//! 服务编排 gRPC 客户端 + Bundle + 领域全局访问器。
//!
//! 基于 volo-grpc 的 [`ServiceOrchestrationClient`] trait 实现，通过注册中心缓存发现服务实例。
//!
//! # 领域全局
//!
//! 访问：[`orchestrator_client()`] → `&'static Arc<dyn ServiceOrchestrationClient>`

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use cmx_core::CallServiceResponse;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::rpc::{FunctionCallResult, RpcError, ServiceOrchestrationClient};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::instrument;

use super::infra::GrpcInfrastructure;
use super::retry::with_retry; // RetryStats 仅由元组解构推断，不需导入类型名（避免 unused_imports）
use super::safe_parse_json;
use crate::bundle::{RpcServiceBundle, ServerDeps, ServerRegistration};

// ==================== 领域全局访问器 ====================

static ORCHESTRATOR_CLIENT: OnceLock<Arc<dyn ServiceOrchestrationClient>> = OnceLock::new();

pub(crate) fn set_client(c: Arc<dyn ServiceOrchestrationClient>) -> Result<(), ()> {
    ORCHESTRATOR_CLIENT.set(c).map_err(|_| ())
}

/// 获取服务编排 RPC 客户端（须先通过 [`crate::factory::init_rpc_clients`] 初始化）。
///
/// # Panics
///
/// 未初始化时 panic。先用 [`crate::global::GlobalRpcClient::is_initialized`] 守卫。
pub fn orchestrator_client() -> &'static Arc<dyn ServiceOrchestrationClient> {
    ORCHESTRATOR_CLIENT
        .get()
        .expect("orchestrator client not initialized")
}

// ==================== 客户端实现 ====================

/// 服务编排 gRPC 客户端。
pub struct OrchestratorGrpcClient {
    /// 共享基础设施（服务发现、Discover 缓存、超时/重试配置）
    infra: Arc<GrpcInfrastructure>,
    /// gRPC 客户端缓存（service_name → client）
    clients: RwLock<HashMap<String, CmxServiceOrchestratorClient>>,
}

impl OrchestratorGrpcClient {
    /// 创建新的服务编排 gRPC 客户端。
    pub fn new(infra: Arc<GrpcInfrastructure>) -> Self {
        Self {
            infra,
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// 获取或创建指定服务的 gRPC 客户端（double-check locking 防止并发重复创建）。
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    async fn get_client(
        &self,
        service_name: &str,
    ) -> Result<CmxServiceOrchestratorClient, RpcError> {
        // 快查：读锁检查缓存
        if let Some(c) = self.clients.read().await.get(service_name) {
            return Ok(c.clone());
        }

        // 慢路径：获取共享 discover + 构建 client
        let discover = self.infra.get_or_create_discover(service_name).await?;
        let client = CmxServiceOrchestratorClientBuilder::new(service_name)
            .discover(discover)
            .rpc_timeout(Some(self.infra.rpc_timeout()))
            .connect_timeout(self.infra.connect_timeout())
            .build();

        // 写锁：double-check 防止并发重复创建
        let mut clients = self.clients.write().await;
        if let Some(c) = clients.get(service_name) {
            return Ok(c.clone());
        }
        clients.insert(service_name.to_string(), client.clone());

        Ok(client)
    }
}

#[async_trait]
impl ServiceOrchestrationClient for OrchestratorGrpcClient {
    #[instrument(target = "cmx_rpc", skip(self, input), fields(service_name = %service_name, service_key = %service_key))]
    async fn call_service(
        &self,
        service_name: &str,
        service_key: &str,
        input: Value,
        options: cmx_traits::service::ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, RpcError> {
        let client = self.get_client(service_name).await?;
        let timeout_ms = self.infra.timeout_ms();
        let max_retries = self.infra.retry_count();

        let service_key_fs: pilota::FastStr = service_key.to_string().into();
        let input_fs: pilota::FastStr = input.to_string().into();
        let debug_node_id = options
            .debug_node_id
            .map(|s| -> pilota::FastStr { s.into() });
        let debug_params: pilota::AHashMap<pilota::FastStr, pilota::FastStr> = options
            .debug_params
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        // 闭包只返回原始 Status，into_inner 在外做一次（使用约束见 retry.rs）
        match with_retry(timeout_ms, max_retries, || {
            let req = ExecuteServiceRequest {
                service_key: service_key_fs.clone(),
                input: input_fs.clone(),
                include_steps: options.include_steps,
                debug: options.debug,
                debug_node_id: debug_node_id.clone(),
                debug_params: debug_params.clone(),
            };
            let client = client.clone();
            async move { client.execute_service(req).await }
        })
        .await
        {
            Ok((resp, stats)) => {
                let resp = resp.into_inner();
                // 成功路径：补回全部结构化字段；success 用业务 success（与 call_function 统一）
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    service_key = %service_key,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts,
                    success = resp.success,
                    "RPC call_service 完成"
                );
                Ok(proto_to_call_service_response(resp))
            }
            Err((e, stats)) => {
                // 失败路径：业务字段 + stats（日志字段零丢失）
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    service_key = %service_key,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts,
                    success = false,
                    error = %e,
                    "RPC call_service 失败"
                );
                Err(e)
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, input), fields(service_name = %service_name, plugin_id = %plugin_id, function_name = %function_name))]
    async fn call_function(
        &self,
        service_name: &str,
        plugin_id: &str,
        function_name: &str,
        input: Value,
    ) -> Result<FunctionCallResult, RpcError> {
        let client = self.get_client(service_name).await?;
        let timeout_ms = self.infra.timeout_ms();
        let max_retries = self.infra.retry_count();

        let plugin_id_fs: pilota::FastStr = plugin_id.to_string().into();
        let function_name_fs: pilota::FastStr = function_name.to_string().into();
        let input_fs: pilota::FastStr = input.to_string().into();

        match with_retry(timeout_ms, max_retries, || {
            let req = CallFunctionRequest {
                plugin_id: plugin_id_fs.clone(),
                function_name: function_name_fs.clone(),
                input: input_fs.clone(),
                initial_input: None,
                debug: false,
            };
            let client = client.clone();
            async move { client.call_function(req).await }
        })
        .await
        {
            Ok((resp, stats)) => {
                let inner = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    plugin_id = %plugin_id,
                    function_name = %function_name,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts,
                    success = inner.success,
                    "RPC call_function 完成"
                );
                Ok(FunctionCallResult {
                    success: inner.success,
                    result: inner
                        .result
                        .map(|s| safe_parse_json(&s, "call_function.result")),
                    elapsed_us: inner.elapsed_us,
                    error: inner.error.map(|s| s.to_string()),
                })
            }
            Err((e, stats)) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    plugin_id = %plugin_id,
                    function_name = %function_name,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts,
                    success = false,
                    error = %e,
                    "RPC call_function 失败"
                );
                Err(e)
            }
        }
    }
}

// ==================== Bundle ====================

/// 服务编排领域 Bundle。
pub struct OrchestratorBundle;

impl RpcServiceBundle for OrchestratorBundle {
    fn name(&self) -> &'static str {
        "orchestrator"
    }

    fn init_client(&self, infra: Arc<GrpcInfrastructure>) {
        set_client(Arc::new(OrchestratorGrpcClient::new(infra)))
            .expect("orchestrator client already initialized");
    }

    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration {
        let service_invoker = deps.service_invoker.clone();
        let runtime_invoker = deps.runtime_invoker.clone();
        let plugin_query = deps.plugin_query.clone();
        ServerRegistration::new(move |server| {
            let impl_ = crate::server::orchestrator::CmxOrchestratorServerImpl::new(
                service_invoker,
                runtime_invoker,
                plugin_query,
            );
            let svc = volo_grpc::server::ServiceBuilder::new(
                CmxServiceOrchestratorServer::new(impl_),
            )
            .build::<CmxServiceOrchestratorRequestRecv, CmxServiceOrchestratorResponseSend>();
            server.add_service(svc)
        })
    }
}

// ==================== proto 转换（step_status 统一到 cmx-biz 单一来源）====================

/// 将 protobuf 响应转换为 [`CallServiceResponse`]。
fn proto_to_call_service_response(resp: ExecuteServiceResponse) -> CallServiceResponse {
    CallServiceResponse {
        success: resp.success,
        output: resp
            .output
            .map(|v| safe_parse_json(&v, "call_service.output")),
        steps: resp
            .steps
            .into_iter()
            .map(|s| cmx_core::ExecutionStep {
                node_id: s.node_id.to_string(),
                node_name: s.node_name.to_string(),
                node_type: s.node_type.to_string(),
                // 统一到 cmx-biz 单一来源（str → enum）
                status: cmx_biz::service_executor::parse_step_status(s.status.as_str()),
                output: s.output.map(|v| safe_parse_json(&v, "step.output")),
                elapsed_us: s.elapsed_us,
                error: s.error.map(|e| e.to_string()),
                previous_output: s
                    .previous_output
                    .map(|v| safe_parse_json(&v, "step.previous_output")),
            })
            .collect(),
        total_elapsed_us: Some(resp.total_elapsed_us),
        error: resp
            .error
            .map(|e| cmx_core::OrchestrationError {
                message: e.message.to_string(),
            }),
    }
}
