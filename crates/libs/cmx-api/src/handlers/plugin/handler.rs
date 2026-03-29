//! 插件管理 HTTP Handler
//!
//! 提供插件安装、卸载、升级、降级、列表查询等 API

use std::path::PathBuf;

use axum::extract::{Multipart, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use tracing::debug;

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::PageParamsDoc;
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
                registry_url: Some(registry_url.clone()),
                package_name: package_name.clone(),
            }
        }
    }
}

/// 插件安装 Handler
///
/// 从指定来源安装插件
#[utoipa::path(
    post,
    path = "/api/plugin/install",
    request_body = PluginInstallRequest,
    responses(
        (status = 200, description = "安装成功", body = ApiResp<String>)
    ),
    tag = "Plugin"
)]
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

    let _resp = InstallResponse {
        plugin_id: result.plugin_id,
        install_path: result.install_path.to_string_lossy().to_string(),
        version: result.version,
        success: result.success,
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok("success".to_string())))
}

/// 插件卸载 Handler
///
/// 卸载指定的插件
#[utoipa::path(
    post,
    path = "/api/plugin/uninstall",
    request_body = PluginUninstallRequest,
    responses(
        (status = 200, description = "卸载成功", body = ApiResp<UninstallResponse>)
    ),
    tag = "Plugin"
)]
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
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 插件升级 Handler
///
/// 升级指定的插件到新版本
#[utoipa::path(
    post,
    path = "/api/plugin/upgrade",
    request_body = PluginUpgradeRequest,
    responses(
        (status = 200, description = "升级成功", body = ApiResp<UpgradeResponse>)
    ),
    tag = "Plugin"
)]
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
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 插件降级 Handler
///
/// 将指定的插件降级到目标版本
#[utoipa::path(
    post,
    path = "/api/plugin/downgrade",
    request_body = PluginDowngradeRequest,
    responses(
        (status = 200, description = "降级成功", body = ApiResp<DowngradeResponse>)
    ),
    tag = "Plugin"
)]
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
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 插件部署 Handler（上传 zip 文件，自动判断安装/升级/覆盖安装）
///
/// 通过 multipart/form-data 上传插件 zip 文件，系统自动判断操作类型。
///
/// 请求字段：
/// - `file`: 插件 zip 包文件（必填）
/// - `target_db_id`: 目标数据库ID（可选）
/// - `force_reinstall`: 是否覆盖安装（可选，默认 false）
#[utoipa::path(
    post,
    path = "/api/plugin/deploy",
    responses(
        (status = 200, description = "部署成功", body = ApiResp<PluginDeployResponse>),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "部署失败")
    ),
    tag = "Plugin"
)]
pub async fn plugin_deploy(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<PluginDeployResponse>>> {
    debug!("插件部署请求（文件上传）");

    let uploads_dir = PathBuf::from("./uploads/plugins");
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut target_db_id: Option<String> = None;
    let mut force_reinstall: bool = false;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| crate::error::Error::BadRequest(format!("解析 multipart 请求失败: {}", e)))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                let data = field.bytes().await
                    .map_err(|e| crate::error::Error::BadRequest(format!("读取文件失败: {}", e)))?;
                file_bytes = Some(data.to_vec());
            }
            "target_db_id" => {
                let val = field.text().await
                    .map_err(|e| crate::error::Error::BadRequest(format!("读取 target_db_id 失败: {}", e)))?;
                if !val.is_empty() {
                    target_db_id = Some(val);
                }
            }
            "force_reinstall" => {
                let val = field.text().await
                    .map_err(|e| crate::error::Error::BadRequest(format!("读取 force_reinstall 失败: {}", e)))?;
                force_reinstall = val == "true" || val == "1";
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        crate::error::Error::BadRequest("未上传文件，请上传插件 zip 包".to_string())
    })?;

    // 确保上传目录存在
    tokio::fs::create_dir_all(&uploads_dir).await
        .map_err(|e| crate::error::Error::InternalError(format!("创建上传目录失败: {}", e)))?;

    // 使用 UUID 重命名保存 zip 文件
    let file_name = format!("{}.zip", uuid::Uuid::new_v4());
    let file_path = uploads_dir.join(&file_name);
    tokio::fs::write(&file_path, &file_bytes).await
        .map_err(|e| crate::error::Error::InternalError(format!("保存文件失败: {}", e)))?;

    // 构建 PluginSource::Local
    let source = cmx_plugin::domain::plugin::PluginSource::Local {
        path: file_path.clone(),
    };

    // 调用 PluginManager.deploy()
    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let deploy_req = cmx_plugin::DeployRequest {
        source,
        db_id: target_db_id,
        force_reinstall,
    };

    let result = manager.deploy(deploy_req).await.map_err(|e| {
        crate::error::Error::InternalError(format!("插件部署失败: {}", e))
    })?;

    let resp = PluginDeployResponse {
        plugin_id: result.plugin_id,
        action: format!("{:?}", result.action).to_lowercase(),
        old_version: result.old_version,
        new_version: result.new_version,
        install_path: result.install_path.to_string_lossy().to_string(),
        success: result.success,
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 将 PluginDbRecord 转换为 PluginInfoResponse
fn convert_db_record_to_response(record: cmx_plugin::infrastructure::database::repository::PluginDbRecord) -> PluginInfoResponse {
    PluginInfoResponse {
        id: record.id,
        plugin_id: record.plugin_id,
        name: record.name,
        version: record.version,
        wasm_path: if record.wasm_path.is_empty() { None } else { Some(record.wasm_path) },
        install_path: record.install_path,
        db_id: if record.db_id.is_empty() { None } else { Some(record.db_id) },
        status: record.status,
        is_system: record.is_system,
        is_locked: record.is_locked,
        domain_code: record.domain_code,
        application_code: record.application_code,
        module_code: record.module_code,
        vendor_name: record.vendor_name,
        vendor_url: record.vendor_url,
        vendor_contact: record.vendor_contact,
        metadata: record.metadata,
        source_type: record.zip_source_type,
        source_url: record.zip_source_url,
        installed_at: Some(record.create_time.to_rfc3339()),
        updated_at: Some(record.update_time.to_rfc3339()),
    }
}

/// 将 PluginInfo 转换为 PluginInfoResponse
fn convert_plugin_info(info: cmx_plugin::domain::plugin::PluginInfo) -> PluginInfoResponse {
    let (source_type, source_url) = match &info.source {
        cmx_plugin::domain::plugin::PluginSource::Local { path } => {
            (Some("local".to_string()), Some(path.to_string_lossy().to_string()))
        }
        cmx_plugin::domain::plugin::PluginSource::Remote { url, .. } => {
            (Some("remote".to_string()), Some(url.clone()))
        }
        cmx_plugin::domain::plugin::PluginSource::Registry {
            package_name,
            ..
        } => (Some("registry".to_string()), Some(package_name.clone())),
    };

    PluginInfoResponse {
        id: String::new(),
        plugin_id: info.id.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        wasm_path: None,
        install_path: info.install_path.to_string_lossy().to_string(),
        db_id: None,
        status: format!("{:?}", info.status),
        is_system: false,
        is_locked: false,
        domain_code: if info.domain_code.is_empty() { None } else { Some(info.domain_code) },
        application_code: if info.application_code.is_empty() { None } else { Some(info.application_code) },
        module_code: if info.module_code.is_empty() { None } else { Some(info.module_code) },
        vendor_name: info.author.clone(),
        vendor_url: None,
        vendor_contact: None,
        metadata: None,
        source_type,
        source_url,
        installed_at: info.installed_at.map(|dt| dt.to_rfc3339()),
        updated_at: info.updated_at.map(|dt| dt.to_rfc3339()),
    }
}

/// 插件列表 Handler
///
/// 获取插件列表，支持按过滤条件筛选
#[utoipa::path(
    post,
    path = "/api/plugin/list",
    request_body = crate::rest::ListParamsDoc<super::request::ApiPluginFilter>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<PluginListResponse>)
    ),
    tag = "Plugin"
)]
pub async fn plugin_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<super::request::ApiPluginFilter>>,
) -> Result<Json<ApiResp<PluginListResponse>>> {
    debug!("插件列表查询: {:?}", params);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let filter: cmx_plugin::domain::plugin::PluginFilter = params.filter
        .unwrap_or_default()
        .into();
    let plugins = manager.repository().list_plugins(&filter).await.map_err(|e| {
        crate::error::Error::InternalError(format!("获取插件列表失败: {}", e))
    })?;

    let plugin_responses: Vec<PluginInfoResponse> = plugins
        .into_iter()
        .map(convert_db_record_to_response)
        .collect();

    Ok(Json(ApiResp::ok(PluginListResponse {
        plugins: plugin_responses,
    })))
}

