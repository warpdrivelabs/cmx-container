//! 调试 HTTP Handler
//!
//! 提供插件调试会话管理等 API

use axum::Json;
use tracing::info;

use crate::api_response::ApiResp;
use crate::error::Result;

use super::response::CurrentDebugSessionResponse;

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
                plugin_id: Some(session.plugin_id),
                project_name: Some(session.plugin_name),
                cmx_pid: Some(session.cmx_pid),
                wasm_path: Some(session.wasm_path),
                source_path: Some(session.source_path),
                debug_function: Some(session.function_name),
                session_id: Some(session.id),
                previous_output: Some(session.previous_output),
                initial_input: Some(session.initial_input),
            };
            Ok(Json(ApiResp::ok(response)))
        }
        None => {
            let response = CurrentDebugSessionResponse {
                has_session: false,
                plugin_id: None,
                project_name: None,
                cmx_pid: None,
                wasm_path: None,
                source_path: None,
                debug_function: None,
                session_id: None,
                previous_output: None,
                initial_input: None,
            };
            Ok(Json(ApiResp::ok(response)))
        }
    }
}
