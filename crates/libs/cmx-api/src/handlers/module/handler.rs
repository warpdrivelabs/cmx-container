//! Module 实体的自定义 Handler
//!
//! 提供模块实体的自定义分页查询功能

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::PageParams;
use cmx_database::crud::CustomQueryService;
use cmx_database::get_default_db_manager;
use tracing::debug;

use cmx_biz::module::ModuleFilter;
use crate::ApiResp;
use crate::app_state::CmxAppState;
use crate::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;

/// Module 自定义分页查询 Handler
///
/// 执行自定义 SQL 查询，关联 cmx_module、cmx_application 和 cmx_domain 表，
/// 返回模块信息及所属应用名称和域名称，支持动态过滤条件和分页参数。
///
/// # SQL 说明
/// - 主表: cmx_module (别名 m)
/// - 关联表1: cmx_application (别名 a)
/// - 关联表2: cmx_domain (别名 d)
/// - 关联条件:
///   - m.application_code = a.code
///   - m.domain_code = d.code
/// - 返回字段:
///   - m.* (模块全部字段)
///   - a.name as application_name (应用名称)
///   - d.name as domain_name (域名称)
///
/// # 功能特性
/// - 支持动态过滤条件（通过 ModuleFilter）
/// - 支持排序和分页
/// - 自动生成 COUNT 查询
#[utoipa::path(
    post,
    path = "/api/module/custom-page",
    request_body = crate::PageParamsDoc<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "Module"
)]
pub async fn module_custom_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<ModuleFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::module_custom_page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let sql = r#"
        SELECT
            m.*,
            a.name as application_name,
            d.name as domain_name
        FROM cmx_module m
        LEFT JOIN cmx_application a ON m.application_code = a.code
        LEFT JOIN cmx_domain d ON m.domain_code = d.code
    "#;

    let list_options = params.to_list_options();
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = CustomQueryService::page_custom(
        mm,
        &db_id,
        None,
        filters,
        list_options,
        sql,
        "cmx_module",
    )
    .await
        .map_err(|e| crate::Error::InternalError(format!("自定义分页查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number ,
        page_size ,
        total as u64,
    )))}
