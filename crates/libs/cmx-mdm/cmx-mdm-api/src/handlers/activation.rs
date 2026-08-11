//! 激活映射配置 CRUD + 手动激活 handler。
//!
//! 对应路由（`cmx-mdm-api/src/lib.rs`）：
//! - `GET /mdm/activations` → [`mdm_activations_list`]
//! - `POST /mdm/activations` → [`mdm_activations_save`]
//! - `POST /mdm/change-requests/activate` → [`mdm_cr_activate`]

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde_json::{json, Value};

use cmx_api::CmxAppState;
use cmx_api::actor::actor_id_i64;
use cmx_api::db_id::resolve_db_id_from_headers;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_database_pg::get_default_pg_db_manager;
use cmx_mdm_model::activation::ActivationConfig;
use cmx_mdm_model::codegen::RandomCodeGenerator;
use cmx_mdm_store_pg as store;

/// 激活映射列表（配置器 UI 用）。
///
/// 按 `sourceDocType` / `crType` 可选过滤。
pub async fn mdm_activations_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<ActivationListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let list = store::list(
        mm,
        &db_id,
        q.source_doc_type.as_deref(),
        q.cr_type.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// 保存激活映射（upsert by activation_code）。配置器 UI 用。
///
/// 返回 `{ activationCode }`。
pub async fn mdm_activations_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<ActivationConfig>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let code = store::upsert(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "activationCode": code }))))
}

/// 删除激活映射（硬删除）。POST body `{ activationCode }`。
///
/// 对应路由 `POST /mdm/activations/delete`（禁用 Path Variable，承接 AGENTS.md §四 第 5 条）。
pub async fn mdm_activations_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<ActivationDeleteBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let n = store::delete_by_code(mm, &db_id, &body.activation_code).await?;
    Ok(Json(ApiResp::ok(
        json!({ "activationCode": body.activation_code, "affected": n }),
    )))
}

/// 删除激活映射请求体。
#[derive(serde::Deserialize)]
pub struct ActivationDeleteBody {
    /// 待删除的激活编码（cmx_mdm_activation 唯一键）。
    #[serde(alias = "activationCode")]
    pub activation_code: String,
}

/// 手动触发激活（审批型 CR 兜底入口 / 内部 CR 直接调）。
///
/// body `{ crId }`，返回激活后的主数据记录 id。
pub async fn mdm_cr_activate(
    State(_s): State<CmxAppState>,
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<ActivateBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let operated_by = actor_id_i64(&svr_ctx);
    let codegen = RandomCodeGenerator;
    let record_id = store::activate(mm, &db_id, body.cr_id, operated_by, &codegen).await?;
    Ok(Json(ApiResp::ok(json!({ "recordId": record_id }))))
}

/// 激活映射列表查询参数。
#[derive(serde::Deserialize)]
pub struct ActivationListQuery {
    /// 源单据类型（可选过滤）。
    #[serde(default, alias = "sourceDocType")]
    pub source_doc_type: Option<String>,
    /// CR 类型（可选过滤）。
    #[serde(default, alias = "crType")]
    pub cr_type: Option<String>,
}

/// 手动激活请求体。
#[derive(serde::Deserialize)]
pub struct ActivateBody {
    /// 待激活的 CR id。
    #[serde(alias = "crId")]
    pub cr_id: i64,
}
