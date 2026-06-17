//! 服务调用 Handler 实现
//!
//! 处理 WASM 插件服务调用和编排执行的 HTTP 请求。
//!
//! # 架构说明
//!
//! 本模块实现了服务层的两类核心功能：
//! 1. **直接函数调用** (`service_call`)：绕过编排层，直接调用 WASM 插件函数
//! 2. **服务编排执行** (`execute_service*`)：通过编排器执行完整的服务流程
//!
//! # 服务上下文 (SVRContext)
//!
//! 所有服务调用都依赖 SVRContext 传递上下文信息：
//! - `initial_input`: 初始输入数据，用于服务编排的起始节点
//! - `headers`: HTTP 请求头，用于传递认证、追踪等信息
//! - `time_in`: 请求进入时间戳
//! - `request_id`: 请求唯一标识，用于日志追踪
//!
//! SVRContext 由 middleware (`CmxSvrContext`) 自动提取并注入。
//!
//! # API 端点
//!
//! | 方法 | 路径 | 功能 | 认证 |
//! |------|------|------|------|
//! | POST | /api/service/call | 直接调用 WASM 插件函数 | 需要 |
//! | POST | /api/service/execute | 执行服务编排 | 需要 |
//! | POST | /api/service/execute/{service-key} | 执行服务编排（路径参数） | 需要 |
//! | GET | /api/service/get | 获取服务定义 | 需要 |
//! | GET | /api/service/by-plugin | 获取插件的所有服务 | 需要 |
//! | POST | /api/service/page | 分页查询服务 | 需要 |
//! | POST | /api/service/delete | 删除服务 | 需要 |
//! | GET | /api/service/exists | 查询服务是否存在 | 需要 |

// ==================== 依赖导入 ====================

use super::models::{
    FunctionCallRequest, FunctionCallResponse,
    OpenApiQuery,
    ServiceByPluginQuery, ServiceDebugPrepareResult, ServiceDetailResponse, ServiceExecuteRequest,
    ServiceExecuteResponse,
    ServiceExecutionStep, ServiceExistsQuery, ServiceGetQuery,
    ServiceListItem, ServiceOrchestrationError,
};
use crate::ApiResp;
use crate::app_state::CmxAppState;
use crate::Error;
use crate::middleware::CmxSvrContext;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use cmx_core::model::service::SVRContext;
use cmx_core::PageParams;
use cmx_database::get_default_db_manager;
use cmx_traits::plugin::PluginQuery;
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::{ServicePageFilter, ServiceQuery, ServiceInvokeOptions};
// use tracing::error;
use std::sync::Arc;

// ==================== 函数直接调用 Handler ====================