/// 插件详情 Handler
///
/// 获取指定插件的详细信息
#[utoipa::path(
    get,
    path = "/api/plugin/{plugin_id}",
    params(
        PluginIdPath
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<PluginInfoResponse>),
        (status = 404, description = "插件不存在")
    ),
    tag = "Plugin"
)]
pub async fn plugin_get(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Path(params): Path<PluginIdPath>,
) -> Result<Json<ApiResp<PluginInfoResponse>>> {
    debug!("插件详情查询: plugin_id={}", params.plugin_id);

    let manager = cmx_plugin::GlobalPluginManager::get().await;

    let plugin = manager.get_plugin(&params.plugin_id).await.map_err(|e| {
        crate::error::Error::InternalError(format!("获取插件详情失败: {}", e))
    })?;

    match plugin {
        Some(info) => Ok(Json(ApiResp::ok(convert_plugin_info(info)))),
        None => Err(crate::error::Error::NotFound(format!("插件 {} 不存在", params.plugin_id))),
    }
}

/// 插件分页 Handler
///
/// 分页获取插件列表，支持按域编码、应用编码、模块编码、状态、名称过滤
#[utoipa::path(
    post,
    path = "/api/plugin/page",
    request_body = PageParamsDoc<super::request::ApiPluginFilter>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<PluginInfoResponse>>)
    ),
    tag = "Plugin"
)]
pub async fn plugin_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<super::request::ApiPluginFilter>>,
) -> Result<Json<ApiResp<Vec<PluginInfoResponse>>>> {
    debug!("插件分页查询: {:?}", params);

    let page = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let skip = params.get_offset() as usize;

    let filter: cmx_plugin::domain::plugin::PluginFilter = params.filter
        .unwrap_or_default()
        .into();

    let manager = cmx_plugin::GlobalPluginManager::get().await;
    let all_plugins = manager.list_plugins(&filter).await.map_err(|e| {
        crate::error::Error::InternalError(format!("获取插件列表失败: {}", e))
    })?;

    let total = all_plugins.len() as u64;

    let paginated_plugins: Vec<PluginInfoResponse> = all_plugins
        .into_iter()
        .skip(skip)
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
