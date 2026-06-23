//! 服务调用 trait 定义。
//!
//! 定义跨模块的服务编排调用接口，cmx-service 的 ServiceInvoker 实现将实现此 trait，
//! 其他模块通过此 trait 调用服务编排而无需直接依赖 cmx-service。

use std::collections::HashMap;

use async_trait::async_trait;
use cmx_core::CallServiceResponse;
use serde_json::Value;

use crate::error::TraitError;

/// 服务调用选项。
#[derive(Debug, Clone, Default)]
pub struct ServiceInvokeOptions {
    /// 是否返回各步骤执行详情。
    pub include_steps: bool,
    /// 是否调试模式。
    pub debug: bool,
    /// 调试目标节点 ID。
    pub debug_node_id: Option<String>,
    /// 调试参数。
    pub debug_params: Option<HashMap<String, String>>,
}

/// 服务调用器 trait。
///
/// 供 cmx-runtime 等模块使用，用于执行服务编排。
/// cmx-service 实现此 trait，实现跨模块解耦。
#[async_trait]
pub trait ServiceInvoker: Send + Sync {
    /// 执行服务编排。
    ///
    /// # Arguments
    ///
    /// * `service_key` - 服务唯一标识。
    /// * `input` - 输入数据（JSON）。
    /// * `options` - 调用选项。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`CallServiceResponse`]。
    ///
    /// # Errors
    ///
    /// 服务不存在、编排执行失败或参数无效时返回 [`TraitError`]。
    async fn invoke_service(
        &self,
        service_key: &str,
        input: Value,
        options: ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, TraitError>;
}
