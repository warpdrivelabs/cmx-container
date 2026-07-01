//! Menu 实体的自定义 Handler
//!
//! 标准 CRUD 由宏生成，此处放置树形查询等自定义 handler。

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_biz::menu::{MenuFilter, MenuService};
use cmx_core::model::data::dataset::DataSet;
use cmx_core::ListParams;
use cmx_database::get_default_db_manager;
use modql::filter::OpValsString;
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use crate::{ApiResp, Result};

/// 查询某模块下的菜单列表（前端组装为树形）
///
/// 通过 filters 传入 module_code（可选），返回扁平菜单列表，
/// 前端按 parent_id/full_path 组装树结构。
#[utoipa::path(
    post,
    path = "/api/menu/tree",
    request_body = ListParams<serde_json::Value>,
    responses((status = 200, description = "查询成功", body = ApiResp<serde_json::Value>)),
    tag = "Menu"
)]
pub async fn menu_tree(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<ListParams<MenuFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::menu_tree", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let dataset = MenuService::list(mm, &db_id, filters, Some(list_options)).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 兼容性示例：构造按 module_code 过滤的辅助函数（供内部调用）
#[allow(dead_code)]
fn filter_by_module(module_code: &str) -> MenuFilter {
    MenuFilter {
        module_code: Some(OpValsString::from(module_code)),
        ..Default::default()
    }
}