/// 直接调用 WASM 插件函数
///
/// 处理 POST /api/service/call 请求，直接调用指定插件的函数。
/// 此接口用于绕过服务编排，直接执行单个插件函数，适用于：
/// - 简单的插件函数调用场景
/// - 调试和测试插件函数
/// - 不需要服务编排的轻量级调用
///
/// # 跨服务调用
///
/// 请求体中的 `server_name`（可选）用于跨服务 gRPC 调用：指定后本接口
/// 会通过 gRPC 将函数调用路由到 `server_name` 标识的远程 CMX 服务上执行，
/// 可用于分布式部署中跨节点调用其他服务上注册的插件函数；
/// 不指定时则在当前服务本地执行。
///
/// # 与 execute_service 的区别
///
/// | 特性 | service_call | execute_service |
/// |------|--------------|-----------------|
/// | 调用方式 | 直接调用单个插件函数 | 通过编排器执行完整服务流程 |
/// | 适用场景 | 简单调用、调试测试 | 复杂业务逻辑、多函数协同 |
/// | 流程控制 | 无 | 支持条件分支、循环、并行等 |
/// | 执行时间 | 通常较短 | 可能较长（多节点） |
/// | 错误处理 | 简单返回错误 | 支持部分成功、步骤回滚 |
///
/// # 参数
/// - `state`: 应用状态（包含运行时调用器、插件查询器等）
/// - `svr_ctx`: 服务上下文（从 middleware 传入，包含 headers、time_in、request_id）
/// - `req`: 请求体
///
/// # 请求体 (FunctionCallRequest)
/// - `plugin_id`: 插件ID，必填，用于定位要调用的插件
/// - `function_name`: 函数名，必填，插件中暴露的函数名
/// - `input`: 输入数据，支持 JSON 对象或字符串，将作为函数输入
/// - `initial_input`: 初始输入数据（可选），用于调试场景，传递服务最开始的入参
/// - `debug`: 是否调试模式，可选，默认 false，调试模式下会返回更详细的执行信息
/// - `server_name`: 目标服务名称（可选），**用于跨服务 gRPC 调用**。当指定时，
///   本请求会通过 gRPC 路由到 `server_name` 标识的远程 CMX 服务上执行对应
///   插件函数或服务编排；不指定则在本地服务内直接执行。常见用途：
///   - 在分布式部署中调用其他节点上的服务编排
///   - 跨服务复用已注册的插件函数（无需在本地重复部署）
///   - 跨服务执行编排子流程以实现服务组合
///
/// # 响应体 (FunctionCallResponse)
/// - `success`: 是否成功，true 表示执行成功
/// - `result`: 函数执行结果（serde_json::Value），插件函数的返回值
/// - `elapsed_us`: 执行耗时（微秒），WASM 函数执行时间
/// - `error`: 错误信息，仅在 success=false 时有值
///
/// # 执行流程
/// 1. 检查插件是否已安装，未安装返回 400 错误
/// 2. 检查 WASM 模块是否已加载，未加载则从插件目录加载
/// 3. 构建 FunctionInput：
///    - 如果提供了 `initial_input`，则使用它作为 `svr_ctx.initial_input`
///    - 否则使用 `input` 作为 `svr_ctx.initial_input`
/// 4. 使用 MessagePack 序列化 FunctionInput（高性能二进制序列化）
/// 5. 调用 runtime.invoke_with_options 执行 WASM 函数
/// 6. 使用 MessagePack 反序列化函数输出为 FunctionOutput
/// 7. 返回 FunctionOutput.result 作为响应
///
/// # 错误处理
/// - 插件未安装：返回 400 错误 ("插件 {} 未安装")
/// - 获取 WASM 路径失败：返回 400 错误
/// - WASM 模块加载失败：返回 500 错误
/// - 输入数据序列化失败：返回 500 错误（使用 MessagePack 时）
/// - WASM 调用失败：返回 500 错误
/// - 输出数据解析失败：返回 500 错误（使用 MessagePack 时）
///
/// # 序列化说明
/// 本函数使用 MessagePack（rmp_serde）进行序列化和反序列化，相比 JSON：
/// - 二进制格式，体积更小，传输效率更高
/// - 序列化/反序列化速度更快
/// - 支持更多 Rust 原生类型
///
/// # 示例
/// ```json
/// POST /api/service/call
/// {
///     "plugin_id": "my-plugin",
///     "function_name": "process_data",
///     "input": {"key": "value"},
///     "debug": false
/// }
/// ```
///
/// # 响应示例（成功）
/// ```json
/// {
///     "code": 0,
///     "msg": "success",
///     "data": {
///         "success": true,
///         "result": {"output": "processed"},
///         "elapsed_us": 1234,
///         "error": null
///     }
/// }
/// ```
///
/// # 响应示例（失败）
/// ```json
/// {
///     "code": 0,
///     "msg": "success",
///     "data": {
///         "success": false,
///         "result": null,
///         "elapsed_us": 0,
///         "error": "WASM 调用失败: function not found"
///     }
/// }
/// ```
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
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<FunctionCallRequest>,
) -> Result<Json<ApiResp<FunctionCallResponse>>, Error> {
    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return call_function_via_rpc(server_name, &req).await;
    }

    // ==================== 获取依赖组件 ====================

    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    // ==================== 调用核心逻辑（cmx-biz） ====================

    let invoke_result = cmx_biz::function_invoker::invoke_plugin_function(
        runtime,
        plugin_query,
        &req.plugin_id,
        &req.function_name,
        req.input.clone(),
        req.initial_input.clone(),
        svr_ctx,
        req.debug,
    )
    .await?;

    // ==================== 构建响应 ====================

    if !invoke_result.success {
        let error_msg = invoke_result.error.unwrap_or_else(|| "未知错误".to_string());
        return Err(Error::business_error(error_msg));
    }

    let response = FunctionCallResponse {
        success: true,
        result: Some(invoke_result.result),
        elapsed_us: invoke_result.elapsed_us,
        error: None,
    };

    Ok(Json(ApiResp::ok(response)))
}

// ==================== 服务编排 Handler ====================

