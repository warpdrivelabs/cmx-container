//! gRPC 客户端实现
//!
//! 基于 volo-grpc 的 RpcClient trait 实现，通过注册中心缓存发现服务实例。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cmx_core::CallServiceResponse;
use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::{FunctionCallResult, RpcClient, RpcError};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::instrument;
use volo_grpc::Status;

use crate::config::GrpcConfig;
use crate::discover::RegistryAwareDiscover;

/// 缓存的 gRPC 客户端
struct CachedClient {
    client: CmxServiceOrchestratorClient,
    _discover: RegistryAwareDiscover,
}

/// 基于 volo-grpc 的 RPC 客户端
pub struct VoloGrpcClient {
    /// 服务实例缓存
    cache: Arc<ServiceInstanceCache>,
    /// gRPC 配置
    config: GrpcConfig,
    /// 注册中心实例（用于缓存穿透时主动订阅）
    registry: Arc<dyn ServiceRegistry>,
    /// 缓存的 gRPC 客户端（service_name → CachedClient）
    clients: RwLock<HashMap<String, CachedClient>>,
}

impl VoloGrpcClient {
    /// 创建新的 gRPC 客户端
    pub fn new(cache: Arc<ServiceInstanceCache>, config: GrpcConfig, registry: Arc<dyn ServiceRegistry>) -> Self {
        Self {
            cache,
            config,
            registry,
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// 创建指定服务的 gRPC 客户端（double-check locking 防止并发重复创建）
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    async fn get_client(
        &self,
        service_name: &str,
    ) -> Result<CmxServiceOrchestratorClient, RpcError> {
        // 快查：读锁检查缓存
        if let Some(cached) = self.clients.read().await.get(service_name) {
            return Ok(cached.client.clone());
        }

        // 慢路径：写锁 + double-check
        let mut clients = self.clients.write().await;
        // 再次检查，防止并发时多个线程都进入写锁路径
        if let Some(cached) = clients.get(service_name) {
            return Ok(cached.client.clone());
        }

        // 如果实例缓存中没有该服务，主动拉取
        if self.cache.get(service_name).map_or(true, |v| v.is_empty()) {
            let instances = self.registry.query_instances(
                service_name,
                self.config.default_group.as_deref(),
                self.config.default_clusters.clone(),
            ).await
                .map_err(|e| RpcError::NoAvailableInstance(format!("服务 '{}' 查询失败: {}", service_name, e)))?;
            self.cache.update(service_name, instances);

            if self.cache.get(service_name).map_or(true, |v| v.is_empty()) {
                return Err(RpcError::NoAvailableInstance(service_name.to_string()));
            }
        }

        // 创建 Discover 并启动监听
        let discover = RegistryAwareDiscover::new(self.cache.clone(), self.config.discover_channel_capacity);
        discover.start_watch(service_name);

        // 构建 volo gRPC 客户端，使用 volo 原生 rpc_timeout 和 connect_timeout
        let rpc_timeout = Duration::from_millis(self.config.timeout_ms);
        let connect_timeout = Duration::from_millis(self.config.connect_timeout_ms);

        let client = CmxServiceOrchestratorClientBuilder::new(service_name)
            .discover(discover.clone())
            .rpc_timeout(Some(rpc_timeout))
            .connect_timeout(connect_timeout)
            .build();

        // 缓存 client（同一写锁内完成，其他并发调用会等待）
        let cached = CachedClient {
            client: client.clone(),
            _discover: discover,
        };
        clients.insert(service_name.to_string(), cached);

        Ok(client)
    }

    /// 判断 gRPC 错误是否可重试
    ///
    /// 可重试的错误：
    /// - UNAVAILABLE：服务不可达
    /// - DEADLINE_EXCEEDED：超时
    /// - RESOURCE_EXHAUSTED：限流场景，重试可能成功
    /// - ABORTED：事务中止，可重试
    ///
    /// 不可重试的错误：INVALID_ARGUMENT、NOT_FOUND、PERMISSION_DENIED 等业务错误
    fn is_retryable_error(status: &Status) -> bool {
        matches!(
            status.code(),
            volo_grpc::Code::Unavailable
                | volo_grpc::Code::DeadlineExceeded
                | volo_grpc::Code::ResourceExhausted
                | volo_grpc::Code::Aborted
        )
    }

    /// 计算重试退避时间（指数退避，上限 800ms）
    ///
    /// 退避序列：50ms → 100ms → 200ms → 400ms → 800ms
    fn retry_backoff(attempt: usize) -> Duration {
        let backoff_ms = 50u64.saturating_mul(1u64 << attempt.min(4));
        Duration::from_millis(backoff_ms.min(800))
    }
}

/// 安全解析 JSON 字符串，解析失败时记录 warn 日志并降级为 Value::Null
fn safe_parse_json(raw: &str, context: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|e| {
        tracing::warn!(
            target: "cmx_rpc",
            error = %e,
            raw = %raw,
            context = context,
            "RPC 返回 JSON 解析失败，降级为 Null"
        );
        Value::Null
    })
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
        let total_budget = Duration::from_millis(self.config.timeout_ms);
        let deadline = start + total_budget;
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

        let max_retries = self.config.retry_count;

        for attempt in 0..=max_retries {
            // 检查总时间预算
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if !remaining.is_zero() && attempt > 0 {
                // 指数退避
                let backoff = Self::retry_backoff(attempt - 1);
                // 退避时间不超过剩余预算
                let actual_backoff = std::cmp::min(backoff, remaining);
                tokio::time::sleep(actual_backoff).await;
            }
            if remaining.is_zero() {
                return Err(RpcError::Timeout(format!(
                    "重试预算耗尽: 总耗时 {}ms",
                    start.elapsed().as_millis()
                )));
            }

            if attempt > 0 {
                tracing::debug!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    service_key = %service_key,
                    attempt = attempt,
                    remaining_ms = remaining.as_millis() as u64,
                    "RPC call_service 重试"
                );
            }

            let req = ExecuteServiceRequest {
                service_key: service_key_fs.clone(),
                input: input.clone(),
                include_steps: options.include_steps,
                debug: options.debug,
                debug_node_id: debug_node_id.clone(),
                debug_params: debug_params.clone(),
            };

            match client.execute_service(req).await {
                Ok(resp) => {
                    let elapsed = start.elapsed();
                    tracing::info!(
                        target: "cmx_rpc",
                        service_name = %service_name,
                        service_key = %service_key,
                        elapsed_us = elapsed.as_micros() as u64,
                        attempts = attempt + 1,
                        success = true,
                        "RPC call_service 完成"
                    );
                    return Ok(proto_to_call_service_response(resp.into_inner()));
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    let is_retryable = Self::is_retryable_error(&e);

                    if is_retryable && attempt < max_retries {
                        tracing::warn!(
                            target: "cmx_rpc",
                            service_name = %service_name,
                            service_key = %service_key,
                            elapsed_us = elapsed.as_micros() as u64,
                            attempt = attempt + 1,
                            max_retries = max_retries,
                            error = %e,
                            "RPC call_service 失败（可重试），准备重试"
                        );
                        continue;
                    }

                    tracing::warn!(
                        target: "cmx_rpc",
                        service_name = %service_name,
                        service_key = %service_key,
                        elapsed_us = elapsed.as_micros() as u64,
                        attempts = attempt + 1,
                        success = false,
                        error = %e,
                        "RPC call_service 失败"
                    );
                    return Err(RpcError::RpcCallFailed(e.to_string()));
                }
            }
        }

