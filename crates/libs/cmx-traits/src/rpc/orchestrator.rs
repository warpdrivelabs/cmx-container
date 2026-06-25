//! 服务编排 RPC 客户端 trait。
//!
//! 对应 gRPC 服务 `CmxServiceOrchestrator`，负责跨实例的服务编排和插件函数调用。

use async_trait::async_trait;
use serde_json::Value;

use crate::rpc::error::RpcError;
use crate::rpc::types::FunctionCallResult;
use crate::service::invoker::ServiceInvokeOptions;

/// 服务编排 RPC 客户端接口（策略模式 — 策略接口）。
///
/// 对应 gRPC 服务 `CmxServiceOrchestrator`，负责跨实例的服务编排和插件函数调用。
///
/// # Arguments
///
/// - `service_name`: 注册中心的服务名，用于发现目标服务实例。
/// - `service_key`: 服务编排的唯一标识（`call_service` 使用）。
/// - `plugin_id` / `function_name`: 插件函数标识（`call_function` 使用）。
#[async_trait]
pub trait ServiceOrchestrationClient: Send + Sync {
    /// 调用远程服务编排（对应 `POST /api/service/execute`）。
    ///
    /// # Arguments
    ///
    /// * `service_name` - 注册中心的服务名，用于发现目标服务实例。
    /// * `service_key` - 服务编排的唯一标识。
    /// * `input` - 输入数据（JSON）。
    /// * `options` - 服务调用选项。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`cmx_core::CallServiceResponse`]。
    ///
    /// # Errors
    ///
    /// * [`RpcError::ServiceNotFound`] - 服务未找到。
    /// * [`RpcError::NoAvailableInstance`] - 无可用实例。
    /// * [`RpcError::Timeout`] - 调用超时。
    async fn call_service(
        &self,
        service_name: &str,
        service_key: &str,
        input: Value,
        options: ServiceInvokeOptions,
    ) -> Result<cmx_core::CallServiceResponse, RpcError>;

    /// 调用远程插件函数（对应 `POST /api/service/call`）。
    ///
    /// # Arguments
    ///
    /// * `service_name` - 注册中心的服务名，用于发现目标服务实例。
    /// * `plugin_id` - 插件 ID。
    /// * `function_name` - 插件函数名。
    /// * `input` - 输入数据（JSON）。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`FunctionCallResult`]。
    ///
    /// # Errors
    ///
    /// * [`RpcError::ServiceNotFound`] - 服务未找到。
    /// * [`RpcError::NoAvailableInstance`] - 无可用实例。
    /// * [`RpcError::Timeout`] - 调用超时。
    async fn call_function(
        &self,
        service_name: &str,
        plugin_id: &str,
        function_name: &str,
        input: Value,
    ) -> Result<FunctionCallResult, RpcError>;
}