/// 执行服务编排（内部实现）
///
/// 核心逻辑：通过 service_key 查询服务定义，加载插件，执行编排流程。
/// 此函数是编排执行的内部实现，供 `execute_service` 和 `execute_service_by_key` 调用。
///
/// # 参数
/// - `state`: 应用状态（包含运行时、服务查询器、插件管理器）
/// - `service_key`: 服务唯一标识，用于查询服务定义
/// - `svr_context`: 服务调用上下文（包含 initial_input、headers、time_in、request_id）
/// - `ExecuteOptions`: 服务执行的一些其他可选参数
/// # 执行选项说明
///
/// 通过 `ExecuteOptions` 配置执行行为：
/// - `include_steps`: 是否记录每个步骤的详细执行信息
/// - `debug`: 是否开启调试模式（调试模式下 include_steps 自动为 true）
/// - `debug_node_id`: 调试目标节点 ID（到达该节点时暂停）
/// - `debug_params`: 调试参数字典（传递给调试器）
///
/// # 执行流程
/// 1. 从服务查询器获取服务定义
/// 2. 创建 Orchestrator 实例
/// 3. 调用 execute_service 执行完整编排
/// 4. 转换执行结果为响应格式
///
/// # 返回值
/// 返回服务执行结果 (ServiceExecuteResponse)，包含：
/// - `success`: 是否完全成功
/// - `output`: 最终输出结果
/// - `steps`: 各节点执行详情（可选）
/// - `total_elapsed_us`: 总耗时
/// - `error`: 错误信息（失败时）
/// - `debug_triggered`: 是否触发了调试暂停
/// - `debug_prepare_result`: 调试准备结果（触发调试暂停时）
async fn execute_service_inner(
    state: &CmxAppState,
    service_key: &str,
    svr_context: SVRContext,
    options: cmx_service::ExecuteOptions,
) -> Result<ServiceExecuteResponse, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let runtime: &Arc<dyn RuntimeInvoker> = state.runtime_invoker()
        .ok_or_else(|| Error::internal_error("运行时未初始化"))?;

    let plugin_query: &Arc<dyn PluginQuery> = state.plugin_query()
        .ok_or_else(|| Error::internal_error("插件管理器未初始化"))?;

    let default_db_id = get_default_db_manager().get_default_db_id().await;

    // ==================== 调用核心逻辑（cmx-biz） ====================

    let core_result = cmx_biz::service_executor::execute_service(
        runtime,
        plugin_query,
        service_query,
        &default_db_id,
        service_key,
        svr_context,
        options,
    )
    .await?;

    // ==================== 映射为 HTTP 响应 ====================

    let response = ServiceExecuteResponse {
        success: core_result.success,
        output: core_result.final_output,
        steps: core_result.steps.into_iter().map(|s| ServiceExecutionStep {
            node_id: s.node_id,
            node_name: s.node_name,
            node_type: s.node_type,
            status: s.status,
            output: s.output,
            elapsed_us: s.elapsed_us,
            error: s.error,
            previous_output: s.previous_output,
        }).collect(),
        total_elapsed_us: core_result.total_elapsed_us,
        error: core_result.error.map(|e| ServiceOrchestrationError {
            message: e.message,
        }),
        debug_triggered: core_result.debug_triggered,
        debug_prepare_result: core_result.debug_prepare_result.map(|d| ServiceDebugPrepareResult {
            code_server_url: d.code_server_url,
            plugin_id: d.plugin_id,
            plugin_name: d.plugin_name,
            plugin_version: d.plugin_version,
            plugin_status: d.plugin_status,
            plugin_install_path: d.plugin_install_path,
            plugin_wasm_path: d.plugin_wasm_path,
            plugin_type: d.plugin_type,
            domain_code: d.domain_code,
            application_code: d.application_code,
            module_code: d.module_code,
            function_name: d.function_name,
            source_path: d.source_path,
            node_id: d.node_id,
            node_name: d.node_name,
        }),
    };

    Ok(response)
}

