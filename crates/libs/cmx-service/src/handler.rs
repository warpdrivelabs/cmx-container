//! HTTP Handler
//!
//! 提供 cmx-api 可调用的 HTTP 处理器，封装服务层逻辑。

use std::sync::Arc;

use cmx_traits::{PluginQuery, RuntimeInvoker};

use crate::error::ServiceError;
use crate::orchestrator::Orchestrator;
use crate::request::{InvokeRequest, InvokeResponse, OrchestrateRequest, OrchestrateResponse};
use crate::service::CmxService;

/// 服务处理器
///
/// 封装 CmxService 和 Orchestrator，提供统一的 HTTP 处理入口。
pub struct ServiceHandler {
    /// 核心服务
    service: Arc<CmxService>,
    /// 编排执行器
    orchestrator: Orchestrator,
}

impl ServiceHandler {
    /// 创建新的服务处理器
    ///
    /// # 参数
    ///
    /// * `service` - 核心服务实例
    pub fn new(service: Arc<CmxService>) -> Self {
        let orchestrator = Orchestrator::new(
            service.runtime().clone(),
            service.plugin_query().clone(),
        );
        Self { service, orchestrator }
    }

    /// 从组件创建服务处理器
    pub fn from_components(
        plugin_query: Arc<dyn PluginQuery>,
        runtime: Arc<dyn RuntimeInvoker>,
    ) -> Self {
        let service = Arc::new(CmxService::with_defaults(plugin_query.clone(), runtime.clone()));
        Self::new(service)
    }

    /// 获取核心服务引用
    pub fn service(&self) -> &Arc<CmxService> {
        &self.service
    }

    /// 处理单次调用请求
    ///
    /// # 参数
    ///
    /// * `request` - 调用请求
    ///
    /// # 返回值
    ///
    /// 返回调用响应。
    pub async fn handle_invoke(&self, request: InvokeRequest) -> InvokeResponse {
        match self.service.invoke(&request).await {
            Ok(response) => response,
            Err(e) => InvokeResponse {
                success: false,
                output: None,
                elapsed_us: 0,
                fuel_consumed: 0,
                error: Some(e.to_string()),
            },
        }
    }

    /// 处理编排执行请求
    ///
    /// # 参数
    ///
    /// * `request` - 编排请求
    ///
    /// # 返回值
    ///
    /// 返回编排响应。
    pub async fn handle_orchestrate(&self, request: OrchestrateRequest) -> OrchestrateResponse {
        match self.orchestrator
            .execute(&request.orchestration, &request.initial_input)
            .await
        {
            Ok(result) => OrchestrateResponse {
                success: result.success,
                final_output: result.final_output,
                step_results: result.step_results,
                total_elapsed_us: result.total_elapsed_us,
                error: result.error,
            },
            Err(e) => OrchestrateResponse {
                success: false,
                final_output: None,
                step_results: Vec::new(),
                total_elapsed_us: 0,
                error: Some(e.to_string()),
            },
        }
    }

    /// 执行预定义编排
    ///
    /// 从数据库或配置加载编排定义并执行。
    ///
    /// # 参数
    ///
    /// * `orchestration_id` - 编排ID
    /// * `input` - 输入数据
    /// * `db_id` - 数据库ID
    /// * `request_id` - 请求ID
    ///
    /// # 返回值
    ///
    /// 返回编排响应。
    pub async fn execute_orchestration(
        &self,
        _orchestration_id: &str,
        _input: &serde_json::Value,
        _db_id: Option<&str>,
        _request_id: Option<&str>,
    ) -> Result<OrchestrateResponse, ServiceError> {
        // TODO: 从数据库或配置加载编排定义
        Err(ServiceError::InternalError(
            "编排定义加载尚未实现".to_string(),
        ))
    }
}
