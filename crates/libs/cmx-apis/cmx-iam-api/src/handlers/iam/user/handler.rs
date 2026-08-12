//! 用户 Handler 实现
//!
//! 薄层 handler，调用 cmx-iam UserService 处理业务逻辑。
//! UserForCreate/UserForUpdate 不 derive Fields，需自定义 handler 调用 Service 层。

use axum::Json;
use axum::extract::{Query, State};
use cmx_core::model::iam::{Role, User};
use tracing::debug;

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Error, Result};

use cmx_iam::user::{AssignRolesRequest, UserFilter, UserForCreate, UserForUpdate};

/// 按用户名查询的 GET 请求参数。
///
/// 用于用户详情、用户角色列表等 GET 端点。username 是用户主账号的唯一标识。
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UsernameQuery {
    /// 用户名。
    pub username: String,
}

/// 创建用户
#[utoipa::path(
    post,
    path = "/api/iam/users/create",
    request_body = UserForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<User>),
        (status = 400, description = "参数错误"),
        (status = 409, description = "用户名已存在")
    ),
    tag = "IAM-User"
)]
pub async fn create_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<UserForCreate>,
) -> Result<Json<ApiResp<User>>> {
    debug!(
        "{:<12} - handler::create_user - username: {}",
        "HANDLER", data.username
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let user = iam
        .user_service
        .create_user(&svr_ctx, data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(user)))
}

/// 获取用户详情（按 username 查询）
#[utoipa::path(
    get,
    path = "/api/iam/users/get",
    params(
        UsernameQuery
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<User>),
        (status = 404, description = "用户不存在")
    ),
    tag = "IAM-User"
)]
pub async fn get_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<UsernameQuery>,
) -> Result<Json<ApiResp<User>>> {
    debug!(
        "{:<12} - handler::get_user - username: {}",
        "HANDLER", params.username
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let user = iam
        .user_service
        .get_user(&params.username)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(user)))
}

/// 更新用户
#[utoipa::path(
    post,
    path = "/api/iam/users/update",
    request_body = cmx_core::UpdatePayload<UserForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<User>),
        (status = 404, description = "用户不存在")
    ),
    tag = "IAM-User"
)]
pub async fn update_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::UpdatePayload<UserForUpdate>>,
) -> Result<Json<ApiResp<User>>> {
    let user_id = payload
        .id
        .as_str()
        .ok_or_else(|| Error::business_error("无效的用户ID".to_string()))?
        .to_string();

    debug!("{:<12} - handler::update_user - id: {}", "HANDLER", user_id);

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let user = iam
        .user_service
        .update_user(&svr_ctx, &user_id, payload.data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(user)))
}

/// 删除用户
#[utoipa::path(
    post,
    path = "/api/iam/users/delete",
    request_body = cmx_core::DeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "IAM-User"
)]
pub async fn delete_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::DeletePayload>,
) -> Result<Json<ApiResp<()>>> {
    let user_ids: Vec<String> = payload
        .ids
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    debug!(
        "{:<12} - handler::delete_user - count: {}",
        "HANDLER",
        user_ids.len()
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    iam.user_service
        .delete_user(&svr_ctx, &user_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 分页查询用户
#[utoipa::path(
    post,
    path = "/api/iam/users/page",
    request_body = cmx_core::PageParams<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<User>>)
    ),
    tag = "IAM-User"
)]
pub async fn page_users(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<UserFilter>>,
) -> Result<Json<ApiResp<Vec<User>>>> {
    debug!("{:<12} - handler::page_users", "HANDLER");

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (users, total) = iam
        .user_service
        .page_users(filters, list_options)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok_with_pagination(
        users,
        page_number,
        page_size,
        total as u64,
    )))
}

/// 列表查询用户
#[utoipa::path(
    post,
    path = "/api/iam/users/list",
    request_body = cmx_core::ListParams<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<User>>)
    ),
    tag = "IAM-User"
)]
pub async fn list_users(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<UserFilter>>,
) -> Result<Json<ApiResp<Vec<User>>>> {
    debug!("{:<12} - handler::list_users", "HANDLER");

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let users = iam
        .user_service
        .list_users(filters, Some(list_options))
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(users)))
}

/// 为用户分配角色
#[utoipa::path(
    post,
    path = "/api/iam/users/assign-roles",
    request_body = AssignRolesRequest,
    responses(
        (status = 200, description = "分配成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "IAM-User"
)]
pub async fn assign_roles(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<AssignRolesRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::assign_roles - username: {}, role_count: {}",
        "HANDLER",
        req.username,
        req.role_ids.len()
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    iam.user_service
        .assign_roles(&svr_ctx, &req.username, &req.role_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 获取用户的角色列表（按 username 查询）
#[utoipa::path(
    get,
    path = "/api/iam/users/roles",
    params(
        UsernameQuery
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<Role>>)
    ),
    tag = "IAM-User"
)]
pub async fn get_user_roles(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<UsernameQuery>,
) -> Result<Json<ApiResp<Vec<Role>>>> {
    debug!(
        "{:<12} - handler::get_user_roles - username: {}",
        "HANDLER", params.username
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let roles = iam
        .user_service
        .get_user_roles(&params.username)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(roles)))
}
