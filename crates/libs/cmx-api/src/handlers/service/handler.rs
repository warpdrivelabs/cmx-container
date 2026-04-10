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
//! | GET | /api/service/list | 获取服务列表 |
//! | GET | /api/service/get | 获取服务定义 |
//! | GET | /api/service/by-plugin | 获取插件的所有服务 |
//! | POST | /api/service/delete | 删除服务 |

// ==================== 依赖导入 ====================

use std::sync::Arc;
use axum::{
    extract::{Query, State},
    Json,
};
use cmx_core::model::service::{FunctionInput, FunctionOutput, SVRContext};
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceQuery};

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Error;

// 导入请求/响应结构体
use super::models::{
    FunctionCallRequest, FunctionCallResponse,
    ServiceExecuteRequest, ServiceExecuteResponse, ServiceExecutionStep,
    ServiceGetQuery, ServiceByPluginQuery,
    ServiceListItem, ServiceDetailResponse,
};

// ==================== 函数直接调用 Handler ====================

/// 函数调用 Handler
///
/// 处理 POST /api/service/call 请求，直接调用指定插件的函数。
///
/// # 统一入参出参
///
/// 所有 WASM 函数都使用统一的入参出参格式：
/// - **入参**: `FunctionInput` — 包含 input、context、txn_id
/// - **出参**: `FunctionOutput` — 包含 result
///
/// # 请求体
/// - `plugin_id`: 插件ID
/// - `function_name`: 函数名
/// - `input`: 输入数据（传递给 FunctionInput.input）
/// - `headers`: HTTP 请求头（传递给 FunctionInput.context.headers）
/// - `txn_id`: 事务ID（可选，传递给 FunctionInput.txn_id）
///
/// # 响应体
/// - `success`: 是否成功
/// - `result`: 函数执行结果（来自 FunctionOutput.result）
/// - `elapsed_us`: 执行耗时（微秒）
/// - `error`: 错误信息
///
/// # 执行流程
/// 1. 获取运行时调用器和插件查询器
/// 2. 检查插件是否已安装
/// 3. 加载 WASM 模块（如果未加载）
/// 4. 构建 FunctionInput（统一入参格式）
/// 5. 序列化并调用 WASM 函数
/// 6. 解析 FunctionOutput（统一出参格式）
/// 7. 返回执行结果
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
    Json(req): Json<FunctionCallRequest>,
) -> Result<Json<ApiResp<FunctionCallResponse>>, Error> {
    // ==================== 获取依赖组件 ====================
    
    // 从应用状态获取运行时调用器
    // 用于调用 WASM 模块中的函数
    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    // 从应用状态获取插件查询器
    // 用于查询插件状态和获取 WASM 文件路径
    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // ==================== 检查插件状态 ====================
    
    // 检查插件是否已安装
    let is_install = plugin_query.is_installed(&req.plugin_id).await
        .map_err(|e| Error::internal_error(format!("检查插件安装状态失败: {}", e)))?;

    // 如果插件未安装，返回业务错误
    if !is_install {
        return Err(Error::business_error(format!("插件 {} 未安装", req.plugin_id)));
    }

    // ==================== 加载 WASM 模块 ====================
    
    // 检查 WASM 模块是否已加载到运行时
    let is_loaded = runtime.is_loaded(&req.plugin_id).await;

    // 如果未加载，尝试加载 WASM 模块
    if !is_loaded {
        // 获取 WASM 文件路径
        let wasm_path = plugin_query.get_wasm_path(&req.plugin_id).await
            .map_err(|e| Error::internal_error(format!("获取 WASM 路径失败: {}", e)))?;

        // 加载 WASM 模块到运行时
        runtime.load_module(&req.plugin_id, &wasm_path).await
            .map_err(|e| Error::internal_error(format!("加载 WASM 模块失败: {}", e)))?;
    }

    // ==================== 构建统一入参 FunctionInput ====================
    
    // 创建服务调用上下文
    // - initial_input: 设置为当前输入，函数可通过 context.initial_input 获取
    // - headers: HTTP 请求头，函数可通过 context.headers 获取
    // - step_outputs: 直接调用没有前序步骤，设为空
    let svr_context = SVRContext::new(req.input.clone(), req.headers);

    // 构建统一入参结构体
    // - input: 当前输入数据
    // - context: 服务调用上下文
    // - txn_id: 事务ID（如果提供）
    let func_input = FunctionInput {
        input: req.input.clone(),
        context: svr_context,
        txn_id: req.txn_id.clone(),
    };

    // ==================== 序列化输入参数 ====================
    
    // 将 FunctionInput 序列化为 JSON 字节数组
    let input_bytes = serde_json::to_vec(&func_input)
        .map_err(|e| Error::bad_request(format!("输入数据序列化失败: {}", e)))?;

    // ==================== 调用 WASM 函数 ====================
    
    // 记录开始时间
    let start_time = std::time::Instant::now();
    
    // 通过运行时调用 WASM 函数
    let invoke_result = runtime.invoke(&req.plugin_id, &req.function_name, &input_bytes).await
        .map_err(|e| Error::internal_error(format!("WASM 调用失败: {}", e)))?;

    // 计算执行耗时
    let elapsed_us = start_time.elapsed().as_micros() as u64;

    // ==================== 解析统一出参 FunctionOutput ====================
    
    // 将输出字节数组反序列化为 FunctionOutput
    let output: FunctionOutput = if invoke_result.output.is_empty() {
        // 输出为空，返回默认的空结果
        FunctionOutput {
            result: String::new(),
        }
    } else {
        // 反序列化为 FunctionOutput
        serde_json::from_slice(&invoke_result.output)
            .map_err(|e| Error::internal_error(format!("输出数据解析失败: {}", e)))?
    };

    // ==================== 构建响应 ====================
    
    // 构建响应结构体
    let response = FunctionCallResponse {
        success: true,
        result: Some(output.result),
        elapsed_us,
        error: None,
    };

    // 返回成功响应
    Ok(Json(ApiResp::ok(response)))
}

