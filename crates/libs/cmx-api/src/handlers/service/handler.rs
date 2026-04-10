//! 服务调用 Handler 实现
//!
//! 处理 WASM 插件服务调用和编排执行的 HTTP 请求。

use std::collections::HashMap;
use std::sync::Arc;
use axum::{
    extract::{Query, State},
    Json,
};
use axum::extract::Path;
use cmx_service::{InvokeRequest, InvokeResponse, OrchestrateRequest, OrchestrateResponse};
use cmx_traits::{CallerData, PluginQuery, RuntimeInvoker, ServiceQuery};
use serde::{Deserialize, Serialize};

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Error;

/// 服务调用 Handler
///
/// 处理 POST /api/service/call 请求，调用 WASM 插件函数。
#[utoipa::path(
    post,
    path = "/api/service/call",
    request_body = InvokeRequest,
    responses(
        (status = 200, description = "调用成功", body = ApiResp<InvokeResponse>)
    ),
    tag = "Service"
)]
pub async fn service_call(
    State(state): State<CmxAppState>,
    Json(req): Json<InvokeRequest>,
) -> Result<Json<ApiResp<InvokeResponse>>, Error> {
    // 获取运行时调用器
    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    // 获取插件查询器
    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // 检查插件是否已安装
    let is_install = plugin_query.is_installed(&req.plugin_id).await
        .map_err(|e| Error::internal_error(format!("检查插件安装状态失败: {}", e)))?;

    if !is_install {
        return Err(Error::business_error(format!("插件 {} 未安装", req.plugin_id)));
    }

    // 检查 WASM 模块是否已加载
    let is_loaded = runtime.is_loaded(&req.plugin_id).await;

    if !is_loaded {
        // 尝试加载 WASM 模块
        let wasm_path = plugin_query.get_wasm_path(&req.plugin_id).await
            .map_err(|e| Error::internal_error(format!("获取 WASM 路径失败: {}", e)))?;

        runtime.load_module(&req.plugin_id, &wasm_path).await
            .map_err(|e| Error::internal_error(format!("加载 WASM 模块失败: {}", e)))?;
    }

    // 构建调用上下文
    let db_id = req.db_id.as_deref().unwrap_or("default");
    let mut caller_data = CallerData::new(&req.plugin_id, db_id);

    if let Some(ref req_id) = req.request_id {
        caller_data = caller_data.with_request_id(req_id);
    }
    if let Some(ref tenant_id) = req.tenant_id {
        caller_data = caller_data.with_tenant_id(tenant_id);
    }

    // 序列化输入
    let input_bytes = serde_json::to_vec(&req.input)
        .map_err(|e| Error::bad_request(format!("输入数据序列化失败: {}", e)))?;

    // 调用 WASM 函数
    let result = runtime.invoke(&req.plugin_id, &req.function_name, &input_bytes, &caller_data).await
        .map_err(|e| Error::internal_error(format!("WASM 调用失败: {}", e)))?;

    // 解析输出
    let output = if result.output.is_empty() {
        None
    } else {
        serde_json::from_slice(&result.output)
            .map_err(|e| Error::internal_error(format!("输出数据解析失败: {}", e)))?
    };

    let response = InvokeResponse {
        success: true,
        output,
        elapsed_us: result.elapsed_us,
        fuel_consumed: result.fuel_consumed.unwrap_or(0),
        error: None,
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 编排执行 Handler
///
/// 处理 POST /api/service/orchestration 请求，执行插件编排。
#[utoipa::path(
    post,
    path = "/api/service/orchestration",
    request_body = OrchestrateRequest,
    responses(
        (status = 200, description = "编排执行成功", body = ApiResp<OrchestrateResponse>)
    ),
    tag = "Service"
)]
pub async fn execute_orchestration(
    State(state): State<CmxAppState>,
    Json(req): Json<OrchestrateRequest>,
) -> Result<Json<ApiResp<OrchestrateResponse>>, Error> {
    // 获取运行时调用器
    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    // 获取插件查询器
    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // 创建编排执行器
    let orchestrator = cmx_service::Orchestrator::new(runtime.clone(), plugin_query.clone());

    // 构建调用上下文
    let db_id = req.db_id.as_deref().unwrap_or("default");
    let mut caller_data = CallerData::new("__orchestration__", db_id);

    if let Some(ref req_id) = req.request_id {
        caller_data = caller_data.with_request_id(req_id);
    }
    if let Some(ref tenant_id) = req.tenant_id {
        caller_data = caller_data.with_tenant_id(tenant_id);
    }

    // 执行编排
    let result = orchestrator.execute(&req.orchestration, &req.initial_input, &caller_data).await
        .map_err(|e| Error::internal_error(format!("编排执行失败: {}", e)))?;

    let response = OrchestrateResponse {
        success: result.success,
        final_output: result.final_output,
        step_results: result.step_results,
        total_elapsed_us: result.total_elapsed_us,
        error: result.error,
    };

    Ok(Json(ApiResp::ok(response)))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExecuteRequest {
    pub service_key: String,
    pub input: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExecuteResponse {
    pub success: bool,
    pub output: Option<String>,
    pub steps: Vec<ServiceExecutionStep>,
    pub total_elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceExecutionStep {
    pub node_id: String,
    pub node_name: String,
    pub output: Option<String>,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceGetQuery {
    pub service_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceByPluginQuery {
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDeleteQuery {
    pub service_key: String,
}

/// 执行服务编排
pub async fn execute_service(
    State(state): State<CmxAppState>,
    Json(req): Json<ServiceExecuteRequest>,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    let orchestrator = cmx_service::OrchestratorV2::new(
        runtime.clone(),
        plugin_query.clone(),
        service_query.clone(),
    );

    let caller_data = CallerData::new("__service__", "default");

    let result = orchestrator.execute_service(
        &req.service_key,
        &req.input,
        req.headers,
        &caller_data,
    ).await
        .map_err(|e| Error::internal_error(format!("服务执行失败: {}", e)))?;

    let response = ServiceExecuteResponse {
        success: result.success,
        output: result.output,
        steps: result.steps.into_iter().map(|s| ServiceExecutionStep {
            node_id: s.node_id,
            node_name: s.node_name,
            output: s.output,
            elapsed_us: s.elapsed_us,
        }).collect(),
        total_elapsed_us: result.total_elapsed_us,
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 获取服务列表
pub async fn list_services(
    State(state): State<CmxAppState>,
) -> Result<Json<ApiResp<Vec<serde_json::Value>>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let services = service_query.list_active_services().await
        .map_err(|e| Error::internal_error(format!("获取服务列表失败: {}", e)))?;

    let values: Vec<serde_json::Value> = services.into_iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "service_key": s.service_key,
            "service_name": s.service_name,
            "description": s.description,
            "plugin_id": s.plugin_id,
            "status": s.status,
            "version": s.version,
        })
    }).collect();

    Ok(Json(ApiResp::ok(values)))
}

/// 获取服务定义
pub async fn get_service(
    State(state): State<CmxAppState>,
    Query(query): Query<ServiceGetQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let service = service_query.get_service(&query.service_key).await
        .map_err(|e| Error::internal_error(format!("获取服务失败: {}", e)))?;

    match service {
        Some(s) => {
            let value = serde_json::json!({
                "id": s.id,
                "service_key": s.service_key,
                "service_name": s.service_name,
                "description": s.description,
                "plugin_id": s.plugin_id,
                "status": s.status,
                "version": s.version,
            });
            Ok(Json(ApiResp::ok(value)))
        }
        None => Err(Error::business_error(format!("服务 {} 不存在", query.service_key))),
    }
}

/// 获取插件的所有服务
pub async fn get_services_by_plugin(
    State(state): State<CmxAppState>,
    Query(query): Query<ServiceByPluginQuery>,
) -> Result<Json<ApiResp<Vec<serde_json::Value>>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let services = service_query.get_services_by_plugin(&query.plugin_id).await
        .map_err(|e| Error::internal_error(format!("获取插件服务失败: {}", e)))?;

    let values: Vec<serde_json::Value> = services.into_iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "service_key": s.service_key,
            "service_name": s.service_name,
            "description": s.description,
            "plugin_id": s.plugin_id,
            "status": s.status,
            "version": s.version,
        })
    }).collect();

    Ok(Json(ApiResp::ok(values)))
}

/// 删除服务
pub async fn delete_service(
    State(state): State<CmxAppState>,
    Path(service_key): Path<String>,
) -> Result<Json<ApiResp<()>>, Error> {
    let service_storage: &Arc<dyn cmx_traits::ServiceStorage> = state.service_storage()
        .ok_or_else(|| Error::internal_error("服务存储未初始化"))?;

    service_storage.delete_service(&service_key).await
        .map_err(|e| Error::internal_error(format!("删除服务失败: {}", e)))?;

    Ok(Json(ApiResp::ok(())))
}
