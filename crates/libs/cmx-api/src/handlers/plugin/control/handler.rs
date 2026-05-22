//! 插件管控 HTTP Handler
//!
//! 提供集中式管控接口，仅执行 DDL/DML + 文件推送，
//! 不触发本地运行时加载，完成后发布 RuntimeLoad 通知。

use axum::extract::{Multipart, State};
use axum::Json;
use tracing::info;
use crate::ApiResp;
use crate::app_state::CmxAppState;
use crate::Result;
use crate::middleware::CmxSvrContext;
use super::request::*;
use super::response::*;
use super::super::handler::convert_source;

/// 管控部署（multipart 上传 ZIP，自动判断安装/升级）
///
/// 上传插件 ZIP 至对象存储（cmx-storage），构建 Storage 类型 PluginSource，
/// 仅执行 DDL/DML，不触发本地运行时加载，完成后发布 RuntimeLoad 通知到集群。
#[utoipa::path(
    post,
    path = "/api/plugin/control/deploy",
    request_body(content = ControlDeployRequest, description = "管控部署参数", content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "部署成功", body = ApiResp<ControlActionResponse>),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "部署失败")
    ),
    tag = "PluginControl"
)]
pub async fn control_deploy(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<ControlActionResponse>>> {
    info!("管控部署请求（文件上传）");

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut target_db_id: Option<String> = None;
    let mut build_type: Option<String> = None;
    let mut app_id: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| crate::Error::BadRequest(format!("解析 multipart 请求失败: {}", e)))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                let file_name_val = field
                    .file_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "plugin.zip".to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| crate::Error::BadRequest(format!("读取文件失败: {}", e)))?;
                file_bytes = Some(data.to_vec());
                file_name = Some(file_name_val);
            }
            "target_db_id" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| crate::Error::BadRequest(format!("读取 target_db_id 失败: {}", e)))?;
                if !val.is_empty() {
                    target_db_id = Some(val);
                }
            }
            "build_type" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| crate::Error::BadRequest(format!("读取 build_type 失败: {}", e)))?;
                build_type = Some(val);
            }
            "app_id" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| crate::Error::BadRequest(format!("读取 app_id 失败: {}", e)))?;
                if !val.is_empty() {
                    app_id = Some(val);
                }
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        crate::Error::BadRequest("未上传文件，请上传插件 zip 包".to_string())
    })?;

    let manager = cmx_plugin::GlobalPluginManager::get();
    let effective_app_id = app_id.unwrap_or_else(|| manager.app_id().to_string());

    // 上传插件文件至对象存储（cmx-storage）
    let storage_service = cmx_storage::global::GlobalStorageService::get().service();
    let upload_request = cmx_storage::types::UploadRequest {
        data: file_bytes.into(),
        original_filename: file_name.clone(),
        content_type: Some("application/zip".to_string()),
        object_type: Some("control_plugin".to_string()),
        object_id: Some(effective_app_id.clone()),
        platform: None,
        user_metadata: None,
        acl: None,
    };
    let file_info = storage_service.upload(upload_request).await
        .map_err(|e| crate::Error::InternalError(format!("上传插件文件至对象存储失败: {}", e)))?;

    info!(
        "管控部署: 插件文件已上传至对象存储, file_id={}, size={}",
        file_info.id, file_info.size
    );

    // 构建 Storage 类型 PluginSource
    let source = cmx_plugin::domain::plugin::PluginSource::Storage {
        file_id: file_info.id.clone(),
        checksum: file_info.hash_info.clone(),
    };

    let control_req = cmx_plugin::service::control::ControlDeployRequest {
        source,
        db_id: target_db_id,
        build_type,
        app_id: Some(effective_app_id.clone()),
    };

    let control_service = manager.control_service();
    let result = control_service.deploy(control_req).await.map_err(|e| {
        crate::Error::InternalError(format!("管控部署失败: {}", e))
    })?;

    Ok(Json(ApiResp::ok(ControlActionResponse {
        plugin_id: result.plugin_id,
        version: result.version,
        action: result.action,
        app_id: result.app_id,
    })))
}

