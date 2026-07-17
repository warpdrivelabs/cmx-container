//! Menu 实体的手写 Handler。
//!
//! 菜单的增删改涉及树形字段(leaf/depth/parent_code/id_path/code_path)的组装与级联,
//! 不能使用标准 CRUD 宏(宏走 GenericCrudService 直接 INSERT/UPDATE/DELETE,绕过 MenuService)。
//! 此处参照 permission 模式手写 create/update/delete/get/list/page,
//! body 内部委托 cmx-biz 的 MenuService 完成树形字段的维护。

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use cmx_api_types::TreeNode;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::{
    DeletePayload, GetParams, ListParams, PageParams, UpdatePayload,
};
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use crate::{ApiResp, Error, Result};

use cmx_biz::menu::{MenuFilter, MenuForCreate, MenuForUpdate, MenuService, MenuTreeNodeData};

/// 创建菜单
///
/// 由 MenuService 计算 id_path/code_path/depth/parent_code 后事务内写入,
/// 并将父节点 leaf 置为 0。
#[utoipa::path(
    post,
    path = "/api/menu/create",
    request_body = MenuForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<serde_json::Value>),
        (status = 500, description = "父菜单不存在或写入失败")
    ),
    tag = "Menu"
)]
pub async fn create_menu(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<MenuForCreate>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::create_menu", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = MenuService::create(mm, &db_id, None, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 获取菜单详情
#[utoipa::path(
    get,
    path = "/api/menu/get",
    params(
        ("id" = String, Query, description = "菜单ID")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<serde_json::Value>),
        (status = 404, description = "菜单不存在")
    ),
    tag = "Menu"
)]
pub async fn get_menu(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<GetParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::get_menu", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = MenuService::get(mm, &db_id, &params.id).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新菜单
///
/// 当 parent_id 变更时级联重算该节点及其所有后代的 depth/id_path/code_path,
/// 并同步维护新旧父节点的 leaf 标志。
#[utoipa::path(
    post,
    path = "/api/menu/update",
    request_body = UpdatePayload<MenuForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<serde_json::Value>),
        (status = 404, description = "菜单不存在")
    ),
    tag = "Menu"
)]
pub async fn update_menu(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::update_menu", "HANDLER");

    // 解析 UpdatePayload<MenuForUpdate>，并检测 data 里是否显式带了 parent_id 键（含 null）。
    // serde 的 Option<String> 无法区分"未传"与"传 null"，故这里用原始 JSON 判断，
    // 传 true 给 service 以支持"parent_id:null → 变根节点"。
    let payload: UpdatePayload<MenuForUpdate> = serde_json::from_value(raw.clone())
        .map_err(|e| Error::business_error(format!("请求体解析失败: {e}")))?;
    // 检测 data 对象是否显式带 parent_id 键（含 null）。
    // serde 的 Option<String> 无法区分"未传"与"传 null"，故用原始 JSON 判断，
    // 传 true 给 service 以支持"parent_id:null → 变根节点"。
    let parent_id_explicit = raw
        .get("data")
        .and_then(|d| d.as_object())
        .map(|o| o.contains_key("parent_id"))
        .unwrap_or(false);

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset =
        MenuService::update(mm, &db_id, payload.id, payload.data, parent_id_explicit).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除菜单
///
/// 级联删除传入节点的所有后代(基于 code_path 前缀),并重置父节点 leaf。
#[utoipa::path(
    post,
    path = "/api/menu/delete",
    request_body = DeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Menu"
)]
pub async fn delete_menu(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::delete_menu", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = MenuService::delete(mm, &db_id, payload.ids).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 列表查询(扁平结构,前端自行组装树)
#[utoipa::path(
    post,
    path = "/api/menu/list",
    request_body = ListParams<serde_json::Value>,
    responses((status = 200, description = "查询成功", body = ApiResp<serde_json::Value>)),
    tag = "Menu"
)]
pub async fn list_menus(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<ListParams<MenuFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::list_menus", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let dataset = MenuService::list(mm, &db_id, filters, Some(list_options)).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 分页查询
#[utoipa::path(
    post,
    path = "/api/menu/page",
    request_body = PageParams<serde_json::Value>,
    responses((status = 200, description = "查询成功", body = ApiResp<serde_json::Value>)),
    tag = "Menu"
)]
pub async fn page_menus(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<MenuFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::page_menus", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = MenuService::page(mm, &db_id, filters, list_options).await?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number,
        page_size,
        total as u64,
    )))
}

/// 菜单树查询参数(支持按域/应用/模块过滤)
#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub struct MenuTreeQuery {
    /// 所属域编码
    pub domain_code: Option<String>,
    /// 所属应用编码
    pub application_code: Option<String>,
    /// 所属模块编码
    pub module_code: Option<String>,
}

/// 获取菜单树(支持按域/应用/模块过滤)。
#[utoipa::path(
    get,
    path = "/api/menu/tree",
    params(MenuTreeQuery),
    responses((status = 200, description = "查询成功", body = ApiResp<Vec<TreeNode<MenuTreeNodeData>>>)),
    tag = "Menu"
)]
pub async fn get_menu_tree(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(query): Query<MenuTreeQuery>,
) -> Result<Json<ApiResp<Vec<TreeNode<MenuTreeNodeData>>>>> {
    debug!(
        "{:<12} - handler::get_menu_tree - domain: {:?}, app: {:?}, module: {:?}",
        "HANDLER", query.domain_code, query.application_code, query.module_code
    );

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let tree = MenuService::get_tree(
        mm,
        &db_id,
        query.domain_code.as_deref(),
        query.application_code.as_deref(),
        query.module_code.as_deref(),
    )
    .await?;

    Ok(Json(ApiResp::ok(tree)))
}
