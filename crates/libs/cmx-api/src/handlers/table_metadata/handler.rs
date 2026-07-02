//! 表元数据查询 Handler
//!
//! 提供 cmx_meta_table_define 表的列表和分页查询功能

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::model::data::request::params::GetParams;
use cmx_database::get_default_db_manager;
use cmx_plugin::infrastructure::database::table_metadata::{
    TableMetadataFilter, TableMetadataService,
};
use modql::filter::OpValsString;
use serde::{Deserialize, Serialize};
use utoipa::IntoParams;

use crate::ApiResp;
use crate::Result;
use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;

/// 根据 table_name 查询表元数据的查询参数
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct TableMetadataGetByNameQuery {
    /// 表名称
    pub table_name: String,
}

/// 获取表元数据详情
///
/// 通过 ID 查询 cmx_meta_table_define 表的详情记录
#[utoipa::path(
    get,
    path = "/api/table-metadata/get",
    params(
        ("id" = String, Query, description = "表定义ID")
    ),
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_get_by_id(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<GetParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = TableMetadataService::get_detail_by_id(mm, &db_id, &params.id)
        .await
        .map_err(|e| crate::Error::business_error(format!("查询详情失败: {}", e)))?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 表元数据列表查询
///
/// 查询 cmx_meta_table_define 表的所有记录，支持按 table_name、plugin_id、db_id 等条件过滤
#[utoipa::path(
    post,
    path = "/api/table-metadata/list",
    request_body = cmx_core::ListParams<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_list(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<cmx_core::ListParams<TableMetadataFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let list_options = params.to_list_options();
    let mut filters = params.filters.clone().filter(|v| !v.is_empty());
    let app_id = cmx_state.app_id();
    // 如果 filters 是 Some，就自动解包并进入循环；如果是 None，需要手动构建一个只包含 app_id 的 filter
    if let Some(filters_vec) = &mut filters {
        for filter in filters_vec.iter_mut() {
            filter
                .app_id
                .get_or_insert(OpValsString::from(app_id.clone()));
        }
    } else {
        // filters 为 None 时，手动构建一个只包含 app_id 条件的 filter
        let default_filter = TableMetadataFilter {
            app_id: Some(OpValsString::from(app_id)),
            ..Default::default()
        };
        filters = Some(vec![default_filter]);
    }

    let dataset = TableMetadataService::list(mm, &db_id, filters, Some(list_options))
        .await
        .map_err(|e| crate::Error::InternalError(format!("列表查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 表元数据分页查询
///
/// 分页查询 cmx_meta_table_define 表的记录，支持按 table_name、plugin_id、db_id 等条件过滤
#[utoipa::path(
    post,
    path = "/api/table-metadata/page",
    request_body = cmx_core::PageParams<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_page(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<cmx_core::PageParams<TableMetadataFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;

    let list_options = params.to_list_options();
    let mut filters = params.filters.clone().filter(|v| !v.is_empty());

    let app_id = cmx_state.app_id();
    // 如果 filters 是 Some，就自动解包并进入循环；如果是 None，需要手动构建一个只包含 app_id 的 filter
    if let Some(filters_vec) = &mut filters {
        for filter in filters_vec.iter_mut() {
            filter
                .app_id
                .get_or_insert(OpValsString::from(app_id.clone()));
        }
    } else {
        // filters 为 None 时，手动构建一个只包含 app_id 条件的 filter
        let default_filter = TableMetadataFilter {
            app_id: Some(OpValsString::from(app_id)),
            ..Default::default()
        };
        filters = Some(vec![default_filter]);
    }

    let (dataset, total) = TableMetadataService::page(mm, &db_id, filters, list_options)
        .await
        .map_err(|e| crate::Error::InternalError(format!("分页查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number,
        page_size,
        total as u64,
    )))
}

/// 根据表名获取表元数据
///
/// 通过 table_name 查询 cmx_meta_table_define 表的详情记录
#[utoipa::path(
    get,
    path = "/api/table-metadata/get-by-name",
    params(TableMetadataGetByNameQuery),
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_get_by_name(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<TableMetadataGetByNameQuery>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let app_id = cmx_state.app_id();

    let dataset =
        TableMetadataService::get_by_table_name(mm, &db_id, &params.table_name, None, &app_id)
            .await
            .map_err(|e| crate::Error::InternalError(format!("根据表名查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 根据 table_name 查询表元数据的查询参数
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct TableMetadataExistsQuery {
    /// 表名称
    pub table_name: String,
}

/// 查询表元数据是否存在
///
/// 处理 GET /api/table-metadata/exists 请求，通过 table_name 查询表是否已存在。
///
/// # 参数
/// - `query`: 查询参数（TableMetadataExistsQuery）
///
/// # 查询参数
/// - `table_name`: 表名称
///
/// # 响应体
/// - code: 0
/// - data: "1" 存在, "0" 不存在
#[utoipa::path(
    get,
    path = "/api/table-metadata/exists",
    params(TableMetadataExistsQuery),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<String>)
    ),
    tag = "TableMetadata"
)]
pub async fn table_metadata_exists(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<TableMetadataExistsQuery>,
) -> Result<Json<ApiResp<String>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let app_id = _cmx_state.app_id();

    let dataset =
        TableMetadataService::get_by_table_name(mm, &db_id, &params.table_name, None, &app_id)
            .await
            .map_err(|e| crate::Error::InternalError(format!("查询表存在性失败: {}", e)))?;

    let exists = !dataset.rows.is_empty();
    Ok(Json(ApiResp::ok(
        if exists { "1" } else { "0" }.to_string(),
    )))
}
