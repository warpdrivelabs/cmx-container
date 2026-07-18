//! 插件数据导入 Handler
//!
//! 通过 multipart form-data 接收 ZIP 文件，委托给 ResourceDataImporter trait 处理。
//! 与 gRPC 路径统一走同一个 trait，保证 category 路由和缓存失效逻辑一致。

use axum::Json;
use axum::extract::{Multipart, State};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use cmx_traits::resource::{
    ResourceDataCategory, ResourceDataCleanupRequest, ResourceDataImportRequest,
};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

/// 导入结果 DTO
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
    /// 新增数量
    pub created_count: u32,
    /// 更新数量
    pub updated_count: u32,
    /// 删除数量
    pub deleted_count: u32,
}

impl From<cmx_traits::resource::ResourceDataImportResult> for ImportResultDto {
    fn from(r: cmx_traits::resource::ResourceDataImportResult) -> Self {
        Self {
            success: r.success,
            message: r.message,
            created_count: r.created_count,
            updated_count: r.updated_count,
            deleted_count: r.deleted_count,
        }
    }
}

/// 清理请求 DTO
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRequest {
    /// 数据类别（可选，默认 perm）
    pub category: Option<String>,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
    /// 插件 ID
    pub plugin_id: String,
    /// 应用 ID
    pub app_id: String,
}

/// 解析 category 字符串。
///
/// 无效的 category 返回错误，而非静默降级为 Perm。
fn parse_category(s: &str) -> Result<ResourceDataCategory> {
    ResourceDataCategory::parse_from_str(s).ok_or_else(|| {
        Error::bad_request(format!(
            "无效的 category: {s}（有效值: menu/perm/form/flow）"
        ))
    })
}

/// 导入插件数据（multipart form-data）
///
/// 接收 ZIP 文件和元数据，通过 ResourceDataImporter trait 处理。
/// gRPC 路径也走同一个 trait，保证逻辑一致。
#[utoipa::path(
    post,
    path = "/api/iam/permissions/import",
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "导入成功", body = ApiResp<ImportResultDto>),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "服务器内部错误")
    ),
    tag = "IAM-Permission"
)]
pub async fn import_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<ImportResultDto>>> {
    debug!("{:<12} - handler::import_permissions", "HANDLER");

    let importer = cmx_state
        .resource_data_importer()
        .ok_or_else(|| Error::business_error("ResourceDataImporter 未初始化".to_string()))?;

    // 解析 multipart 字段
    let mut file_data: Option<Vec<u8>> = None;
    let mut category: Option<String> = None;
    let mut domain_code = String::new();
    let mut application_code = String::new();
    let mut module_code = String::new();
    let mut plugin_id = String::new();
    let mut app_id = String::new();
    let mut version = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| Error::bad_request(format!("读取文件失败: {e}")))?;
                file_data = Some(bytes.to_vec());
            }
            "category" => {
                category = Some(field.text().await.unwrap_or_default());
            }
            "domain_code" => {
                domain_code = field.text().await.unwrap_or_default();
            }
            "application_code" => {
                application_code = field.text().await.unwrap_or_default();
            }
            "module_code" => {
                module_code = field.text().await.unwrap_or_default();
            }
            "plugin_id" => {
                plugin_id = field.text().await.unwrap_or_default();
            }
            "app_id" => {
                app_id = field.text().await.unwrap_or_default();
            }
            "version" => {
                version = field.text().await.unwrap_or_default();
            }
            other => {
                warn!(field = %other, "忽略未知的 multipart 字段");
            }
        }
    }

    let zip_data = file_data.ok_or_else(|| Error::bad_request("缺少 file 字段".to_string()))?;

    if domain_code.is_empty() || application_code.is_empty() || module_code.is_empty() {
        return Err(Error::bad_request(
            "domain_code/application_code/module_code 不能为空".to_string(),
        ));
    }

    // 解析 category(默认 Perm,与旧版兼容)
    let parsed_category = match category.as_deref() {
        Some(s) => parse_category(s)?,
        None => ResourceDataCategory::Perm,
    };

    // plugin_id/app_id/version 仅 Perm(插件权限导入)场景需要;
    // Form/Menu/Table(模块资源导入)无插件上下文,允许为空。
    if matches!(parsed_category, ResourceDataCategory::Perm)
        && (plugin_id.is_empty() || app_id.is_empty() || version.is_empty())
    {
        return Err(Error::bad_request(
            "Perm 类别导入需要 plugin_id/app_id/version 非空".to_string(),
        ));
    }

    let request = ResourceDataImportRequest {
        category: parsed_category,
        domain_code,
        application_code,
        module_code,
        plugin_id,
        app_id,
        version,
        zip_data,
    };

    let result = importer
        .import_data(request)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(ImportResultDto::from(result))))
}

/// 清理插件数据
#[utoipa::path(
    post,
    path = "/api/iam/permissions/cleanup",
    request_body = CleanupRequest,
    responses(
        (status = 200, description = "清理成功", body = ApiResp<ImportResultDto>),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "服务器内部错误")
    ),
    tag = "IAM-Permission"
)]
pub async fn cleanup_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<CleanupRequest>,
) -> Result<Json<ApiResp<ImportResultDto>>> {
    debug!("{:<12} - handler::cleanup_permissions", "HANDLER");

    let importer = cmx_state
        .resource_data_importer()
        .ok_or_else(|| Error::business_error("ResourceDataImporter 未初始化".to_string()))?;

    let request = ResourceDataCleanupRequest {
        category: match req.category.as_deref() {
            Some(s) => parse_category(s)?,
            None => ResourceDataCategory::Perm,
        },
        domain_code: req.domain_code,
        application_code: req.application_code,
        module_code: req.module_code,
        plugin_id: req.plugin_id,
        app_id: req.app_id,
    };

    let result = importer
        .cleanup_data(request)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(ImportResultDto::from(result))))
}
