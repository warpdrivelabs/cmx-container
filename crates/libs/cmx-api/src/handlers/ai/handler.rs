//! AI 中继 Handler 实现（一期薄代理）。
//!
//! Handler 是薄层：解析请求 → 委托 [`cmx_ai::OpenCodeClient`] 转发到 OpenCode → 包 [`ApiResp`] 返回。
//! AI 错误经 `From<AiError> for cmx_api_types::Error` 自动 `?` 传播为 HTTP 错误。
//! SSE 订阅端点（`GET /ai/events`）在 handler 内部校验 query `access_token`（EventSource 无法发 header）。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tracing::{debug, warn};

use cmx_ai::types::*;
use cmx_ai::{AiSseEvent, get_client, get_registry};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

// ───────────────────────── 查询参数 ─────────────────────────

/// `GET /ai/events` 查询参数。
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// 订阅的会话 id（OpenCode `ses_*`）。
    pub session_id: String,
    /// 访问令牌（JWT）—— 由 mw_auth 中间件校验（支持 query `access_token` 兜底）。
    /// 这里保留字段以兼容旧客户端，但不再二次校验（中间件已注入 AuthContext）。
    #[serde(default)]
    pub access_token: Option<String>,
}

// ───────────────────────── 会话管理 ─────────────────────────

/// `POST /api/ai/sessions` —— 创建新会话。
///
/// 转发 OpenCode `POST /session`，返回 Session 对象。一期 sid 直接透传 OpenCode 的 `ses_*`。
#[utoipa::path(
    post,
    path = "/api/ai/sessions",
    request_body = CreateSessionReq,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<SessionInfo>)
    ),
    tag = "AI"
)]
pub async fn create_session(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    body: Option<Json<CreateSessionReq>>,
) -> Result<Json<ApiResp<SessionInfo>>> {
    debug!("{:<12} - handler::ai::create_session", "HANDLER");
    let client = require_client()?;
    let oc_body = match &body {
        Some(Json(req)) => {
            let mut v = serde_json::json!({});
            if let Some(t) = &req.title {
                v["title"] = serde_json::Value::String(t.clone());
            }
            v
        }
        None => serde_json::json!({}),
    };
    let session = client.create_session(&oc_body).await?;
    let info = SessionInfo {
        session_id: session
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        title: session
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from),
        created_at: session
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(|v| v.as_i64()),
    };
    Ok(Json(ApiResp::ok(info)))
}

/// `POST /api/ai/sessions/{sid}/messages` —— 异步发送消息。
///
/// 转发 OpenCode `POST /session/{sid}/prompt_async`（返回 204），生成过程经 SSE 推送。
#[utoipa::path(
    post,
    path = "/api/ai/sessions/{sid}/messages",
    request_body = SendMessageReq,
    responses(
        (status = 202, description = "已接受，结果走 SSE", body = ApiResp<serde_json::Value>),
        (status = 409, description = "该会话已有活跃生成流（同一 session 仅允许一条活跃流）", body = ApiResp<serde_json::Value>),
        (status = 503, description = "AI 服务未就绪")
    ),
    tag = "AI"
)]
pub async fn send_message(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
    Json(req): Json<SendMessageReq>,
) -> Response {
    debug!(session_id = %sid, "{:<12} - handler::ai::send_message", "HANDLER");
    let client = match require_client() {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    // session 级活跃生成锁：同一 session 仅允许一条活跃生成流，并发时返回 409（文档 4.7）。
    let registry = match get_registry() {
        Some(r) => r,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResp::<serde_json::Value>::fail(
                    503,
                    "AI 功能未启用：请配置 opencode.enabled=true 并部署 OpenCode 服务",
                )),
            )
                .into_response();
        }
    };
    if !registry.try_acquire_session(&sid) {
        return (
            StatusCode::CONFLICT,
            Json(ApiResp::<serde_json::Value>::fail(
                409,
                "该会话已有活跃的生成流，请等待完成或中止后再试",
            )),
        )
            .into_response();
    }

    let body = serde_json::to_value(&req).unwrap_or(serde_json::json!({}));
    match client.prompt_async(&sid, &body).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(ApiResp::<serde_json::Value>::ok_no_data()),
        )
            .into_response(),
        Err(e) => {
            // 转发失败：释放锁（避免该 session 永远无法再次发消息）。
            registry.release_session(&sid);
            warn!(session_id = %sid, error = %e, "异步发送消息失败");
            crate::Error::from(e).into_response()
        }
    }
}