/// 执行服务编排
///
/// 处理 POST /api/service/execute 请求，执行完整的服务编排流程。
/// 通过编排器协调多个 WASM 插件函数的执行，实现复杂的业务逻辑。
///
/// # 跨服务调用
///
/// 请求体中的 `server_name`（可选）用于跨服务 gRPC 调用：指定后本接口
/// 会将编排执行请求通过 gRPC 路由到 `server_name` 标识的远程 CMX 服务上
/// 执行，常用于在分布式部署中跨节点调用其他服务上注册的服务编排，
/// 实现跨服务的编排组合（如 A 服务编排中嵌入 B 服务的编排子流程）；
/// 不指定时则在当前服务本地执行编排流程。
///
/// # 参数
/// - `state`: 应用状态
/// - `svr_ctx`: 服务上下文（从 middleware 传入）
/// - `req`: 请求体
///
/// # 请求体 (ServiceExecuteRequest)
/// - `service_key`: 服务唯一标识，必填
/// - `input`: 初始输入数据，支持 JSON 对象或字符串，作为编排流程的起始输入
/// - `include_steps`: 是否返回步骤数据，可选，默认 false
/// - `debug`: 是否开启调试模式，可选，默认 false
/// - `debug_node_id`: 调试目标节点ID，开启 debug 时必填
/// - `debug_params`: debug相关的参数 HashMap<string, string>
/// - `server_name`: 目标服务名称（可选），**用于跨服务 gRPC 调用**。当指定时，
///   本编排执行请求会通过 gRPC 路由到 `server_name` 标识的远程 CMX 服务上执行，
///   适用于：
///   - 在分布式部署中调用其他 CMX 节点上的服务编排
///   - 在 A 服务编排中嵌入调用 B 服务的编排子流程，实现跨服务编排组合
///   - 将耗时编排任务分发到指定节点执行以实现负载分流
///   不指定时则在当前服务本地执行编排流程
///
/// # 响应体 (ServiceExecuteResponse)
/// - `success`: 是否成功，编排中任何节点失败都为 false
/// - `output`: 最终输出（serde_json::Value），编排流程最后一个节点的输出
/// - `steps`: 各步骤执行记录（当 include_steps=true 时），包含每个节点的：
///   - `node_id`: 节点ID
///   - `node_name`: 节点名称
///   - `node_type`: 节点类型（Plugin、Condition、Merge 等）
///   - `status`: 执行状态（Success、Failed、Skipped、DebugPaused）
///   - `output`: 节点输出
///   - `elapsed_us`: 节点执行耗时
///   - `error`: 错误信息（失败时）
/// - `total_elapsed_us`: 整个编排流程的总耗时（微秒）
/// - `error`: 错误详情（失败时），包含 message 字段
/// - `debug_triggered`: 是否触发了调试暂停（调试模式时）
/// - `debug_prepare_result`: 调试准备结果（触发调试暂停时），包含插件详情和 code-server URL
///
/// # SVRContext 传递
/// - `initial_input`: 从请求的 input 字段设置
/// - `headers`: 从 HTTP 请求头提取
/// - `time_in`: 请求进入时间
/// - `request_id`: 请求追踪ID
///
/// # 错误处理
/// - 服务不存在：返回 code=1 的失败响应
/// - 编排执行失败：返回 code=1 的失败响应，包含错误信息
///
/// # 示例
/// ```json
/// POST /api/service/execute
/// {
///     "service_key": "order-process",
///     "input": {"order_id": "12345"},
///     "include_steps": true
/// }
/// ```
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
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<ServiceExecuteRequest>,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    let include_steps = req.include_steps.unwrap_or(false);
    let debug = req.debug.unwrap_or(false);
    let debug_node_id = req.debug_node_id.clone();
    let debug_params = req.debug_params.clone();

    let mut svr_ctx = svr_ctx;
    svr_ctx.initial_input = req.input.clone();

    let include_steps = include_steps || debug;
    let options = cmx_service::ExecuteOptions::new(include_steps)
        .with_debug(debug, debug_node_id, debug_params);
    if req.service_key.clone().is_none(){
        return Ok(Json(ApiResp::fail(1, "service_key 不能为空")));
    }
    let service_key = req.service_key.clone().unwrap();

    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return execute_service_via_rpc(server_name, &service_key, &req).await;
    }

    let response = execute_service_inner(&state, &service_key, svr_ctx, options).await?;

    // 失败时返回错误码
    if !response.success {
        let error_message = response.error.as_ref().map(|e| e.message.clone()).unwrap_or_default();
        return Ok(Json(ApiResp::fail_with_data(1, error_message, response)));
    }

    Ok(Json(ApiResp::ok(response)))
}

