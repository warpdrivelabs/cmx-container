//! 模型中心 handler（数据库初始化 + 模块部署，真实落库）（cmx-model-api 层）。

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Result};

/// 从认证上下文取 (user_id, user_name)；缺省用占位，避免未登录环境（如本地）阻塞演示。
fn model_operator(c: &cmx_core::model::service::context::SVRContext) -> (String, String) {
    match c.auth_context.as_ref() {
        Some(a) => (
            if a.user_id.trim().is_empty() {
                "system".to_string()
            } else {
                a.user_id.clone()
            },
            if a.username.trim().is_empty() {
                "系统".to_string()
            } else {
                a.username.clone()
            },
        ),
        None => ("system".to_string(), "系统".to_string()),
    }
}

/// 模型中心查询参数（db_id 定位目标库）。
#[derive(Debug, Deserialize)]
pub struct ModelQuery {
    pub db_id: String,
}

/// `GET /api/model/db-state?db_id=` —— 库门闸 + 每模块每 kind scenario。
pub async fn model_db_state(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<ModelQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_model_deploy::db_state(&q.db_id).await?,
    )))
}

/// `POST /api/model/init` —— 初始化目标库（建台账系统表 + 写 meta + 历史）。
pub async fn model_init(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let db_id = body
        .get("db_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 db_id"))?;
    let (uid, uname) = model_operator(&c);
    Ok(Json(ApiResp::ok(
        cmx_model_deploy::init_db(db_id, &uid, &uname).await?,
    )))
}

/// `POST /api/model/deploy` —— 部署一批定义（create/upgrade）到目标库。
/// body: { db_id, items:[{ kind, domain, application, module, file }] }
pub async fn model_deploy(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let db_id = body
        .get("db_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 db_id"))?;
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err(cmx_api_types::Error::bad_request("items 为空"));
    }
    let (uid, uname) = model_operator(&c);
    Ok(Json(ApiResp::ok(
        cmx_model_deploy::deploy(db_id, &items, &uid, &uname).await?,
    )))
}

/// `POST /api/model/deploy-plan-stream` —— SSE 流式生成部署执行计划（只读预览，不落库）。
/// body: { db_id, items:[{ kind, domain, application, module, file }] }。
pub async fn model_deploy_plan_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return cmx_api_types::Error::bad_request("items 为空").into_response();
    }

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<cmx_model_deploy::InitEvent>();
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break;
                }
            }
        });
        cmx_model_deploy::deploy_plan_stream(&db_id, &items, &etx).await;
        drop(etx);
        let _ = forward.await;
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/model/deploy-stream` —— SSE 流式部署模块（编译/DDL/台账/历史/完成）。
/// body: { db_id, items:[{ kind, domain, application, module, file }] }。
pub async fn model_deploy_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return cmx_api_types::Error::bad_request("items 为空").into_response();
    }
    let (uid, uname) = model_operator(&c);

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<cmx_model_deploy::InitEvent>();
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break;
                }
            }
        });
        cmx_model_deploy::deploy_stream(&db_id, &items, &uid, &uname, &etx).await;
        drop(etx);
        let _ = forward.await;
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/model/init-plan-stream` —— SSE 流式生成初始化/系统表升级计划（只读预览，不落库）。
/// body: { db_id }。
pub async fn model_init_plan_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<cmx_model_deploy::InitEvent>();
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break;
                }
            }
        });
        cmx_model_deploy::init_plan_stream(&db_id, &etx).await;
        drop(etx);
        let _ = forward.await;
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/model/init-stream` —— SSE 流式初始化（连接/建表/写台账/完成，实时推进度）。
/// body: { db_id }。EventSource 不能带鉴权头，前端用 fetch 流式读取（同通知中心）。
pub async fn model_init_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };
    let (uid, uname) = model_operator(&c);

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 后台跑初始化，把领域事件转成 SSE Event（named event + json data）推给客户端。
    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<cmx_model_deploy::InitEvent>();
        // 转发 task：领域事件 → SSE。
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break; // 客户端断开
                }
            }
        });
        cmx_model_deploy::init_db_stream(&db_id, &uid, &uname, &etx).await;
        drop(etx); // 关闭领域通道 → forward 结束
        let _ = forward.await;
        // 补一个终止事件，前端据此关闭流。
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
