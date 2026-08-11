//! AI 对话中继 + 本地编辑代理 handler。

use axum::Json;
use axum::extract::Path;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// `POST /api/ai/chat` —— 转发到 DeepSeek/OpenAI 兼容服务。未配置返回 501 业务码。
pub async fn ai_chat(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    if !cmx_portal::ai::is_configured() {
        return Ok(Json(ApiResp::fail(
            501,
            "AI 服务未配置：请设置 CMX_AI_API_KEY 或 DEEPSEEK_API_KEY",
        )));
    }
    Ok(Json(ApiResp::ok(cmx_portal::ai::chat(&body).await?)))
}

// ───────────────────────── AI 本地编辑代理 ─────────────────────────

/// `GET /api/agent/capabilities` —— 代理能力 / 工具清单。
pub async fn agent_capabilities(
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::agent::flow::capabilities())))
}

/// `POST /api/agent/message` —— 一次性返回事件序列。
pub async fn agent_message(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::agent::flow::message(&body).await?,
    )))
}

/// `POST /api/agent/message/stream` —— SSE 真流式事件。
///
/// 每个 agent 事件（planner 决策、plan、tool_call/tool_result、assistant 总结…）在产生的当下
/// 即经 mpsc 通道推送给客户端，而非跑完整个流程再一次性下发。flow 在独立 task 上运行，其 `emit`
/// 回调把事件即时投递到通道；SSE 流从通道逐条读取并下发。协议与 Node 一致：meta / agent_event* / done|error。
pub async fn agent_message_stream(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let messages = cmx_portal::agent::flow::normalize_messages(
        body.get("messages").unwrap_or(&serde_json::Value::Null),
    );
    let context = body
        .get("context")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let conv_id = body
        .get("conversationId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("conv_{}", std::process::id()));

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 先推 meta（与 Node 协议一致）。
    let _ = tx.send(Ok(Event::default()
        .event("meta")
        .json_data(serde_json::json!({ "conversationId": conv_id }))
        .unwrap_or_default()));

    // flow 在独立 task 上运行：每个事件在产生当下即推送（真流式），而非跑完再批量下发。
    let conv_done = conv_id.clone();
    tokio::spawn(async move {
        let tx_emit = tx.clone();
        let result = cmx_portal::agent::flow::run_agent_flow(&messages, &context, move |ev| {
            let _ = tx_emit.send(Ok(Event::default()
                .event("agent_event")
                .json_data(&ev)
                .unwrap_or_default()));
        })
        .await;
        match result {
            Ok(_) => {
                let _ = tx.send(Ok(Event::default()
                    .event("done")
                    .json_data(serde_json::json!({ "conversationId": conv_done }))
                    .unwrap_or_default()));
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default()
                    .event("error")
                    .json_data(serde_json::json!({ "error": e.to_string() }))
                    .unwrap_or_default()));
            }
        }
    });

    // 通道 → SSE 流：逐条读取，客户端实时收到。
    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/agent/approvals/:id` —— 审批决定。
pub async fn agent_approval(
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let decision = body.get("decision").and_then(|v| v.as_str()).unwrap_or("");
    Ok(Json(ApiResp::ok(
        cmx_portal::agent::flow::handle_approval(&id, decision).await?,
    )))
}
