//! 插件市场 HTTP Handler
//!
//! 提供插件市场的 API 处理函数，包括：
//! - 插件分页查询、详情查询
//! - 发布、更新、删除插件
//! - 版本列表、版本详情
//! - 从市场安装插件
//! - 评分、评分列表
//! - 分类列表、热门插件

use axum::extract::{Query, State};
use axum::Json;
use cmx_database::get_default_db_manager;
use cmx_plugin::marketplace::model::MarketplaceFilter;
use std::sync::Arc;
use tracing::{debug, info};

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::{Error, Result};
use crate::middleware::CmxSvrContext;
use crate::rest::PageParamsDoc;
use super::request::*;
use super::response::*;

/// 获取市场服务实例
///
/// 从全局 DatabaseManager 创建 MarketplaceService。
/// 由于 MarketplaceService 是无状态的，每次请求创建新实例开销很小。
async fn get_marketplace_service() -> cmx_plugin::MarketplaceService {
    let db_manager = get_default_db_manager();
    let default_db_id = db_manager.get_default_db_id().await;

    let repo = Arc::new(cmx_plugin::MarketplaceRepository::new(
        db_manager.clone(),
        default_db_id,
    ));
    let stats_service = Arc::new(cmx_plugin::StatsService::new(repo.clone()));
    cmx_plugin::MarketplaceService::new(repo, stats_service)
}

/// 将 MarketplacePlugin 模型转换为响应结构体
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

/// 将 MarketplacePluginVersion 模型转换为响应结构体
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

/// 分页查询插件市场插件
///
/// 支持关键词搜索、分类过滤、状态过滤、排序等
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/page",
    request_body = PageParamsDoc<MarketplacePluginFilter>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplacePluginResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<MarketplacePluginFilter>>,
) -> Result<Json<ApiResp<Vec<MarketplacePluginResponse>>>> {
    debug!("插件市场分页查询: {:?}", params);

    let page = params.get_page() as u64;
    let page_size = params.get_size() as u64;

    let api_filter = params.filter.unwrap_or_default();
    let filter = MarketplaceFilter {
        keyword: api_filter.keyword,
        category: api_filter.category,
        tags: api_filter.tags,
        status: api_filter.status.or(Some("published".to_string())),
        domain_code: api_filter.domain_code,
        application_code: api_filter.application_code,
        module_code: api_filter.module_code,
        sort_by: api_filter.sort_by,
        sort_order: api_filter.sort_order,
    };

    let service = get_marketplace_service().await;
    let (plugins, total) = service.page_plugins(&filter, page, page_size).await
        .map_err(|e| Error::internal_error(format!("查询市场插件失败: {}", e)))?;

    let responses: Vec<MarketplacePluginResponse> = plugins
        .into_iter()
        .map(convert_plugin_to_response)
        .collect();

    Ok(Json(ApiResp::ok_with_pagination(responses, page, page_size, total)))
}

/// 查询单条插件市场插件
///
/// 通过 id 或 plugin_id 查询插件详情
#[utoipa::path(
    get,
    path = "/api/marketplace/plugin",
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

    // 优先使用 plugin_id 查询，其次使用 id
    let plugin = if let Some(plugin_id) = &params.plugin_id {
        service.get_plugin_by_plugin_id(plugin_id).await
    } else if let Some(id) = &params.id {
        service.get_plugin_by_id(id).await
    } else {
        return Err(Error::bad_request("请提供 id 或 plugin_id 参数"));
    }.map_err(|e| Error::internal_error(format!("查询市场插件详情失败: {}", e)))?;

    let plugin = plugin.ok_or_else(|| Error::not_found("插件不存在"))?;

    // 获取版本列表
    let versions = service.get_plugin_versions(&plugin.plugin_id).await
        .map_err(|e| Error::internal_error(format!("查询版本列表失败: {}", e)))?;

    // 获取最新稳定版本
    let latest_version = service.get_latest_stable_version(&plugin.plugin_id).await
        .map_err(|e| Error::internal_error(format!("查询最新版本失败: {}", e)))?;

    let detail = MarketplacePluginDetailResponse {
        plugin: convert_plugin_to_response(plugin),
        latest_version: latest_version.map(convert_version_to_response),
        versions: versions.into_iter().map(convert_version_to_response).collect(),
    };

    Ok(Json(ApiResp::ok(detail)))
}

/// 发布插件到市场
///
/// 创建插件记录和版本记录
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin",
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

    let service = get_marketplace_service().await;
    let plugin = service.publish_plugin(
        req.plugin_id,
        req.name,
        req.description,
        req.short_description,
        req.category,
        req.tags,
        req.license_type,
        req.vendor_name,
        req.vendor_url,
        req.vendor_contact,
        req.homepage_url,
        req.documentation_url,
        req.repository_url,
        req.icon_url,
        req.domain_code,
        req.application_code,
        req.module_code,
        req.plugin_type,
        req.version,
        req.download_url,
        req.package_size,
        req.checksum,
        req.changelog,
        req.release_notes,
        req.min_platform_version,
        req.max_platform_version,
        None,
        None,
    ).await.map_err(|e| Error::internal_error(format!("发布插件失败: {}", e)))?;

    Ok(Json(ApiResp::ok(convert_plugin_to_response(plugin))))
}

