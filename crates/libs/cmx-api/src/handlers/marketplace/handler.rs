//! 插件市场 HTTP Handler
//!
//! 定义插件市场所有 REST API 端点，遵循 axum-handler-generator 规范：
//! - 使用独立路径区分不同操作（如 `/plugin/get`、`/plugin/publish`）
//! - 使用结构体参数传递请求数据
//! - 使用 modql FilterNodes 进行查询过滤
//!
//! API 路由前缀：`/api/marketplace`

use axum::extract::{Query, State};
use axum::Json;
use cmx_database::get_default_db_manager;
use std::sync::Arc;
use tracing::{debug, info};

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::{Error, Result};
use crate::middleware::CmxSvrContext;
use crate::rest::PageParamsDoc;
use super::request::*;
use super::response::*;

use cmx_plugin::marketplace::model::{
    MarketplacePluginFilter, MarketplacePluginForCreate, MarketplacePluginForUpdate,
    MarketplacePluginVersionForCreate, MarketplaceRatingFilter, MarketplaceRatingForCreate,
};

/// 获取插件市场服务实例
///
/// 从数据库管理器初始化仓库、统计服务和业务服务
///
/// # Returns
///
/// MarketplaceService 实例
async fn get_marketplace_service() -> cmx_plugin::MarketplaceService {
    let db_manager = get_default_db_manager();
    let default_db_id = db_manager.get_default_db_id().await;

    let repo = Arc::new(cmx_plugin::MarketplaceRepository::new(
        db_manager.clone(),
        default_db_id.clone(),
    ));
    let stats_service = Arc::new(cmx_plugin::StatsService::new(repo.clone()));
    cmx_plugin::MarketplaceService::new(repo, stats_service, db_manager.clone(), default_db_id)
}

/// 将插件实体转换为 API 响应结构
///
/// # Arguments
///
/// * `plugin` - 插件业务实体
///
/// # Returns
///
/// API 响应结构。
fn convert_plugin_to_response(plugin: cmx_plugin::MarketplacePlugin) -> MarketplacePluginResponse {
    MarketplacePluginResponse {
        id: plugin.id,
        plugin_id: plugin.plugin_id,
        name: plugin.name,
        description: plugin.description,
        short_description: plugin.short_description,
        icon_url: plugin.icon_url,
        category: plugin.category,
        tags: plugin.tags,
        vendor_name: plugin.vendor_name,
        vendor_url: plugin.vendor_url,
        vendor_contact: plugin.vendor_contact,
        license_type: plugin.license_type,
        homepage_url: plugin.homepage_url,
        documentation_url: plugin.documentation_url,
        repository_url: plugin.repository_url,
        status: plugin.status,
        is_featured: plugin.is_featured,
        is_official: plugin.is_official,
        avg_rating: plugin.avg_rating,
        rating_count: plugin.rating_count,
        download_count: plugin.download_count,
        install_count: plugin.install_count,
        domain_code: plugin.domain_code,
        application_code: plugin.application_code,
        module_code: plugin.module_code,
        plugin_type: plugin.plugin_type,
        create_time: plugin.create_time,
        update_time: plugin.update_time,
        create_by: plugin.create_by,
        create_name: plugin.create_name,
        update_by: plugin.update_by,
        update_name: plugin.update_name,
    }
}

/// 将版本实体转换为 API 响应结构
///
/// # Arguments
///
/// * `version` - 版本业务实体
///
/// # Returns
///
/// API 响应结构。
fn convert_version_to_response(version: cmx_plugin::MarketplacePluginVersion) -> MarketplaceVersionResponse {
    MarketplaceVersionResponse {
        id: version.id,
        plugin_id: version.plugin_id,
        version: version.version,
        version_rank: version.version_rank,
        changelog: version.changelog,
        release_notes: version.release_notes,
        download_url: version.download_url,
        package_size: version.package_size,
        checksum: version.checksum,
        min_platform_version: version.min_platform_version,
        max_platform_version: version.max_platform_version,
        dependencies: version.dependencies,
        compatibility: version.compatibility,
        status: version.status,
        is_latest: version.is_latest,
        is_stable: version.is_stable,
        download_count: version.download_count,
        published_at: version.published_at,
        create_time: version.create_time,
        update_time: version.update_time,
    }
}

