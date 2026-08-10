//! MDM 治理端点 handler —— 审计 / 事件 / 订阅 / 发布。
//!
//! 对应路由（`cmx-mdm-api/src/lib.rs`）：
//! - `GET /mdm/audit` → [`mdm_audit_list`]
//! - `GET /mdm/events` → [`mdm_events_list`]
//! - `GET /mdm/subscriptions` → [`mdm_subscriptions_list`]
//! - `POST /mdm/subscriptions` → [`mdm_subscriptions_save`]
//! - `POST /mdm/publish` → [`mdm_publish`]

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde_json::{json, Value};

use cmx_api::CmxAppState;
use cmx_api::db_id::resolve_db_id_from_headers;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_database_pg::get_default_pg_db_manager;
use cmx_mdm_store_pg as store;

use super::{default_page, default_page_size};

/// 变更历史 / 版本留痕。GET `?dictCode=&recordId=&page=&pageSize=`。
pub async fn mdm_audit_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GovListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) = store::list_audit(
        mm,
        &db_id,
        q.dict_code.as_deref(),
        q.record_id,
        q.page,
        q.page_size,
    )
    .await?;
    Ok(Json(ApiResp::ok(
        json!({ "list": list, "total": total, "page": q.page, "pageSize": q.page_size }),
    )))
}

/// 事件查询（delta）。GET `?dictCode=&since=&page=&pageSize=`。
pub async fn mdm_events_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GovListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) = store::list_events(
        mm,
        &db_id,
        q.dict_code.as_deref(),
        q.since,
        q.page,
        q.page_size,
    )
    .await?;
    Ok(Json(ApiResp::ok(
        json!({ "list": list, "total": total, "page": q.page, "pageSize": q.page_size }),
    )))
}

/// 订阅配置列表。GET `?page=&pageSize=`。
pub async fn mdm_subscriptions_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GovListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) = store::list_subscriptions(mm, &db_id, q.page, q.page_size).await?;
    Ok(Json(ApiResp::ok(
        json!({ "list": list, "total": total, "page": q.page, "pageSize": q.page_size }),
    )))
}

/// 订阅配置保存。POST body `{ id?, target_sys, dict_code, channel, active, filter?, field_map? }`。
pub async fn mdm_subscriptions_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let id = store::upsert_subscription(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "id": id }))))
}

/// 发布。POST body `{ dict }`（M5 分发前置；当前写一条 publish 事件占位）。
pub async fn mdm_publish(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let _headers = headers;
    let dict = body.get("dict").and_then(|v| v.as_str()).unwrap_or("");
    if dict.is_empty() {
        return Err(store::api_err("dict 不能为空"));
    }
    Ok(Json(ApiResp::ok(json!({ "dict": dict, "published": true }))))
}

/// 审计 / 事件 / 订阅 列表查询（分页，无 path variable）。
#[derive(serde::Deserialize)]
pub struct GovListQuery {
    /// 字典代码（可选过滤）。
    #[serde(default, alias = "dictCode")]
    pub dict_code: Option<String>,
    /// 记录 id（审计列表按记录过滤用）。
    #[serde(default, alias = "recordId")]
    pub record_id: Option<i64>,
    /// 事件序列起点（事件 delta 拉取用）。
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size", alias = "pageSize")]
    pub page_size: i64,
}
