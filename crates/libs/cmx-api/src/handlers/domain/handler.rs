//! Domain 实体的自定义 Handler
//!
//! 展示如何创建自定义的 HTTP Handler

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;
use utoipa::ToSchema;

use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::handlers::domain::{DomainService, DomainTreeNodeData};
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::rest::header_parse::get_db_id_from_header;
use crate::rest::TreeNode;





/// 搜索的请求参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchParams {
    /// 搜索关键字
    pub keyword: String,
    /// 页码（从 1 开始）
    pub page: Option<i64>,
    /// 每页数量
    pub page_size: Option<i64>,
}

impl SearchParams {
    /// 获取页码
    pub fn get_page(&self) -> i64 {
        self.page.unwrap_or(1)
    }

    /// 获取每页数量
    pub fn get_page_size(&self) -> i64 {
        self.page_size.unwrap_or(20)
    }
}

/// 搜索 Handler
///
/// 根据关键字搜索 Domain 实体
#[utoipa::path(
    post,
    path = "/api/domains/search",
    request_body = SearchParams,
    responses(
        (status = 200, description = "搜索成功", body = ApiResp<Value>)
    ),
    tag = "Domain"
)]
pub async fn search(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<SearchParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::search", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let keyword = params.keyword.clone();
    let page = params.get_page();
    let page_size = params.get_page_size();

    let (dataset, total) = DomainService::search(&mm, &db_id, &keyword, page, page_size).await?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page as u64,
        page_size as u64,
        total as u64,
    )))
}

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

    let tree = DomainService::get_tree(&mm, &db_id).await?;

    Ok(Json(ApiResp::ok(tree)))
}