// ==================== 服务编排 Handler ====================

/// 执行服务编排
///
/// 处理 POST /api/service/execute 请求，执行服务编排。
///
/// # 请求体
/// - `service_key`: 服务唯一标识
/// - `input`: 初始输入数据
/// - `headers`: HTTP 请求头
///
/// # 响应体
/// - `success`: 是否成功
/// - `output`: 最终输出
/// - `steps`: 各步骤执行记录
/// - `total_elapsed_us`: 总耗时
///
/// # 执行流程
/// 1. 获取服务查询器、运行时调用器、插件查询器
/// 2. 创建编排执行器 V2
/// 3. 构建调用上下文
/// 4. 执行服务编排
/// 5. 构建响应
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
    Json(req): Json<ServiceExecuteRequest>,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    // ==================== 获取依赖组件 ====================
    
    // 从应用状态获取服务查询器
    // 用于获取服务编排定义
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    // 从应用状态获取运行时调用器
    // 用于调用 WASM 模块中的函数
    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    // 从应用状态获取插件查询器
    // 用于查询插件状态和获取 WASM 文件路径
    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // ==================== 创建编排执行器 ====================
    
    // 创建编排执行器 V2
    // 支持基于 Flow JSON 的 DAG 编排执行
    let orchestrator = cmx_service::OrchestratorV2::new(
        runtime.clone(),
        plugin_query.clone(),
        service_query.clone(),
    );

    // ==================== 执行服务编排 ====================
    
    // 调用编排执行器的 execute_service 方法
    // - service_key: 服务唯一标识，用于加载编排定义
    // - input: 初始输入，传递给第一个函数节点
    // - headers: 请求头，通过 SVRContext 传递给所有函数
    let result = orchestrator.execute_service(
        &req.service_key,
        &req.input,
        req.headers,
    ).await
        .map_err(|e| Error::internal_error(format!("服务执行失败: {}", e)))?;

    // ==================== 构建响应 ====================
    
    // 将编排执行结果转换为 API 响应结构体
    let response = ServiceExecuteResponse {
        success: result.success,
        output: result.output,
        // 将 ExecutionStep 转换为 ServiceExecutionStep
        steps: result.steps.into_iter().map(|s| ServiceExecutionStep {
            node_id: s.node_id,
            node_name: s.node_name,
            output: s.output,
            elapsed_us: s.elapsed_us,
        }).collect(),
        total_elapsed_us: result.total_elapsed_us,
    };

    // 返回成功响应
    Ok(Json(ApiResp::ok(response)))
}

