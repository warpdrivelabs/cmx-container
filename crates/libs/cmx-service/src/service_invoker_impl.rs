//! 服务调用器实现
//!
//! 组合 RuntimeInvoker + PluginQuery + ServiceQuery，
//! 通过 Orchestrator 执行完整的服务编排流程。

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use cmx_core::model::service::SVRContext;
use cmx_core::{CallServiceResponse, OrchestrationError};
use cmx_traits::{
    PluginQuery, RuntimeInvoker, ServiceInvoker, ServiceInvokeOptions,
    ServiceQuery, TraitError,
};

use crate::orchestrator::ExecuteOptions;
use crate::orchestrator::Orchestrator;

/// 服务调用器实现
///
/// 组合 RuntimeInvoker + PluginQuery + ServiceQuery，
/// 通过 Orchestrator 执行完整的服务编排流程。
pub struct ServiceInvokerImpl {
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    service_query: Arc<dyn ServiceQuery>,
    default_db_id: String,
}

impl ServiceInvokerImpl {
    /// 创建服务调用器
    pub fn new(
        runtime: Arc<dyn RuntimeInvoker>,
        plugin_query: Arc<dyn PluginQuery>,
        service_query: Arc<dyn ServiceQuery>,
        default_db_id: String,
    ) -> Self {
        Self {
            runtime,
            plugin_query,
            service_query,
            default_db_id,
        }
    }
}

#[async_trait]
impl ServiceInvoker for ServiceInvokerImpl {
    async fn invoke_service(
        &self,
        service_key: &str,
        input: serde_json::Value,
        options: ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, TraitError> {
        let svr_context = SVRContext::new(
            input,
            std::collections::HashMap::new(),
            Utc::now(),
            uuid::Uuid::new_v4().to_string(),
        );

        let exec_options = ExecuteOptions::new(options.include_steps)
            .with_debug(options.debug, options.debug_node_id, options.debug_params);

        let orchestrator = Orchestrator::new(
            self.runtime.clone(),
            self.plugin_query.clone(),
            self.service_query.clone(),
            self.default_db_id.clone(),
        );

        let result = orchestrator
            .execute_service(service_key, svr_context, exec_options)
            .await
            .map_err(|e| TraitError::Internal(format!("服务编排执行失败: {}", e)))?;

        Ok(CallServiceResponse {
            success: result.success,
            output: result.output,
            steps: result.steps,
            total_elapsed_us: Some(result.total_elapsed_us),
            error: result.error.map(|e| OrchestrationError { message: e.message }),
        })
    }
}