/// 执行服务编排（路径参数版本）
///
/// 处理 POST /api/service/execute/{service-key} 请求，执行服务编排。
/// 与 `execute_service` 的区别在于 service_key 从 URL 路径获取，优先级高于请求体。
/// 适用于 service_key 可能包含复杂字符不适合放在请求体中的场景。
///
/// # 跨服务调用
///
/// 请求体中的 `server_name`（可选）用于跨服务 gRPC 调用：指定后本接口
/// 会将编排执行请求通过 gRPC 路由到 `server_name` 标识的远程 CMX 服务上执行。
/// 配合 URL 路径中的 `service_key`（常含路径分隔符）可以精准地在指定服务
/// 上执行指定的层级化服务编排，实现"路径定位 + 服务路由"的跨服务调用；
/// 不指定时则在当前服务本地执行编排流程。
///
/// # 参数
/// - `state`: 应用状态
/// - `svr_ctx`: 服务上下文（从 middleware 传入）
/// - `service_key`: 服务唯一标识（从 URL 路径获取）
/// - `req`: 请求体（input 和 include_steps）
///
/// # 路径参数
/// - `service-key`: 服务唯一标识，优先于请求体中的 service_key
///   - 可包含路径分隔符（如 `domain/app/service`）
///   - 会自动 URL 解码
///
/// # 请求体 (ServiceExecuteRequest)
/// - `input`: 初始输入数据，作为编排流程的起始输入
/// - `include_steps`: 是否返回步骤数据，可选，默认 false
/// - `debug_params`: debug相关的参数 HashMap<string, string>
/// - `server_name`: 目标服务名称（可选），**用于跨服务 gRPC 调用**。当指定时，
///   本编排执行请求会通过 gRPC 路由到 `server_name` 标识的远程 CMX 服务上执行。
///   配合 URL 路径中的 `service_key` 可以实现"在指定服务上执行特定编排"的
///   跨服务调用能力，典型场景：
///   - 通过 URL 路径传递层级化 `service_key`（如 `domain/app/service`），
///     同时通过 `server_name` 路由到目标服务
///   - 在分布式部署中跨节点复用已注册的服务编排
///   不指定时则在当前服务本地执行编排流程
///
/// # 响应体
/// 与 `execute_service` 相同
///
/// # 与 execute_service 的区别
/// | 特性 | execute_service | execute_service_by_key |
/// |------|-----------------|------------------------|
/// | service_key 位置 | 请求体 | URL 路径 |
/// | 适用场景 | 简单服务 key | 复杂/层级 service_key |
///
/// # 示例
/// ```json
/// POST /api/service/execute/my-domain/my-service
/// {
///     "input": {"data": "test"},
///     "include_steps": true
/// }
/// ```
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
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Path(service_key): Path<String>,
    Json(req): Json<ServiceExecuteRequest>,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    let include_steps = req.include_steps.unwrap_or(false);
    let debug = req.debug.unwrap_or(false);
    let debug_node_id = req.debug_node_id.clone();
    let debug_params = req.debug_params.clone();

    let mut svr_ctx = svr_ctx;
    svr_ctx.initial_input = req.input.clone();


    let include_steps = include_steps || debug;
    let options = cmx_service::ExecuteOptions::new(include_steps)
        .with_debug(debug, debug_node_id, debug_params);

    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return execute_service_via_rpc(server_name, &service_key, &req).await;
    }

    let response = execute_service_inner(&state, &service_key, svr_ctx, options).await?;

    Ok(Json(ApiResp::ok(response)))
}

// ==================== 跨服务 RPC 调用辅助函数 ====================

/// 通过 RPC 调用远程插件函数
///
/// 当请求中指定了 `server_name` 时，通过 gRPC 将请求路由到远程服务执行。
async fn call_function_via_rpc(
    server_name: &str,
    req: &FunctionCallRequest,
) -> Result<Json<ApiResp<FunctionCallResponse>>, Error> {
    if !cmx_rpc::GlobalRpcClient::is_initialized() {
        return Err(Error::business_error("RPC 服务未启用，无法进行跨服务调用"));
    }
    let rpc_client = cmx_rpc::GlobalRpcClient::get();
    let result = rpc_client
        .call_function(server_name, &req.plugin_id, &req.function_name, req.input.clone())
        .await
        .map_err(|e| Error::business_error(format!("RPC 调用失败: {}", e)))?;

    Ok(Json(ApiResp::ok(FunctionCallResponse {
        success: result.success,
        result: result.result,
        elapsed_us: result.elapsed_us,
        error: result.error,
    })))
}

