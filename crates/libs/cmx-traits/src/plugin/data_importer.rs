//! 插件数据导入器 trait 定义。
//!
//! 定义插件数据（权限、菜单、表单、流程）导入到基础服务中心的统一接口，
//! 供 cmx-rpc（gRPC 服务端）和 cmx-api（HTTP 端点）统一调用。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::TraitError;

/// 数据类别枚举。
///
/// 与 cmx-plugin 的 `DataCategory` 对应，定义在 cmx-traits 供跨 crate 使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginDataCategory {
    /// 菜单数据。
    Menu,
    /// 权限数据。
    Perm,
    /// 表单数据。
    Form,
    /// 流程数据。
    Flow,
}

impl PluginDataCategory {
    /// 从目录名转换（如 "permdata" → Perm）。
    pub fn from_dir_name(dir_name: &str) -> Option<Self> {
        match dir_name {
            "menudata" => Some(Self::Menu),
            "permdata" => Some(Self::Perm),
            "formdata" => Some(Self::Form),
            "flowdata" => Some(Self::Flow),
            _ => None,
        }
    }

    /// 转为 proto 字符串标识。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Perm => "perm",
            Self::Form => "form",
            Self::Flow => "flow",
        }
    }

    /// 从 proto 字符串标识解析。
    pub fn parse_from_str(s: &str) -> Option<Self> {
        match s {
            "menu" => Some(Self::Menu),
            "perm" => Some(Self::Perm),
            "form" => Some(Self::Form),
            "flow" => Some(Self::Flow),
            _ => None,
        }
    }
}

/// 插件数据导入请求。
#[derive(Debug, Clone)]
pub struct PluginDataImportRequest {
    /// 数据类别。
    pub category: PluginDataCategory,
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

/// 插件数据清理请求。
#[derive(Debug, Clone)]
pub struct PluginDataCleanupRequest {
    /// 数据类别。
    pub category: PluginDataCategory,
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

/// 插件数据导入结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDataImportResult {
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

/// 插件数据查询（导出）结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDataListResult {
    /// 是否成功。
    pub success: bool,
    /// 结果消息。
    pub message: String,
    /// JSON 序列化的定义列表字节（如 `Vec<FormDefinition>` 序列化后的 JSON 数组）。
    pub json_data: Vec<u8>,
}

/// 插件数据导入器 trait。
///
/// 定义将插件数据导入到基础服务中心的统一接口。
/// cmx-iam 的 `PluginDataImporterImpl` 实现此 trait，
/// HTTP 端点和 gRPC 服务端均通过此 trait 调用。
#[async_trait]
pub trait PluginDataImporter: Send + Sync {
    /// 导入插件数据（解压 ZIP → 解析 → 比对 DB → 事务写入）。
    async fn import_data(
        &self,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, TraitError>;

    /// 清理插件数据（按三元组物理删除所有匹配记录）。
    async fn cleanup_data(
        &self,
        request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, TraitError>;

    /// 查询（导出）插件数据，返回 JSON 序列化的定义列表。
    ///
    /// 按 `request.category` 路由到对应资源的 `list_*` 方法，
    /// 返回序列化后的 JSON 字节（供远程导出场景复用）。
    async fn list_data(
        &self,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataListResult, TraitError>;
}
