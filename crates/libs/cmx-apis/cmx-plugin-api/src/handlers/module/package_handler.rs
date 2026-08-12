//! 模块迁移包导入/导出 Handler
//!
//! - POST /api/module/package/import  上传模块 zip 导入(multipart)
//! - GET  /api/module/package/export  导出模块迁移包(返回 zip)

use axum::Json;
use axum::extract::{Multipart, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use cmx_database::get_default_db_manager;
use cmx_plugin::common::{PackageUtils, PackageUtilsDeps};
use cmx_plugin::service::module_export::ModuleExportService;
use cmx_plugin::service::module_install::{ModuleInstallService, ModulePackageSource};

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::rest::header_parse::get_db_id_from_header;
use cmx_api_core::{ApiResp, Result};

/// 模块迁移包导入请求参数
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ModulePackageImportRequest {
    /// 模块 zip 包文件（必填）
    #[schema(content_media_type = "application/octet-stream")]
    pub file: Vec<u8>,

    /// 是否强制降级覆盖新版本（可选，默认 false）
    pub force: Option<bool>,
}

/// 导出查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ExportQuery {
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 导入响应
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleImportResponse {
    pub success: bool,
    pub skipped: bool,
    pub reason: String,
    pub module_code: String,
    pub package_version: String,
    pub plugin_count: usize,
}

/// 导入模块迁移包(multipart 上传 zip)
///
/// 通过 multipart/form-data 上传模块 zip 文件。
///
/// 请求字段：
/// - `file`: 模块 zip 包文件（必填）
/// - `force`: 是否强制降级覆盖新版本（可选，默认 false）
#[utoipa::path(
    post,
    path = "/api/module/package/import",
    request_body(content = ModulePackageImportRequest, description = "导入参数", content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "导入成功", body = ApiResp<ModuleImportResponse>),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "导入失败")
    ),
    tag = "Module"
)]
pub async fn module_package_import(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<ModuleImportResponse>>> {
    debug!("{:<12} - handler::module_package_import", "HANDLER");

    // 1. 接收 multipart zip
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut force: bool = false;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| cmx_api_core::Error::BadRequest(format!("解析 multipart 请求失败: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| cmx_api_core::Error::BadRequest(format!("读取文件失败: {e}")))?;
                file_bytes = Some(data.to_vec());
            }
            "force" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| cmx_api_core::Error::BadRequest(format!("读取 force 失败: {e}")))?;
                force = val == "true" || val == "1";
            }
            _ => {}
        }
    }
    let file_bytes = file_bytes
        .ok_or_else(|| cmx_api_core::Error::BadRequest("未上传文件，请上传模块 zip 包".to_string()))?;

    // 2. 构造 ModuleInstallService
    let manager = cmx_plugin::GlobalPluginManager::get();
    let plugin_root = manager.settings().plugin_root.clone();
    let package_utils = PackageUtils::new(PackageUtilsDeps {
        plugin_root: plugin_root.clone(),
        temp_root: std::path::PathBuf::from("./temp"),
        storage: None,
    });
    // 通过 deploy_service 共享(模块包内插件子包复用 deploy 自动判断升级/安装)
    let deploy_svc = std::sync::Arc::new(manager.deploy_service().clone());
    let module_install_svc = ModuleInstallService::new(package_utils, deploy_svc);
    // 注入模块资源定义导入器集合(表单/菜单/元数据/权限统一委托,消除重复 SQL)
    let module_install_svc = if let Some(importers) = cmx_state.definition_importers() {
        module_install_svc.with_definition_importers(importers.clone())
    } else {
        module_install_svc
    };

    // 3. 执行导入(含版本校验)
    let result = module_install_svc
        .install_module_package(ModulePackageSource::Bytes(file_bytes), force, None)
        .await
        .map_err(|e| match e {
            cmx_plugin::error::PluginError::CenterData(msg) => cmx_api_core::Error::BadRequest(msg),
            other => cmx_api_core::Error::InternalError(format!("导入失败: {other}")),
        })?;

    Ok(Json(ApiResp::ok(ModuleImportResponse {
        success: result.success,
        skipped: result.skipped,
        reason: result.reason,
        module_code: result.module_code,
        package_version: result.package_version,
        plugin_count: result.plugin_count,
    })))
}

/// 导出模块迁移包(返回 zip 流,自动生成 package_version 时间戳)
#[utoipa::path(
    get,
    path = "/api/module/package/export",
    params(ExportQuery),
    responses((status = 200, description = "导出成功", content_type = "application/zip")),
    tag = "Module"
)]
pub async fn module_package_export(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<ExportQuery>,
) -> Result<axum::response::Response> {
    debug!("{:<12} - handler::module_package_export", "HANDLER");

    let manager = cmx_plugin::GlobalPluginManager::get();
    let plugin_root = manager.settings().plugin_root.clone();
    let mut export_svc = ModuleExportService::new(plugin_root);
    // 注入模块资源定义导入器集合(导出时用 list_* 方法,消除内联 SQL)
    if let Some(importers) = cmx_state.definition_importers() {
        export_svc = export_svc.with_definition_importers(importers.clone());
    }

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let zip_bytes = export_svc
        .export_module(
            mm,
            &db_id,
            &q.domain_code,
            &q.application_code,
            &q.module_code,
        )
        .await
        .map_err(|e| cmx_api_core::Error::InternalError(format!("{e}")))?;

    info!(module_code = %q.module_code, size = zip_bytes.len(), "模块包导出成功");

    let filename = format!(
        "module_{}_{}_{}.zip",
        q.domain_code,
        q.module_code,
        chrono::Local::now().format("%Y%m%d%H%M%S")
    );
    let content_disposition = format!("attachment; filename=\"{filename}\"")
        .parse()
        .unwrap_or_else(|_| {
            warn!("无效的 Content-Disposition, 使用默认值");
            axum::http::HeaderValue::from_static("attachment")
        });
    let content_length = axum::http::HeaderValue::from_str(&zip_bytes.len().to_string())
        .unwrap_or_else(|_| {
            warn!("无效的 Content-Length, 省略该头");
            axum::http::HeaderValue::from_static("0")
        });

    Ok((
        axum::http::StatusCode::OK,
        axum::response::AppendHeaders([
            (axum::http::header::CONTENT_DISPOSITION, content_disposition),
            (axum::http::header::CONTENT_LENGTH, content_length),
        ]),
        axum::body::Body::from(zip_bytes),
    )
        .into_response())
}