/// 通过 RPC 调用远程服务编排
///
/// 当请求中指定了 `server_name` 时，通过 gRPC 将服务编排请求路由到远程服务执行。
async fn execute_service_via_rpc(
    server_name: &str,
    service_key: &str,
    req: &ServiceExecuteRequest,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    if !cmx_rpc::GlobalRpcClient::is_initialized() {
        return Err(Error::business_error("RPC 服务未启用，无法进行跨服务调用"));
    }
    let rpc_client = cmx_rpc::GlobalRpcClient::get();
    let options = ServiceInvokeOptions {
        include_steps: req.include_steps.unwrap_or(false),
        debug: req.debug.unwrap_or(false),
        debug_node_id: req.debug_node_id.clone(),
        debug_params: req.debug_params.clone(),
    };
    let result = rpc_client
        .call_service(server_name, service_key, req.input.clone(), options)
        .await
        .map_err(|e| Error::business_error(format!("RPC 调用失败: {}", e)))?;

    // 将 CallServiceResponse 转换为 ServiceExecuteResponse
    let response = ServiceExecuteResponse {
        success: result.success,
        output: result.output,
        steps: result.steps.into_iter().map(|s| ServiceExecutionStep {
            node_id: s.node_id,
            node_name: s.node_name,
            node_type: s.node_type,
            status: cmx_biz::service_executor::step_status_to_str(&s.status).to_string(),
            output: s.output,
            elapsed_us: s.elapsed_us,
            error: s.error,
            previous_output: s.previous_output,
        }).collect(),
        total_elapsed_us: result.total_elapsed_us.unwrap_or(0),
        error: result.error.map(|e| ServiceOrchestrationError { message: e.message }),
        debug_triggered: None,
        debug_prepare_result: None,
    };
    Ok(Json(ApiResp::ok(response)))
}

// ==================== 服务查询 Handler ====================

/// 获取服务定义
///
/// 处理 GET /api/service/get 请求，返回指定服务的详细信息。
/// 用于查看服务的完整配置，包括基本信息、关联插件、分类等。
///
/// # 参数
/// - `state`: 应用状态
/// - `query`: 查询参数
///
/// # 查询参数 (ServiceGetQuery)
/// - `service_key`: 服务唯一标识，必填
///
/// # 响应体 (ServiceDetailResponse)
/// 返回服务详情，包含以下字段：
/// - 基本信息：`id`, `service_key`, `service_name`, `description`
/// - 关联信息：`plugin_id`, `version`, `status`
/// - 分类信息：`domain_code`, `application_code`, `module_code`
/// - 名称信息：`domain_name`, `application_name`, `module_name`
/// - 配置信息：`config`（JSON 格式的服务配置）
///
/// # 错误处理
/// - 服务不存在：返回 400 错误
///
/// # 示例
/// ```bash
/// GET /api/service/get?service_key=order-process
/// ```
///
/// # 响应示例
/// ```json
/// {
///     "code": 0,
///     "data": {
///         "id": 1,
///         "service_key": "order-process",
///         "service_name": "订单处理服务",
///         "description": "处理订单创建和支付流程",
///         "plugin_id": "order-plugin",
///         "status": "active",
///         "version": "1.0.0",
///         "config": {"timeout": 30000},
///         "domain_code": "ecommerce",
///         "application_code": "order",
///         "module_code": "process",
///         "domain_name": "电子商务",
///         "application_name": "订单系统",
///         "module_name": "处理模块"
///     }
/// }
/// ```
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
        .map_err(|e| Error::business_error(format!("获取服务失败: {}", e)))?;

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
                config: s.config.unwrap_or_default(),
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
/// 处理 GET /api/service/by-plugin 请求，返回指定插件关联的所有服务列表。
/// 用于查看某个插件部署了哪些服务，或管理插件相关的服务。
///
/// # 参数
/// - `state`: 应用状态
/// - `query`: 查询参数
///
/// # 查询参数 (ServiceByPluginQuery)
/// - `plugin_id`: 插件ID，必填
///
/// # 响应体
/// 返回服务信息数组 (Vec<ServiceListItem>)，每个服务包含：
/// - `id`: 服务ID
/// - `service_key`: 服务唯一标识
/// - `service_name`: 服务名称
/// - `description`: 服务描述
/// - `plugin_id`: 关联的插件ID
/// - `status`: 服务状态
/// - `version`: 服务版本
/// - `domain_code`, `application_code`, `module_code`: 分类编码
/// - `domain_name`, `application_name`, `module_name`: 分类名称
///
/// # 错误处理
/// - 插件查询失败：返回 500 错误
///
/// # 示例
/// ```bash
/// GET /api/service/by-plugin?plugin_id=order-plugin
/// ```
///
/// # 响应示例
/// ```json
/// {
///     "code": 0,
///     "data": [
///         {
///             "id": 1,
///             "service_key": "order-create",
///             "service_name": "创建订单",
///             "plugin_id": "order-plugin",
///             "status": "active"
///         },
///         {
///             "id": 2,
///             "service_key": "order-pay",
///             "service_name": "订单支付",
///             "plugin_id": "order-plugin",
///             "status": "active"
///         }
///     ]
/// }
/// ```
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
        .map_err(|e| Error::business_error(format!("获取插件服务失败: {}", e)))?;

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
            plugin_name: s.plugin_name,
            api_doc: s.api_doc,
        }
    }).collect();

    Ok(Json(ApiResp::ok(items)))
}