        // 循环不会正常结束（所有路径都有 return 或 continue），
        // 但编译器需要返回值类型推断
        unreachable!("retry loop must return before exiting")
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
        let total_budget = Duration::from_millis(self.config.timeout_ms);
        let deadline = start + total_budget;
        let client = self.get_client(service_name).await?;

        let max_retries = self.config.retry_count;

        for attempt in 0..=max_retries {
            // 检查总时间预算
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if !remaining.is_zero() && attempt > 0 {
                let backoff = Self::retry_backoff(attempt - 1);
                let actual_backoff = std::cmp::min(backoff, remaining);
                tokio::time::sleep(actual_backoff).await;
            }
            if remaining.is_zero() {
                return Err(RpcError::Timeout(format!(
                    "重试预算耗尽: 总耗时 {}ms",
                    start.elapsed().as_millis()
                )));
            }

            if attempt > 0 {
                tracing::debug!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    plugin_id = %plugin_id,
                    function_name = %function_name,
                    attempt = attempt,
                    remaining_ms = remaining.as_millis() as u64,
                    "RPC call_function 重试"
                );
            }

            let req = CallFunctionRequest {
                plugin_id: plugin_id.to_string().into(),
                function_name: function_name.to_string().into(),
                input: input.to_string().into(),
                initial_input: None,
                debug: false,
            };

