//! 任务中心薄 axum handler + SSE 端点。
//!
//! 控制流（提交/列表/详情/暂停/恢复/停止/重启/删除）走普通 HTTP → [`JobManager`]；
//! 进度流走 SSE（`GET /jobs/{id}/events`），按 job_id 扇出（母版 = cmx-ai `subscribe_events`）。
//! SSE 鉴权复用 mw_auth 的 query `access_token` 兜底（EventSource 无法发 header），无需白名单。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, CmxAppState, Result};

use cmx_job_core::{
    ControlOutcome, Job, JobOrigin, JobStatus, SubmitRequest, manager as job_manager,
};

// ───────────────────────── 内部辅助 ─────────────────────────

/// 取全局 JobManager；未初始化返回 503。
fn require_manager() -> Result<&'static cmx_job_core::JobManager> {
    job_manager().ok_or_else(|| {
        cmx_api::Error::ServiceUnavailable("任务中心未初始化（init_job_subsystem 未调用）".into())
    })
}

/// ControlOutcome → HTTP 响应（Accepted=200 / NotFound=404 / Rejected=409）。
fn control_response(out: ControlOutcome) -> Response {
    match out {
        ControlOutcome::Accepted => {
            Json(ApiResp::ok(json!({ "accepted": true }))).into_response()
        }
        ControlOutcome::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ApiResp::<Value>::fail(404, "作业不存在")),
        )
            .into_response(),
        ControlOutcome::Rejected(why) => (
            StatusCode::CONFLICT,
            Json(ApiResp::<Value>::fail(409, why)),
        )
            .into_response(),
    }
}

/// Job → 列表/详情 JSON（camelCase 字段，前端直接消费）。
/// `kindClass`/`singleton` 从 handler 能力查出（供前端区分批处理 vs 常驻消费者，渲染吞吐视图）。
fn job_json(j: &Job, mgr: &cmx_job_core::JobManager) -> Value {
    let caps = mgr.caps_of(&j.kind);
    let kind_class = match caps.map(|c| c.kind_class) {
        Some(cmx_job_core::JobClass::Service) => "service",
        _ => "batch",
    };
    json!({
        "id": j.id.to_string(),           // i64 → string，避免 JS 大整数精度丢失
        "kind": j.kind,
        "kindClass": kind_class,
        "singleton": caps.map(|c| c.singleton).unwrap_or(false),
        "title": j.title,
        "status": j.status.as_str(),
        "progress": j.progress,
        "result": j.result,
        "error": j.error,
        "priority": j.priority,
        "origin": j.origin,
        "createdAt": j.created_at,
        "startedAt": j.started_at,
        "finishedAt": j.finished_at,
    })
}

/// 已注册种类 + 元数据（前端下拉/识别 Service 类）。
fn kinds_meta_json(mgr: &cmx_job_core::JobManager) -> Value {
    let arr: Vec<Value> = mgr
        .kinds_meta()
        .into_iter()
        .map(|(k, caps)| {
            json!({
                "kind": k,
                "kindClass": if matches!(caps.kind_class, cmx_job_core::JobClass::Service) { "service" } else { "batch" },
                "singleton": caps.singleton,
                "pausable": caps.pausable,
            })
        })
        .collect();
    Value::Array(arr)
}

/// 解析路径里的 job id（非法 → 400）。
fn parse_id(raw: &str) -> Result<i64> {
    raw.parse::<i64>()
        .map_err(|_| cmx_api::Error::BadRequest(format!("非法作业 id: {raw}")))
}

// ───────────────────────── 提交 / 查询 ─────────────────────────

/// `POST /api/jobs` —— 提交作业（前端发起路径，方案 §11①）。
///
/// body: `{ kind, params?, title?, priority? }`；返回 `{ id }`。
pub async fn submit_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(ctx): CmxSvrContext,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<ApiResp<Value>>> {
    let mgr = require_manager()?;
    let user = ctx
        .auth_context
        .as_ref()
        .map(|a| a.username.clone())
        .filter(|s| !s.is_empty());
    let id = mgr
        .submit(req, JobOrigin::Frontend { user })
        .await
        .map_err(|e| match e.code {
            409 => cmx_api::Error::Conflict(e.message),   // 单例约束：已有活跃实例
            _ => cmx_api::Error::BadRequest(e.message),
        })?;
    Ok(Json(ApiResp::ok(json!({ "id": id.to_string() }))))
}

/// `GET /api/jobs` 查询参数。
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `GET /api/jobs` —— 作业列表（过滤 kind/status，倒序，limit 截断；合并内存热态 + 持久化历史）。
pub async fn list_jobs(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mgr = require_manager()?;
    let status = q.status.as_deref().and_then(parse_status);
    let jobs = mgr
        .list(q.kind.as_deref(), status, q.limit.unwrap_or(200))
        .await;
    let items: Vec<Value> = jobs.iter().map(|j| job_json(j, mgr)).collect();
    Ok(Json(ApiResp::ok(json!({
        "items": items,
        "kinds": mgr.kinds(),
        "kindsMeta": kinds_meta_json(mgr),
    }))))
}

