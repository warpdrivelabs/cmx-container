//! 角色组 Handler 实现
//!
//! 薄层 handler，调用 cmx-iam RoleGroupService 处理业务逻辑。

use axum::extract::{Query, State};
use axum::Json;
use cmx_core::model::iam::{RoleGroup, RoleGroupTreeNode};
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

use cmx_iam::role_group::{RoleGroupFilter, RoleGroupForCreate, RoleGroupForUpdate};

/// 创建角色组
#[utoipa::path(
    post,
    path = "/api/iam/role-groups/create",
    request_body = RoleGroupForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<RoleGroup>)
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn create_role_group(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<RoleGroupForCreate>,
) -> Result<Json<ApiResp<RoleGroup>>> {
    debug!("{:<12} - handler::create_role_group - name: {}", "HANDLER", data.name);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let role_group = iam
        .role_group_service
        .create_role_group(&svr_ctx, data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role_group)))
}

/// 获取角色组详情
#[utoipa::path(
    get,
    path = "/api/iam/role-groups/get",
    params(
        ("id" = String, Query, description = "角色组ID")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<RoleGroup>),
        (status = 404, description = "角色组不存在")
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn get_role_group(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<cmx_core::GetParams>,
) -> Result<Json<ApiResp<RoleGroup>>> {
    debug!("{:<12} - handler::get_role_group - id: {}", "HANDLER", params.id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let role_group = iam
        .role_group_service
        .get_role_group(&params.id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role_group)))
}

/// 更新角色组
#[utoipa::path(
    post,
    path = "/api/iam/role-groups/update",
    request_body = cmx_core::UpdatePayload<RoleGroupForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<RoleGroup>),
        (status = 404, description = "角色组不存在")
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn update_role_group(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::UpdatePayload<RoleGroupForUpdate>>,
) -> Result<Json<ApiResp<RoleGroup>>> {
    let role_group_id = payload
        .id
        .as_str()
        .ok_or_else(|| Error::business_error("无效的角色组ID".to_string()))?
        .to_string();

    debug!("{:<12} - handler::update_role_group - id: {}", "HANDLER", role_group_id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let role_group = iam
        .role_group_service
        .update_role_group(&svr_ctx, &role_group_id, payload.data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role_group)))
}

/// 删除角色组
#[utoipa::path(
    post,
    path = "/api/iam/role-groups/delete",
    request_body = cmx_core::DeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>),
        (status = 409, description = "角色组下存在子组或关联角色")
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn delete_role_group(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::DeletePayload>,
) -> Result<Json<ApiResp<()>>> {
    let role_group_ids: Vec<String> = payload
        .ids
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    debug!("{:<12} - handler::delete_role_group - count: {}", "HANDLER", role_group_ids.len());

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    iam.role_group_service
        .delete_role_group(&svr_ctx, &role_group_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 分页查询角色组
#[utoipa::path(
    post,
    path = "/api/iam/role-groups/page",
    request_body = crate::PageParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<RoleGroup>>)
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn page_role_groups(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<RoleGroupFilter>>,
) -> Result<Json<ApiResp<Vec<RoleGroup>>>> {
    debug!("{:<12} - handler::page_role_groups", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let current = params.get_page() as u64;
    let size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filter = params.filters
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default();

    let (role_groups, total) = iam
        .role_group_service
        .page_role_groups(filter, list_options)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok_with_pagination(role_groups, current, size, total as u64)))
}

/// 列表查询角色组
#[utoipa::path(
    post,
    path = "/api/iam/role-groups/list",
    request_body = crate::ListParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<RoleGroup>>)
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn list_role_groups(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<RoleGroupFilter>>,
) -> Result<Json<ApiResp<Vec<RoleGroup>>>> {
    debug!("{:<12} - handler::list_role_groups", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let list_options = params.to_list_options();
    let filter = params.filters
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default();

    let role_groups = iam
        .role_group_service
        .list_role_groups(filter, Some(list_options))
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(role_groups)))
}

/// 获取角色组树
#[utoipa::path(
    get,
    path = "/api/iam/role-groups/tree",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<RoleGroupTreeNode>>)
    ),
    tag = "IAM-RoleGroup"
)]
pub async fn get_role_group_tree(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Vec<RoleGroupTreeNode>>>> {
    debug!("{:<12} - handler::get_role_group_tree", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let tree = iam
        .role_group_service
        .get_role_group_tree()
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(tree)))
}
