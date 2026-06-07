//! gRPC 客户端实现
//!
//! 基于 volo-grpc 的 RpcClient trait 实现，通过注册中心缓存发现服务实例。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cmx_core::CallServiceResponse;
use cmx_registry_config::registry::ServiceInstanceCache;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::{FunctionCallResult, RpcClient, RpcError};
use serde_json::Value;
use tracing::instrument;

use crate::discover::RegistryAwareDiscover;

/// 基于 volo-grpc 的 RPC 客户端
pub struct VoloGrpcClient {
    /// 服务实例缓存
    cache: Arc<ServiceInstanceCache>,
    /// 调用超时时间（毫秒）
    timeout_ms: u64,
}

impl VoloGrpcClient {
    /// 创建新的 gRPC 客户端
    pub fn new(cache: Arc<ServiceInstanceCache>, timeout_ms: u64) -> Self {
        Self { cache, timeout_ms }
    }

    /// 创建指定服务的 gRPC 客户端
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    async fn get_client(
        &self,
        service_name: &str,
    ) -> Result<CmxServiceOrchestratorClient, RpcError> {
        // 确保缓存中有该服务的实例
        let instances = self.cache.get(service_name).ok_or_else(|| {
            RpcError::NoAvailableInstance(service_name.to_string())
        })?;
        if instances.is_empty() {
            return Err(RpcError::NoAvailableInstance(service_name.to_string()));
        }

        // 创建 Discover 并启动监听
        let discover = RegistryAwareDiscover::new(self.cache.clone());
        discover.start_watch(service_name);

        // 构建 volo gRPC 客户端
        let client = CmxServiceOrchestratorClientBuilder::new(service_name)
            .discover(discover)
            .build();

        Ok(client)
    }
}

#[async_trait]
impl RpcClient for VoloGrpcClient {
    #[instrument(target = "cmx_rpc", skip(self, input), fields(service_name = %service_name, service_key = %service_key))]
    async fn call_service(
        &self,
        service_name: &str,
        service_key: &str,
        input: Value,
        options: cmx_traits::ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, RpcError> {
        let start = std::time::Instant::now();

        let client = self.get_client(service_name).await?;

        let service_key_fs: pilota::FastStr = service_key.to_string().into();
        let input: pilota::FastStr = input.to_string().into();
        let debug_node_id = options.debug_node_id.map(|s| -> pilota::FastStr { s.into() });
        let debug_params: pilota::AHashMap<pilota::FastStr, pilota::FastStr> = options
            .debug_params
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        let req = ExecuteServiceRequest {
            service_key: service_key_fs,
            input,
            include_steps: options.include_steps,
            debug: options.debug,
            debug_node_id,
            debug_params,
        };

        let result = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            client.execute_service(req),
        )
        .await;

        let elapsed = start.elapsed();

        match result {
            Ok(Ok(resp)) => {
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    service_key = %service_key,
                    elapsed_us = elapsed.as_micros() as u64,
                    success = true,
                    "RPC call_service 完成"
                );
                Ok(proto_to_call_service_response(resp.into_inner()))
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    service_key = %service_key,
                    elapsed_us = elapsed.as_micros() as u64,
                    success = false,
                    error = %e,
                    "RPC call_service 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
            Err(_) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    service_key = %service_key,
                    elapsed_us = elapsed.as_micros() as u64,
                    success = false,
                    "RPC call_service 超时"
                );
                Err(RpcError::Timeout(format!("调用超时: {}ms", self.config.timeout_ms)))
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
        let start = std::time::Instant::now();

        let client = self.get_client(service_name).await?;

        let req = CallFunctionRequest {
            plugin_id: plugin_id.to_string().into(),
            function_name: function_name.to_string().into(),
            input: input.to_string().into(),
            initial_input: None,
            debug: false,
        };

        let result = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            client.call_function(req),
        )
        .await;

        let elapsed = start.elapsed();

        match result {
            Ok(Ok(resp)) => {
                let inner = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    plugin_id = %plugin_id,
                    function_name = %function_name,
                    elapsed_us = elapsed.as_micros() as u64,
                    success = true,
                    "RPC call_function 完成"
                );
                Ok(FunctionCallResult {
                    success: inner.success,
                    result: inner
                        .result
                        .map(|s| serde_json::from_str(&s).unwrap_or(Value::Null)),
                    elapsed_us: inner.elapsed_us,
                    error: inner.error.map(|s| s.to_string()),
                })
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    plugin_id = %plugin_id,
                    function_name = %function_name,
                    elapsed_us = elapsed.as_micros() as u64,
                    success = false,
                    error = %e,
                    "RPC call_function 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
            Err(_) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    plugin_id = %plugin_id,
                    function_name = %function_name,
                    elapsed_us = elapsed.as_micros() as u64,
                    success = false,
                    "RPC call_function 超时"
                );
                Err(RpcError::Timeout(format!("调用超时: {}ms", self.config.timeout_ms)))
            }
        }
    }
}

/// 将 protobuf 响应转换为 CallServiceResponse
fn proto_to_call_service_response(resp: ExecuteServiceResponse) -> CallServiceResponse {
    CallServiceResponse {
        success: resp.success,
        output: resp.output.map(|v| serde_json::from_str(&v).unwrap_or(Value::Null)),
        steps: resp.steps.into_iter().map(|s| cmx_core::ExecutionStep {
            node_id: s.node_id.to_string(),
            node_name: s.node_name.to_string(),
            node_type: s.node_type.to_string(),
            status: parse_step_status(&s.status),
            output: s.output.map(|v| serde_json::from_str(&v).unwrap_or(Value::Null)),
            elapsed_us: s.elapsed_us,
            error: s.error.map(|e| e.to_string()),
            previous_output: s.previous_output.map(|v| serde_json::from_str(&v).unwrap_or(Value::Null)),
        }).collect(),
        total_elapsed_us: Some(resp.total_elapsed_us),
        error: resp.error.map(|e| cmx_core::OrchestrationError {
            message: e.message.to_string(),
        }),
    }
}

/// 解析步骤状态字符串
fn parse_step_status(status: &pilota::FastStr) -> cmx_core::StepStatus {
    match status.as_str() {
        "Success" => cmx_core::StepStatus::Success,
        "Failed" => cmx_core::StepStatus::Failed,
        "Skipped" => cmx_core::StepStatus::Skipped,
        "DebugPaused" => cmx_core::StepStatus::DebugPaused,
        _ => cmx_core::StepStatus::Failed,
    }
}
