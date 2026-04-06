//! 服务调用 Handler 实现
//!
//! 处理 WASM 插件服务调用和编排执行的 HTTP 请求。

use axum::{
    extract::State,
    Json,
};
use cmx_service::{InvokeRequest, InvokeResponse, OrchestrateRequest, OrchestrateResponse};
use cmx_traits::{CallerData, PluginQuery, RuntimeInvoker};
use std::sync::Arc;

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Error;

/// 服务调用 Handler
///
/// 处理 POST /api/service/call 请求，调用 WASM 插件函数。
///
/// # 请求体
///
/// ```json
/// {
///     "plugin_id": "my-plugin",
///     "function_name": "handle_request",
///     "input": {"data": "value"},
///     "db_id": "default",
///     "request_id": "req-001",
///     "tenant_id": null
/// }
/// ```
///
/// # 响应
///
/// ```json
/// {
///     "code": 0,
///     "message": "success",
///     "data": {
///         "success": true,
///         "output": {"result": "processed"},
///         "elapsed_us": 1234,
///         "fuel_consumed": 5000,
///         "error": null
///     }
/// }
/// ```
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

    // 检查插件是否已激活
    let is_active = plugin_query.is_active(&req.plugin_id).await
        .map_err(|e| Error::internal_error(format!("检查插件状态失败: {}", e)))?;

    if !is_active {
        return Err(Error::bad_request(format!("插件 {} 未激活", req.plugin_id)));
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
///
/// # 请求体
///
/// ```json
/// {
///     "orchestration": {
///         "id": "order-flow",
///         "name": "订单处理流程",
///         "steps": [
///             {
///                 "step_id": "validate",
///                 "plugin_id": "validator-plugin",
///                 "function_name": "validate_order",
///                 "input": {"type": "static", "value": {"order_id": "12345"}},
///                 "parallel": false
///             }
///         ]
///     },
///     "initial_input": {},
///     "db_id": "default"
/// }
/// ```
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
