use async_trait::async_trait;
use serde_json::Value;

use crate::service_invoker::ServiceInvokeOptions;

/// RPC 调用错误类型
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// 服务未找到
    #[error("服务未找到: {0}")]
    ServiceNotFound(String),
    /// 无可用实例
    #[error("无可用实例: {0}")]
    NoAvailableInstance(String),
    /// RPC 调用失败
    #[error("RPC 调用失败: {0}")]
    RpcCallFailed(String),
    /// 不支持的协议
    #[error("不支持的协议: {0}")]
    UnsupportedProtocol(String),
    /// 调用超时
    #[error("调用超时: {0}")]
    Timeout(String),
}

/// 插件函数调用结果
///
/// RPC 方式调用远程插件函数后的返回结果。
/// 与 cmx-api 中的 FunctionCallResponse 字段一致，但定义在 cmx-traits 中避免反向依赖。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionCallResult {
    /// 是否执行成功
    pub success: bool,
    /// 函数执行结果（JSON 格式，失败时为 None）
    pub result: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 错误信息（成功时为 None）
    pub error: Option<String>,
}

/// RPC 调用统一接口（策略模式 — 策略接口）
///
/// # 参数说明
/// - `service_name`: 注册中心的服务名，用于发现目标服务实例
/// - `service_key`: 服务编排的唯一标识（call_service 使用）
/// - `plugin_id` / `function_name`: 插件函数标识（call_function 使用）
#[async_trait]
pub trait RpcClient: Send + Sync {
    /// 调用远程服务编排（对应 POST /api/service/execute）
    async fn call_service(
        &self,
        service_name: &str,
        service_key: &str,
        input: Value,
        options: ServiceInvokeOptions,
    ) -> Result<cmx_core::CallServiceResponse, RpcError>;

    /// 调用远程插件函数（对应 POST /api/service/call）
    async fn call_function(
        &self,
        service_name: &str,
        plugin_id: &str,
        function_name: &str,
        input: Value,
    ) -> Result<FunctionCallResult, RpcError>;
}
