//! 服务调用 Handler 实现
//!
//! 处理 WASM 插件服务调用和编排执行的 HTTP 请求。
//!
//! # API 端点
//!
//! | 方法 | 路径 | 功能 |
//! |------|------|------|
//! | POST | /api/service/call | 直接调用 WASM 插件函数 |
//! | POST | /api/service/execute | 执行服务编排 |
//! | POST | /api/service/execute/{service-key} | 执行服务编排（路径参数版本） |
//! | GET | /api/service/list | 获取服务列表 |
//! | POST | /api/service/page | 分页查询服务 |
//! | GET | /api/service/get | 获取服务定义 |
//! | GET | /api/service/by-plugin | 获取插件的所有服务 |
//! | POST | /api/service/delete | 删除服务 |

// ==================== 依赖导入 ====================

use std::collections::HashMap;
use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum::http::HeaderMap;
use cmx_core::model::service::{FunctionInput, FunctionOutput, SVRContext};
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceQuery, ServicePageFilter};
use log::error;
use cmx_core::PageParams;
use cmx_database::get_default_db_manager;
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Error;
use crate::middleware::CmxSvrContext;
// 导入请求/响应结构体
use super::models::{
    FunctionCallRequest, FunctionCallResponse,
    ServiceExecuteRequest, ServiceExecuteResponse, ServiceExecutionStep,
    ServiceGetQuery, ServiceByPluginQuery,
    ServiceListItem, ServiceDetailResponse,
};

// ==================== 辅助函数 ====================

/// 将 HeaderMap 转换为 HashMap<String, String>
///
/// # 参数
/// * `headers` - HTTP 请求头
///
/// # 返回值
/// 返回标准库 HashMap
fn extract_headers(headers: &HeaderMap) -> std::collections::HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|s| (k.to_string(), s.to_string()))
        })
        .collect()
}

/// 将 serde_json::Value 转换为字符串
///
/// 支持 JSON 对象或字符串直接传递
///
/// # 参数
/// * `value` - JSON 值
///
/// # 返回值
/// 返回字符串格式的 JSON
fn value_to_string(value: serde_json::Value) -> Result<String, Error> {
    if value.is_string() {
        Ok(value.as_str().unwrap_or("").to_string())
    } else {
        serde_json::to_string(&value)
            .map_err(|e| Error::bad_request(format!("输入数据序列化失败: {}", e)))
    }
}

// ==================== 函数直接调用 Handler ====================

