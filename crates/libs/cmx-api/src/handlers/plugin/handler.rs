//! 插件管理 HTTP Handler
//!
//! 提供插件安装、卸载、升级、降级、列表查询等 API

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use tracing::debug;

use crate::api_response::{ApiResp, Pagination};
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;

use super::request::*;
use super::response::*;

/// 从请求转换为 cmx_plugin 的 PluginSource
fn convert_source(req: &PluginSourceRequest) -> cmx_plugin::domain::plugin::PluginSource {
    match req {
        PluginSourceRequest::Local { path } => {
            cmx_plugin::domain::plugin::PluginSource::Local {
                path: PathBuf::from(path),
            }
        }
        PluginSourceRequest::Remote { url, checksum } => {
            cmx_plugin::domain::plugin::PluginSource::Remote {
                url: url.clone(),
                checksum: checksum.clone(),
            }
        }
        PluginSourceRequest::Registry {
            registry_url,
            package_name,
        } => {
            cmx_plugin::domain::plugin::PluginSource::Registry {
                registry_url: registry_url.clone(),
                package_name: package_name.clone(),
            }
        }
    }
}

/// 插件安装 Handler
pub async fn plugin_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginInstallRequest>,
) -> Result<Json<ApiResp<String>>> {
    debug!("插件安装请求: {:?}", req);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let install_req = cmx_plugin::service::install::InstallRequest {
        source: convert_source(&req.source),
        db_id: req.target_db_id,
        auto_activate: false,
        version_constraint: None,
    };

    let result = manager.install(install_req).await.map_err(|e| {
        crate::error::Error::InternalError(format!("插件安装失败: {}", e))
    })?;

    let resp = InstallResponse {
        plugin_id: result.plugin_id,
        install_path: result.install_path.to_string_lossy().to_string(),
        success: result.success,
        message: result.message,
    };

    Ok(Json(ApiResp::ok("success".to_string())))
}

