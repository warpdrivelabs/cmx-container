//! Domain 实体的自定义 Handler
//!
//! 展示如何创建自定义的 HTTP Handler

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_database::get_default_db_manager;
use tracing::debug;

use cmx_biz::domain::{DomainService, DomainTreeNodeData};
use crate::Result;
use crate::middleware::CmxSvrContext;
use crate::ApiResp;
use crate::app_state::CmxAppState;
use crate::rest::header_parse::get_db_id_from_header;
use crate::TreeNode;






/// 查询域-应用-模块树形结构 Handler
///
/// 查询所有启用且未归档的域、应用、模块数据，
/// 按 域→应用→模块 三级层级组织，同级按 sort_order 排序。
#[utoipa::path(
    post,
    path = "/api/domains/tree",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<TreeNode<DomainTreeNodeData>>>)
    ),
    tag = "Domain"
)]
pub async fn get_tree(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Vec<TreeNode<DomainTreeNodeData>>>>> {
    debug!("{:<12} - handler::get_tree", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let tree = DomainService::get_tree(mm, &db_id).await?;

    Ok(Json(ApiResp::ok(tree)))
}