/// `GET /api/jobs/{id}` —— 作业详情（含完整快照；内存未命中回落持久化）。
pub async fn get_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let mgr = require_manager()?;
    let id = parse_id(&id)?;
    match mgr.get(id).await {
        Some(j) => Ok(Json(ApiResp::ok(job_json(&j, mgr)))),
        None => Err(cmx_api::Error::NotFound(format!("作业 {id} 不存在"))),
    }
}

fn parse_status(s: &str) -> Option<JobStatus> {
    match s {
        "pending" => Some(JobStatus::Pending),
        "running" => Some(JobStatus::Running),
        "paused" => Some(JobStatus::Paused),
        "cancelling" => Some(JobStatus::Cancelling),
        "cancelled" => Some(JobStatus::Cancelled),
        "completed" => Some(JobStatus::Completed),
        "failed" => Some(JobStatus::Failed),
        _ => None,
    }
}

// ───────────────────────── 控制（方案 §4.3）─────────────────────────

/// `POST /api/jobs/{id}/pause`。
pub async fn pause_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Response {
    let (mgr, id) = match parse_control(&id) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    control_response(mgr.pause(id).await)
}

/// `POST /api/jobs/{id}/resume`。
pub async fn resume_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Response {
    let (mgr, id) = match parse_control(&id) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    control_response(mgr.resume(id).await)
}

/// `POST /api/jobs/{id}/cancel`。
pub async fn cancel_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Response {
    let (mgr, id) = match parse_control(&id) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    control_response(mgr.cancel(id).await)
}

/// `POST /api/jobs/{id}/restart` —— 重启（Fresh：派生新作业），返回新 id。
pub async fn restart_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Response {
    let (mgr, id) = match parse_control(&id) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match mgr.restart(id).await {
        Ok(new_id) => {
            Json(ApiResp::ok(json!({ "id": new_id.to_string() }))).into_response()
        }
        Err(out) => control_response(out),
    }
}

/// `DELETE /api/jobs/{id}` —— 归档作业（仅终态；RU/HI 分离：转移到历史表而非真删）。
pub async fn delete_job(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Response {
    let mgr = match require_manager() {
        Ok(m) => m,
        Err(e) => return e.into_response(),
    };
    let id = match id.parse::<i64>() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResp::<Value>::fail(400, "非法作业 id")),
            )
                .into_response();
        }
    };
    control_response(mgr.archive(id).await)
}

/// `GET /api/jobs/history` 查询参数（过滤 + 分页）。
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// 页码（1-based，默认 1）。
    #[serde(default)]
    pub page: Option<usize>,
    /// 每页条数（默认 20，上限 200）。
    #[serde(default)]
    pub page_size: Option<usize>,
}

/// `GET /api/jobs/history` —— 历史作业列表（cmx_job_hi，过滤 kind/status，archived_at 倒序，分页）。
///
/// 返回 `{ items, total, page, pageSize, totalPages, kinds }`——total/totalPages 与过滤条件一致。
pub async fn list_history(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mgr = require_manager()?;
    let status = q.status.as_deref().and_then(parse_status);
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(20).clamp(1, 200);
    let offset = (page - 1) * page_size;
    let jobs = mgr
        .list_history(q.kind.as_deref(), status, offset, page_size)
        .await;
    let total = mgr.count_history(q.kind.as_deref(), status).await;
    let total_pages = if total == 0 { 0 } else { (total as usize).div_ceil(page_size) };
    let items: Vec<Value> = jobs.iter().map(|j| job_json(j, mgr)).collect();
    Ok(Json(ApiResp::ok(json!({
        "items": items,
        "total": total,
        "page": page,
        "pageSize": page_size,
        "totalPages": total_pages,
        "kinds": mgr.kinds(),
        "kindsMeta": kinds_meta_json(mgr),
    }))))
}

/// `GET /api/jobs/history/{id}` —— 单条历史作业详情。
pub async fn get_history(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let mgr = require_manager()?;
    let id = parse_id(&id)?;
    match mgr.get_history(id).await {
        Some(j) => Ok(Json(ApiResp::ok(job_json(&j, mgr)))),
        None => Err(cmx_api::Error::NotFound(format!("历史作业 {id} 不存在"))),
    }
}