/// 执行插件函数
///
/// 处理 POST /api/service/call 请求，直接调用指定插件的函数。
///
/// # 参数
/// - `state`: 应用状态（包含运行时调用器、插件查询器等）
/// - `req`: 请求体（FunctionCallRequest）
/// - `_svr_ctx`: 服务上下文（CmxSvrContext）
/// - `headers`: HTTP 请求头
///
/// # 请求体
/// - `plugin_id`: 插件ID
/// - `function_name`: 函数名
/// - `input`: 输入数据（支持 JSON 对象或字符串）
///
/// # 响应体
/// - `success`: 是否成功
/// - `result`: 函数执行结果
/// - `elapsed_us`: 执行耗时（微秒）
/// - `error`: 错误信息
#[utoipa::path(
    post,
    path = "/api/service/call",
    request_body = FunctionCallRequest,
    responses(
        (status = 200, description = "调用成功", body = ApiResp<FunctionCallResponse>)
    ),
    tag = "Service"
)]
pub async fn service_call(
    State(state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(req): Json<FunctionCallRequest>,
) -> Result<Json<ApiResp<FunctionCallResponse>>, Error> {
    // ==================== 获取依赖组件 ====================

    // 从应用状态获取运行时调用器
    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    // 从应用状态获取插件查询器
    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // ==================== 检查插件状态 ====================

    let is_install = plugin_query.is_installed(&req.plugin_id).await
        .map_err(|e| Error::internal_error(format!("检查插件安装状态失败: {}", e)))?;

    if !is_install {
        return Err(Error::business_error(format!("插件 {} 未安装", req.plugin_id)));
    }

    // ==================== 加载 WASM 模块 ====================

    let is_loaded = runtime.is_loaded(&req.plugin_id).await;

    if !is_loaded {
        let wasm_path = plugin_query.get_wasm_path(&req.plugin_id).await
            .map_err(|e| Error::internal_error(format!("获取 WASM 路径失败: {}", e)))?;

        runtime.load_module(&req.plugin_id, &wasm_path).await
            .map_err(|e| Error::internal_error(format!("加载 WASM 模块失败: {}", e)))?;
    }

    // ==================== 提取请求头和 input ====================

    let svr_headers = extract_headers(&headers);
    let input_str = value_to_string(req.input)?;

    // ==================== 构建 FunctionInput ====================

    let svr_context = SVRContext::new(input_str.clone(), svr_headers);
    let func_input = FunctionInput {
        input: input_str,
        context: svr_context,
        binary_data: HashMap::new(),
    };

    // ==================== 调用 WASM 函数 ====================

    let start_time = std::time::Instant::now();
    let input_bytes = serde_json::to_vec(&func_input)
        .map_err(|e| Error::bad_request(format!("输入数据序列化失败: {}", e)))?;

    let invoke_result = runtime.invoke(&req.plugin_id, &req.function_name, &input_bytes).await
        .map_err(|e| Error::internal_error(format!("WASM 调用失败: {}", e)))?;

    let elapsed_us = start_time.elapsed().as_micros() as u64;

    // ==================== 解析 FunctionOutput ====================

    let output: FunctionOutput = if invoke_result.output.is_empty() {
        FunctionOutput::new(String::new())
    } else {
        serde_json::from_slice(&invoke_result.output)
            .map_err(|e| Error::internal_error(format!("输出数据解析失败: {}", e)))?
    };

    // ==================== 构建响应 ====================

    let response = FunctionCallResponse {
        success: true,
        result: Some(output.result),
        elapsed_us,
        error: None,
    };

    Ok(Json(ApiResp::ok(response)))
}

// ==================== 服务编排 Handler ====================

/// 执行服务编排（内部实现）
///
/// 核心逻辑：通过 service_key 查询服务定义，加载插件，执行编排流程。
///
/// # 参数
/// - `state`: 应用状态
/// - `service_key`: 服务唯一标识
/// - `input`: 输入数据（JSON 字符串）
/// - `svr_headers`: HTTP 请求头
///
/// # 返回值
/// 返回服务执行结果
async fn execute_service_inner(
    state: &CmxAppState,
    service_key: &str,
    input: String,
    svr_headers: std::collections::HashMap<String, String>,
) -> Result<ServiceExecuteResponse, Error> {
    // ==================== 获取依赖组件 ====================

    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // ==================== 创建编排执行器并执行 ====================

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let orchestrator = cmx_service::OrchestratorV2::new(
        runtime.clone(),
        plugin_query.clone(),
        service_query.clone(),
        default_db_id,
    );

    let result = orchestrator.execute_service(
        service_key,
        &input,
        svr_headers,
    ).await
        .map_err(|e| {
            error!("服务{}执行失败: {:?}", service_key, e);
            return  Error::internal_error(format!("服务执行失败: {}", e));
        })?;

    // ==================== 构建响应 ====================

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

    Ok(response)
}

/// 执行服务编排
///
/// 处理 POST /api/service/execute 请求，执行服务编排。
///
/// # 参数
/// - `state`: 应用状态
/// - `req`: 请求体（ServiceExecuteRequest）
/// - `_svr_ctx`: 服务上下文（CmxSvrContext）
/// - `headers`: HTTP 请求头
///
/// # 请求体
/// - `service_key`: 服务唯一标识
/// - `input`: 初始输入数据（支持 JSON 对象或字符串）
///
/// # 响应体
/// - `success`: 是否成功
/// - `output`: 最终输出
/// - `steps`: 各步骤执行记录
/// - `total_elapsed_us`: 总耗时
#[utoipa::path(
    post,
    path = "/api/service/execute",
    request_body = ServiceExecuteRequest,
    responses(
        (status = 200, description = "服务编排执行成功", body = ApiResp<ServiceExecuteResponse>)
    ),
    tag = "Service"
)]
pub async fn execute_service(
    State(state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(req): Json<ServiceExecuteRequest>,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    // ==================== 提取请求头和 input ====================

    let svr_headers = extract_headers(&headers);
    let input_str = value_to_string(req.input)?;

    // ==================== 执行服务编排 ====================

    let response = execute_service_inner(&state, &req.service_key, input_str, svr_headers).await?;

    Ok(Json(ApiResp::ok(response)))
}

/// 执行服务编排（路径参数版本）
///
/// 处理 POST /api/service/execute/{service-key} 请求，执行服务编排。
/// service_key 从 URL 路径获取，优先于请求体中的 service_key。
///
/// # 参数
/// - `state`: 应用状态
/// - `service_key`: 服务唯一标识（从 URL 路径获取）
/// - `_svr_ctx`: 服务上下文（CmxSvrContext）
/// - `headers`: HTTP 请求头
/// - `req`: 请求体（ServiceExecuteRequest）
///
/// # 路径参数
/// - `service-key`: 服务唯一标识
///
/// # 请求体
/// - `input`: 初始输入数据（支持 JSON 对象或字符串）
/// - `service_key`: 会被路径参数覆盖
///
/// # 响应体
/// - `success`: 是否成功
/// - `output`: 最终输出
/// - `steps`: 各步骤执行记录
/// - `total_elapsed_us`: 总耗时
#[utoipa::path(
    post,
    path = "/api/service/execute/{service-key}",
    request_body = ServiceExecuteRequest,
    responses(
        (status = 200, description = "服务编排执行成功", body = ApiResp<ServiceExecuteResponse>)
    ),
    tag = "Service"
)]
pub async fn execute_service_by_key(
    State(state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Path(service_key): Path<String>,
    Json(req): Json<ServiceExecuteRequest>,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    // ==================== 提取请求头和 input ====================

    let svr_headers = extract_headers(&headers);
    let input_str = value_to_string(req.input)?;

    // ==================== 执行服务编排（路径参数优先） ====================

    let response = execute_service_inner(&state, &service_key, input_str, svr_headers).await?;

    Ok(Json(ApiResp::ok(response)))
}

// ==================== 服务查询 Handler ====================

/// 获取服务列表
///
/// 处理 GET /api/service/list 请求，返回所有启用的服务。
///
/// # 参数
/// - `state`: 应用状态
///
/// # 响应体
/// 返回服务信息数组
#[utoipa::path(
    get,
    path = "/api/service/list",
    responses(
        (status = 200, description = "获取服务列表成功", body = ApiResp<Vec<ServiceListItem>>)
    ),
    tag = "Service"
)]
pub async fn list_services(
    State(state): State<CmxAppState>,
) -> Result<Json<ApiResp<Vec<ServiceListItem>>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let services = service_query.list_active_services().await
        .map_err(|e| Error::internal_error(format!("获取服务列表失败: {}", e)))?;

    let items: Vec<ServiceListItem> = services.into_iter().map(|s| {
        ServiceListItem {
            id: s.id,
            service_key: s.service_key,
            service_name: s.service_name,
            description: s.description,
            plugin_id: s.plugin_id,
            status: s.status,
            version: s.version,
            domain_code: s.domain_code,
            application_code: s.application_code,
            module_code: s.module_code,
            domain_name: s.domain_name,
            application_name: s.application_name,
            module_name: s.module_name,
        }
    }).collect();

    Ok(Json(ApiResp::ok(items)))
}

