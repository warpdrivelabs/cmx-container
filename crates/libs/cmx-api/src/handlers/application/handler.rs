//! Application 实体的自定义 Handler
//!
//! 写操作（create/update/delete）手写委托 ApplicationService 以触发 DAM 资产文件副作用
//! （应用改名时级联搬移目录 + 重写 module 列；删除时引用完整性校验）。
//! 另提供自定义分页查询（联表带 domain_name）。

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use cmx_core::PageParams;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::{DeletePayload, UpdatePayload};
use cmx_database::crud::CustomQueryService;
use cmx_database::get_default_db_manager;
use tracing::debug;

use crate::ApiResp;
use crate::Result;
use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use cmx_biz::application::{ApplicationFilter, ApplicationForCreate, ApplicationForUpdate, ApplicationService};

/// Application 自定义分页查询 Handler
///
/// 执行自定义 SQL 查询，关联 cmx_application 和 cmx_domain 表，
/// 返回应用信息及所属域名称，支持动态过滤条件和分页参数。
///
/// # SQL 说明
/// - 主表: cmx_application (别名 a)
/// - 关联表: cmx_domain (别名 d)
/// - 关联条件: a.domain_code = d.code
/// - 返回字段: a.* (应用全部字段) + d.name as domain_name (域名称)
///
/// # 功能特性
/// - 支持动态过滤条件（通过 ApplicationFilter）
/// - 支持排序和分页
/// - 自动生成 COUNT 查询
#[utoipa::path(
    post,
    path = "/api/applications/custom-page",
    request_body = cmx_core::PageParams<serde_json::Value>,
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "Application"
)]
pub async fn application_custom_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<ApplicationFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::application_custom_page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let sql = r#"
        SELECT
            a.*,
            d.name as domain_name
        FROM cmx_application a
        LEFT JOIN cmx_domain d ON a.domain_code = d.code
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
        "cmx-application",
    )
    .await
    .map_err(|e| crate::Error::InternalError(format!("自定义分页查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number,
        page_size,
        total as u64,
    )))
}

/// 创建应用 Handler
///
/// 委托 ApplicationService::create。写库后确保应用级资源目录存在。
#[utoipa::path(
    post,
    path = "/api/applications/create",
    request_body = ApplicationForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Application"
)]
pub async fn create_application(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<ApplicationForCreate>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::create_application", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = ApplicationService::create(mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新应用 Handler
///
/// 委托 ApplicationService::update。若 code 变更，触发 DAM 资产目录搬移 + module 列重写。
#[utoipa::path(
    post,
    path = "/api/applications/update",
    request_body = UpdatePayload<ApplicationForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Application"
)]
pub async fn update_application(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<UpdatePayload<ApplicationForUpdate>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::update_application", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = ApplicationService::update(mm, &db_id, payload.id, payload.data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除应用 Handler
///
/// 委托 ApplicationService::delete。删前校验应用下无 module。
#[utoipa::path(
    post,
    path = "/api/applications/delete",
    request_body = DeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Application"
)]
pub async fn delete_application(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::delete_application", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = ApplicationService::delete(mm, &db_id, payload.ids).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