/// 管控安装
///
/// 仅执行 DDL/DML，不触发本地运行时加载。
#[utoipa::path(
    post,
    path = "/api/plugin/control/install",
    request_body = ControlInstallRequest,
    responses(
        (status = 200, description = "安装成功", body = ApiResp<ControlActionResponse>)
    ),
    tag = "PluginControl"
)]
pub async fn control_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ControlInstallRequest>,
) -> Result<Json<ApiResp<ControlActionResponse>>> {
    info!("管控安装请求: app_id={:?}", req.app_id);

    let manager = cmx_plugin::GlobalPluginManager::get();
    let effective_app_id = req.app_id.clone().unwrap_or_else(|| manager.app_id().to_string());

    let control_req = cmx_plugin::service::control::ControlInstallRequest {
        source: convert_source(&req.source),
        db_id: req.target_db_id,
        build_type: req.build_type,
        app_id: Some(effective_app_id),
    };

    let control_service = manager.control_service();
    let result = control_service.install(control_req).await.map_err(|e| {
        crate::Error::InternalError(format!("管控安装失败: {}", e))
    })?;

    Ok(Json(ApiResp::ok(ControlActionResponse {
        plugin_id: result.plugin_id,
        version: result.version,
        action: result.action,
        app_id: result.app_id,
    })))
}

/// 管控升级
///
/// 仅执行 DDL/DML，不触发本地运行时加载。
#[utoipa::path(
    post,
    path = "/api/plugin/control/upgrade",
    request_body = ControlUpgradeRequest,
    responses(
        (status = 200, description = "升级成功", body = ApiResp<ControlActionResponse>)
    ),
    tag = "PluginControl"
)]
pub async fn control_upgrade(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ControlUpgradeRequest>,
) -> Result<Json<ApiResp<ControlActionResponse>>> {
    info!("管控升级请求: plugin_id={}, app_id={:?}", req.plugin_id, req.app_id);

    let manager = cmx_plugin::GlobalPluginManager::get();
    let effective_app_id = req.app_id.clone().unwrap_or_else(|| manager.app_id().to_string());

    let control_req = cmx_plugin::service::control::ControlUpgradeRequest {
        plugin_id: req.plugin_id,
        target_version: req.target_version,
        source: convert_source(&req.source),
        build_type: req.build_type,
        app_id: Some(effective_app_id),
    };

    let control_service = manager.control_service();
    let result = control_service.upgrade(control_req).await.map_err(|e| {
        crate::Error::InternalError(format!("管控升级失败: {}", e))
    })?;

    Ok(Json(ApiResp::ok(ControlActionResponse {
        plugin_id: result.plugin_id,
        version: result.version,
        action: result.action,
        app_id: result.app_id,
    })))
}

/// 管控降级
///
/// 仅执行 DDL/DML，不触发本地运行时加载。
#[utoipa::path(
    post,
    path = "/api/plugin/control/downgrade",
    request_body = ControlDowngradeRequest,
    responses(
        (status = 200, description = "降级成功", body = ApiResp<ControlActionResponse>)
    ),
    tag = "PluginControl"
)]
pub async fn control_downgrade(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ControlDowngradeRequest>,
) -> Result<Json<ApiResp<ControlActionResponse>>> {
    info!("管控降级请求: plugin_id={}, app_id={:?}", req.plugin_id, req.app_id);

    let manager = cmx_plugin::GlobalPluginManager::get();
    let effective_app_id = req.app_id.clone().unwrap_or_else(|| manager.app_id().to_string());

    let control_req = cmx_plugin::service::control::ControlDowngradeRequest {
        plugin_id: req.plugin_id,
        target_version: req.target_version,
        app_id: Some(effective_app_id),
    };

    let control_service = manager.control_service();
    let result = control_service.downgrade(control_req).await.map_err(|e| {
        crate::Error::InternalError(format!("管控降级失败: {}", e))
    })?;

    Ok(Json(ApiResp::ok(ControlActionResponse {
        plugin_id: result.plugin_id,
        version: result.version,
        action: result.action,
        app_id: result.app_id,
    })))
}

/// 管控卸载
///
/// 发布 RuntimeUnload 通知，不执行本地卸载。
#[utoipa::path(
    post,
    path = "/api/plugin/control/uninstall",
    request_body = ControlUninstallRequest,
    responses(
        (status = 200, description = "卸载成功", body = ApiResp<String>)
    ),
    tag = "PluginControl"
)]
pub async fn control_uninstall(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ControlUninstallRequest>,
) -> Result<Json<ApiResp<String>>> {
    info!("管控卸载请求: plugin_id={}, app_id={:?}", req.plugin_id, req.app_id);

    let manager = cmx_plugin::GlobalPluginManager::get();
    let effective_app_id = req.app_id.clone().unwrap_or_else(|| manager.app_id().to_string());

    let control_req = cmx_plugin::service::control::ControlUninstallRequest {
        plugin_id: req.plugin_id.clone(),
        app_id: Some(effective_app_id),
    };

    let control_service = manager.control_service();
    control_service.uninstall(control_req).await.map_err(|e| {
        crate::Error::InternalError(format!("管控卸载失败: {}", e))
    })?;

    Ok(Json(ApiResp::ok(format!("插件 {} 已提交卸载", req.plugin_id))))
}
