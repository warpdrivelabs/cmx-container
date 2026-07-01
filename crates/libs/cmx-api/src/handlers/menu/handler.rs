//! Menu 实体的自定义 Handler
//!
//! 标准 CRUD(含 list/page)由宏生成，此处仅放树形查询。

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_api_types::TreeNode;
use cmx_biz::menu::{MenuService, MenuTreeNodeData};
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use crate::{ApiResp, Result};

// /// 菜单列表查询(扁平结构,前端自行组装)
// /// 已注释:通用的 list 接口已由 register_crud_handlers_module!(menu_crud) 宏生成
// #[utoipa::path(
//     post,
//     path = "/api/menu/list",
//     request_body = ListParams<serde_json::Value>,
//     responses((status = 200, description = "查询成功", body = ApiResp<serde_json::Value>)),
//     tag = "Menu"
// )]
// pub async fn menu_list(
//     State(_cmx_state): State<CmxAppState>,
//     CmxSvrContext(_svr_ctx): CmxSvrContext,
//     headers: HeaderMap,
//     Json(params): Json<ListParams<MenuFilter>>,
// ) -> Result<Json<ApiResp<DataSet>>> {
//     debug!("{:<12} - handler::menu_list", "HANDLER");
//
//     let mm = get_default_db_manager();
//     let db_id = get_db_id_from_header(&headers).await;
//
//     let list_options = params.to_list_options();
//     let filters = params.filters.clone().filter(|v| !v.is_empty());
//
//     let dataset = MenuService::list(mm, &db_id, filters, Some(list_options)).await?;
//
//     Ok(Json(ApiResp::ok(dataset)))
// }

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

/// 获取菜单树(支持按域/应用/模块过滤)
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