            match client.call_function(req).await {
                Ok(resp) => {
                    let elapsed = start.elapsed();
                    let inner = resp.into_inner();
                    tracing::info!(
                        target: "cmx_rpc",
                        service_name = %service_name,
                        plugin_id = %plugin_id,
                        function_name = %function_name,
                        elapsed_us = elapsed.as_micros() as u64,
                        attempts = attempt + 1,
                        success = true,
                        "RPC call_function 完成"
                    );
                    return Ok(FunctionCallResult {
                        success: inner.success,
                        result: inner
                            .result
                            .map(|s| safe_parse_json(&s, "call_function.result")),
                        elapsed_us: inner.elapsed_us,
                        error: inner.error.map(|s| s.to_string()),
                    });
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    let is_retryable = Self::is_retryable_error(&e);

                    if is_retryable && attempt < max_retries {
                        tracing::warn!(
                            target: "cmx_rpc",
                            service_name = %service_name,
                            plugin_id = %plugin_id,
                            function_name = %function_name,
                            elapsed_us = elapsed.as_micros() as u64,
                            attempt = attempt + 1,
                            max_retries = max_retries,
                            error = %e,
                            "RPC call_function 失败（可重试），准备重试"
                        );
                        continue;
                    }

                    tracing::warn!(
                        target: "cmx_rpc",
                        service_name = %service_name,
                        plugin_id = %plugin_id,
                        function_name = %function_name,
                        elapsed_us = elapsed.as_micros() as u64,
                        attempts = attempt + 1,
                        success = false,
                        error = %e,
                        "RPC call_function 失败"
                    );
                    return Err(RpcError::RpcCallFailed(e.to_string()));
                }
            }
        }

        unreachable!("retry loop must return before exiting")
    }
}

/// 将 protobuf 响应转换为 CallServiceResponse
fn proto_to_call_service_response(resp: ExecuteServiceResponse) -> CallServiceResponse {
    CallServiceResponse {
        success: resp.success,
        output: resp.output.map(|v| safe_parse_json(&v, "call_service.output")),
        steps: resp.steps.into_iter().map(|s| cmx_core::ExecutionStep {
            node_id: s.node_id.to_string(),
            node_name: s.node_name.to_string(),
            node_type: s.node_type.to_string(),
            status: parse_step_status(&s.status),
            output: s.output.map(|v| safe_parse_json(&v, "step.output")),
            elapsed_us: s.elapsed_us,
            error: s.error.map(|e| e.to_string()),
            previous_output: s.previous_output.map(|v| safe_parse_json(&v, "step.previous_output")),
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
        _ => {
            tracing::warn!(
                target: "cmx_rpc",
                raw_status = %status,
                "收到未知的 StepStatus 字符串，按 Failed 处理（请升级 cmx-core 或检查版本对齐）"
            );
            cmx_core::StepStatus::Failed
        }
    }
}
