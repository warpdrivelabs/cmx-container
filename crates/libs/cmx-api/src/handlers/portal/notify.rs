//! 通知中心 handler（任务/消息/日志 + SSE 主动推送）。

use axum::Json;
use axum::extract::Query;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// 从认证上下文取当前用户 id（通知按用户隔离）。
fn notify_user_id(c: &cmx_core::model::service::context::SVRContext) -> Result<String> {
    c.auth_context
        .as_ref()
        .map(|a| a.user_id.clone())
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| cmx_api_types::Error::unauthorized("未登录或无用户标识"))
}

#[derive(Debug, serde::Deserialize)]
pub struct NotifyListQuery {
    #[serde(default)]
    pub center: Option<String>,
}

/// `GET /api/notifications/centers` —— 三中心元信息（前端下拉用）。
pub async fn notify_centers(
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::notify::store::centers_meta())))
}

/// `GET /api/notifications/counts` —— 当前用户各中心未读数 + 合计（红色角标）。
pub async fn notify_counts(
    CmxSvrContext(c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let counts = cmx_portal::notify::store::counts(&uid).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(counts).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `GET /api/notifications?center=task|message|log` —— 当前用户通知列表（缺 center 则全部）。
pub async fn notify_list(
    CmxSvrContext(c): CmxSvrContext,
    Query(q): Query<NotifyListQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let center = match q.center.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(
            cmx_portal::notify::store::NotifyCenter::parse(s).ok_or_else(|| {
                cmx_api_types::Error::bad_request("center 仅支持 task/message/log")
            })?,
        ),
        None => None,
    };
    let items = cmx_portal::notify::store::list(&uid, center).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/notifications/publish` —— 发布一条通知（也用于后端/服务端主动推送的入口）。
/// 默认发给当前用户；body 带 userId 时发给指定用户（服务端代发场景）。
pub async fn notify_publish(
    CmxSvrContext(c): CmxSvrContext,
    Json(mut input): Json<cmx_portal::notify::store::NotifyInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    if input
        .user_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        input.user_id = Some(notify_user_id(&c)?);
    }
    let saved = cmx_portal::notify::store::publish(input).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(saved).map_err(cmx_portal::PortalError::from)?,
    )))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyMarkInput {
    #[serde(default)]
    pub center: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub all: bool,
}

/// `POST /api/notifications/mark-read` —— 标记已读：{ center, id } 标单条；{ all:true, center? } 标全部。
pub async fn notify_mark_read(
    CmxSvrContext(c): CmxSvrContext,
    Json(input): Json<NotifyMarkInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let center = match input
        .center
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => Some(
            cmx_portal::notify::store::NotifyCenter::parse(s).ok_or_else(|| {
                cmx_api_types::Error::bad_request("center 仅支持 task/message/log")
            })?,
        ),
        None => None,
    };
    if input.all {
        let n = cmx_portal::notify::store::mark_all_read(&uid, center).await?;
        return Ok(Json(ApiResp::ok(serde_json::json!({ "marked": n }))));
    }
    let center = center.ok_or_else(|| cmx_api_types::Error::bad_request("标单条需提供 center"))?;
    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| cmx_api_types::Error::bad_request("标单条需提供 id"))?;
    let changed = cmx_portal::notify::store::mark_read(&uid, center, id).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "changed": changed }))))
}

/// `GET /api/notifications/stream` —— SSE：服务端主动推送本用户的新通知与角标刷新。
/// 浏览器用 fetch + 流读消费（携带 Authorization 头），订阅进程内 broadcast，仅下发本人事件。
pub async fn notify_stream(
    CmxSvrContext(c): CmxSvrContext,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let uid = match notify_user_id(&c) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 连接建立先推一次当前 counts，保证角标立刻准确（不必等下一次推送）。
    if let Ok(counts) = cmx_portal::notify::store::counts(&uid).await {
        let _ = tx.send(Ok(Event::default()
            .event("counts")
            .json_data(counts)
            .unwrap_or_default()));
    }

    // 订阅 broadcast：只转发属于本用户的事件。连接断开时该 task 自然结束。
    let mut sub = cmx_portal::notify::hub::subscribe();
    let uid_filter = uid.clone();
    tokio::spawn(async move {
        loop {
            match sub.recv().await {
                Ok(ev) => {
                    if ev.user_id != uid_filter {
                        continue;
                    }
                    let sent = tx.send(Ok(Event::default()
                        .event(&ev.kind)
                        .json_data(&ev.data)
                        .unwrap_or_default()));
                    if sent.is_err() {
                        break; // 客户端已断开
                    }
                }
                // 滞后丢消息：忽略，继续（计数以文件为准，下次 counts 事件会纠正）。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
