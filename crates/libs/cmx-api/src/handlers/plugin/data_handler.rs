//! 通用插件数据导入/查询 HTTP 端点。
//!
//! 提供 `/api/plugin/data/import`(POST multipart) 和 `/api/plugin/data/list`(GET)
//! 两个端点,供远程模式(http_url/http_discovery)的 Remote 定义导入器调用。
//!
//! 与 `/api/iam/permissions/import` 的区别:本端点是通用的,按 multipart/query 的
//! `category` 字段路由到 Form/Menu/Perm/Flow,不局限于权限。

use axum::extract::{Multipart, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::debug;

use cmx_traits::plugin::{PluginDataCategory, PluginDataImportRequest};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// 解析 category 字符串,无效时返回错误。
fn parse_category(s: &str) -> Result<PluginDataCategory> {
    PluginDataCategory::parse_from_str(s).ok_or_else(|| {
        crate::Error::BadRequest(format!(
            "无效的 category: {s}（有效值: menu/perm/form/flow）"
        ))
    })
}

/// 导入结果 DTO
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    pub success: bool,
    pub message: String,
    pub created_count: u32,
    pub updated_count: u32,
    pub deleted_count: u32,
}

/// 查询结果 DTO
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResultDto {
    pub success: bool,
    pub message: String,
    /// JSON 序列化的定义列表(base64 编码传输,避免嵌套 JSON 转义)
    pub json_data: String,
}

/// 查询参数
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub category: String,
    pub domain_code: Option<String>,
    pub application_code: Option<String>,
    pub module_code: String,
}

/// 通用插件数据导入(POST multipart)
///
/// 接收 ZIP 文件和元数据,通过 PluginDataImporter trait 处理。
/// 按 `category` 字段路由到 Form/Menu/Perm 等类别。
pub async fn import_plugin_data(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<ImportResultDto>>> {
    debug!("{:<12} - handler::import_plugin_data", "HANDLER");

    let importer = cmx_state
        .plugin_data_importer()
        .ok_or_else(|| crate::Error::InternalError("PluginDataImporter 未初始化".to_string()))?;

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
                    .map_err(|e| crate::Error::BadRequest(format!("读取文件失败: {e}")))?;
                file_data = Some(bytes.to_vec());
            }
            "category" => category = Some(field.text().await.unwrap_or_default()),
            "domain_code" => domain_code = field.text().await.unwrap_or_default(),
            "application_code" => application_code = field.text().await.unwrap_or_default(),
            "module_code" => module_code = field.text().await.unwrap_or_default(),
            "plugin_id" => plugin_id = field.text().await.unwrap_or_default(),
            "app_id" => app_id = field.text().await.unwrap_or_default(),
            "version" => version = field.text().await.unwrap_or_default(),
            _ => {}
        }
    }

    let zip_data = file_data
        .ok_or_else(|| crate::Error::BadRequest("缺少 file 字段".to_string()))?;

    if domain_code.is_empty() || application_code.is_empty() || module_code.is_empty() {
        return Err(crate::Error::BadRequest(
            "domain_code/application_code/module_code 不能为空".to_string(),
        ));
    }

    let parsed_category = match category.as_deref() {
        Some(s) => parse_category(s)
            .map_err(|e| crate::Error::BadRequest(format!("无效的 category: {e}")))?,
        None => PluginDataCategory::Perm,
    };

    // Perm 类别需要 plugin_id/app_id/version
    if matches!(parsed_category, PluginDataCategory::Perm)
        && (plugin_id.is_empty() || app_id.is_empty() || version.is_empty())
    {
        return Err(crate::Error::BadRequest(
            "Perm 类别导入需要 plugin_id/app_id/version 非空".to_string(),
        ));
    }

    let request = PluginDataImportRequest {
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
        .map_err(|e| crate::Error::InternalError(e.to_string()))?;

    Ok(Json(ApiResp::ok(ImportResultDto {
        success: result.success,
        message: result.message,
        created_count: result.created_count,
        updated_count: result.updated_count,
        deleted_count: result.deleted_count,
    })))
}

/// 通用插件数据查询/导出(GET)
///
/// 按 `category` 查询指定模块的资源定义,返回 JSON 序列化的定义列表。
pub async fn list_plugin_data(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResp<ListResultDto>>> {
    debug!("{:<12} - handler::list_plugin_data", "HANDLER");

    let importer = cmx_state
        .plugin_data_importer()
        .ok_or_else(|| crate::Error::InternalError("PluginDataImporter 未初始化".to_string()))?;

    let category = parse_category(&q.category)
        .map_err(|e| crate::Error::BadRequest(format!("无效的 category: {e}")))?;

    let request = PluginDataImportRequest {
        category,
        domain_code: q.domain_code.unwrap_or_default(),
        application_code: q.application_code.unwrap_or_default(),
        module_code: q.module_code,
        plugin_id: String::new(),
        app_id: String::new(),
        version: String::new(),
        zip_data: Vec::new(),
    };

    let result = importer
        .list_data(request)
        .await
        .map_err(|e| crate::Error::InternalError(e.to_string()))?;

    // json_data 是 UTF-8 JSON 字节,直接转 String 返回
    let json_str = String::from_utf8(result.json_data).unwrap_or_default();

    Ok(Json(ApiResp::ok(ListResultDto {
        success: result.success,
        message: result.message,
        json_data: json_str,
    })))
}
