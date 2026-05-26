/*
//! 插件管控 API 请求结构体
//!
//! 定义管控部署、安装、升级、降级、卸载操作的请求参数

use serde::Deserialize;
use utoipa::ToSchema;

/// 管控部署请求（multipart 上传）
#[derive(Debug, Deserialize, ToSchema)]
pub struct ControlDeployRequest {
    /// 目标数据库ID
    pub target_db_id: Option<String>,
    /// 构建类型（debug/release）
    pub build_type: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控安装请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ControlInstallRequest {
    /// 插件来源
    pub source: super::super::request::PluginSourceRequest,
    /// 目标数据库ID
    pub target_db_id: Option<String>,
    /// 构建类型
    pub build_type: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控升级请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ControlUpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本（用于版本一致性校验）
    pub target_version: String,
    /// 插件来源
    pub source: super::super::request::PluginSourceRequest,
    /// 构建类型
    pub build_type: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控降级请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ControlDowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控卸载请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ControlUninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 应用ID
    pub app_id: Option<String>,
}
*/