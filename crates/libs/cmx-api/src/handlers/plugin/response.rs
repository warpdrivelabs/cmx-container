//! 插件管理 API 响应结构体
//!
//! 定义插件安装、卸载、升级、降级等操作的响应参数

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// 插件信息响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfoResponse {
    /// 主键ID
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 插件名称
    pub name: String,
    /// 插件描述
    pub  description: Option<String>,
    /// 版本
    pub version: String,
    /// WASM 文件路径
    pub wasm_path: Option<String>,
    /// 安装路径
    pub install_path: String,

    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 数据库ID
    pub db_id: Option<String>,
    /// 状态
    pub status: String,
    /// 是否系统插件
    pub is_system: bool,
    /// 是否锁定
    pub is_locked: bool,
    /// 域编码
    pub domain_code: Option<String>,
    /// 应用编码
    pub application_code: Option<String>,
    /// 模块编码
    pub module_code: Option<String>,
    /// 域名称
    pub domain_name: Option<String>,
    /// 应用名称
    pub application_name: Option<String>,
    /// 模块名称
    pub module_name: Option<String>,

    /// 开发商名称
    pub vendor_name: Option<String>,
    /// 开发商URL
    pub vendor_url: Option<String>,
    /// 开发商联系方式
    pub vendor_contact: Option<String>,
    /// 扩展元数据
    pub metadata: Option<serde_json::Value>,
    /// 来源类型: local, url, registry
    pub source_type: Option<String>,
    /// 来源URL
    pub source_url: Option<String>,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 插件列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct PluginListResponse {
    /// 插件列表
    pub plugins: Vec<PluginInfoResponse>,
}

/// 安装响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: String,
    /// 插件版本
    pub version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}

/// 卸载响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}

/// 升级响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 旧版本
    pub old_version: String,
    /// 新版本
    pub new_version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}

/// 降级响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DowngradeResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 旧版本
    pub old_version: String,
    /// 目标版本
    pub target_version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}

/// 插件部署响应（自动判断安装/升级/覆盖安装）
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDeployResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 操作类型: "install" | "upgrade" | "reinstall" | "already_installed"
    pub action: String,
    /// 旧版本（仅 upgrade/reinstall 时有值）
    pub old_version: Option<String>,
    /// 新版本
    pub new_version: String,
    /// 安装路径
    pub install_path: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}

/// 插件函数响应
///
/// 用于返回插件 api.json 的内容
#[derive(Debug, Serialize, ToSchema)]
pub struct PluginFunctionsResponse {
    /// 是否成功获取插件函数
    pub success: bool,
    /// 插件函数列表（JSON 格式的 api.json 内容）
    pub functions: serde_json::Value,
}
