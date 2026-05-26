use std::collections::HashMap;

use async_trait::async_trait;
use cmx_core::CallServiceResponse;
use serde_json::Value;

use crate::error::TraitError;


/// 服务调用选项
#[derive(Debug, Clone)]
pub struct ServiceInvokeOptions {
    /// 是否返回各步骤执行详情
    pub include_steps: bool,
    /// 是否调试模式
    pub debug: bool,
    /// 调试目标节点ID
    pub debug_node_id: Option<String>,
    /// 调试参数
    pub debug_params: Option<HashMap<String, String>>,
}

impl Default for ServiceInvokeOptions {
    fn default() -> Self {
        Self {
            include_steps: false,
            debug: false,
            debug_node_id: None,
            debug_params: None,
        }
    }
}

#[async_trait]
pub trait ServiceInvoker: Send + Sync {
    async fn invoke_service(
        &self,
        service_key: &str,
        input: Value,
        options: ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, TraitError>;
}
