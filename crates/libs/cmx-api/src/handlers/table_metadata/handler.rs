//! 表元数据查询 Handler
//!
//! 提供 cmx_meta_table_define 表的列表和分页查询功能

use axum::http::HeaderMap;
use axum::extract::State;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use cmx_plugin::infrastructure::database::table_metadata::{
    TableMetadataFilter, TableMetadataService,
};

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;

/// 表元数据列表查询
///
/// 查询 cmx_meta_table_define 表的所有记录，支持按 table_name、plugin_id、db_id 等条件过滤
#[utoipa::path(
    post,
    path = "/api/table-metadata/list",
    request_body = crate::rest::param_doc::ListParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<cmx_core::ListParams<TableMetadataFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let list_options = params.to_list_options();
    let filter = params.filter.clone();

    let dataset =
        TableMetadataService::list(mm, &db_id, filter, Some(list_options))
            .await
            .map_err(|e| crate::error::Error::InternalError(format!("列表查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 表元数据分页查询
///
/// 分页查询 cmx_meta_table_define 表的记录，支持按 table_name、plugin_id、db_id 等条件过滤
#[utoipa::path(
    post,
    path = "/api/table-metadata/page",
    request_body = crate::rest::param_doc::PageParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<cmx_core::PageParams<TableMetadataFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;

    let list_options = params.to_list_options();
    let filter = params.filter.clone();

    let (dataset, total) =
        TableMetadataService::page(mm, &db_id, filter, list_options)
            .await
            .map_err(|e| crate::error::Error::InternalError(format!("分页查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number,
        page_size,
        total as u64,
    )))
}
