//! 插件管理 API 响应结构体
//!
//! 定义插件安装、卸载、升级、降级等操作的响应参数

use serde::Serialize;
use utoipa::ToSchema;

/// 插件信息响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfoResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 插件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 描述
    pub description: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 来源类型
    pub source_type: String,
    /// 来源 URL
    pub source_url: Option<String>,
    /// 状态
    pub status: String,
    /// 安装时间
    pub installed_at: Option<String>,
    /// 更新时间
    pub updated_at: Option<String>,
    /// 安装路径
    pub install_path: String,
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