// ==================== 服务分页查询 Handler ====================

/// 分页查询服务列表
///
/// 处理 POST /api/service/page 请求，支持多条件组合查询和分页。
/// 用于管理后台或服务列表展示场景。
///
/// # 参数
/// - `state`: 应用状态
/// - `params`: 分页查询参数
///
/// # 请求体 (PageParams<ServicePageFilter>)
/// - `filter`: 查询条件（ServicePageFilter），可选
///   - `domain_code`: 域编码，精确匹配
///   - `application_code`: 应用编码，精确匹配
///   - `module_code`: 模块编码，精确匹配
///   - `status`: 服务状态，精确匹配（如 "active", "inactive"）
///   - `keyword`: 关键字搜索，模糊匹配服务名称和描述
/// - `page`: 页码，从 1 开始
/// - `size`: 每页数量，建议不超过 100
///
/// # 响应体
/// 返回分页结果，包含：
/// - `items`: 服务列表（Vec<ServiceListItem>）
/// - `page`: 当前页码
/// - `size`: 每页数量
/// - `total`: 总记录数
///
/// # 错误处理
/// - 分页查询失败：返回 500 错误
///
/// # 示例
/// ```json
/// POST /api/service/page
/// {
///     "filter": {
///         "domain_code": "ecommerce",
///         "status": "active",
///         "keyword": "order"
///     },
///     "page": 1,
///     "size": 20
/// }
/// ```
///
/// # 响应示例
/// ```json
/// {
///     "code": 0,
///     "msg": "success",
///     "data": {
///         "items": [...],
///         "page": 1,
///         "size": 20,
///         "total": 45
///     }
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/service/page",
    request_body = crate::PageParamsDoc<serde_json::Value>,
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
        .map_err(|e| Error::business_error(format!("分页查询失败: {}", e)))?;

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
            plugin_name: s.plugin_name,
            api_doc: s.api_doc,
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
/// **危险操作：删除后数据无法恢复，请谨慎使用。**
///
/// # 参数
/// - `state`: 应用状态
/// - `req`: 请求体
///
/// # 请求体 (ServiceDeleteQuery)
/// - `service_key`: 服务唯一标识，必填
///
/// # 删除范围
/// 此操作会同时删除：
/// - `cmx_service_define` 表中的服务定义记录
/// - `cmx_service_define_version` 表中该服务的所有版本记录
///
/// # 错误处理
/// - 服务不存在：返回 500 错误
/// - 删除失败：返回 500 错误
///
/// # 示例
/// ```json
/// POST /api/service/delete
/// {
///     "service_key": "order-process"
/// }
/// ```
///
/// # 注意事项
/// 1. **不可逆操作**：物理删除，不会进入回收站
/// 2. **级联删除**：会同时删除所有版本
/// 3. **建议**：生产环境中建议先停用服务，确认无影响后再删除
#[utoipa::path(
    post,
    path = "/api/service/delete",
    request_body = crate::handlers::service::models::ServiceDeleteQuery,
    responses(
        (status = 200, description = "删除服务成功", body = crate::UnitResp)
    ),
    tag = "Service"
)]
pub async fn delete_service(
    State(state): State<CmxAppState>,
    Json(req): Json<crate::handlers::service::models::ServiceDeleteQuery>,
) -> Result<Json<crate::UnitResp>, Error> {
    let service_storage: &Arc<dyn cmx_traits::service::ServiceStorage> = state.service_storage()
        .ok_or_else(|| Error::internal_error("服务存储未初始化"))?;

    let app_id = state.app_id();

    service_storage.delete_service(&req.service_key, &app_id, None, None).await
        .map_err(|e| Error::business_error(format!("删除服务失败: {}", e)))?;

    Ok(Json(crate::UnitResp::msg("删除成功")))
}

