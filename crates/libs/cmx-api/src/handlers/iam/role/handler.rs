//! 角色 Handler 实现
//!
//! 薄层 handler，调用 cmx-iam RoleService 处理业务逻辑。

use axum::extract::{Query, State};
use axum::Json;
use cmx_core::model::iam::{Permission, Role};
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

use cmx_iam::role::{
    AssignPermissionsRequest, AssignRoleUsersRequest, RoleFilter, RoleForCreate, RoleForUpdate,
};

/// 创建角色
#[utoipa::path(
    post,
    path = "/api/iam/roles/create",
    request_body = RoleForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<Role>),
        (status = 409, description = "角色编码已存在")
    ),
    tag = "IAM-Role"
)]
pub async fn create_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<RoleForCreate>,
) -> Result<Json<ApiResp<Role>>> {
    debug!("{:<12} - handler::create_role - code: {}", "HANDLER", data.code);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let role = iam
        .role_service
        .create_role(&svr_ctx, data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role)))
}

/// 获取角色详情
#[utoipa::path(
    get,
    path = "/api/iam/roles/get",
    params(
        ("id" = String, Query, description = "角色ID")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Role>),
        (status = 404, description = "角色不存在")
    ),
    tag = "IAM-Role"
)]
pub async fn get_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<cmx_core::GetParams>,
) -> Result<Json<ApiResp<Role>>> {
    debug!("{:<12} - handler::get_role - id: {}", "HANDLER", params.id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let role = iam
        .role_service
        .get_role(&params.id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role)))
}

/// 更新角色
#[utoipa::path(
    post,
    path = "/api/iam/roles/update",
    request_body = cmx_core::UpdatePayload<RoleForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<Role>),
        (status = 404, description = "角色不存在")
    ),
    tag = "IAM-Role"
)]
pub async fn update_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::UpdatePayload<RoleForUpdate>>,
) -> Result<Json<ApiResp<Role>>> {
    let role_id = payload
        .id
        .as_str()
        .ok_or_else(|| Error::business_error("无效的角色ID".to_string()))?
        .to_string();

    debug!("{:<12} - handler::update_role - id: {}", "HANDLER", role_id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let role = iam
        .role_service
        .update_role(&svr_ctx, &role_id, payload.data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role)))
}

/// 删除角色
#[utoipa::path(
    post,
    path = "/api/iam/roles/delete",
    request_body = cmx_core::DeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>),
        (status = 400, description = "内置角色不可删除")
    ),
    tag = "IAM-Role"
)]
pub async fn delete_role(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::DeletePayload>,
) -> Result<Json<ApiResp<()>>> {
    let role_ids: Vec<String> = payload
        .ids
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    debug!("{:<12} - handler::delete_role - count: {}", "HANDLER", role_ids.len());

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    iam.role_service
        .delete_role(&svr_ctx, &role_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 分页查询角色
#[utoipa::path(
    post,
    path = "/api/iam/roles/page",
    request_body = crate::PageParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<Role>>)
    ),
    tag = "IAM-Role"
)]
pub async fn page_roles(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<RoleFilter>>,
) -> Result<Json<ApiResp<Vec<Role>>>> {
    debug!("{:<12} - handler::page_roles", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let current = params.get_page() as u64;
    let size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filter = params.filters
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default();

    let (roles, total) = iam
        .role_service
        .page_roles(filter, list_options)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok_with_pagination(roles, current, size, total as u64)))
}

/// 列表查询角色
#[utoipa::path(
    post,
    path = "/api/iam/roles/list",
    request_body = crate::ListParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<Role>>)
    ),
    tag = "IAM-Role"
)]
pub async fn list_roles(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<RoleFilter>>,
) -> Result<Json<ApiResp<Vec<Role>>>> {
    debug!("{:<12} - handler::list_roles", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let list_options = params.to_list_options();
    let filter = params.filters
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default();

    let roles = iam
        .role_service
        .list_roles(filter, Some(list_options))
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(roles)))
}

/// 为角色分配权限
#[utoipa::path(
    post,
    path = "/api/iam/roles/assign-permissions",
    request_body = AssignPermissionsRequest,
    responses(
        (status = 200, description = "分配成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "IAM-Role"
)]
pub async fn assign_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<AssignPermissionsRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::assign_permissions - role: {}, perm_count: {}",
        "HANDLER", req.role_id, req.permission_ids.len()
    );

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    iam.role_service
        .assign_permissions(&svr_ctx, &req.role_id, &req.permission_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 批量给角色分配用户（全量替换）
#[utoipa::path(
    post,
    path = "/api/iam/roles/assign-users",
    request_body = AssignRoleUsersRequest,
    responses(
        (status = 200, description = "分配成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "IAM-Role"
)]
pub async fn assign_role_users(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<AssignRoleUsersRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::assign_role_users - role: {}, user_count: {}",
        "HANDLER", req.role_id, req.user_ids.len()
    );

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    iam.role_service
        .assign_role_users(&svr_ctx, &req.role_id, &req.user_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 获取角色的权限列表
#[utoipa::path(
    get,
    path = "/api/iam/roles/permissions",
    params(
        ("id" = String, Query, description = "角色ID")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<Permission>>)
    ),
    tag = "IAM-Role"
)]
pub async fn get_role_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<cmx_core::GetParams>,
) -> Result<Json<ApiResp<Vec<Permission>>>> {
    debug!("{:<12} - handler::get_role_permissions - id: {}", "HANDLER", params.id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let permissions = iam
        .role_service
        .get_role_permissions(&params.id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(permissions)))
}