/// 获取服务定义
///
/// 处理 GET /api/service/get 请求，返回指定服务的详细信息。
///
/// # 参数
/// - `state`: 应用状态
/// - `query`: 查询参数（service_key）
///
/// # 查询参数
/// - `service_key`: 服务唯一标识
///
/// # 响应体
/// 返回服务详情
#[utoipa::path(
    get,
    path = "/api/service/get",
    params(ServiceGetQuery),
    responses(
        (status = 200, description = "获取服务详情成功", body = ApiResp<ServiceDetailResponse>)
    ),
    tag = "Service"
)]
pub async fn get_service(
    State(state): State<CmxAppState>,
    Query(query): Query<ServiceGetQuery>,
) -> Result<Json<ApiResp<ServiceDetailResponse>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let service = service_query.get_service(&query.service_key).await
        .map_err(|e| Error::internal_error(format!("获取服务失败: {}", e)))?;

    match service {
        Some(s) => {
            let detail = ServiceDetailResponse {
                id: s.id,
                service_key: s.service_key,
                service_name: s.service_name,
                description: s.description,
                plugin_id: s.plugin_id,
                status: s.status,
                version: s.version,
                domain_code: s.domain_code,
                application_code: s.application_code,
                module_code: s.module_code,
                domain_name: s.domain_name,
                application_name: s.application_name,
                module_name: s.module_name,
            };
            Ok(Json(ApiResp::ok(detail)))
        }
        None => Err(Error::business_error(format!("服务 {} 不存在", query.service_key))),
    }
}

