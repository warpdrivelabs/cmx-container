//! cmx-ai-api —— AI 生成能力中继模块的 HTTP 层（一期薄代理）。
//!
//! 路由经 `/api/ai/*` 暴露，转发到 OpenCode(:4096)。SSE 事件流按 sessionID 分发。
//! 所有 handler 走正常认证（Authorization: Bearer），仅 `GET /ai/events` 因 EventSource
//! 无法发 header，改在 handler 内部校验 query `access_token`（该端点需加入认证白名单）。
//!
//! AiModule 实现 cmx-api-core 的 ModuleRoutes，由 cmx-platform-app 合并进主路由。
//! AiApiDoc 提供本模块的 OpenApi 切片，由 platform-app 用 OpenApi::merge() 聚合。

pub mod handler;
pub mod openapi;

pub use openapi::AiApiDoc;

use axum::Router;
use axum::routing::{delete, get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;

/// AI 中继模块路由聚合（实现 cmx-api-core 的 ModuleRoutes，由 platform-app 合并）。
pub struct AiModule;

impl ModuleRoutes for AiModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 创建会话
            .route("/ai/sessions", post(handler::create_session))
            // 会话级操作（{sid} = OpenCode ses_*）
            .route("/ai/sessions/{sid}/messages", post(handler::send_message))
            .route("/ai/sessions/{sid}/answer", post(handler::answer_question))
            .route("/ai/sessions/{sid}/approval", post(handler::approve))
            .route("/ai/sessions/{sid}/abort", post(handler::abort_session))
            // 隐式上下文回传（插件工具 ↔ 前端桥接，无询问框）
            .route(
                "/ai/sessions/{sid}/context-request",
                post(handler::context_request),
            )
            .route(
                "/ai/sessions/{sid}/context-response",
                post(handler::context_response),
            )
            .route("/ai/sessions/{sid}", delete(handler::delete_session))
            // SSE 事件流（按 sessionID 分发，query token 鉴权）
            .route("/ai/events", get(handler::subscribe_events))
    }

    fn prefix() -> &'static str {
        "ai"
    }

    fn module_name(&self) -> &'static str {
        "ai"
    }
}
