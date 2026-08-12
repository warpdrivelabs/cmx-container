//! Domain 实体的自定义 Handler
//!
//! 写操作（create/update/delete）手写，委托 DomainService 以触发 DAM 资产文件副作用
//! （域改名时级联搬移目录 + 重写 module/application 列；删除时引用完整性校验）。
//! 读操作（get/list/page/tree）复用 rest::handler 泛型函数 + 自定义 get_tree。

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::{DeletePayload, UpdatePayload};
use cmx_database::get_default_db_manager;
use tracing::debug;

use cmx_api_core::ApiResp;
use cmx_api_core::Result;
use cmx_api_core::TreeNode;
use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::rest::header_parse::get_db_id_from_header;
use cmx_biz::domain::{DomainForCreate, DomainForUpdate, DomainService, DomainTreeNodeData};

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

    let tree = DomainService::get_tree(mm, &db_id).await?;

    Ok(Json(ApiResp::ok(tree)))
}

/// 创建域 Handler
///
/// 委托 DomainService::create。域无文件副作用（域级目录在创建应用/模块时才创建）。
#[utoipa::path(
    post,
    path = "/api/domains/create",
    request_body = DomainForCreate,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Domain"
)]
pub async fn create_domain(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<DomainForCreate>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::create_domain", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = DomainService::create(mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新域 Handler
///
/// 委托 DomainService::update。若 code 变更，触发 DAM 资产目录搬移 + 列重写。
#[utoipa::path(
    post,
    path = "/api/domains/update",
    request_body = UpdatePayload<DomainForUpdate>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Domain"
)]
pub async fn update_domain(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<UpdatePayload<DomainForUpdate>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::update_domain", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = DomainService::update(mm, &db_id, payload.id, payload.data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除域 Handler
///
/// 委托 DomainService::delete。删前校验域下无 application/module。
#[utoipa::path(
    post,
    path = "/api/domains/delete",
    request_body = DeletePayload,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Domain"
)]
pub async fn delete_domain(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::delete_domain", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = DomainService::delete(mm, &db_id, payload.ids).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
