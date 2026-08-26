//! 二次开发平台 HTTP Handler（W1 构建 / W3 触发 / W5 工作区）。
//!
//! 把已落地的库层能力（cmx-build / cmx-plugin-trigger / cmx-dev-workspace）接到活的 HTTP 端点。
//! 所有 handler 走既有惯例：free `async fn` → `Result<Json<ApiResp<Value>>>`，db_id 取默认库。
//!
//! - **W5**：`POST /api/dev/vscode/register` —— 给孤儿端点一个后端归属（落 `cmx_dev_workspace`）。
//! - **W1**：`POST /api/dev/build/jobs`（提交构建）/ `GET /api/dev/build/jobs/{id}` / `GET /api/dev/build/jobs`。
//! - **W3**：`POST /api/dev/trigger/bindings`（增）/ `GET /api/dev/trigger/bindings`（列）/ `DELETE .../{id}`。
//! - **W6**：`GET /api/dev/plugins/{id}/versions`（版本历史）/ `POST /api/dev/plugins/{id}/rollback`（一键回滚）。

use axum::extract::Path;
use axum::Json;
use chrono::Utc;
use cmx_build::{BuildJob, BuildJobStore, BuildRequest, BuildStatus};
use cmx_build_store_pg::PgBuildJobStore;
use cmx_database::get_default_db_manager;
use cmx_dev_workspace::{DevWorkspaceStore, PgDevWorkspaceStore, RegisterRequest};
use cmx_plugin_trigger::{TriggerBinding, TriggerBindingStore, TriggerKind};
use cmx_plugin_trigger_store_pg::PgTriggerBindingStore;
use serde_json::{json, Value};

use crate::{ApiResp, Error, Result};

async fn db_id() -> String {
    get_default_db_manager().get_default_db_id().await
}

fn ise(msg: impl Into<String>) -> Error {
    Error::InternalError(msg.into())
}

// ─────────────────── W5 · 工作区注册（孤儿端点归属） ───────────────────

/// `POST /api/dev/vscode/register` —— 扩展上报工作区（落 `cmx_dev_workspace`，幂等 upsert）。
pub async fn vscode_register(Json(req): Json<RegisterRequest>) -> Result<Json<ApiResp<Value>>> {
    let store = PgDevWorkspaceStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let ws = store.register(&req).await.map_err(ise)?;
    Ok(Json(ApiResp::ok(json!(ws))))
}

/// `GET /api/dev/workspaces` —— 列已注册工作区。
pub async fn list_workspaces() -> Result<Json<ApiResp<Value>>> {
    let store = PgDevWorkspaceStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let ws = store.list().await.map_err(ise)?;
    Ok(Json(ApiResp::ok(json!(ws))))
}

/// `GET /api/dev/workspaces/authz?dev={dev}` —— W5 M1 Nginx auth_request 子请求校验。
///
/// 返回 200 放行、403 拒绝（供 nginx `auth_request` 判断当前用户可否进 `{dev}` 的工作区）。
/// M1 起步：dev 已注册即放行；接入真实身份后按 `dev_id == 当前用户` 或租户成员判定。
#[derive(serde::Deserialize)]
pub struct AuthzQuery {
    pub dev: String,
}

