//! 插件管理 API 请求结构体
//!
//! 定义插件安装、卸载、升级、降级等操作的请求参数

use serde::Deserialize;

/// 插件安装请求
#[derive(Debug, Deserialize)]
pub struct PluginInstallRequest {
    /// 插件ID（可选，如果不提供则从 plugin.json 或 manifest 中获取）
    pub plugin_id: Option<String>,
    /// 插件来源
    pub source: PluginSourceRequest,
    /// 目标数据库ID
    pub target_db_id: Option<String>,
    /// 目标数据库类型
    pub target_db_type: Option<String>,
    /// 目标节点列表
    pub target_nodes: Option<Vec<String>>,
    /// 插件配置
    pub config: Option<serde_json::Value>,
    /// 是否强制安装
    pub force: Option<bool>,
    /// 跳过验证
    pub skip_validation: Option<bool>,
    /// 操作人
    pub operator: String,
}

/// 插件来源请求
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginSourceRequest {
    /// 本地文件
    Local {
        /// 文件路径
        path: String,
    },
    /// 远程URL
    Remote {
        /// URL 地址
        url: String,
        /// 校验和（可选）
        checksum: Option<String>,
    },
    /// 远程注册表
    Registry {
        /// 注册表URL（可选）
        registry_url: Option<String>,
        /// 包名称
        package_name: String,
    },
}

/// 插件卸载请求
#[derive(Debug, Deserialize)]
pub struct PluginUninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制卸载
    pub force: Option<bool>,
}

/// 插件升级请求
#[derive(Debug, Deserialize)]
pub struct PluginUpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 插件来源
    pub source: PluginSourceRequest,
    /// 版本约束（可选）
    pub version_constraint: Option<String>,
    /// 是否强制升级
    pub force: Option<bool>,
    /// 操作人
    pub operator: String,
}

/// 插件降级请求
#[derive(Debug, Deserialize)]
pub struct PluginDowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 操作人
    pub operator: String,
}

/// 插件列表查询参数
#[derive(Debug, Deserialize)]
pub struct PluginListQuery {
    /// 插件状态过滤
    pub status: Option<String>,
    /// 域编码过滤
    pub domain_code: Option<String>,
    /// 应用编码过滤
    pub application_code: Option<String>,
}

/// 插件分页查询参数
#[derive(Debug, Deserialize)]
pub struct PluginPageQuery {
    /// 页码（从1开始）
    pub page: Option<u64>,
    /// 每页条数
    pub page_size: Option<u64>,
    /// 插件状态过滤
    pub status: Option<String>,
    /// 域编码过滤
    pub domain_code: Option<String>,
}

/// 插件ID路径参数
#[derive(Debug, Deserialize)]
pub struct PluginIdPath {
    /// 插件ID
    pub plugin_id: String,
}