/// 更新插件市场信息
///
/// 更新插件的基本信息，仅更新提供的字段
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

    let service = get_marketplace_service().await;
    service.update_plugin(
        &req.plugin_id,
        req.name.as_deref(),
        req.description.as_deref(),
        req.short_description.as_deref(),
        req.category.as_deref(),
        req.tags.as_deref(),
        req.status.as_deref(),
        req.is_featured,
        req.is_official,
        req.icon_url.as_deref(),
        req.license_type.as_deref(),
        req.homepage_url.as_deref(),
        req.documentation_url.as_deref(),
        req.repository_url.as_deref(),
        req.vendor_name.as_deref(),
        req.vendor_url.as_deref(),
        req.vendor_contact.as_deref(),
    ).await.map_err(|e| Error::internal_error(format!("更新市场插件失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("更新成功")))
}

/// 删除插件（逻辑删除）
///
/// 将插件标记为已归档
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
    service.delete_plugin(&req.plugin_id).await
        .map_err(|e| Error::internal_error(format!("删除市场插件失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("删除成功")))
}

/// 版本列表查询
///
/// 查询指定插件的所有版本
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/version/list",
    request_body = MarketplaceVersionFilter,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplaceVersionResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_version_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(filter): Json<MarketplaceVersionFilter>,
) -> Result<Json<ApiResp<Vec<MarketplaceVersionResponse>>>> {
    debug!("查询版本列表: {:?}", filter);

    let plugin_id = filter.plugin_id.as_deref().unwrap_or("");
    if plugin_id.is_empty() {
        return Err(Error::bad_request("请提供 plugin_id 参数"));
    }

    let service = get_marketplace_service().await;
    let versions = service.get_plugin_versions(plugin_id).await
        .map_err(|e| Error::internal_error(format!("查询版本列表失败: {}", e)))?;

    let responses: Vec<MarketplaceVersionResponse> = versions
        .into_iter()
        .map(convert_version_to_response)
        .collect();

    Ok(Json(ApiResp::ok(responses)))
}

/// 版本详情查询
///
/// 通过 id 查询版本详情
#[utoipa::path(
    get,
    path = "/api/marketplace/plugin/version",
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
    let version = service.get_version_by_id(&id).await
        .map_err(|e| Error::internal_error(format!("查询版本详情失败: {}", e)))?;

    let version = version.ok_or_else(|| Error::not_found("版本不存在"))?;

    Ok(Json(ApiResp::ok(convert_version_to_response(version))))
}

/// 从市场安装插件
///
/// 根据插件ID和版本号从市场安装插件到本地
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

    // 获取版本信息
    let version_info = if let Some(ref version) = req.version {
        service.get_version(&req.plugin_id, version).await
    } else {
        service.get_latest_stable_version(&req.plugin_id).await
    }.map_err(|e| Error::internal_error(format!("查询版本信息失败: {}", e)))?;

    let version_info = version_info.ok_or_else(|| Error::not_found("版本不存在"))?;

    // 获取下载地址
    let download_url = version_info.download_url.clone()
        .ok_or_else(|| Error::bad_request("该版本没有提供下载地址"))?;

    // 调用 PluginManager 执行安装
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

    let result = manager.install(install_req).await.map_err(|e| {
        Error::internal_error(format!("插件安装失败: {}", e))
    })?;

    // 记录下载和安装统计
    let version_str = &version_info.version;
    if let Err(e) = service.record_download(&req.plugin_id, version_str, "marketplace").await {
        // 统计记录失败不影响安装结果
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

/// 评分
///
/// 对指定插件进行评分和评论
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

    let service = get_marketplace_service().await;
    service.rate_plugin(
        &req.plugin_id,
        "system", // TODO: 从 svr_ctx 获取用户ID
        req.rating,
        req.review.as_deref(),
        None,
        None,
    ).await.map_err(|e| Error::internal_error(format!("评分失败: {}", e)))?;

    Ok(Json(crate::api_response::UnitResp::msg("评分成功")))
}

/// 评分列表查询
///
/// 查询指定插件的评分列表
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/rating/list",
    request_body = MarketplaceRatingFilter,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<MarketplaceRatingResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_rating_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(filter): Json<MarketplaceRatingFilter>,
) -> Result<Json<ApiResp<Vec<MarketplaceRatingResponse>>>> {
    debug!("查询评分列表: {:?}", filter);

    let plugin_id = filter.plugin_id.as_deref().unwrap_or("");
    if plugin_id.is_empty() {
        return Err(Error::bad_request("请提供 plugin_id 参数"));
    }

    let service = get_marketplace_service().await;
    let ratings = service.list_ratings(plugin_id, filter.status.as_deref()).await
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

/// 分类列表查询
///
/// 返回所有分类及其插件数量
#[utoipa::path(
    post,
    path = "/api/marketplace/category/list",
    request_body = MarketplaceCategoryFilter,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<CategoryResponse>>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_category_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(_filter): Json<MarketplaceCategoryFilter>,
) -> Result<Json<ApiResp<Vec<CategoryResponse>>>> {
    debug!("查询分类列表");

    let service = get_marketplace_service().await;
    let categories = service.get_categories().await
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

/// 热门插件列表查询
///
/// 根据指定天数内的下载量排序，返回热门插件
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
    let plugins = service.get_trending_plugins(days, limit).await
        .map_err(|e| Error::internal_error(format!("查询热门插件失败: {}", e)))?;

    let responses: Vec<MarketplacePluginResponse> = plugins
        .into_iter()
        .map(convert_plugin_to_response)
        .collect();

    Ok(Json(ApiResp::ok(responses)))
}