/// `POST /api/ai/sessions/{sid}/answer` —— 回答 AI 询问。
///
/// 转发 OpenCode `POST /question/{requestID}/reply`。
#[utoipa::path(
    post,
    path = "/api/ai/sessions/{sid}/answer",
    request_body = AnswerReq,
    responses(
        (status = 200, description = "回答成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "AI"
)]
pub async fn answer_question(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
    Json(req): Json<AnswerReq>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!(session_id = %sid, question_id = %req.question_id, "{:<12} - handler::ai::answer_question", "HANDLER");
    let client = require_client()?;
    // 优先用前端传的 question_id；同时清理 pending 状态。
    if let Some(reg) = get_registry() {
        reg.take_pending_question(&sid);
    }
    client.reply_question(&req.question_id, req.answers).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "replied": true }))))
}

/// `POST /api/ai/sessions/{sid}/approval` —— 审批决策。
///
/// `decision:"approve"` → OpenCode `reply:"once"`；`decision:"reject"` → `reply:"reject"`。
#[utoipa::path(
    post,
    path = "/api/ai/sessions/{sid}/approval",
    request_body = ApprovalReq,
    responses(
        (status = 200, description = "审批回复成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "AI"
)]
pub async fn approve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
    Json(req): Json<ApprovalReq>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!(session_id = %sid, approval_id = %req.approval_id, decision = ?req.decision, "{:<12} - handler::ai::approve", "HANDLER");
    let client = require_client()?;
    if let Some(reg) = get_registry() {
        reg.take_pending_permission(&sid);
    }
    let (reply, msg) = match req.decision {
        ApprovalDecision::Approve => ("once", req.comment.as_deref()),
        ApprovalDecision::Reject => ("reject", req.comment.as_deref()),
    };
    client
        .reply_permission(&req.approval_id, reply, msg)
        .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "replied": true }))))
}

/// `POST /api/ai/sessions/{sid}/context-request` —— 隐式上下文请求（插件工具发起）。
///
/// 插件工具（如 GetCurrentPage）需要前端当前页面信息时调用：
/// 1. 注册 pending（oneshot channel），广播 `context_request` SSE 给前端；
/// 2. 挂起等待前端回传（30s 超时兜底）；
/// 3. 前端 POST context-response 后解除挂起，返回页面信息。
///
/// 全程无询问框——前端收到 SSE 后自动收集并回传。
#[utoipa::path(
    post,
    path = "/api/ai/sessions/{sid}/context-request",
    request_body = ContextRequestReq,
    responses(
        (status = 200, description = "前端已回传页面信息", body = ApiResp<serde_json::Value>),
        (status = 504, description = "前端未在超时内回传")
    ),
    tag = "AI"
)]
pub async fn context_request(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
    Json(req): Json<ContextRequestReq>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!(session_id = %sid, request_id = %req.request_id, want = ?req.want, "{:<12} - handler::ai::context_request", "HANDLER");

    let reg = match get_registry() {
        Some(r) => r,
        None => {
            return Err(crate::Error::ServiceUnavailable(
                "AI 功能未启用：请在配置中设置 opencode.enabled=true".into(),
            ));
        }
    };

    // 1. 注册 pending（oneshot），拿到 receiver 供 await。
    let rx = reg.register_context_request(&sid, &req.request_id);

    // 2. 广播 context_request SSE 给该 session 的所有前端订阅。
    reg.broadcast(
        &sid,
        AiSseEvent::new(
            "context_request",
            &ContextRequestEvent {
                request_id: req.request_id.clone(),
                want: req.want.clone(),
            },
        ),
    );

    // 3. 挂起等待前端回传（30s 超时兜底，防前端离线/切走导致永久挂起）。
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(data)) => {
            debug!(session_id = %sid, request_id = %req.request_id, "隐式上下文回传成功");
            Ok(Json(ApiResp::ok(data)))
        }
        Ok(Err(_)) => {
            // sender 已 drop（理论上不会发生：register 和 resolve 都走同一 DashMap）。
            warn!(session_id = %sid, request_id = %req.request_id, "上下文回传 channel 异常关闭");
            Err(crate::Error::InternalError(
                "上下文回传 channel 异常关闭".into(),
            ))
        }
        Err(_) => {
            // 超时：前端未响应，清理 pending，返回空信息让工具优雅降级。
            warn!(session_id = %sid, request_id = %req.request_id, "上下文回传超时（30s）");
            Ok(Json(ApiResp::ok(serde_json::json!({
                "error": "前端未在超时内回传当前页面信息",
                "timed_out": true,
            }))))
        }
    }
}

