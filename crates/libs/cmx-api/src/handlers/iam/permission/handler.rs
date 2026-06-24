//! 权限 Handler 实现
//!
//! 薄层 handler，调用 cmx-iam PermissionService 处理业务逻辑。

use axum::extract::{Query, State};
use axum::Json;
use cmx_core::model::iam::{Permission, PermissionTreeNode};
use serde::Deserialize;
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

use cmx_iam::permission::{PermissionFilter, PermissionForCreate, PermissionForUpdate};

/// 创建权限
#[utoipa::path(
    post,
    path = "/api/iam/permissions/create",
    request_body = PermissionForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<Permission>),
        (status = 409, description = "权限编码已存在")
    ),
    tag = "IAM-Permission"
)]
pub async fn create_permission(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<PermissionForCreate>,
) -> Result<Json<ApiResp<Permission>>> {
    debug!("{:<12} - handler::create_permission - code: {}", "HANDLER", data.code);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let permission = iam
        .permission_service
        .create_permission(&svr_ctx, data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(permission)))
}

/// 获取权限详情
#[utoipa::path(
    get,
    path = "/api/iam/permissions/get",
    params(
        ("id" = String, Query, description = "权限ID")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Permission>),
        (status = 404, description = "权限不存在")
    ),
    tag = "IAM-Permission"
)]
pub async fn get_permission(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<cmx_core::GetParams>,
) -> Result<Json<ApiResp<Permission>>> {
    debug!("{:<12} - handler::get_permission - id: {}", "HANDLER", params.id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let permission = iam
        .permission_service
        .get_permission(&params.id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(permission)))
}

/// 更新权限
#[utoipa::path(
    post,
    path = "/api/iam/permissions/update",
    request_body = cmx_core::UpdatePayload<PermissionForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<Permission>),
        (status = 404, description = "权限不存在")
    ),
    tag = "IAM-Permission"
)]
pub async fn update_permission(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::UpdatePayload<PermissionForUpdate>>,
) -> Result<Json<ApiResp<Permission>>> {
    let permission_id = payload
        .id
        .as_str()
        .ok_or_else(|| Error::business_error("无效的权限ID".to_string()))?
        .to_string();

    debug!("{:<12} - handler::update_permission - id: {}", "HANDLER", permission_id);

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let permission = iam
        .permission_service
        .update_permission(&svr_ctx, &permission_id, payload.data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(permission)))
}

/// 删除权限
#[utoipa::path(
    post,
    path = "/api/iam/permissions/delete",
    request_body = cmx_core::DeletePayload,
    responses(
        (status = 200, description = "删除成功")
    ),
    tag = "IAM-Permission"
)]
pub async fn delete_permission(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::DeletePayload>,
) -> Result<Json<ApiResp<()>>> {
    let permission_ids: Vec<String> = payload
        .ids
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    debug!("{:<12} - handler::delete_permission - count: {}", "HANDLER", permission_ids.len());

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    iam.permission_service
        .delete_permission(&svr_ctx, &permission_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 分页查询权限
#[utoipa::path(
    post,
    path = "/api/iam/permissions/page",
    request_body = crate::PageParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "IAM-Permission"
)]
pub async fn page_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<PermissionFilter>>,
) -> Result<Json<ApiResp<Vec<Permission>>>> {
    debug!("{:<12} - handler::page_permissions", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let current = params.get_page() as u64;
    let size = params.get_size() as u64;
    let filter = params.filters
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default();

    let (permissions, total) = iam
        .permission_service
        .page_permissions(filter, current, size)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok_with_pagination(permissions, current, size, total as u64)))
}

/// 列表查询权限
#[utoipa::path(
    post,
    path = "/api/iam/permissions/list",
    request_body = crate::ListParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "IAM-Permission"
)]
pub async fn list_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<PermissionFilter>>,
) -> Result<Json<ApiResp<Vec<Permission>>>> {
    debug!("{:<12} - handler::list_permissions", "HANDLER");

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let filter = params.filters
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default();

    let permissions = iam
        .permission_service
        .list_permissions(filter)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(permissions)))
}

/// 权限树查询参数。
///
/// 支持按域/应用/模块编码做多层过滤，缺省时返回全量权限树。
#[derive(Debug, Deserialize, Default)]
pub struct PermissionTreeQuery {
    /// 所属域编码（如 platform/tenant）。
    pub domain_code: Option<String>,
    /// 所属应用编码。
    pub app_code: Option<String>,
    /// 所属模块编码。
    pub module_code: Option<String>,
}

/// 获取权限树（支持按域/应用/模块过滤）
#[utoipa::path(
    get,
    path = "/api/iam/permissions/tree",
    params(
        ("domain_code" = Option<String>, Query, description = "所属域编码"),
        ("app_code" = Option<String>, Query, description = "所属应用编码"),
        ("module_code" = Option<String>, Query, description = "所属模块编码")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<PermissionTreeNode>>)
    ),
    tag = "IAM-Permission"
)]
pub async fn get_permission_tree(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(query): Query<PermissionTreeQuery>,
) -> Result<Json<ApiResp<Vec<PermissionTreeNode>>>> {
    debug!(
        "{:<12} - handler::get_permission_tree - domain: {:?}, app: {:?}, module: {:?}",
        "HANDLER", query.domain_code, query.app_code, query.module_code
    );

    let iam = cmx_state.iam().ok_or_else(|| {
        Error::business_error("IAM 服务未初始化".to_string())
    })?;

    let tree = iam
        .permission_service
        .get_permission_tree(
            query.domain_code.as_deref(),
            query.app_code.as_deref(),
            query.module_code.as_deref(),
        )
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(tree)))
}