/// 分页查询 Handler
///
/// 支持多条件过滤（名称模糊匹配、分类/状态/域/应用/模块精确匹配），
/// 默认只返回已发布且未归档的插件
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `params` - 分页查询参数，包含过滤条件和分页信息
///
/// # Returns
///
/// 插件分页列表。
///
/// # Errors
///
/// * `Error::internal_error` - 查询失败时返回。
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/page",
    request_body = PageParamsDoc<MarketplacePluginFilterDoc>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplacePluginResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<MarketplacePluginFilterDoc>>,
) -> Result<Json<ApiResp<Vec<MarketplacePluginResponse>>>> {
    debug!("插件市场分页查询");

    let page = params.get_page() as u64;
    let page_size = params.get_size() as u64;

    let filters: Option<Vec<MarketplacePluginFilter>> = if let Some(filter) = params.filter.clone() {
        Some(vec![filter.into()])
    } else if let Some(fs) = params.filters.clone() {
        if !fs.is_empty() {
            Some(fs.into_iter().map(Into::into).collect())
        } else {
            None
        }
    } else {
        None
    };

    let list_options = params.to_list_options();

    let service = get_marketplace_service().await;
    let (plugins, total) = service
        .page_plugins(filters, list_options)
        .await
        .map_err(|e| Error::internal_error(format!("查询市场插件失败: {}", e)))?;

    let responses: Vec<MarketplacePluginResponse> = plugins
        .into_iter()
        .map(convert_plugin_to_response)
        .collect();

    Ok(Json(ApiResp::ok_with_pagination(responses, page, page_size, total)))
}

/// 详情查询 Handler
///
/// 支持通过 id（主键）或 plugin_id（业务 ID）查询
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `params` - 查询参数，支持 id 或 plugin_id 二选一
///
/// # Returns
///
/// 插件详情，包含基本信息、版本列表和最新版本
///
/// # Errors
///
/// * `Error::bad_request` - 未提供查询参数时返回
/// * `Error::not_found` - 插件不存在时返回
/// * `Error::internal_error` - 查询失败时返回
#[utoipa::path(
    get,
    path = "/api/marketplace/plugin/get",
    params(MarketplacePluginGetParams),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<MarketplacePluginDetailResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_get_by_id(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<MarketplacePluginGetParams>,
) -> Result<Json<ApiResp<MarketplacePluginDetailResponse>>> {
    debug!("查询市场插件详情: {:?}", params);

    let service = get_marketplace_service().await;

    let plugin = if let Some(plugin_id) = &params.plugin_id {
        service.get_plugin_by_plugin_id(plugin_id).await
    } else if let Some(id) = &params.id {
        service.get_plugin_by_id(id).await
    } else {
        return Err(Error::bad_request("请提供 id 或 plugin_id 参数"));
    }
    .map_err(|e| Error::internal_error(format!("查询市场插件详情失败: {}", e)))?;

    let plugin = plugin.ok_or_else(|| Error::not_found("插件不存在"))?;

    let versions = service
        .get_plugin_versions(&plugin.plugin_id)
        .await
        .map_err(|e| Error::internal_error(format!("查询版本列表失败: {}", e)))?;

    let latest_version = service
        .get_latest_stable_version(&plugin.plugin_id)
        .await
        .map_err(|e| Error::internal_error(format!("查询最新版本失败: {}", e)))?;

    let detail = MarketplacePluginDetailResponse {
        plugin: convert_plugin_to_response(plugin),
        latest_version: latest_version.map(convert_version_to_response),
        versions: versions.into_iter().map(convert_version_to_response).collect(),
    };

    Ok(Json(ApiResp::ok(detail)))
}

