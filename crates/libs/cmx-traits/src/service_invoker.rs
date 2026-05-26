//! 服务调用器 trait 定义
//!
//! 封装服务编排执行能力，供宿主函数等场景使用。
//! 实现类内部组合 RuntimeInvoker + PluginQuery + ServiceQuery，
//! 通过 Orchestrator 执行完整的服务编排流程。

use std::collections::HashMap;

use async_trait::async_trait;
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

/// 服务调用结果
#[derive(Debug, Clone)]
pub struct ServiceInvokeResult {
    /// 是否成功
    pub success: bool,
    /// 最终输出
    pub output: Option<Value>,
    /// 错误信息
    pub error: Option<String>,
    /// 总耗时（微秒）
    pub elapsed_us: Option<u64>,
}

/// 服务调用器 trait
///
/// 封装服务编排执行能力，供宿主函数等场景使用。
/// 实现类内部组合 RuntimeInvoker + PluginQuery + ServiceQuery，
/// 通过 Orchestrator 执行完整的服务编排流程。
#[async_trait]
pub trait ServiceInvoker: Send + Sync {
    /// 调用服务编排
    ///
    /// # 参数
    /// - `service_key`: 服务唯一标识
    /// - `input`: 传递给服务的输入数据
    /// - `options`: 调用选项
    async fn invoke_service(
        &self,
        service_key: &str,
        input: Value,
        options: ServiceInvokeOptions,
    ) -> Result<ServiceInvokeResult, TraitError>;
}
