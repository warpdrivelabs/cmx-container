//! SysDatasource 实体的自定义 Handler
//!
//! 提供数据源管理的 HTTP Handler

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;
use utoipa::ToSchema;

use crate::Result;
use crate::middleware::CmxSvrContext;
use crate::handlers::sys_datasource::{
    SysDatasourceForCreate, SysDatasourceForUpdate, SysDatasourceService,
};
use crate::ApiResp;
use crate::rest::header_parse::get_db_id_from_header;
use crate::app_state::CmxAppState;

/// 按 db_id 查询的请求参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct GetByDbIdParams {
    /// 数据源标识
    pub db_id: String,
}

/// 更新请求 Payload
#[derive(Debug, Deserialize, ToSchema)]
pub struct DatasourceUpdatePayload {
    /// 数据源 ID
    pub id: String,
    /// 更新数据
    pub data: SysDatasourceForUpdate,
}

/// 删除请求 Payload
#[derive(Debug, Deserialize, ToSchema)]
pub struct DatasourceDeletePayload {
    /// 要删除的数据源 ID 列表
    pub ids: Vec<String>,
}

/// 按 db_id 查询数据源 Handler
///
/// 根据数据源标识查询数据源配置
#[utoipa::path(
    post,
    path = "/api/sys-datasource/by-db-id",
    request_body = GetByDbIdParams,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Value>)
    ),
    tag = "SysDatasource"
)]
pub async fn get_by_db_id(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<GetByDbIdParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::get_by_db_id", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let target_db_id = params.db_id.clone();
    let dataset = SysDatasourceService::get_by_db_id(mm, &db_id, &target_db_id).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 创建数据源 Handler
///
/// 创建新的数据源配置并自动注册到 DatabaseManager
#[utoipa::path(
    post,
    path = "/api/sys-datasource/create-custom",
    request_body = SysDatasourceForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<Value>)
    ),
    tag = "SysDatasource"
)]
pub async fn create_datasource(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<SysDatasourceForCreate>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::create_datasource", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = SysDatasourceService::create(mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新数据源 Handler
///
/// 更新数据源配置，根据 status 自动管理内存中的数据源注册:
/// - status=0（禁用）: 注销数据源
/// - status=1（启用）: 先注销再重新注册
#[utoipa::path(
    post,
    path = "/api/sys-datasource/update-custom",
    request_body = DatasourceUpdatePayload,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<Value>)
    ),
    tag = "SysDatasource"
)]
pub async fn update_datasource(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DatasourceUpdatePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::update_datasource", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let id = payload.id.clone();
    let data = payload.data;
    let dataset = SysDatasourceService::update(mm, &db_id, &id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除数据源 Handler
///
/// 删除数据源配置并自动从 DatabaseManager 注销
#[utoipa::path(
    post,
    path = "/api/sys-datasource/delete-custom",
    request_body = DatasourceDeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<Value>)
    ),
    tag = "SysDatasource"
)]
pub async fn delete_datasource(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DatasourceDeletePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::delete_datasource", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = SysDatasourceService::delete(mm, &db_id, payload.ids).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 测试数据源连接 Handler
///
/// 测试指定数据源的连接是否正常
#[utoipa::path(
    get,
    path = "/api/sys-datasource/test-connection",
    params(
        ("db_id" = String, Query, description = "数据源标识")
    ),
    responses(
        (status = 200, description = "测试完成", body = ApiResp<bool>)
    ),
    tag = "SysDatasource"
)]
pub async fn test_connection(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Query(params): Query<GetByDbIdParams>,
) -> Result<Json<ApiResp<bool>>> {
    debug!("{:<12} - handler::test_connection", "HANDLER");

    let mm = get_default_db_manager();
    let result = SysDatasourceService::test_connection(mm, &params.db_id).await?;

    Ok(Json(ApiResp::ok(result)))
}