pub async fn workspace_authz(
    axum::extract::Query(q): axum::extract::Query<AuthzQuery>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let store = PgDevWorkspaceStore::new(db_id().await);
    if store.ensure_schema().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    // dev 对应工作区存在且 active → 放行。（接真实认证后再叠加 dev==user / 租户成员校验。）
    match store.get(&q.dev).await {
        Ok(Some(ws)) if ws.status == "active" => StatusCode::OK.into_response(),
        Ok(_) => (StatusCode::FORBIDDEN, "工作区不存在或未激活").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ─────────────────── W1 · 构建作业 ───────────────────

/// `POST /api/dev/build/jobs` —— 提交构建作业（落库 Queued；实际编译由 Build Service worker 消费）。
///
/// M0 端点：先落作业记录 + 返回 job_id，编译执行的 worker 装配（cmx-build::Builder + Pipeline）
/// 为后续独立部署项；此处不在请求线程内 spawn cargo（守 W1 铁律）。
pub async fn submit_build_job(Json(req): Json<BuildRequest>) -> Result<Json<ApiResp<Value>>> {
    let store = PgBuildJobStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let id = new_job_id();
    let job = BuildJob {
        id: id.clone(),
        workspace_id: req.workspace_id.clone(),
        plugin_id: None,
        tenant_id: req.tenant_id.clone(),
        status: BuildStatus::Queued,
        target: req.target.clone(),
        profile: req.profile.clone(),
        wasm_path: None,
        artifact_zip_path: None,
        rev: None,
        error_summary: None,
        submitted_by: req.submitted_by.clone(),
        submitted_at: Utc::now(),
        finished_at: None,
        duration_ms: None,
    };
    store.create(&job).await.map_err(ise)?;

    // 若平台已装配全局构建执行器 → 后台真跑编译（cargo 在后台 task，不在请求线程），受配额门控；
    // 未装配 → 仅落作业记录（M0 回退），编译由独立 Build Service worker 后续消费。
    let (status, executing, denied) = if let Some(exec) = cmx_build::global::try_get() {
        match exec.submit(id.clone(), req.clone()) {
            cmx_build::SubmitOutcome::Accepted(_) => ("building", true, None),
            cmx_build::SubmitOutcome::Denied(msg) => {
                // 配额拒绝：作业置 Failed，返回原因。
                let _ = store.update_status(&id, BuildStatus::Failed, Some(&msg)).await;
                ("failed", false, Some(msg))
            }
        }
    } else {
        ("queued", false, None)
    };
    Ok(Json(ApiResp::ok(json!({
        "jobId": id,
        "status": status,
        "executing": executing,
        "denied": denied
    }))))
}

/// `GET /api/dev/build/jobs/{id}/logs` —— SSE 流式编译日志（作业在跑时可订阅）。
///
/// 作业未在跑（或平台未装配执行器）→ 返回一次性提示事件后结束。
pub async fn stream_build_logs(Path(id): Path<String>) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    match cmx_build::global::try_get().and_then(|e| e.subscribe(&id)) {
        Some(mut sub) => {
            tokio::spawn(async move {
                loop {
                    match sub.recv().await {
                        Ok(cmx_build::BuildLogEvent::Line(l)) => {
                            if tx.send(Ok(Event::default().event("line").data(l))).is_err() {
                                break;
                            }
                        }
                        Ok(cmx_build::BuildLogEvent::Done { status, error }) => {
                            let _ = tx.send(Ok(Event::default()
                                .event("done")
                                .json_data(json!({ "status": status, "error": error }))
                                .unwrap_or_default()));
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }
        None => {
            let _ = tx.send(Ok(Event::default()
                .event("done")
                .json_data(json!({ "status": "notRunning", "error": "作业未在执行或平台未装配构建执行器" }))
                .unwrap_or_default()));
        }
    }

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|ev| (ev, rx))
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/dev/build/jobs/{id}` —— 查作业状态/结果。
pub async fn get_build_job(Path(id): Path<String>) -> Result<Json<ApiResp<Value>>> {
    let store = PgBuildJobStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let job = store
        .get(&id)
        .await
        .map_err(ise)?
        .ok_or_else(|| Error::InternalError(format!("构建作业 {id} 不存在")))?;
    Ok(Json(ApiResp::ok(json!(job))))
}

/// `GET /api/dev/build/jobs` —— 列最近构建作业。
pub async fn list_build_jobs() -> Result<Json<ApiResp<Value>>> {
    let store = PgBuildJobStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let jobs = store.list_recent(50).await.map_err(ise)?;
    Ok(Json(ApiResp::ok(json!(jobs))))
}

// ─────────────────── W3 · 触发绑定 ───────────────────

/// `POST /api/dev/trigger/bindings` —— 增/改一条触发绑定。
pub async fn save_trigger_binding(Json(b): Json<TriggerBinding>) -> Result<Json<ApiResp<Value>>> {
    let store = PgTriggerBindingStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let id = store.save(&b).await.map_err(ise)?;
    Ok(Json(ApiResp::ok(json!({ "id": id }))))
}

/// `GET /api/dev/trigger/bindings?kind=event|cron|bizhook` —— 列某类绑定。
pub async fn list_trigger_bindings(
    axum::extract::Query(q): axum::extract::Query<KindQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let store = PgTriggerBindingStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let kind = parse_kind(q.kind.as_deref().unwrap_or("event"));
    let list = store.list_by_kind(kind).await.map_err(ise)?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// `DELETE /api/dev/trigger/bindings/{id}` —— 删一条绑定。
pub async fn delete_trigger_binding(Path(id): Path<i64>) -> Result<Json<ApiResp<Value>>> {
    let store = PgTriggerBindingStore::new(db_id().await);
    store.ensure_schema().await.map_err(ise)?;
    let n = store.delete(id).await.map_err(ise)?;
    Ok(Json(ApiResp::ok(json!({ "deleted": n }))))
}

#[derive(serde::Deserialize)]
pub struct KindQuery {
    pub kind: Option<String>,
}

fn parse_kind(s: &str) -> TriggerKind {
    match s {
        "cron" => TriggerKind::Cron,
        "bizhook" => TriggerKind::BizHook,
        _ => TriggerKind::Event,
    }
}

/// 生成作业 id（时间戳派生，避免额外 uuid 依赖）。
fn new_job_id() -> String {
    format!("build-{}", Utc::now().timestamp_micros())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_req_deserialize() {
        let j = json!({ "targetVersion": "1.0.0", "operator": "alice" });
        let r: RollbackReq = serde_json::from_value(j).unwrap();
        assert_eq!(r.target_version, "1.0.0");
        assert_eq!(r.operator.as_deref(), Some("alice"));
        // operator 可省。
        let r2: RollbackReq = serde_json::from_value(json!({ "targetVersion": "2.0" })).unwrap();
        assert!(r2.operator.is_none());
    }

    #[test]
    fn authz_query_parse() {
        let q: AuthzQuery = serde_json::from_value(json!({ "dev": "alice" })).unwrap();
        assert_eq!(q.dev, "alice");
    }
}

// ─────────────────── W6 · 插件版本历史 + 一键回滚 ───────────────────

/// `GET /api/dev/plugins/{id}/versions` —— 列某插件的版本历史（含 is_current 标记）。
pub async fn plugin_versions(Path(plugin_id): Path<String>) -> Result<Json<ApiResp<Value>>> {
    use cmx_plugin::infrastructure::database::version_history::VersionHistoryRepository;
    let dbm = get_default_db_manager().clone();
    let db_id = get_default_db_manager().get_default_db_id().await;
    let repo = VersionHistoryRepository::new(dbm, db_id);
    let versions = repo
        .list_versions(&plugin_id)
        .await
        .map_err(|e| ise(format!("列版本历史失败: {e}")))?;
    // 精简为 UX 友好字段。
    let items: Vec<Value> = versions
        .into_iter()
        .map(|v| {
            json!({
                "version": v.version,
                "isCurrent": v.is_current,
                "installedAt": v.installed_at,
                "wasmPath": v.wasm_path,
                "sourceType": v.zip_source_type,
                "archived": v.archived != 0,
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({ "pluginId": plugin_id, "versions": items }))))
}

/// `POST /api/dev/plugins/{id}/rollback` body：`{ targetVersion, operator? }` —— 一键回滚到指定历史版本。
#[derive(serde::Deserialize)]
pub struct RollbackReq {
    #[serde(rename = "targetVersion")]
    pub target_version: String,
    #[serde(default)]
    pub operator: Option<String>,
}

pub async fn plugin_rollback(
    Path(plugin_id): Path<String>,
    Json(req): Json<RollbackReq>,
) -> Result<Json<ApiResp<Value>>> {
    use cmx_plugin::service::downgrade::DowngradeRequest;
    use cmx_plugin::GlobalPluginManager;

    let dreq = DowngradeRequest {
        plugin_id: plugin_id.clone(),
        target_version: req.target_version.clone(),
        source: None,
        operator: req.operator.clone(),
        app_id: None,
    };
    let resp = GlobalPluginManager::get()
        .downgrade(dreq)
        .await
        .map_err(|e| ise(format!("回滚失败: {e}")))?;
    Ok(Json(ApiResp::ok(json!({
        "pluginId": resp.plugin_id,
        "oldVersion": resp.old_version,
        "newVersion": resp.new_version,
        "success": resp.success,
        "message": resp.message,
    }))))
}
