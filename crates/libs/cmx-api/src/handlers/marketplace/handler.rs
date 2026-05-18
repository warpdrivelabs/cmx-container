//! 插件市场 HTTP Handler
//!
//! 定义插件市场所有 REST API 端点，遵循 axum-handler-generator 规范：
//! - 使用独立路径区分不同操作（如 `/plugin/get`、`/plugin/publish`）
//! - 使用结构体参数传递请求数据
//! - 使用 modql FilterNodes 进行查询过滤
//!
//! API 路由前缀：`/api/marketplace`

use axum::extract::{Multipart, Query, State};
use axum::Json;
use cmx_database::get_default_db_manager;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::ApiResp;
use crate::app_state::CmxAppState;
use crate::{Error, Result};
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
        storage_file_id: version.storage_file_id,
        package_size: version.package_size,
        checksum: version.checksum,
        min_platform_version: version.min_platform_version,
        max_platform_version: version.max_platform_version,
        dependencies: version.dependencies,
        compatibility: version.compatibility,
        status: version.status,
        is_latest: version.is_latest.unwrap_or(0),
        is_stable: version.is_stable.unwrap_or(0),
        download_count: version.download_count.unwrap_or(0),
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
/// 如果插件不存在，则创建新插件记录。
///
/// 使用 multipart/form-data 接收请求：
/// - `plugin_info`: JSON 字符串，包含插件元信息
/// - `file`: 二进制文件，插件包（.zip/.wasm）
///
/// 文件上传到 cmx-storage 后，自动获取 file_id、URL、大小和校验和。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）
/// * `_svr_ctx` - 服务器上下文（框架注入）
/// * `multipart` - multipart 表单数据
///
/// # Returns
///
/// 发布成功后的插件完整信息
///
/// # Errors
///
/// * `Error::bad_request` - 缺少必要字段时返回
/// * `Error::internal_error` - 上传或发布失败时返回
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/publish",
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "发布成功", body = ApiResp<MarketplacePluginResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_publish(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<MarketplacePluginResponse>>> {
    let mut plugin_info_str: Option<String> = None;
    let mut file_data: Option<bytes::Bytes> = None;
    let mut file_name = String::new();
    let mut file_content_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "plugin_info" => {
                let text = field.text().await.map_err(|e| {
                    Error::bad_request(format!("读取 plugin_info 字段失败: {}", e))
                })?;
                plugin_info_str = Some(text);
            }
            "file" => {
                file_name = field
                    .file_name()
                    .unwrap_or("plugin.zip")
                    .to_string();
                file_content_type = field.content_type().map(String::from);
                file_data = Some(field.bytes().await.map_err(|e| {
                    Error::bad_request(format!("读取 file 字段失败: {}", e))
                })?);
            }
            _ => {}
        }
    }

    let info_str =
        plugin_info_str.ok_or_else(|| Error::bad_request("缺少 plugin_info 字段"))?;
    let req: PublishPluginRequest =
        serde_json::from_str(&info_str).map_err(|e| {
            Error::bad_request(format!("解析 plugin_info JSON 失败: {}", e))
        })?;
    let file_bytes =
        file_data.ok_or_else(|| Error::bad_request("缺少 file 字段"))?;

    info!(
        "发布插件到市场: plugin_id={}, version={}, file={} ({} bytes)",
        req.plugin_id,
        req.version,
        file_name,
        file_bytes.len()
    );

    let storage_service = cmx_storage::global::GlobalStorageService::get().service();
    let upload_request = cmx_storage::types::UploadRequest {
        data: file_bytes,
        original_filename: Some(file_name.clone()),
        content_type: file_content_type.or(Some("application/zip".to_string())),
        object_type: Some("marketplace_plugin".to_string()),
        object_id: Some(req.plugin_id.clone()),
        platform: None,
        user_metadata: None,
        acl: None,
    };
    let file_info = storage_service.upload(upload_request).await.map_err(|e| {
        Error::internal_error(format!("上传插件包到存储失败: {}", e))
    })?;

    info!(
        "插件包已上传到 cmx-storage: file_id={}, url={}, size={}",
        file_info.id, file_info.url, file_info.size
    );

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
        download_url: Some(file_info.url),
        storage_file_id: Some(file_info.id),
        package_size: Some(file_info.size),
        checksum: file_info.hash_info,
        min_platform_version: req.min_platform_version,
        max_platform_version: req.max_platform_version,
        dependencies: None,
        compatibility: None,
        status: Some("published".to_string()),
        is_latest: Some(1),
        is_stable: Some(1),
        published_at: Some(chrono::Utc::now()),
        allow_version_overwrite: false,
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
        (status = 200, description = "更新成功", body = crate::UnitResp)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_update(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<UpdateMarketplacePluginRequest>,
) -> Result<Json<crate::UnitResp>> {
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

    Ok(Json(crate::UnitResp::msg("更新成功")))
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
        (status = 200, description = "删除成功", body = crate::UnitResp)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_delete(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<DeleteMarketplacePluginRequest>,
) -> Result<Json<crate::UnitResp>> {
    info!("删除市场插件: plugin_id={}", req.plugin_id);

    let service = get_marketplace_service().await;
    service
        .delete_plugin(&req.plugin_id)
        .await
        .map_err(|e| Error::internal_error(format!("删除市场插件失败: {}", e)))?;

    Ok(Json(crate::UnitResp::msg("删除成功")))
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
/// 根据 plugin_id 和可选的 version 从市场下载并安装插件。
/// 优先使用 cmx-storage 的 storage_file_id 下载插件包，
/// 若 storage_file_id 不存在（兼容旧数据），降级使用 download_url。
/// 安装后自动记录下载和安装统计。
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

    let install_req = cmx_plugin::marketplace::model::MarketInstallRequest {
        plugin_id: req.plugin_id,
        version: req.version,
        db_id: req.db_id,
        auto_activate: Some(req.auto_activate),
    };

    let result = service
        .install_from_marketplace(&install_req)
        .await
        .map_err(|e| Error::internal_error(format!("插件安装失败: {}", e)))?;

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
        (status = 200, description = "评分成功", body = crate::UnitResp)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_rate(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<RatePluginRequest>,
) -> Result<Json<crate::UnitResp>> {
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

    Ok(Json(crate::UnitResp::msg("评分成功")))
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

/// 从市场升级插件。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）。
/// * `_svr_ctx` - 服务器上下文（框架注入）。
/// * `req` - 升级请求，包含 `plugin_id`、`target_version`、`force`。
///
/// # Returns
///
/// 升级成功返回 `ApiResp<MarketUpgradeResponse>`。
///
/// # Errors
///
/// 升级失败时返回内部错误。
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/upgrade",
    request_body = MarketUpgradeRequest,
    responses(
        (status = 200, description = "升级成功", body = ApiResp<MarketUpgradeResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_upgrade(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<MarketUpgradeRequest>,
) -> Result<Json<ApiResp<MarketUpgradeResponse>>> {
    info!("从市场升级插件: plugin_id={}, target_version={:?}", req.plugin_id, req.target_version);

    let service = get_marketplace_service().await;

    let result = service
        .upgrade_from_marketplace(&req.plugin_id, req.target_version.as_deref(), req.force)
        .await
        .map_err(|e| Error::internal_error(format!("插件升级失败: {}", e)))?;

    let response = MarketUpgradeResponse {
        plugin_id: result.plugin_id,
        old_version: Some(result.old_version),
        new_version: Some(result.new_version),
        success: result.success,
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 检查已安装插件的市场更新。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）。
/// * `_svr_ctx` - 服务器上下文（框架注入）。
/// * `req` - 检查更新请求，可选指定 `plugin_ids`。
///
/// # Returns
///
/// 检查完成返回 `ApiResp<CheckUpdatesResponse>`。
///
/// # Errors
///
/// 检查更新失败时返回内部错误。
#[utoipa::path(
    post,
    path = "/api/marketplace/plugin/check-updates",
    request_body = CheckUpdatesRequest,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<CheckUpdatesResponse>)
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_check_updates(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<CheckUpdatesRequest>,
) -> Result<Json<ApiResp<CheckUpdatesResponse>>> {
    info!("检查插件更新: plugin_ids={:?}", req.plugin_ids);

    let manager = cmx_plugin::GlobalPluginManager::get();
    let filter = cmx_plugin::domain::plugin::PluginFilter {
        status: None,
        name: None,
        domain_code: None,
        application_code: None,
        module_code: None,
    };
    let all_plugins = manager
        .repository()
        .list_plugins(&filter)
        .await
        .map_err(|e| Error::internal_error(format!("查询已安装插件失败: {}", e)))?;

    let plugins_to_check: Vec<_> = if let Some(ref ids) = req.plugin_ids {
        all_plugins
            .into_iter()
            .filter(|p| ids.contains(&p.plugin_id))
            .collect()
    } else {
        all_plugins
    };

    let service = get_marketplace_service().await;
    let updates = service
        .check_updates(&plugins_to_check)
        .await
        .map_err(|e| Error::internal_error(format!("检查更新失败: {}", e)))?;

    let response = CheckUpdatesResponse {
        updates: updates
            .iter()
            .map(|u| PluginUpdateInfoResponse {
                plugin_id: u.plugin_id.clone(),
                plugin_name: u.plugin_name.clone(),
                current_version: u.current_version.clone(),
                current_marketplace_source_id: u.current_marketplace_source_id.clone(),
                latest_version: u.latest_version.clone(),
                has_update: u.has_update,
            })
            .collect(),
        checked_at: chrono::Utc::now().to_rfc3339(),
    };
    Ok(Json(ApiResp::ok(response)))
}

/// 下载插件包。
///
/// 通过 `storage_file_id` 从 cmx-storage 获取文件并返回文件流，
/// 用于外部客户端（独立部署场景）下载插件包。
///
/// # Arguments
///
/// * `_cmx_state` - 应用状态（框架注入）。
/// * `_svr_ctx` - 服务器上下文（框架注入）。
/// * `params` - 下载查询参数，包含 `plugin_id` 和可选 `version`。
///
/// # Returns
///
/// 插件包文件流（`application/octet-stream`）。
///
/// # Errors
///
/// 版本不存在或文件不可用时返回错误。
#[utoipa::path(
    get,
    path = "/api/marketplace/plugin/download",
    params(MarketDownloadParams),
    responses(
        (status = 200, description = "下载成功"),
        (status = 404, description = "版本不存在"),
        (status = 503, description = "版本文件暂不可用")
    ),
    tag = "MarketplacePlugin"
)]
pub async fn marketplace_plugin_download(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<MarketDownloadParams>,
) -> Result<axum::response::Response> {
    info!("下载插件包: plugin_id={}, version={:?}", params.plugin_id, params.version);

    let service = get_marketplace_service().await;
    let version_info = if let Some(ref version) = params.version {
        service.get_version(&params.plugin_id, version).await
    } else {
        service.get_latest_stable_version(&params.plugin_id).await
    }
        .map_err(|e| Error::internal_error(format!("查询版本信息失败: {}", e)))?;

    let version_info = version_info.ok_or_else(|| Error::not_found("版本不存在"))?;

    let storage_file_id = version_info
        .storage_file_id
        .ok_or_else(|| Error::internal_error("版本文件暂不可用"))?;

    let storage_service = cmx_storage::global::GlobalStorageService::get().service();
    let file_download = storage_service
        .download(&storage_file_id)
        .await
        .map_err(|e| Error::internal_error(format!("下载文件失败: {}", e)))?;

    if let Err(e) = service
        .record_download(&params.plugin_id, &version_info.version, "download_api")
        .await
    {
        warn!("记录下载统计失败: {}", e);
    }

    let filename = format!("{}-{}.zip", params.plugin_id, version_info.version);
    let body = axum::body::Body::from(file_download.data);

    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .unwrap())
}