/// 获取插件的所有服务
///
/// 处理 GET /api/service/by-plugin 请求，返回指定插件的所有服务。
///
/// # 参数
/// - `state`: 应用状态
/// - `query`: 查询参数（plugin_id）
///
/// # 查询参数
/// - `plugin_id`: 插件ID
///
/// # 响应体
/// 返回服务信息数组
#[utoipa::path(
    get,
    path = "/api/service/by-plugin",
    params(ServiceByPluginQuery),
    responses(
        (status = 200, description = "获取插件服务列表成功", body = ApiResp<Vec<ServiceListItem>>)
    ),
    tag = "Service"
)]
pub async fn get_services_by_plugin(
    State(state): State<CmxAppState>,
    Query(query): Query<ServiceByPluginQuery>,
) -> Result<Json<ApiResp<Vec<ServiceListItem>>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let services = service_query.get_services_by_plugin(&query.plugin_id).await
        .map_err(|e| Error::internal_error(format!("获取插件服务失败: {}", e)))?;

    let items: Vec<ServiceListItem> = services.into_iter().map(|s| {
        ServiceListItem {
            id: s.id,
            service_key: s.service_key,
            service_name: s.service_name,
            description: s.description,
            plugin_id: s.plugin_id,
            status: s.status,
            version: s.version,
            domain_code: s.domain_code,
            application_code: s.application_code,
            module_code: s.module_code,
            domain_name: s.domain_name,
            application_name: s.application_name,
            module_name: s.module_name,
        }
    }).collect();

    Ok(Json(ApiResp::ok(items)))
}

// ==================== 服务分页查询 Handler ====================

/// 分页查询服务列表
///
/// 处理 POST /api/service/page 请求，支持多条件组合查询。
///
/// # 参数
/// - `state`: 应用状态
/// - `req`: 请求体（ServicePageRequest）
///
/// # 请求体
/// - `service_key`: 服务 key 模糊查询（可选）
/// - `service_name`: 服务名称模糊查询（可选）
/// - `plugin_id`: 插件 ID 精确匹配（可选）
/// - `domain_code`: 域代码精确匹配（可选）
/// - `application_code`: 应用代码精确匹配（可选）
/// - `module_code`: 模块代码精确匹配（可选）
/// - `page`: 页码（从 1 开始，默认 1）
/// - `size`: 每页大小（默认 10）
///
/// # 响应体
/// 返回分页结果
#[utoipa::path(
    post,
    path = "/api/service/page",
    request_body = crate::rest::param_doc::PageParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "分页查询成功", body = ApiResp<Vec<ServiceListItem>>)
    ),
    tag = "Service"
)]
pub async fn page_services(
    State(state): State<CmxAppState>,
    Json(params): Json<PageParams<ServicePageFilter>>,
) -> Result<Json<ApiResp<Vec<ServiceListItem>>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let filter = params.filter.clone().unwrap_or_default();
    let page = params.get_page() as u64;
    let size = params.get_size() as u64;

    let result = service_query.page_services(filter, page, size).await
        .map_err(|e| Error::internal_error(format!("分页查询失败: {}", e)))?;

    let items: Vec<ServiceListItem> = result.items.into_iter().map(|s| {
        ServiceListItem {
            id: s.id,
            service_key: s.service_key,
            service_name: s.service_name,
            description: s.description,
            plugin_id: s.plugin_id,
            status: s.status,
            version: s.version,
            domain_code: s.domain_code,
            application_code: s.application_code,
            module_code: s.module_code,
            domain_name: s.domain_name,
            application_name: s.application_name,
            module_name: s.module_name,
        }
    }).collect();

    Ok(Json(ApiResp::ok_with_pagination(
        items,
        page,
        size,
        result.total,
    )))
}

// ==================== 服务删除 Handler ====================

/// 删除服务
///
/// 处理 POST /api/service/delete 请求，物理删除服务定义及其所有版本。
///
/// # 参数
/// - `state`: 应用状态
/// - `req`: 请求体（ServiceDeleteQuery）
///
/// # 请求体
/// - `service_key`: 服务唯一标识
///
/// # 注意事项
/// - 物理删除，不可恢复
/// - 同时删除 cmx_service_define 和 cmx_service_define_version 表中的记录
#[utoipa::path(
    post,
    path = "/api/service/delete",
    request_body = crate::handlers::service::models::ServiceDeleteQuery,
    responses(
        (status = 200, description = "删除服务成功", body = crate::api_response::UnitResp)
    ),
    tag = "Service"
)]
pub async fn delete_service(
    State(state): State<CmxAppState>,
    Json(req): Json<crate::handlers::service::models::ServiceDeleteQuery>,
) -> Result<Json<crate::api_response::UnitResp>, Error> {
    let service_storage: &Arc<dyn cmx_traits::ServiceStorage> = state.service_storage()
        .ok_or_else(|| Error::internal_error("服务存储未初始化"))?;

    service_storage.delete_service(&req.service_key, None, None).await
        .map_err(|e| Error::internal_error(format!("删除服务失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("删除成功")))
}