/// 插件卸载 Handler
pub async fn plugin_uninstall(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginUninstallRequest>,
) -> Result<Json<ApiResp<UninstallResponse>>> {
    debug!("插件卸载请求: {:?}", req);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let uninstall_req = cmx_plugin::service::uninstall::UninstallRequest {
        plugin_id: req.plugin_id.clone(),
        force: req.force.unwrap_or(false),
        operator: "system".to_string(),
    };

    let result = manager.uninstall(uninstall_req).await.map_err(|e| {
        crate::error::Error::InternalError(format!("插件卸载失败: {}", e))
    })?;

    let resp = UninstallResponse {
        plugin_id: result.plugin_id,
        success: result.success,
        message: result.message,
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 插件升级 Handler
pub async fn plugin_upgrade(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginUpgradeRequest>,
) -> Result<Json<ApiResp<UpgradeResponse>>> {
    debug!("插件升级请求: {:?}", req);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let upgrade_req = cmx_plugin::service::upgrade::UpgradeRequest {
        plugin_id: req.plugin_id.clone(),
        source: convert_source(&req.source),
        version_constraint: req.version_constraint,
        force: req.force.unwrap_or(false),
        operator: req.operator,
    };

    let result = manager.upgrade(upgrade_req).await.map_err(|e| {
        crate::error::Error::InternalError(format!("插件升级失败: {}", e))
    })?;

    let resp = UpgradeResponse {
        plugin_id: result.plugin_id,
        old_version: result.old_version,
        new_version: result.new_version,
        success: result.success,
        message: result.message,
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 插件降级 Handler
pub async fn plugin_downgrade(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginDowngradeRequest>,
) -> Result<Json<ApiResp<DowngradeResponse>>> {
    debug!("插件降级请求: {:?}", req);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let downgrade_req = cmx_plugin::service::downgrade::DowngradeRequest {
        plugin_id: req.plugin_id.clone(),
        target_version: req.target_version.clone(),
        source: None,
        operator: req.operator,
    };

    let result = manager.downgrade(downgrade_req).await.map_err(|e| {
        crate::error::Error::InternalError(format!("插件降级失败: {}", e))
    })?;

    let resp = DowngradeResponse {
        plugin_id: result.plugin_id,
        old_version: result.old_version,
        target_version: result.new_version,
        success: result.success,
        message: result.message,
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 转换 PluginInfo 为 PluginInfoResponse
fn convert_plugin_info(info: cmx_plugin::domain::plugin::PluginInfo) -> PluginInfoResponse {
    let (source_type, source_url) = match &info.source {
        cmx_plugin::domain::plugin::PluginSource::Local { path } => {
            ("local".to_string(), Some(path.to_string_lossy().to_string()))
        }
        cmx_plugin::domain::plugin::PluginSource::Remote { url, .. } => {
            ("remote".to_string(), Some(url.clone()))
        }
        cmx_plugin::domain::plugin::PluginSource::Registry {
            package_name,
            ..
        } => ("registry".to_string(), Some(package_name.clone())),
    };

    PluginInfoResponse {
        plugin_id: info.id.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        description: info.description.clone(),
        author: info.author.clone(),
        source_type,
        source_url,
        status: format!("{:?}", info.status),
        installed_at: info.installed_at.map(|dt| dt.to_rfc3339()),
        updated_at: info.updated_at.map(|dt| dt.to_rfc3339()),
        install_path: info.install_path.to_string_lossy().to_string(),
    }
}

/// 插件列表 Handler
pub async fn plugin_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(query): Query<PluginListQuery>,
) -> Result<Json<ApiResp<PluginListResponse>>> {
    debug!("插件列表查询: {:?}", query);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let filter = cmx_plugin::domain::plugin::PluginFilter::default();
    let plugins = manager.list_plugins(&filter).await.map_err(|e| {
        crate::error::Error::InternalError(format!("获取插件列表失败: {}", e))
    })?;

    let plugin_responses: Vec<PluginInfoResponse> = plugins
        .into_iter()
        .filter(|p| {
            if let Some(ref status) = query.status {
                let status_str = format!("{:?}", p.status);
                if !status_str.to_lowercase().contains(&status.to_lowercase()) {
                    return false;
                }
            }
            true
        })
        .map(convert_plugin_info)
        .collect();

    Ok(Json(ApiResp::ok(PluginListResponse {
        plugins: plugin_responses,
    })))
}

/// 插件详情 Handler
pub async fn plugin_get(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Path(PluginIdPath { plugin_id }): Path<PluginIdPath>,
) -> Result<Json<ApiResp<PluginInfoResponse>>> {
    debug!("插件详情查询: plugin_id={}", plugin_id);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let plugin = manager.get_plugin(&plugin_id).await.map_err(|e| {
        crate::error::Error::InternalError(format!("获取插件详情失败: {}", e))
    })?;

    match plugin {
        Some(info) => Ok(Json(ApiResp::ok(convert_plugin_info(info)))),
        None => Err(crate::error::Error::NotFound(format!("插件 {} 不存在", plugin_id))),
    }
}

/// 插件分页 Handler
pub async fn plugin_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(query): Query<PluginPageQuery>,
) -> Result<Json<ApiResp<Vec<PluginInfoResponse>>>> {
    debug!("插件分页查询: {:?}", query);

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let skip = (page - 1) * page_size;

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let filter = cmx_plugin::domain::plugin::PluginFilter::default();
    let all_plugins = manager.list_plugins(&filter).await.map_err(|e| {
        crate::error::Error::InternalError(format!("获取插件列表失败: {}", e))
    })?;

    let total = all_plugins.len() as u64;

    let paginated_plugins: Vec<PluginInfoResponse> = all_plugins
        .into_iter()
        .skip(skip as usize)
        .take(page_size as usize)
        .map(convert_plugin_info)
        .collect();

    Ok(Json(ApiResp::ok_with_pagination(
        paginated_plugins,
        page,
        page_size,
        total,
    )))
}
