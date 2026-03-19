//! REST Handler
//!
//! 提供通用 CRUD 的 REST Handler 函数。
//!
//! 注意：DatabaseManager 通过 get_default_db_manager() 全局获取，不需要通过 state 传递

use axum::extract::{FromRequestParts, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use modql::field::HasSeaFields;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::debug;

use crate::crud::service::{GenericCrudService, UpdateItem};
use crate::crud::traits::DbBmc;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::response::ApiResp;
use crate::rest::header_parse::get_db_id_from_header;
use crate::rest::params::{DeletePayload, GetParams, ListParams, PageParams, UpdatePayload};
use crate::state::CmxAppState;

/// 创建单个实体 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `data` - 要创建的实体数据（从 JSON body 提取）
///
/// # 返回值
/// 返回包含创建结果的 ApiResp
pub async fn create<MC, E>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<E>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    debug!("{:<12} - handler::create", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = GenericCrudService::<MC>::create(&mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 批量创建实体 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `data` - 要创建的实体数据向量（从 JSON body 提取）
///
/// # 返回值
/// 返回包含创建结果的 ApiResp
pub async fn create_many<MC, E>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<Vec<E>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    debug!("{:<12} - handler::create_many", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = GenericCrudService::<MC>::create_many(&mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 根据主键获取单条实体的 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 查询参数（从 URL 查询参数提取，?id=xxx&db_id=xxx）
///
/// # 返回值
/// 返回包含查询结果的 ApiResp
pub async fn get_by_id<MC>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<GetParams>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    debug!("{:<12} - handler::get_by_id", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let id = params.id.clone();
    let dataset = GenericCrudService::<MC>::get(&mm, &db_id, id.into()).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新单个实体 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `payload` - 更新请求数据（从 JSON body 提取）
///
/// # 返回值
/// 返回包含更新后结果的 ApiResp
pub async fn update<MC, E>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<UpdatePayload<E>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    debug!("{:<12} - handler::update", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = GenericCrudService::<MC>::update(&mm, &db_id, payload.id, payload.data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 批量更新实体 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `data` - 更新数据向量（从 JSON body 提取）
///
/// # 返回值
/// 返回包含更新结果的 ApiResp
pub async fn update_many<MC, E>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<Vec<UpdateItem<E>>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    debug!("{:<12} - handler::update_many", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = GenericCrudService::<MC>::update_many(&mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除实体 Handler（支持单个和批量）
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `payload` - 删除请求数据（从 JSON body 提取）
///
/// # 返回值
/// 返回包含删除信息的 ApiResp
pub async fn delete<MC>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    debug!("{:<12} - handler::delete", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = GenericCrudService::<MC>::delete(&mm, &db_id, payload.ids).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 列表查询的 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 查询参数（从 JSON body 提取）
///
/// # 返回值
/// 返回包含查询结果的 ApiResp
pub async fn list<MC, F>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<ListParams<F>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    F: DeserializeOwned + Into<modql::filter::FilterGroups> + Clone,
{
    debug!("{:<12} - handler::list", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let list_options = params.to_list_options();
    let filter = params.filter.clone();
    let dataset = GenericCrudService::<MC, F>::list(&mm, &db_id, filter, Some(list_options)).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 分页查询的 Handler
///
/// # 参数
/// * `cmx_state` - 应用状态
/// * `svr_ctx` - 服务上下文
/// * `headers` - HTTP 请求头
/// * `params` - 查询参数（从 JSON body 提取）
///
/// # 返回值
/// 返回包含查询结果和分页信息的 ApiResp
pub async fn page<MC, F>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<F>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    F: DeserializeOwned + Into<modql::filter::FilterGroups> + Clone,
{
    debug!("{:<12} - handler::page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;

    let list_options = params.to_list_options();
    let filter = params.filter.clone();
    let (dataset, total) = GenericCrudService::<MC, F>::page(&mm, &db_id, filter, list_options).await?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number,
        page_size,
        total as u64,
    )))
}

