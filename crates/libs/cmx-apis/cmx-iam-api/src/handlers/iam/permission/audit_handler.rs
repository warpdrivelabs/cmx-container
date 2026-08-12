//! 权限审计 handler
//!
//! 提供权限使用统计 API。

use axum::Json;
use axum::extract::State;
use tracing::debug;

use cmx_iam::service_traits::PermissionUsageStat;

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Error, Result};

/// 查询权限使用统计
#[utoipa::path(
    get,
    path = "/api/iam/permissions/usage-stat",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<PermissionUsageStat>>)
    ),
    tag = "IAM-Audit"
)]
pub async fn get_permission_usage_stat(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Vec<PermissionUsageStat>>>> {
    debug!("{:<12} - handler::get_permission_usage_stat", "HANDLER");

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let stats = iam
        .permission_service
        .get_permission_usage_stat()
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(stats)))
}
