//! 插件管理 HTTP Handler
//!
//! 提供插件安装、卸载、升级、降级、列表查询等 API

use std::path::PathBuf;

use axum::extract::{Multipart, Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::DateTime;
use tracing::{debug, info};
use cmx_utils::ConfigManager;
use crate::ApiResp;
use crate::app_state::CmxAppState;
use crate::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::PageParamsDoc;
use super::request::*;
use super::response::*;

/// 从请求转换为 cmx_plugin 的 PluginSource
pub fn convert_source(req: &PluginSourceRequest) -> cmx_plugin::domain::plugin::PluginSource {
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
        PluginSourceRequest::Marketplace {
            marketplace_url,
            plugin_id,
        } => {
            cmx_plugin::domain::plugin::PluginSource::Marketplace {
                marketplace_url: Some(marketplace_url.clone()),
                plugin_id: plugin_id.clone(),
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
        (status = 200, description = "安装成功", body = ApiResp<InstallResponse>)
    ),
    tag = "Plugin"
)]
pub async fn plugin_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginInstallRequest>,
) -> Result<Json<ApiResp<InstallResponse>>> {
    debug!("插件安装请求: {:?}", req);

    let manager = cmx_plugin::GlobalPluginManager::get();

    let app_id = manager.app_id().to_string();
    let install_req = cmx_plugin::service::install::InstallRequest {
        source: convert_source(&req.source),
        db_id: req.target_db_id,
        auto_activate: false,
        version_constraint: None,
        build_type: None,
        marketplace_source_id: None,
        app_id: Some(app_id),
    };

    let result = manager.install(install_req).await.map_err(|e| {
        crate::Error::InternalError(format!("插件安装失败: {}", e))
    })?;

    let resp = InstallResponse {
        plugin_id: result.plugin_id,
        install_path: result.install_path.to_string_lossy().to_string(),
        version: result.version,
        success: result.success,
        message: Some(result.message),
    };

    Ok(Json(ApiResp::ok(resp)))
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

    let manager = cmx_plugin::GlobalPluginManager::get();

    let app_id = manager.app_id().to_string();
    let uninstall_req = cmx_plugin::service::uninstall::UninstallRequest {
        plugin_id: req.plugin_id.clone(),
        force: req.force.unwrap_or(false),
        operator: "system".to_string(),
        app_id: Some(app_id),
    };

    let result = manager.uninstall(uninstall_req).await.map_err(|e| {
        crate::Error::InternalError(format!("插件卸载失败: {}", e))
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

    let manager = cmx_plugin::GlobalPluginManager::get();

    let app_id = manager.app_id().to_string();
    let upgrade_req = cmx_plugin::service::upgrade::UpgradeRequest {
        plugin_id: req.plugin_id.clone(),
        source: convert_source(&req.source),
        version_constraint: req.version_constraint,
        force: req.force.unwrap_or(false),
        operator: req.operator,
        build_type: None,
        marketplace_source_id: None,
        app_id: Some(app_id),
    };

    let result = manager.upgrade(upgrade_req).await.map_err(|e| {
        crate::Error::InternalError(format!("插件升级失败: {}", e))
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

    let manager = cmx_plugin::GlobalPluginManager::get();

    let app_id = manager.app_id().to_string();
    let downgrade_req = cmx_plugin::service::downgrade::DowngradeRequest {
        plugin_id: req.plugin_id.clone(),
        target_version: req.target_version.clone(),
        source: None,
        operator: req.operator,
        app_id: Some(app_id),
    };

    let result = manager.downgrade(downgrade_req).await.map_err(|e| {
        crate::Error::InternalError(format!("插件降级失败: {}", e))
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
  // 核心：声明请求体结构体，utoipa 会根据字段生成表单
     request_body(content = PluginDeployRequest, description = "部署参数", content_type = "multipart/form-data"),
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
    info!("插件部署请求（文件上传）");


    let uploads_root = ConfigManager::global().get_string("plugin.upload_root")
        .unwrap_or("plugins/uploads".to_string());

    let uploads_dir = PathBuf::from(uploads_root);
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut target_db_id: Option<String> = None;
    let mut force_reinstall: bool = false;
    //构建类型 debug /release
    let mut build_type:Option<String> = None;
    let mut publish_to_marketplace: Option<bool> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| crate::Error::BadRequest(format!("解析 multipart 请求失败: {}", e)))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                let data = field.bytes().await
                    .map_err(|e| crate::Error::BadRequest(format!("读取文件失败: {}", e)))?;
                file_bytes = Some(data.to_vec());
            }
            "target_db_id" => {
                let val = field.text().await
                    .map_err(|e| crate::Error::BadRequest(format!("读取 target_db_id 失败: {}", e)))?;
                if !val.is_empty() {
                    target_db_id = Some(val);
                }
            }
            "force_reinstall" => {
                let val = field.text().await
                    .map_err(|e| crate::Error::BadRequest(format!("读取 force_reinstall 失败: {}", e)))?;
                force_reinstall = val == "true" || val == "1";
            }
            "build_type" => {
                let val = field.text().await
                    .map_err(|e| crate::Error::BadRequest(format!("读取 build_type 失败: {}", e)))?;
                build_type = Some(val);
            }
            "publish_to_marketplace" => {
                let text = field.text().await.map_err(|e| crate::Error::internal_error(format!("读取字段失败: {}", e)))?;
                publish_to_marketplace = text.parse().ok();
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        crate::Error::BadRequest("未上传文件，请上传插件 zip 包".to_string())
    })?;

    // 确保上传目录存在
    tokio::fs::create_dir_all(&uploads_dir).await
        .map_err(|e| crate::Error::InternalError(format!("创建上传目录失败: {}", e)))?;

    // 使用 UUID 重命名保存 zip 文件
    let file_name = format!("{}.zip", uuid::Uuid::new_v4());
    let file_path = uploads_dir.join(&file_name);
    tokio::fs::write(&file_path, &file_bytes).await
        .map_err(|e| crate::Error::InternalError(format!("保存文件失败: {}", e)))?;

    let abs_path = std::fs::canonicalize(&file_path)
        .map_err(|e| crate::Error::InternalError(format!("获取文件绝对路径失败: {}", e)))?;

    // 如果需要发布到市场，先解析插件定义并发布
    let marketplace_source_id: Option<String>;
    let marketplace_publish_info: Option<cmx_plugin::service::marketplace_publisher::MarketplacePublishInfo>;
    let source: cmx_plugin::domain::plugin::PluginSource;

    if publish_to_marketplace.unwrap_or(true) {
        let plugin_def = tokio::task::spawn_blocking({
            let abs_path = abs_path.clone();
            move || cmx_plugin::common::DefinitionUtils::parse_from_zip(&abs_path)
        })
        .await
        .map_err(|e| crate::Error::InternalError(format!("解析插件定义失败: {}", e)))?
        .map_err(|e| crate::Error::InternalError(format!("解析插件定义失败: {}", e)))?;

        let publish_req = cmx_plugin::service::marketplace_publisher::PublishFromDeployRequest {
            plugin_id: plugin_def.id.clone(),
            version: plugin_def.version.clone().unwrap_or_else(|| "1.0.0".to_string()),
            plugin_def: plugin_def.clone(),
            zip_file_path: abs_path.clone(),
        };

        let result = cmx_plugin::service::marketplace_publisher::MarketplacePublisher::publish_from_deploy(&publish_req)
            .await
            .map_err(|e| crate::Error::InternalError(format!("发布到插件市场失败: {}", e)))?;

        // 先取需要的数据，再消费 result
        let file_url = result.file_url.clone();
        let marketplace_version_id = result.marketplace_version_id.clone();

        marketplace_source_id = Some(marketplace_version_id);
        marketplace_publish_info = Some(result.into());

        // 发布后使用 Remote source
        source = cmx_plugin::domain::plugin::PluginSource::Remote {
            url: file_url,
            checksum: None,
        };
    } else {
        marketplace_source_id = None;
        marketplace_publish_info = None;

        // 未发布则使用 Local source
        source = cmx_plugin::domain::plugin::PluginSource::Local {
            path: abs_path,
        };
    }

    let manager = cmx_plugin::GlobalPluginManager::get();

    let app_id = manager.app_id().to_string();
    let deploy_req = cmx_plugin::DeployRequest {
        source,
        db_id: target_db_id,
        force_reinstall,
        build_type,
        publish_to_marketplace: false,
        app_id: Some(app_id),
        marketplace_source_id,
        marketplace_publish_info,
    };

    let result = manager.deploy(deploy_req).await.map_err(|e| {
        crate::Error::InternalError(format!("插件部署失败: {}", e))
    })?;

    let resp = PluginDeployResponse {
        plugin_id: result.plugin_id,
        action: format!("{:?}", result.action).to_lowercase(),
        old_version: result.old_version,
        new_version: result.new_version,
        install_path: result.install_path.to_string_lossy().to_string(),
        success: result.success,
        message: Some(result.message),
        marketplace_publish: result.marketplace_publish.map(|info| MarketplacePublishInfoResponse {
            marketplace_plugin_id: info.marketplace_plugin_id,
            marketplace_version_id: info.marketplace_version_id,
            storage_file_id: info.storage_file_id,
            is_new_plugin: info.is_new_plugin,
        }),
    };

    Ok(Json(ApiResp::ok(resp)))
}

/// 将 PluginDbRecord 转换为 PluginInfoResponse
fn convert_db_record_to_response(record: cmx_plugin::infrastructure::database::repository::PluginRecord) -> PluginInfoResponse {
    PluginInfoResponse {
        id: record.id,
        plugin_id: record.plugin_id,
        name: record.name,
        description: record.description,
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
        domain_name: record.domain_name,
        application_name: record.application_name,
        module_name: record.module_name,
        vendor_name: record.vendor_name,
        vendor_url: record.vendor_url,
        vendor_contact: record.vendor_contact,
        metadata: record.metadata,
        source_type: record.zip_source_type,
        source_url: record.zip_source_url,
        plugin_type: record.plugin_type,
        source_path: record.source_path,
        create_time: record.create_time,
        update_time: record.update_time,
        create_by: record.create_by,
        create_name: record.create_name,
        update_by: record.update_by,
        update_name: record.update_name,
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
        cmx_plugin::domain::plugin::PluginSource::Marketplace {
            plugin_id,
            ..
        } => (Some("marketplace".to_string()), Some(plugin_id.clone())),
        cmx_plugin::domain::plugin::PluginSource::Storage { file_id, .. } => {
            (Some("storage".to_string()), Some(file_id.clone()))
        }
    };

    PluginInfoResponse {
        id: String::new(),
        plugin_id: info.id.clone(),
        name: info.name.clone(),
        description: info.description.clone(),
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
        domain_name: None,
        application_name: None,
        module_name: None,
        vendor_name: info.author.clone(),
        vendor_url: None,
        vendor_contact: None,
        metadata: None,
        source_type,
        source_url,
        plugin_type: Some(info.plugin_type),
        source_path: info.source_path,
        create_time: DateTime::default(),
        update_time: DateTime::default(),
        create_by: None,
        create_name: None,
        update_by: None,
        update_name: None,
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
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<super::request::ApiPluginFilter>>,
) -> Result<Json<ApiResp<PluginListResponse>>> {
    debug!("插件列表查询: {:?}", params);

    let manager = cmx_plugin::GlobalPluginManager::get();

    let mut filter: cmx_plugin::domain::plugin::PluginFilter = params.filter
        .unwrap_or_default()
        .into();

    let app_id = cmx_state.app_id();
    if filter.app_id.is_none() {
        filter.app_id = Some(app_id);
    }

    let plugins = manager.repository().list_plugins(&filter).await.map_err(|e| {
        crate::Error::InternalError(format!("获取插件列表失败: {}", e))
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

    let manager = cmx_plugin::GlobalPluginManager::get();

    let plugin = manager.get_plugin(&params.plugin_id).await.map_err(|e| {
        crate::Error::InternalError(format!("获取插件详情失败: {}", e))
    })?;

    match plugin {
        Some(info) => Ok(Json(ApiResp::ok(convert_plugin_info(info)))),
        // None => Err(crate::Error::NotFound(format!("插件 {} 不存在", params.plugin_id))),
        None => Ok(Json(ApiResp::fail(1, "插件不存在"))),
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
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<super::request::ApiPluginFilter>>,
) -> Result<Json<ApiResp<Vec<PluginInfoResponse>>>> {
    debug!("插件分页查询: {:?}", params);

    let page = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let skip = params.get_offset() as usize;

    let mut filter: cmx_plugin::domain::plugin::PluginFilter = params.filter
        .unwrap_or_default()
        .into();

    let app_id = cmx_state.app_id();
    if filter.app_id.is_none() {
        filter.app_id = Some(app_id);
    }

    let manager = cmx_plugin::GlobalPluginManager::get();
    let all_plugins = manager.repository().list_plugins(&filter).await.map_err(|e| {
        crate::Error::InternalError(format!("获取插件列表失败: {}", e))
    })?;

    let total = all_plugins.len() as u64;

    let paginated_plugins: Vec<PluginInfoResponse> = all_plugins
        .into_iter()
        .skip(skip)
        .take(page_size as usize)
        .map(convert_db_record_to_response)
        .collect();

    Ok(Json(ApiResp::ok_with_pagination(
        paginated_plugins,
        page,
        page_size,
        total,
    )))
}

/// 查询插件是否存在
///
/// 处理 GET /api/plugin/exists 请求，通过 plugin_id 查询插件是否已存在。
///
/// # 参数
/// - `query`: 查询参数（PluginExistsQuery）
///
/// # 查询参数
/// - `plugin_id`: 插件ID
///
/// # 响应体
/// - code: 0
/// - data: "1" 存在, "0" 不存在
#[utoipa::path(
    get,
    path = "/api/plugin/exists",
    params(PluginExistsQuery),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<String>)
    ),
    tag = "Plugin"
)]
pub async fn plugin_exists(
    Query(query): Query<PluginExistsQuery>,
) -> Result<Json<ApiResp<String>>> {
    let manager = cmx_plugin::GlobalPluginManager::get();
    let exists = manager.repository().plugin_exists(&query.plugin_id).await
        .map_err(|e| crate::Error::internal_error(format!("查询插件存在性失败: {}", e)))?;

    Ok(Json(ApiResp::ok(if exists { "1" } else { "0" }.to_string())))
}

/// 批量获取插件函数列表
///
/// 处理 POST /api/plugin/functions 请求，批量获取多个插件的 api.json 文件内容。
///
/// # 参数
/// - `request`: 请求体（PluginFunctionsRequest），包含 plugin_ids 列表
///
/// # 响应
/// - 成功：返回 Map<plugin_id, api.json的JSON内容>
/// - 失败：返回错误信息
#[utoipa::path(
    post,
    path = "/api/plugin/functions",
    request_body = PluginFunctionsRequest,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<std::collections::HashMap<String, PluginFunctionsResponse>>)
    ),
    tag = "Plugin"
)]
pub async fn plugin_functions(
    Json(request): Json<PluginFunctionsRequest>,
) -> Result<Json<ApiResp<std::collections::HashMap<String, PluginFunctionsResponse>>>> {
    let manager = cmx_plugin::GlobalPluginManager::get();
    let mut result = std::collections::HashMap::new();

    for plugin_id in &request.plugin_ids {
        match manager.get_plugin(plugin_id).await {
            Ok(Some(plugin_info)) => {
                let api_json_path = plugin_info.install_path.join("api").join("api.json");
                match tokio::fs::read_to_string(&api_json_path).await {
                    Ok(content) => {
                        match serde_json::from_str::<serde_json::Value>(&content) {
                            Ok(json_value) => {
                                result.insert(plugin_id.clone(), PluginFunctionsResponse {
                                    success: true,
                                    plugin_name: plugin_info.name.clone(),
                                    plugin_version: plugin_info.version.clone(),
                                    functions: json_value,
                                });
                            }
                            Err(e) => {
                                result.insert(plugin_id.clone(), PluginFunctionsResponse {
                                    success: false,
                                    plugin_name: plugin_info.name.clone(),
                                    plugin_version: plugin_info.version.clone(),
                                    functions: serde_json::json!({
                                        "error": format!("解析 api.json 失败: {}", e)
                                    }),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        result.insert(plugin_id.clone(), PluginFunctionsResponse {
                            success: false,
                            plugin_name: plugin_info.name.clone(),
                            plugin_version: plugin_info.version.clone(),
                            functions: serde_json::json!({
                                "error": format!("读取 api.json 失败: {}", e)
                            }),
                        });
                    }
                }
            }
            Ok(None) => {
                result.insert(plugin_id.clone(), PluginFunctionsResponse {
                    success: false,
                    plugin_name: "".to_string(),
                    plugin_version: "".to_string(),
                    functions: serde_json::json!({
                        "error": format!("插件 {} 不存在", plugin_id)
                    }),
                });
            }
            Err(e) => {
                result.insert(plugin_id.clone(), PluginFunctionsResponse {
                    success: false,
                    plugin_name: "".to_string(),
                    plugin_version: "".to_string(),
                    functions: serde_json::json!({
                        "error": format!("获取插件信息失败: {}", e)
                    }),
                });
            }
        }
    }

    Ok(Json(ApiResp::ok(result)))
}
