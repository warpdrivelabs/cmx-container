//! Domain 实体的自定义 Handler
//!
//! 展示如何创建自定义的 HTTP Handler

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use tracing::debug;

use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::models::domain::{DomainForCreate, DomainService};
use crate::response::ApiResp;
use crate::rest::header_parse::get_db_id_from_header;
use crate::state::CmxAppState;

/// 按名称查询的请求参数
#[derive(Debug, Deserialize)]
pub struct GetByNameParams {
    /// 域名
    pub name: String,
    /// 数据库 ID（可选）
    #[serde(default)]
    pub db_id: Option<String>,
}


/// 按名称查询 Handler
///
/// # 接口
/// POST /api/domains/by-name
///
/// # 请求体
/// ```json
/// {
///     "name": "example.com",
///     "db_id": "tenant1"  // 可选
/// }
/// ```
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 查询参数
pub async fn get_by_name(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
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
#[derive(Debug, Deserialize, Clone)]
pub struct BatchCreateParams {
    /// 要创建的数据列表
    pub items: Vec<DomainForCreate>,
}


/// 批量创建 Handler
///
/// # 接口
/// POST /api/domains/batch-create
///
/// # 请求体
/// ```json
/// {
///     "items": [
///         {"code": "domain1", "name": "Domain 1"},
///         {"code": "domain2", "name": "Domain 2"}
///     ]
/// }
/// ```
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 批量创建参数
pub async fn batch_create(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<BatchCreateParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::batch_create", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let items = params.items.clone();
    let results = DomainService::batch_create(&mm, &db_id, items).await?;

    Ok(Json(ApiResp::ok(results)))
}

/// 搜索的请求参数
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// 搜索关键字
    pub keyword: String,
    /// 页码（从 1 开始）
    pub page: Option<i64>,
    /// 每页数量
    pub page_size: Option<i64>,
    /// 数据库 ID（可选）
    #[serde(default)]
    pub db_id: Option<String>,
}

impl SearchParams {
    /// 获取数据库 ID
    pub async fn get_db_id(&self) -> String {
        self.db_id
            .clone()
            .unwrap_or(get_default_db_manager().get_default_db_id().await)
    }

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
/// # 接口
/// POST /api/domains/search
///
/// # 请求体
/// ```json
/// {
///     "keyword": "example",
///     "page": 1,
///     "page_size": 20,
///     "db_id": "tenant1"  // 可选
/// }
/// ```
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 搜索参数
pub async fn search(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<SearchParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::search", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
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
/// # 接口
/// GET /api/domains/count-by-status?db_id=tenant1
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 查询参数
pub async fn count_by_status(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<GetByNameParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::count_by_status", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = DomainService::count_by_status(&mm, &db_id).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