/// `POST /api/ai/sessions/{sid}/context-response` —— 前端回传当前页面信息。
///
/// 前端收到 `context_request` SSE 后自动收集页面信息，调本端点投递回后端，
/// 解除对应工具调用的挂起。
#[utoipa::path(
    post,
    path = "/api/ai/sessions/{sid}/context-response",
    request_body = ContextResponseReq,
    responses(
        (status = 200, description = "回传成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "AI"
)]
pub async fn context_response(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
    Json(req): Json<ContextResponseReq>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!(session_id = %sid, request_id = %req.request_id, "{:<12} - handler::ai::context_response", "HANDLER");

    let reg = match get_registry() {
        Some(r) => r,
        None => {
            return Err(crate::Error::ServiceUnavailable(
                "AI 功能未启用：请在配置中设置 opencode.enabled=true".into(),
            ));
        }
    };

    let resolved = reg.resolve_context_request(&req.request_id, req.data);
    Ok(Json(ApiResp::ok(serde_json::json!({
        "resolved": resolved,
    }))))
}

/// `POST /api/ai/sessions/{sid}/abort` —— 中止当前生成。
#[utoipa::path(
    post,
    path = "/api/ai/sessions/{sid}/abort",
    responses(
        (status = 200, description = "中止成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "AI"
)]
pub async fn abort_session(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!(session_id = %sid, "{:<12} - handler::ai::abort_session", "HANDLER");
    let client = require_client()?;
    client.abort(&sid).await?;
    // 主动中止：立即释放活跃锁（SSE 流可能不再推送 idle）。
    if let Some(reg) = get_registry() {
        reg.release_session(&sid);
    }
    Ok(Json(ApiResp::ok(serde_json::json!({ "aborted": true }))))
}

/// `DELETE /api/ai/sessions/{sid}` —— 删除会话。
#[utoipa::path(
    delete,
    path = "/api/ai/sessions/{sid}",
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "AI"
)]
pub async fn delete_session(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!(session_id = %sid, "{:<12} - handler::ai::delete_session", "HANDLER");
    let client = require_client()?;
    client.delete_session(&sid).await?;
    // 清理本 session 的前端订阅与 pending 状态。
    if let Some(reg) = get_registry() {
        reg.purge(&sid);
    }
    Ok(Json(ApiResp::ok(serde_json::json!({ "deleted": true }))))
}

/// `GET /api/ai/events` —— SSE 事件流（按 sessionID 分发）。
///
/// **鉴权**：EventSource 无法发送 Authorization header，故通过 query `access_token` 传 JWT，
/// handler 内部用全局 AuthService 校验。该端点需加入认证白名单（`/api/ai/events`）让 mw_auth 放行。
///
/// 订阅指定 session 的事件：`GET /api/ai/events?session_id=ses_xxx&access_token=eyJ...`
#[utoipa::path(
    get,
    path = "/api/ai/events",
    params(
        ("session_id" = String, Query, description = "订阅的会话 id（OpenCode ses_*）"),
        ("access_token" = String, Query, description = "JWT 访问令牌（EventSource 无法发 header）")
    ),
    responses(
        (status = 200, description = "SSE 事件流（text/event-stream）", content_type = "text/event-stream"),
        (status = 401, description = "access_token 无效")
    ),
    tag = "AI"
)]
pub async fn subscribe_events(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<EventsQuery>,
) -> Response {
    debug!(session_id = %q.session_id, "{:<12} - handler::ai::subscribe_events", "HANDLER");

    // 鉴权已由 mw_auth 中间件完成（支持 Authorization 头与 query access_token 兜底），
    // 到这里 AuthContext 已注入 CmxSvrContext，无需二次校验。

    // 1. 取 registry 并订阅。
    let registry = match get_registry() {
        Some(r) => r,
        None => {
            warn!("AI 功能未启用（opencode.enabled=false），SSE 订阅拒绝");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResp::<serde_json::Value>::fail(
                    503,
                    "AI 功能未启用：请配置 opencode.enabled=true 并部署 OpenCode 服务",
                )),
            )
                .into_response();
        }
    };
    let rx = registry.subscribe(&q.session_id);

    // 3. mpsc receiver → SSE 流。每个 AiSseEvent 转为 axum Event（event 字段=类型，data=JSON）。
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| {
            let event = Event::default().event(ev.event_name).data(ev.payload);
            (Ok::<Event, std::convert::Infallible>(event), rx)
        })
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ───────────────────────── 内部辅助 ─────────────────────────

/// 获取全局 OpenCode 客户端；未初始化返回 503 业务错误。
fn require_client() -> Result<&'static cmx_ai::OpenCodeClient> {
    get_client().ok_or_else(|| {
        crate::Error::ServiceUnavailable(
            "AI 功能未启用：请在配置中设置 opencode.enabled=true 并确保 OpenCode 服务已部署".into(),
        )
    })
}