/// 发布 Handler
///
/// 如果插件已存在（根据 plugin_id 判断），则更新插件信息并创建新版本；
/// 如果插件不存在，则创建新插件记录
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `req` - 发布请求，包含插件基本信息和版本信息
///
/// # Returns
///
/// 发布成功后的插件完整信息
///
/// # Errors
///
/// * `Error::internal_error` - 发布失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/publish",
    request_body = PublishPluginRequest,
    responses(
        (status = 200, description = "发布成功", body = ApiResp<MarketplacePluginResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_publish(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<PublishPluginRequest>,
) -> Result<Json<ApiResp<MarketplacePluginResponse>>> {
    info!("发布插件到市场: plugin_id={}, version={}", req.plugin_id, req.version);

    let plugin_req = MarketplacePluginForCreate {
        plugin_id: req.plugin_id.clone(),
        name: req.name,
        description: req.description,
        short_description: req.short_description,
        icon_url: req.icon_url,
        category: req.category,
        tags: req.tags,
        vendor_name: req.vendor_name,
        vendor_url: req.vendor_url,
        vendor_contact: req.vendor_contact,
        license_type: req.license_type,
        homepage_url: req.homepage_url,
        documentation_url: req.documentation_url,
        repository_url: req.repository_url,
        status: Some("published".to_string()),
        is_featured: None,
        is_official: None,
        domain_code: req.domain_code,
        application_code: req.application_code,
        module_code: req.module_code,
        plugin_type: req.plugin_type,
    };

    let version_req = MarketplacePluginVersionForCreate {
        plugin_id: req.plugin_id,
        version: req.version,
        version_rank: Some(0),
        changelog: req.changelog,
        release_notes: req.release_notes,
        download_url: req.download_url,
        package_size: req.package_size,
        checksum: req.checksum,
        min_platform_version: req.min_platform_version,
        max_platform_version: req.max_platform_version,
        dependencies: None,
        compatibility: None,
        status: Some("published".to_string()),
        is_latest: Some(1),
        is_stable: Some(1),
    };

    let service = get_marketplace_service().await;
    let plugin = service
        .publish_plugin(plugin_req, version_req)
        .await
        .map_err(|e| Error::internal_error(format!("发布插件失败: {}", e)))?;

    Ok(Json(ApiResp::ok(convert_plugin_to_response(plugin))))
}

/// 更新 Handler
///
/// 仅更新提供的非 None 字段，其他字段保持原值
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `req` - 更新请求，包含 plugin_id 和要更新的字段
///
/// # Returns
///
/// 更新成功返回 UnitResp
///
/// # Errors
///
/// * `Error::internal_error` - 更新失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/update",
    request_body = UpdateMarketplacePluginRequest,
    responses(
        (status = 200, description = "更新成功", body = crate::api_response::UnitResp)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_update(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<UpdateMarketplacePluginRequest>,
) -> Result<Json<crate::api_response::UnitResp>> {
    debug!("更新市场插件信息: plugin_id={}", req.plugin_id);

    let update_data = MarketplacePluginForUpdate {
        name: req.name,
        description: req.description,
        short_description: req.short_description,
        icon_url: req.icon_url,
        category: req.category,
        tags: req.tags,
        vendor_name: req.vendor_name,
        vendor_url: req.vendor_url,
        vendor_contact: req.vendor_contact,
        license_type: req.license_type,
        homepage_url: req.homepage_url,
        documentation_url: req.documentation_url,
        repository_url: req.repository_url,
        status: req.status,
        is_featured: req.is_featured,
        is_official: req.is_official,
        domain_code: req.domain_code,
        application_code: req.application_code,
        module_code: req.module_code,
        plugin_type: req.plugin_type,
    };

    let service = get_marketplace_service().await;
    service
        .update_plugin(&req.plugin_id, update_data)
        .await
        .map_err(|e| Error::internal_error(format!("更新市场插件失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("更新成功")))
}

/// 删除 Handler
///
/// 执行逻辑删除，将插件的 archived 字段设为 1
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `req` - 删除请求，包含要删除的 plugin_id
///
/// # Returns
///
/// 删除成功返回 UnitResp。
///
/// # Errors
///
/// * `Error::internal_error` - 删除失败时返回。
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/delete",
    request_body = DeleteMarketplacePluginRequest,
    responses(
        (status = 200, description = "删除成功", body = crate::api_response::UnitResp)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_delete(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<DeleteMarketplacePluginRequest>,
) -> Result<Json<crate::api_response::UnitResp>> {
    info!("删除市场插件: plugin_id={}", req.plugin_id);

    let service = get_marketplace_service().await;
    service
        .delete_plugin(&req.plugin_id)
        .await
        .map_err(|e| Error::internal_error(format!("删除市场插件失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("删除成功")))
}

/// 版本列表查询 Handler
///
/// 根据 plugin_id 查询插件的所有版本，按 version_rank 降序排列。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `filter` - 查询过滤条件，必须提供 plugin_id
///
/// # Returns
///
/// 插件的所有未归档版本列表。
///
/// # Errors
///
/// * `Error::bad_request` - 未提供 plugin_id 时返回
/// * `Error::internal_error` - 查询失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/version/list",
    request_body = MarketplacePluginVersionFilterDoc,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplaceVersionResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_version_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(filter): Json<MarketplacePluginVersionFilterDoc>,
) -> Result<Json<ApiResp<Vec<MarketplaceVersionResponse>>>> {
    debug!("查询版本列表: {:?}", filter);

    let plugin_id = filter
        .plugin_id
        .clone()
        .ok_or_else(|| Error::bad_request("请提供 plugin_id 参数"))?;

    let service = get_marketplace_service().await;
    let versions = service
        .get_plugin_versions(&plugin_id)
        .await
        .map_err(|e| Error::internal_error(format!("查询版本列表失败: {}", e)))?;

    let responses: Vec<MarketplaceVersionResponse> =
        versions.into_iter().map(convert_version_to_response).collect();

    Ok(Json(ApiResp::ok(responses)))
}

/// 版本详情查询 Handler
///
/// 根据版本主键 ID 查询版本详情
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `params` - 查询参数，必须提供 id
///
/// # Returns
///
/// 版本详情。
///
/// # Errors
///
/// * `Error::bad_request` - 未提供 id 时返回
/// * `Error::not_found` - 版本不存在时返回
/// * `Error::internal_error` - 查询失败时返回
#[utoipa::path(
    get,
    path = "/api/marketplace/plugin/version/get",
    params(MarketplaceVersionGetParams),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<MarketplaceVersionResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_version_get_by_id(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<MarketplaceVersionGetParams>,
) -> Result<Json<ApiResp<MarketplaceVersionResponse>>> {
    debug!("查询版本详情: {:?}", params);

    let id = params.id.ok_or_else(|| Error::bad_request("请提供 id 参数"))?;

    let service = get_marketplace_service().await;
    let version = service
        .get_version_by_id(&id)
        .await
        .map_err(|e| Error::internal_error(format!("查询版本详情失败: {}", e)))?;

    let version = version.ok_or_else(|| Error::not_found("版本不存在"))?;

    Ok(Json(ApiResp::ok(convert_version_to_response(version))))
}

/// 从市场安装 Handler
///
/// 根据 plugin_id 和可选的 version 从市场下载并安装插件
/// 安装后自动记录下载和安装统计
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `req` - 安装请求，包含 plugin_id、version（可选）、db_id（可选）、auto_activate
///
/// # Returns
///
/// 安装结果，包含插件 ID、安装路径和版本信息
///
/// # Errors
///
/// * `Error::not_found` - 版本不存在时返回
/// * `Error::bad_request` - 版本没有下载地址时返回
/// * `Error::internal_error` - 安装或统计记录失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/install",
    request_body = MarketInstallRequest,
    responses(
        (status = 200, description = "安装成功", body = ApiResp<MarketInstallResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<MarketInstallRequest>,
) -> Result<Json<ApiResp<MarketInstallResponse>>> {
    info!("从市场安装插件: plugin_id={}, version={:?}", req.plugin_id, req.version);

    let service = get_marketplace_service().await;

    let version_info = if let Some(ref version) = req.version {
        service.get_version(&req.plugin_id, version).await
    } else {
        service.get_latest_stable_version(&req.plugin_id).await
    }
    .map_err(|e| Error::internal_error(format!("查询版本信息失败: {}", e)))?;

    let version_info = version_info.ok_or_else(|| Error::not_found("版本不存在"))?;

    let download_url = version_info
        .download_url
        .clone()
        .ok_or_else(|| Error::bad_request("该版本没有提供下载地址"))?;

    let manager = cmx_plugin::GlobalPluginManager::get();
    let install_req = cmx_plugin::service::install::InstallRequest {
        source: cmx_plugin::domain::plugin::PluginSource::Remote {
            url: download_url.clone(),
            checksum: version_info.checksum.clone(),
        },
        db_id: req.db_id.clone(),
        auto_activate: req.auto_activate,
        version_constraint: None,
        build_type: None,
    };

    let result = manager
        .install(install_req)
        .await
        .map_err(|e| Error::internal_error(format!("插件安装失败: {}", e)))?;

    let version_str = &version_info.version;
    if let Err(e) = service.record_download(&req.plugin_id, version_str, "marketplace").await {
        tracing::warn!("记录下载统计失败: {}", e);
    }
    if let Err(e) = service.record_install(&req.plugin_id).await {
        tracing::warn!("记录安装统计失败: {}", e);
    }

    let response = MarketInstallResponse {
        plugin_id: result.plugin_id,
        install_path: Some(result.install_path.to_string_lossy().to_string()),
        version: Some(result.version),
        success: result.success,
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 评分 Handler
///
/// 评分范围 1-5 分，评分后自动更新插件的平均评分统计。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）。
/// * `_svr_ctx` - 服务器上下文（框架注入）。
/// * `req` - 评分请求，包含 plugin_id、rating（1-5）和可选的 review。
///
/// # Returns
///
/// 评分成功返回 UnitResp。
///
/// # Errors
///
/// * `Error::internal_error` - 评分失败时返回。
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/rate",
    request_body = RatePluginRequest,
    responses(
        (status = 200, description = "评分成功", body = crate::api_response::UnitResp)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_rate(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<RatePluginRequest>,
) -> Result<Json<crate::api_response::UnitResp>> {
    info!("插件评分: plugin_id={}, rating={}", req.plugin_id, req.rating);

    let rate_req = MarketplaceRatingForCreate {
        plugin_id: req.plugin_id,
        user_id: "system".to_string(),
        rating: Some(req.rating),
        review: req.review,
        status: Some("approved".to_string()),
    };

    let service = get_marketplace_service().await;
    service
        .rate_plugin(rate_req)
        .await
        .map_err(|e| Error::internal_error(format!("评分失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("评分成功")))
}

/// 评分列表查询 Handler
///
/// 支持按 plugin_id、user_id、status 过滤
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `filter` - 查询过滤条件
///
/// # Returns
///
/// 符合条件的评分列表。
///
/// # Errors
///
/// * `Error::internal_error` - 查询失败时返回。
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/rating/list",
    request_body = MarketplaceRatingFilterDoc,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplaceRatingResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_rating_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(filter): Json<MarketplaceRatingFilterDoc>,
) -> Result<Json<ApiResp<Vec<MarketplaceRatingResponse>>>> {
    debug!("查询评分列表: {:?}", filter);

    let modql_filter: MarketplaceRatingFilter = filter.into();

    let service = get_marketplace_service().await;
    let ratings = service
        .list_ratings(Some(vec![modql_filter]), None)
        .await
        .map_err(|e| Error::internal_error(format!("查询评分列表失败: {}", e)))?;

    let responses: Vec<MarketplaceRatingResponse> = ratings
        .into_iter()
        .map(|r| MarketplaceRatingResponse {
            id: r.id,
            plugin_id: r.plugin_id,
            user_id: r.user_id,
            rating: r.rating,
            review: r.review,
            status: r.status,
            create_time: r.create_time,
            update_time: r.update_time,
            create_name: r.create_name,
        })
        .collect();

    Ok(Json(ApiResp::ok(responses)))
}

/// Marketplace 插件分类列表查询 Handler
///
/// 统计各分类下的插件数量，按数量降序排列。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
///
/// # Returns
///
/// 分类信息列表，包含分类名称和插件数量
///
/// # Errors
///
/// * `Error::internal_error` - 查询失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/category/list",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<CategoryResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_category_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Vec<CategoryResponse>>>> {
    debug!("查询分类列表");

    let service = get_marketplace_service().await;
    let categories = service
        .get_categories()
        .await
        .map_err(|e| Error::internal_error(format!("查询分类列表失败: {}", e)))?;

    let responses: Vec<CategoryResponse> = categories
        .into_iter()
        .map(|c| CategoryResponse {
            category: c.category,
            count: c.count,
        })
        .collect();

    Ok(Json(ApiResp::ok(responses)))
}

/// Marketplace 热门插件查询 Handler
///
/// 根据最近 N 天的下载量统计，返回最热门的插件列表
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `filter` - 查询过滤条件，包含 days（默认7天）和 limit（默认10个）
///
/// # Returns
///
/// 热门插件列表，按下载量降序排列
///
/// # Errors
///
/// * `Error::internal_error` - 查询失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/stats/trending/list",
    request_body = TrendingFilter,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplacePluginResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_trending_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(filter): Json<TrendingFilter>,
) -> Result<Json<ApiResp<Vec<MarketplacePluginResponse>>>> {
    debug!("查询热门插件: days={:?}, limit={:?}", filter.days, filter.limit);

    let days = filter.days.unwrap_or(7);
    let limit = filter.limit.unwrap_or(10);

    let service = get_marketplace_service().await;
    let plugins = service
        .get_trending_plugins(days, limit)
        .await
        .map_err(|e| Error::internal_error(format!("查询热门插件失败: {}", e)))?;

    let responses: Vec<MarketplacePluginResponse> = plugins
        .into_iter()
        .map(convert_plugin_to_response)
        .collect();

    Ok(Json(ApiResp::ok(responses)))
}
