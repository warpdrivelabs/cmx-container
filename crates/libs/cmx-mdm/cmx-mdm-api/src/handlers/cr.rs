//! M2 CR 变更请求 handler —— 审批流转 / 列表 / 详情。
//!
//! 新建 CR 走平台标准 `/doc/save`，本模块仅覆盖审批流转与查询。
//!
//! 对应路由（`cmx-mdm-api/src/lib.rs`）：
//! - `POST /mdm/change-requests/submit` → [`mdm_cr_submit`]
//! - `POST /mdm/change-requests/approve` → [`mdm_cr_approve`]
//! - `POST /mdm/change-requests/reject` → [`mdm_cr_reject`]
//! - `POST /mdm/change-requests/abort` → [`mdm_cr_abort`]
//! - `GET /mdm/change-requests` → [`mdm_cr_list`]
//! - `GET /mdm/change-requests/detail` → [`mdm_cr_detail`]

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde_json::{json, Value};

use cmx_api_core::CmxAppState;
use cmx_api_core::actor::actor_id_i64;
use cmx_api_core::db_id::resolve_db_id_from_headers;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Result};

use cmx_database_pg::get_default_pg_db_manager;
use cmx_mdm_model::codegen::RandomCodeGenerator;
use cmx_mdm_store_pg as store;

use super::{default_page, default_page_size};

/// 提交审批：draft / rejected → approving（驳回后可直接编辑重新提交，无需 clone 新 CR）。
pub async fn mdm_cr_submit(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::check_status_in(mm, &db_id, None, body.cr_id, &["draft", "rejected"]).await?;
    store::set_cr_status_pub(mm, &db_id, None, body.cr_id, "approving").await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "approving" }))))
}

/// 审批通过：approving → 激活器单事务 → activated。
///
/// 方案 A：直接对 approving 的 CR 调激活器（激活器接受 approving），失败回滚到 approving。
pub async fn mdm_cr_approve(
    State(_s): State<CmxAppState>,
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::check_status(mm, &db_id, None, body.cr_id, "approving").await?;
    let operated_by = actor_id_i64(&svr_ctx);
    let codegen = RandomCodeGenerator;
    let record_id = store::activate(mm, &db_id, body.cr_id, operated_by, &codegen).await?;
    Ok(Json(ApiResp::ok(
        json!({ "crId": body.cr_id, "status": "activated", "recordId": record_id }),
    )))
}

/// 驳回：approving → rejected（`cm_*` 全程不动）。
pub async fn mdm_cr_reject(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<RejectBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::check_status(mm, &db_id, None, body.cr_id, "approving").await?;
    store::set_cr_status_pub(mm, &db_id, None, body.cr_id, "rejected").await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "rejected" }))))
}

/// 作废：draft → aborted。
pub async fn mdm_cr_abort(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::abort_cr(mm, &db_id, body.cr_id).await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "aborted" }))))
}

/// CR 列表（query: `?docStatus=&withPayload=`）。
pub async fn mdm_cr_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<CrListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) =
        store::list_cr(mm, &db_id, q.doc_status.as_deref(), q.page, q.page_size, q.with_payload)
            .await?;
    Ok(Json(ApiResp::ok(json!({
        "list": list, "total": total, "page": q.page, "pageSize": q.page_size,
    }))))
}

/// CR 详情（query: `?crId=`，返回头 + 行）。
pub async fn mdm_cr_detail(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<CrDetailQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let detail = store::get_cr_detail(mm, &db_id, q.cr_id).await?;
    Ok(Json(ApiResp::ok(detail)))
}

/// 通用 CR id body（submit/approve/reject/abort 复用）。
#[derive(serde::Deserialize)]
pub struct CrIdBody {
    /// CR id。
    #[serde(alias = "crId")]
    pub cr_id: i64,
}

/// 驳回请求体（带可选原因）。
#[derive(serde::Deserialize)]
pub struct RejectBody {
    /// CR id。
    #[serde(alias = "crId")]
    pub cr_id: i64,
    /// 驳回原因（可选）。
    #[serde(default)]
    pub reason: Option<String>,
}

/// CR 列表查询（分页）。
#[derive(serde::Deserialize)]
pub struct CrListQuery {
    /// 单据状态过滤（可选）。
    #[serde(default, alias = "docStatus")]
    pub doc_status: Option<String>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size", alias = "pageSize")]
    pub page_size: i64,
    /// 是否返回 payload（列表默认 false 不查 payload，影响效率）。
    #[serde(default, alias = "withPayload")]
    pub with_payload: bool,
}

/// CR 详情查询。
#[derive(serde::Deserialize)]
pub struct CrDetailQuery {
    /// CR id。
    #[serde(alias = "crId")]
    pub cr_id: i64,
}