/// 查询服务是否存在
///
/// 处理 GET /api/service/exists 请求，通过 service_key 查询服务是否已存在。
/// 用于在创建服务前检查是否重复，或验证服务是否可用。
///
/// # 参数
/// - `state`: 应用状态
/// - `query`: 查询参数
///
/// # 查询参数 (ServiceExistsQuery)
/// - `service_key`: 服务唯一标识，必填
///
/// # 响应体
/// - `code`: 0 表示成功
/// - `data`: 返回字符串 "1" 表示服务存在，"0" 表示不存在
///
/// # 业务逻辑
/// - `service_key` 在数据库中是唯一索引，不允许重复
/// - 创建新服务前应先调用此接口检查是否已存在
/// - 可用于验证服务删除是否成功
///
/// # 错误处理
/// - 查询失败：返回 500 错误
///
/// # 示例
/// ```bash
/// GET /api/service/exists?service_key=order-process
/// ```
///
/// # 响应示例（存在）
/// ```json
/// {
///     "code": 0,
///     "msg": "success",
///     "data": "1"
/// }
/// ```
///
/// # 响应示例（不存在）
/// ```json
/// {
///     "code": 0,
///     "msg": "success",
///     "data": "0"
/// }
/// ```
#[utoipa::path(
    get,
    path = "/api/service/exists",
    params(ServiceExistsQuery),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<String>)
    ),
    tag = "Service"
)]
pub async fn service_exists(
    State(state): State<CmxAppState>,
    Query(query): Query<ServiceExistsQuery>,
) -> Result<Json<ApiResp<String>>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let service = service_query.get_service(&query.service_key).await
        .map_err(|e| Error::business_error(format!("查询服务存在性失败: {}", e)))?;

    let exists = service.is_some();
    Ok(Json(ApiResp::ok(if exists { "1" } else { "0" }.to_string())))
}

#[utoipa::path(
    get,
    path = "/api/service/openapi",
    params(OpenApiQuery),
    responses(
        (status = 200, description = "OpenAPI 3.0 文档", body = Value)
    ),
    tag = "Service"
)]
pub async fn get_openapi_spec(
    State(state): State<CmxAppState>,
    Query(query): Query<OpenApiQuery>,
) -> Result<Json<serde_json::Value>, Error> {
    let service_query: &Arc<dyn ServiceQuery> = state.service_query()
        .ok_or_else(|| Error::internal_error("服务查询器未初始化"))?;

    let filter = ServicePageFilter {
        plugin_id: query.plugin_id,
        domain_code: query.domain_code,
        application_code: query.application_code,
        module_code: query.module_code,
        ..Default::default()
    };
    let page_size = 1000u64;
    let mut all_items = Vec::new();
    let mut page = 1u64;

    loop {
        let result = service_query.page_services(filter.clone(), page, page_size).await
            .map_err(|e| Error::business_error(format!("查询服务失败: {}", e)))?;
        all_items.extend(result.items);
        let total = result.total;
        if (page * page_size) >= total {
            break;
        }
        page += 1;
    }

    let mut paths = serde_json::Map::new();
    let mut schemas = serde_json::Map::new();
    let mut tags = Vec::new();
    let mut seen_tags = std::collections::HashSet::new();

    for svc in &all_items {
        if let Some(api_doc_str) = &svc.api_doc {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(api_doc_str) {
                if let Some(path) = doc.get("path").and_then(|v| v.as_str()) {
                    if let Some(path_item) = doc.get("path_item") {
                        let mut op = path_item.clone();
                        let tag = build_tag(&svc.domain_name, &svc.application_name, &svc.module_name);
                        if !seen_tags.contains(&tag) {
                            seen_tags.insert(tag.clone());
                            tags.push(serde_json::json!({
                                "name": tag,
                                "description": format!("域:{} > 应用:{} > 模块:{}",
                                    svc.domain_name, svc.application_name, svc.module_name)
                            }));
                        }
                        if let Some(post) = op.get_mut("post") {
                            post["tags"] = serde_json::json!([tag]);
                        }
                        paths.insert(path.to_string(), op);
                    }
                    if let Some(doc_schemas) = doc.get("schemas").and_then(|v| v.as_object()) {
                        schemas.extend(doc_schemas.clone());
                    }
                }
            }
        }
    }

    let openapi = serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "CMX 服务编排 API",
            "version": "1.0.0",
            "description": "所有服务编排的统一接口文档，可直接导入 Swagger/Postman/Apifox 等工具"
        },
        "tags": tags,
        "paths": paths,
        "components": {
            "schemas": schemas
        }
    });

    Ok(Json(openapi))
}

fn build_tag(domain: &str, app: &str, module: &str) -> String {
    let mut parts = Vec::new();
    if !domain.is_empty() { parts.push(domain); }
    if !app.is_empty() { parts.push(app); }
    if !module.is_empty() { parts.push(module); }
    if parts.is_empty() { return "未分类".to_string(); }
    parts.join("/")
}
