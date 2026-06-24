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

/// 分配临时角色请求载荷。
///
/// 在 `effective_from` 至 `effective_until` 窗口内为用户赋予 `role_id` 指定角色，
/// 到期后由 cmx-iam 后台调度任务自动清理。`source` 用于审计追踪（默认 manual）。
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct AssignTempRoleRequest {
    /// 目标用户 ID。
    pub user_id: String,
    /// 待分配的角色 ID。
    pub role_id: String,
    /// 授权生效起始时间（UTC）。
    pub effective_from: DateTime<Utc>,
    /// 授权失效截止时间（UTC）。
    pub effective_until: DateTime<Utc>,
    /// 授权原因（可选，记录于审计日志）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 授权来源（manual/workflow/emergency 等），默认 "manual"。
    #[serde(skip_serializing_if = "Option::is_none", default = "default_source")]
    pub source: Option<String>,
}

fn default_source() -> Option<String> {
    Some("manual".to_string())
}

/// 撤销单条临时角色授权请求载荷。
///
/// 撤销后该 assignment 不再生效，关联用户的角色集合会立即回退。
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct RevokeTempRoleRequest {
    /// 待撤销的授权记录 ID。
    pub assignment_id: String,
    /// 撤销原因（可选，记录于审计日志）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 批量撤销临时角色授权请求载荷。
///
/// 通过单次调用撤销多条授权记录，提升管理后台批量操作效率。
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct RevokeTempRolesBatchRequest {
    /// 待撤销的授权记录 ID 列表。
    pub assignment_ids: Vec<String>,
    /// 撤销原因（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 延长临时角色授权有效期请求载荷。
///
/// 仅能延长（`new_effective_until` 须晚于当前 `effective_until`），不能缩短。
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema)]
pub struct ExtendTempRoleRequest {
    /// 待延长的授权记录 ID。
    pub assignment_id: String,
    /// 新的失效截止时间（UTC）。
    pub new_effective_until: DateTime<Utc>,
    /// 延长原因（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 临时授权查询参数。
///
/// 至少需要 `user_id` 或 `role_id` 之一。`status` 默认 all，支持按生命周期状态过滤。
#[derive(Debug, Deserialize)]
#[derive(utoipa::IntoParams)]
pub struct TempAssignmentQuery {
    /// 按用户 ID 过滤（可选）。
    pub user_id: Option<String>,
    /// 按角色 ID 过滤（可选）。
    pub role_id: Option<String>,
    /// 状态过滤：all | active | expired | revoked，默认 all。
    #[serde(default)]
    pub status: String,
}

/// 批量撤销操作的响应载荷。
///
/// 返回实际受影响的记录数，便于前端展示。
#[derive(Debug, Serialize)]
#[derive(utoipa::ToSchema)]
pub struct RevokeBatchResponse {
    /// 受影响的记录数。
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
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

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
        .map_err(|e| Error::business_error(e.to_string()))?;

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
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    iam.user_service
        .revoke_temp_role(&svr_ctx, &req.assignment_id, req.reason.as_deref())
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

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
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let affected = iam
        .user_service
        .revoke_temp_roles_batch(&svr_ctx, &req.assignment_ids, req.reason.as_deref())
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

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
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    iam.user_service
        .extend_temp_role(
            &svr_ctx,
            &req.assignment_id,
            req.new_effective_until,
            req.reason.as_deref(),
        )
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

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
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let status_filter = parse_status_filter(&params.status);

    let assignments = if let Some(user_id) = &params.user_id {
        iam.user_service
            .get_user_temp_assignments(user_id, status_filter)
            .await
            .map_err(|e| Error::business_error(e.to_string()))?
    } else if let Some(role_id) = &params.role_id {
        iam.user_service
            .get_role_temp_assigned_users(role_id, status_filter)
            .await
            .map_err(|e| Error::business_error(e.to_string()))?
    } else {
        return Err(Error::BusinessError(
            "必须提供 user_id 或 role_id 参数".to_string(),
        ));
    };

    Ok(Json(ApiResp::ok(assignments)))
}