// ==================== 服务查询 Handler ====================

/// 获取服务列表
///
/// 处理 GET /api/service/list 请求，返回所有启用的服务。
///
/// # 响应体
/// 返回服务信息数组，每个服务包含：
/// - `id`: 主键ID
/// - `service_key`: 服务唯一标识
/// - `service_name`: 服务名称
/// - `description`: 服务描述
/// - `plugin_id`: 所属插件ID
/// - `status`: 状态（1-启用）
/// - `version`: 当前版本
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
    // 从应用状态获取服务查询器
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    // 查询所有启用的服务
    let services = service_query.list_active_services().await
        .map_err(|e| Error::internal_error(format!("获取服务列表失败: {}", e)))?;

    // 将服务信息转换为 ServiceListItem 数组
    let items: Vec<ServiceListItem> = services.into_iter().map(|s| {
        ServiceListItem {
            id: s.id,
            service_key: s.service_key,
            service_name: s.service_name,
            description: s.description,
            plugin_id: s.plugin_id,
            status: s.status,
            version: s.version,
        }
    }).collect();

    // 返回成功响应
    Ok(Json(ApiResp::ok(items)))
}

/// 获取服务定义
///
/// 处理 GET /api/service/get 请求，返回指定服务的详细信息。
///
/// # 查询参数
/// - `service_key`: 服务唯一标识
///
/// # 响应体
/// 返回服务详情，包含：
/// - `id`: 主键ID
/// - `service_key`: 服务唯一标识
/// - `service_name`: 服务名称
/// - `description`: 服务描述
/// - `plugin_id`: 所属插件ID
/// - `status`: 状态
/// - `version`: 当前版本
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
    // 从应用状态获取服务查询器
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    // 根据 service_key 查询服务
    let service = service_query.get_service(&query.service_key).await
        .map_err(|e| Error::internal_error(format!("获取服务失败: {}", e)))?;

    // 处理查询结果
    match service {
        // 服务存在，返回服务详情
        Some(s) => {
            let detail = ServiceDetailResponse {
                id: s.id,
                service_key: s.service_key,
                service_name: s.service_name,
                description: s.description,
                plugin_id: s.plugin_id,
                status: s.status,
                version: s.version,
            };
            Ok(Json(ApiResp::ok(detail)))
        }
        // 服务不存在，返回业务错误
        None => Err(Error::business_error(format!("服务 {} 不存在", query.service_key))),
    }
}

/// 获取插件的所有服务
///
/// 处理 GET /api/service/by-plugin 请求，返回指定插件的所有服务。
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
    // 从应用状态获取服务查询器
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    // 根据 plugin_id 查询所有服务
    let services = service_query.get_services_by_plugin(&query.plugin_id).await
        .map_err(|e| Error::internal_error(format!("获取插件服务失败: {}", e)))?;

    // 将服务信息转换为 ServiceListItem 数组
    let items: Vec<ServiceListItem> = services.into_iter().map(|s| {
        ServiceListItem {
            id: s.id,
            service_key: s.service_key,
            service_name: s.service_name,
            description: s.description,
            plugin_id: s.plugin_id,
            status: s.status,
            version: s.version,
        }
    }).collect();

    // 返回成功响应
    Ok(Json(ApiResp::ok(items)))
}

// ==================== 服务删除 Handler ====================

/// 删除服务
///
/// 处理 POST /api/service/delete 请求，物理删除服务定义及其所有版本。
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
    // 从应用状态获取服务存储器
    let service_storage: &Arc<dyn cmx_traits::ServiceStorage> = state.service_storage()
        .ok_or_else(|| Error::internal_error("服务存储未初始化"))?;

    // 物理删除服务定义及其所有版本
    service_storage.delete_service(&req.service_key).await
        .map_err(|e| Error::internal_error(format!("删除服务失败: {}", e)))?;

    // 返回成功响应
    Ok(Json(crate::api_response::UnitResp::msg("删除成功")))
}
