//! 资源数据交换 DTO（导入/清理请求与结果）。

use serde::{Deserialize, Serialize};

use crate::resource::category::ResourceDataCategory;

/// 资源数据导入请求。
#[derive(Debug, Clone)]
pub struct ResourceDataImportRequest {
    /// 数据类别。
    pub category: ResourceDataCategory,
    /// 域编码。
    pub domain_code: String,
    /// 应用编码。
    pub application_code: String,
    /// 模块编码。
    pub module_code: String,
    /// 插件 ID。
    pub plugin_id: String,
    /// 应用 ID。
    pub app_id: String,
    /// 插件版本。
    pub version: String,
    /// ZIP 压缩数据。
    pub zip_data: Vec<u8>,
}

/// 资源数据清理请求。
#[derive(Debug, Clone)]
pub struct ResourceDataCleanupRequest {
    /// 数据类别。
    pub category: ResourceDataCategory,
    /// 域编码。
    pub domain_code: String,
    /// 应用编码。
    pub application_code: String,
    /// 模块编码。
    pub module_code: String,
    /// 插件 ID。
    pub plugin_id: String,
    /// 应用 ID。
    pub app_id: String,
}

/// 资源数据导入结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDataImportResult {
    /// 是否成功。
    pub success: bool,
    /// 结果消息。
    pub message: String,
    /// 新增数量。
    pub created_count: u32,
    /// 更新数量。
    pub updated_count: u32,
    /// 删除数量。
    pub deleted_count: u32,
}

/// 资源数据查询（导出）结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDataListResult {
    /// 是否成功。
    pub success: bool,
    /// 结果消息。
    pub message: String,
    /// JSON 序列化的定义列表字节（如 `Vec<FormDefinition>` 序列化后的 JSON 数组）。
    pub json_data: Vec<u8>,
}
