//! SysDatasource 实体的自定义 Handler
//!
//! 提供数据源管理的 HTTP Handler

use std::sync::Arc;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::{DataSet, Schema};
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use crate::crud::UpdateItem;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::models::sys_datasource::{
    SysDatasourceForCreate, SysDatasourceForUpdate, SysDatasourceService,
};
use crate::response::ApiResp;
use crate::rest::header_parse::get_db_id_from_header;
use crate::state::CmxAppState;

/// 按 db_id 查询的请求参数
#[derive(Debug, Deserialize)]
pub struct GetByDbIdParams {
    /// 数据源标识
    pub db_id: String,
}

/// 更新请求 Payload
#[derive(Debug, Deserialize)]
pub struct DatasourceUpdatePayload {
    /// 数据源 ID
    pub id: String,
    /// 更新数据
    pub data: SysDatasourceForUpdate,
}

/// 删除请求 Payload
#[derive(Debug, Deserialize)]
pub struct DatasourceDeletePayload {
    /// 要删除的数据源 ID 列表
    pub ids: Vec<String>,
}

/// 按 db_id 查询数据源 Handler
///
/// # 接口
/// POST /api/sys-datasource/by-db-id
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
    let dataset = SysDatasourceService::get_by_db_id(&mm, &db_id, &target_db_id).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 创建数据源 Handler
///
/// # 接口
/// POST /api/sys-datasource/create-custom
pub async fn create_datasource(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<SysDatasourceForCreate>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::create_datasource", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = SysDatasourceService::create(&mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新数据源 Handler
///
/// # 接口
/// POST /api/sys-datasource/update-custom
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
    let dataset = SysDatasourceService::update(&mm, &db_id, &id, data).await?;



    Ok(Json(ApiResp::ok(dataset)))
}



/// 删除数据源 Handler
///
/// # 接口
/// POST /api/sys-datasource/delete-custom
pub async fn delete_datasource(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DatasourceDeletePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::delete_datasource", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = SysDatasourceService::delete(&mm, &db_id, payload.ids).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 测试数据源连接 Handler
///
/// # 接口
/// GET /api/sys-datasource/test-connection?db_id=tenant1
pub async fn test_connection(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Query(params): Query<GetByDbIdParams>,
) -> Result<Json<ApiResp<bool>>> {
    debug!("{:<12} - handler::test_connection", "HANDLER");

    let mm = get_default_db_manager();
    let result = SysDatasourceService::test_connection(&mm, &params.db_id).await?;

    Ok(Json(ApiResp::ok(result)))
}

/// 列出所有已注册数据源 Handler
///
/// # 接口
/// GET /api/sys-datasource/registered
pub async fn list_registered(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
) -> Result<Json<ApiResp<Vec<String>>>> {
    debug!("{:<12} - handler::list_registered", "HANDLER");

    let mm = get_default_db_manager();
    let list = SysDatasourceService::list_registered(&mm);

    Ok(Json(ApiResp::ok(list)))
}
