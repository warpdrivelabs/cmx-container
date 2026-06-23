//! 角色审计 handler
//!
//! 提供角色权限差异比较 API。

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use tracing::debug;

use cmx_iam::service_traits::PermissionDiffResponse;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

/// 权限差异查询参数
#[derive(Debug, Deserialize)]
#[derive(utoipa::IntoParams)]
pub struct PermissionDiffQuery {
    pub role_id_1: String,
    pub role_id_2: String,
}

/// 比较两个角色的权限差异
#[utoipa::path(
    get,
    path = "/api/iam/roles/permission-diff",
    params(
        PermissionDiffQuery
    ),
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "IAM-Role-Audit"
)]
pub async fn get_permission_diff(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<PermissionDiffQuery>,
) -> Result<Json<ApiResp<PermissionDiffResponse>>> {
    debug!(
        "{:<12} - handler::get_permission_diff - r1: {}, r2: {}",
        "HANDLER", params.role_id_1, params.role_id_2
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let result = iam
        .role_service
        .get_permission_diff(&params.role_id_1, &params.role_id_2)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(result)))
}