/// 控制端点公共前置：取 manager + 解析 id。失败返回现成 Response。
fn parse_control(raw_id: &str) -> std::result::Result<(&'static cmx_job_core::JobManager, i64), Response> {
    let mgr = require_manager().map_err(|e| e.into_response())?;
    let id = raw_id.parse::<i64>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResp::<Value>::fail(400, "非法作业 id")),
        )
            .into_response()
    })?;
    Ok((mgr, id))
}

// ───────────────────────── SSE（方案 §6）─────────────────────────

/// `GET /api/jobs/{id}/events` 查询参数。
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// 访问令牌（EventSource 无法发 header，mw_auth 走 query 兜底校验；此处保留字段兼容）。
    #[serde(default)]
    pub access_token: Option<String>,
}

/// `GET /api/jobs/{id}/events` —— SSE 事件流（订阅单作业实时进度，方案 §6.1）。
///
/// **鉴权**：mw_auth 支持 query `access_token` 兜底并完成完整校验，AuthContext 已注入，无需白名单。
/// 订阅后立即补发一帧 `snapshot`（当前完整快照）→ 之后收 state/progress/item/log/result/error/done 增量。
pub async fn subscribe_events(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
    Query(_q): Query<EventsQuery>,
) -> Response {
    let mgr = match require_manager() {
        Ok(m) => m,
        Err(e) => return e.into_response(),
    };
    let id = match id.parse::<i64>() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResp::<Value>::fail(400, "非法作业 id")),
            )
                .into_response();
        }
    };

    // 本节点属主（内存有热态）→ 走实时 hub 流（首帧 snapshot + 增量）。
    if let Some(snapshot) = mgr.snapshot_event(id) {
        let rx = mgr.hub().subscribe(id);
        let head = futures::stream::once(async move {
            Ok::<Event, std::convert::Infallible>(
                Event::default()
                    .event(snapshot.event_name)
                    .data(snapshot.payload),
            )
        });
        let tail = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|ev| {
                let event = Event::default().event(ev.event_name).data(ev.payload);
                (Ok::<Event, std::convert::Infallible>(event), rx)
            })
        });
        let stream = futures::StreamExt::chain(head, tail);
        return Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response();
    }

    // 非本节点属主（分布式：作业在他节点跑）→ DB 轮询合成流。
    // 首帧 snapshot（读 DB）+ 每 1s 轮询 DB 合成 progress/state，直至终态 done。
    // 无跨节点事件总线时的务实方案（deadpool 无持久连接不能 LISTEN/NOTIFY）。
    let first = match mgr.get(id).await {
        Some(j) => j,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResp::<Value>::fail(404, "作业不存在")),
            )
                .into_response();
        }
    };
    let mgr2 = mgr.clone();
    let stream = futures::stream::unfold(
        (mgr2, id, Some(first), 0u64, false),
        |(mgr, id, first, last_rev, done)| async move {
            if done {
                return None;
            }
            // 首帧：snapshot。
            if let Some(job) = first {
                let ev = Event::default()
                    .event("snapshot")
                    .data(
                        serde_json::json!({
                            "status": job.status.as_str(),
                            "progress": job.progress,
                        })
                        .to_string(),
                    );
                return Some((
                    Ok::<Event, std::convert::Infallible>(ev),
                    (mgr, id, None, job.progress.rev, false),
                ));
            }
            // 后续：轮询 DB。
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            match mgr.get(id).await {
                Some(job) => {
                    let terminal = job.status.is_terminal();
                    let ev = if terminal {
                        Event::default().event("state").data(
                            serde_json::json!({ "status": job.status.as_str(), "rev": job.progress.rev }).to_string(),
                        )
                    } else {
                        Event::default().event("progress").data(
                            serde_json::json!({
                                "done": job.progress.done, "total": job.progress.total,
                                "ok": job.progress.ok, "failed": job.progress.failed,
                                "percent": job.progress.percent(), "message": job.progress.message,
                                "phase": job.progress.phase, "rev": job.progress.rev,
                            }).to_string(),
                        )
                    };
                    Some((Ok(ev), (mgr, id, None, job.progress.rev.max(last_rev), terminal)))
                }
                None => None,
            }
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/jobs/events` —— 汇总 SSE 流（订阅全部作业的状态变化，供列表页实时刷新，方案 §6.1）。
///
/// 与单作业流不同：无 job id、无 snapshot 首帧；只推 `job` 事件（作业级摘要：id/status/percent/…），
/// 前端据此 upsert 列表行。任何作业提交/状态跃迁/进度去抖点都会广播一条。
pub async fn subscribe_summary(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(_q): Query<EventsQuery>,
) -> Response {
    let mgr = match require_manager() {
        Ok(m) => m,
        Err(e) => return e.into_response(),
    };
    // 订阅汇总频道（SUMMARY_CHANNEL）。
    let rx = mgr
        .summary_hub()
        .subscribe(cmx_job_core::SUMMARY_CHANNEL);
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
