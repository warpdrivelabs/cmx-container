//! 查重规则配置 handler。
//!
//! 规则维护内嵌查重界面，无独立管理页。
//!
//! 对应路由（`cmx-mdm-api/src/lib.rs`）：
//! - `GET /mdm/match-configs` → [`mdm_match_configs_list`]
//! - `POST /mdm/match-configs` → [`mdm_match_configs_save`]
//! - `POST /mdm/match-configs/delete` → [`mdm_match_configs_delete`]

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde_json::{json, Value};

use cmx_api_core::CmxAppState;
use cmx_api_core::db_id::resolve_db_id_from_headers;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Result};

use cmx_database_pg::get_default_pg_db_manager;
use cmx_mdm_store_pg as store;

/// 查重规则列表。GET `?dictCode=`（可空，空则列全部）。
pub async fn mdm_match_configs_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<MatchConfigQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let list = store::list_match_config(mm, &db_id, q.dict_code.as_deref()).await?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// 查重规则保存（upsert）。POST body `{ id?, ruleName, dictCode, targetTable, specs, clusterKeys, surviveFields, thresholds? }`。
///
/// id 缺省 / 0 = 新建；id 非零或 (dictCode, ruleName) 已存在 = 更新。返回规则 id（i64）。
pub async fn mdm_match_configs_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let id = store::upsert_match_config(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "id": id }))))
}

/// 查重规则删除（软删 is_active=FALSE）。POST body `{ configId }`。
pub async fn mdm_match_configs_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<MatchConfigDeleteBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let n = store::delete_match_config(mm, &db_id, body.config_id).await?;
    Ok(Json(ApiResp::ok(
        json!({ "configId": body.config_id, "affected": n }),
    )))
}

/// 查重规则列表查询。
#[derive(serde::Deserialize)]
pub struct MatchConfigQuery {
    /// 字典代码（可选过滤，空则列全部）。
    #[serde(default, alias = "dictCode")]
    pub dict_code: Option<String>,
}

/// 查重规则删除请求体。
#[derive(serde::Deserialize)]
pub struct MatchConfigDeleteBody {
    /// 待删除的规则 id。
    #[serde(alias = "configId")]
    pub config_id: i64,
}
