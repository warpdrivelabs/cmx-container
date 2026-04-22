//! 调试 HTTP Handler
//!
//! 提供插件调试会话管理等 API

use axum::Json;
use axum::extract::Path;
use tracing::info;

use crate::api_response::ApiResp;
use crate::error::Result;

use super::request::InvokeRequest;
use super::response::CurrentDebugSessionResponse;

/// 调试插件 Handler
///
/// 启动指定插件的调试会话
#[utoipa::path(
    post,
    path = "/api/debug/{name}",
    request_body = InvokeRequest,
    params(
        ("name" = String, Path, description = "插件名称或ID")
    ),
    responses(
        (status = 200, description = "调试会话启动成功", body = ApiResp<cmx_debug::DebugResponse>),
        (status = 404, description = "插件不存在")
    ),
    tag = "Debug"
)]
pub async fn debug_plugin(
    Path(name): Path<String>,
    Json(req): Json<InvokeRequest>,
) -> Result<Json<ApiResp<cmx_debug::DebugResponse>>> {
    info!("[api] debug_plugin called for: {}", name);

    match cmx_debug::start_debug_session_by_name_async(
        &name,
        req.function.clone(),
        req.args.clone(),
        req.data.clone(),
    )
    .await
    {
        Ok(response) => {
            info!(
                "[api] debug_plugin success: session_id={:?}",
                response.session_id
            );
            Ok(Json(ApiResp::ok(response)))
        }
        Err(e) => {
            info!("[api] debug_plugin error: {}", e);
            Err(crate::error::Error::NotFound(format!(
                "插件调试失败: {}",
                e
            )))
        }
    }
}

/// 获取当前调试会话 Handler
///
/// 获取当前活跃的调试会话信息
#[utoipa::path(
    get,
    path = "/api/debug/current",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<CurrentDebugSessionResponse>)
    ),
    tag = "Debug"
)]
pub async fn get_current_debug_session() -> Result<Json<ApiResp<CurrentDebugSessionResponse>>> {
    match cmx_debug::get_active_session() {
        Some(session) => {
            let response = CurrentDebugSessionResponse {
                has_session: true,
                project_name: Some(session.plugin_name),
                cmx_pid: Some(session.cmx_pid),
                wasm_path: Some(session.wasm_path),
                source_path: Some(session.source_path),
                debug_function: Some(session.function_name),
                session_id: Some(session.id),
            };
            Ok(Json(ApiResp::ok(response)))
        }
        None => {
            let response = CurrentDebugSessionResponse {
                has_session: false,
                project_name: None,
                cmx_pid: None,
                wasm_path: None,
                source_path: None,
                debug_function: None,
                session_id: None,
            };
            Ok(Json(ApiResp::ok(response)))
        }
    }
}
