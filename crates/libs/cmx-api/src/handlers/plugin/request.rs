//! 插件管理 API 请求结构体
//!
//! 定义插件安装、卸载、升级、降级等操作的请求参数

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// 插件安装请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginInstallRequest {
    /// 插件来源
    pub source: PluginSourceRequest,
    /// 目标数据库ID
    pub target_db_id: Option<String>,
}

/// 插件来源请求
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginSourceRequest {
    /// 本地路径
    Local {
        /// 本地文件路径
        path: String,
    },
    /// 远程 URL
    Remote {
        /// 远程 URL
        url: String,
        /// 校验和
        checksum: Option<String>,
    },
    /// 注册表
    Registry {
        /// 注册表 URL
        registry_url: String,
        /// 包名
        package_name: String,
    },
}

/// 插件卸载请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginUninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制卸载
    pub force: Option<bool>,
}

/// 插件升级请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginUpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 插件来源
    pub source: PluginSourceRequest,
    /// 版本约束
    pub version_constraint: Option<String>,
    /// 是否强制升级
    pub force: Option<bool>,
    /// 操作者
    pub operator: String,
}

/// 插件降级请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginDowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 操作者
    pub operator: String,
}

/// 插件列表查询参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginListQuery {
    /// 状态过滤
    pub status: Option<String>,
}

/// 插件分页查询参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginPageQuery {
    /// 页码
    pub page: Option<u64>,
    /// 每页数量
    pub page_size: Option<u64>,
}

/// 插件ID路径参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginIdPath {
    /// 插件ID
    pub plugin_id: String,
}
