//! 临时角色授权 handler
//!
//! 提供临时角色分配/撤销/批量撤销/延长有效期/查询列表等 API。

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use cmx_iam::service_traits::{TempAssignmentStatusFilter, UserRoleAssignment};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

/// 分配临时角色请求
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct AssignTempRoleRequest {
    pub user_id: String,
    pub role_id: String,
    pub effective_from: DateTime<Utc>,
    pub effective_until: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default = "default_source")]
    pub source: Option<String>,
}

fn default_source() -> Option<String> {
    Some("manual".to_string())
}

/// 撤销临时角色请求
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct RevokeTempRoleRequest {
    pub assignment_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 批量撤销临时角色请求
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct RevokeTempRolesBatchRequest {
    pub assignment_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 延长临时授权请求
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct ExtendTempRoleRequest {
    pub assignment_id: String,
    pub new_effective_until: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 临时授权查询参数
#[derive(Debug, Deserialize)]
#[derive(utoipa::IntoParams)]
pub struct TempAssignmentQuery {
    pub user_id: Option<String>,
    pub role_id: Option<String>,
    /// all | active | expired | revoked（默认 all）
    #[serde(default)]
    pub status: String,
}

/// 批量撤销响应
#[derive(Debug, Serialize)]
#[derive(utoipa::ToSchema)]
pub struct RevokeBatchResponse {
    pub affected: u64,
}

fn parse_status_filter(s: &str) -> TempAssignmentStatusFilter {
    match s {
        "active" => TempAssignmentStatusFilter::Active,
        "expired" => TempAssignmentStatusFilter::Expired,
        "revoked" => TempAssignmentStatusFilter::Revoked,
        _ => TempAssignmentStatusFilter::All,
    }
}

/// 分配临时角色
#[utoipa::path(
    post,
    path = "/api/iam/users/assign-temp-role",
    request_body = AssignTempRoleRequest,
    responses(
        (status = 200, description = "分配成功")
    ),
    tag = "IAM-User-Temp"
)]
pub async fn assign_temp_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<AssignTempRoleRequest>,
) -> Result<Json<ApiResp<UserRoleAssignment>>> {
    debug!(
        "{:<12} - handler::assign_temp_role - user: {}, role: {}",
        "HANDLER", req.user_id, req.role_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::InternalError("IAM 服务未初始化".to_string()))?;

    let source = req.source.as_deref().unwrap_or("manual");
    let assignment = iam
        .user_service
        .assign_temp_role(
            &svr_ctx,
            &req.user_id,
            &req.role_id,
            req.effective_from,
            req.effective_until,
            req.reason.as_deref(),
            source,
        )
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    Ok(Json(ApiResp::ok(assignment)))
}

/// 撤销临时角色
#[utoipa::path(
    post,
    path = "/api/iam/users/revoke-temp-role",
    request_body = RevokeTempRoleRequest,
    responses(
        (status = 200, description = "撤销成功")
    ),
    tag = "IAM-User-Temp"
)]
pub async fn revoke_temp_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<RevokeTempRoleRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::revoke_temp_role - assignment: {}",
        "HANDLER", req.assignment_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::InternalError("IAM 服务未初始化".to_string()))?;

    iam.user_service
        .revoke_temp_role(&svr_ctx, &req.assignment_id, req.reason.as_deref())
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 批量撤销临时角色
#[utoipa::path(
    post,
    path = "/api/iam/users/revoke-temp-roles-batch",
    request_body = RevokeTempRolesBatchRequest,
    responses(
        (status = 200, description = "批量撤销成功")
    ),
    tag = "IAM-User-Temp"
)]
pub async fn revoke_temp_roles_batch(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<RevokeTempRolesBatchRequest>,
) -> Result<Json<ApiResp<RevokeBatchResponse>>> {
    debug!(
        "{:<12} - handler::revoke_temp_roles_batch - count: {}",
        "HANDLER",
        req.assignment_ids.len()
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::InternalError("IAM 服务未初始化".to_string()))?;

    let affected = iam
        .user_service
        .revoke_temp_roles_batch(&svr_ctx, &req.assignment_ids, req.reason.as_deref())
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    Ok(Json(ApiResp::ok(RevokeBatchResponse { affected })))
}

/// 延长临时授权有效期
#[utoipa::path(
    post,
    path = "/api/iam/users/extend-temp-role",
    request_body = ExtendTempRoleRequest,
    responses(
        (status = 200, description = "延长成功")
    ),
    tag = "IAM-User-Temp"
)]
pub async fn extend_temp_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<ExtendTempRoleRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::extend_temp_role - assignment: {}, new_until: {}",
        "HANDLER", req.assignment_id, req.new_effective_until
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::InternalError("IAM 服务未初始化".to_string()))?;

    iam.user_service
        .extend_temp_role(
            &svr_ctx,
            &req.assignment_id,
            req.new_effective_until,
            req.reason.as_deref(),
        )
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 查询临时授权列表
///
/// 支持按 user_id 或 role_id 查询，status 过滤
#[utoipa::path(
    get,
    path = "/api/iam/users/temp-assignments",
    params(
        TempAssignmentQuery
    ),
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "IAM-User-Temp"
)]
pub async fn get_temp_assignments(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<TempAssignmentQuery>,
) -> Result<Json<ApiResp<Vec<UserRoleAssignment>>>> {
    debug!(
        "{:<12} - handler::get_temp_assignments - user_id: {:?}, role_id: {:?}, status: {}",
        "HANDLER", params.user_id, params.role_id, params.status
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::InternalError("IAM 服务未初始化".to_string()))?;

    let status_filter = parse_status_filter(&params.status);

    let assignments = if let Some(user_id) = &params.user_id {
        iam.user_service
            .get_user_temp_assignments(user_id, status_filter)
            .await
            .map_err(|e| Error::InternalError(e.to_string()))?
    } else if let Some(role_id) = &params.role_id {
        iam.user_service
            .get_role_temp_assigned_users(role_id, status_filter)
            .await
            .map_err(|e| Error::InternalError(e.to_string()))?
    } else {
        return Err(Error::BusinessError(
            "必须提供 user_id 或 role_id 参数".to_string(),
        ));
    };

    Ok(Json(ApiResp::ok(assignments)))
}
