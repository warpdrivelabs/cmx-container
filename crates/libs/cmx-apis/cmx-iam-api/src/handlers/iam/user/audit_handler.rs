//! 用户审计 handler
//!
//! 提供用户有效权限查询 API。

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use tracing::debug;

use cmx_iam::service_traits::EffectivePermissionsResponse;

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Error, Result};

/// 用户有效权限查询参数。
///
/// 用于查询用户合并后的有效权限集合。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct EffectivePermissionsQuery {
    /// 用户 ID。
    pub user_id: String,
}

/// 查询用户有效权限（合并永久 + 临时授权）
#[utoipa::path(
    get,
    path = "/api/iam/users/effective-permissions",
    params(
        EffectivePermissionsQuery
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<EffectivePermissionsResponse>)
    ),
    tag = "IAM-Audit"
)]
pub async fn get_effective_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<EffectivePermissionsQuery>,
) -> Result<Json<ApiResp<EffectivePermissionsResponse>>> {
    debug!(
        "{:<12} - handler::get_effective_permissions - user_id: {}",
        "HANDLER", params.user_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let result = iam
        .user_service
        .get_effective_permissions(&params.user_id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(result)))
}
