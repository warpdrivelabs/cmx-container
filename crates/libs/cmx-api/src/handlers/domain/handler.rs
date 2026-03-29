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
use crate::handlers::domain::{DomainForCreate, DomainService};
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::rest::header_parse::get_db_id_from_header;

/// 按名称查询的请求参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct GetByNameParams {
    /// 域名
    pub name: String,

}

/// 按名称查询 Handler
///
/// 根据域名查询 Domain 实体
#[utoipa::path(
    post,
    path = "/api/domains/by-name",
    request_body = GetByNameParams,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Value>)
    ),
    tag = "Domain"
)]
pub async fn get_by_name(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<GetByNameParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::get_by_name", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let name = params.name.clone();
    let dataset = DomainService::get_by_name(&mm, &db_id, &name).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 批量创建的请求参数
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct BatchCreateParams {
    /// 要创建的数据列表
    pub items: Vec<DomainForCreate>,
}



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

/// 统计按状态 Handler
///
/// 按状态统计 Domain 数量
#[utoipa::path(
    get,
    path = "/api/domains/count-by-status",
    responses(
        (status = 200, description = "统计成功", body = ApiResp<Value>)
    ),
    tag = "Domain"
)]
pub async fn count_by_status(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(_params): Query<GetByNameParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::count_by_status", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = DomainService::count_by_status(&mm, &db_id).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
