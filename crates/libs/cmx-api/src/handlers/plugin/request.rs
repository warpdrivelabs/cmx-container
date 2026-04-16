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
    pub operator: Option<String>,
}

/// 插件降级请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginDowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 操作者
    pub operator: Option<String>,
}

/// 插件列表查询参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginListQuery {
    /// 状态过滤
    pub status: Option<String>,
}



/// 插件部署请求参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginDeployRequest {
    /// 插件 zip 包文件
    #[schema(content_media_type = "application/octet-stream")]
    pub file: Vec<u8>,

    /// 目标数据库 ID (可选)
    pub target_db_id: Option<String>,

    /// 是否覆盖安装 (可选，默认 false)
    pub force_reinstall: Option<bool>,
}


/// API 层插件过滤条件
///
/// 用于分页查询接口的过滤参数
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct ApiPluginFilter {
    /// 按状态筛选
    pub status: Option<String>,
    /// 按名称筛选（模糊匹配）
    pub name: Option<String>,
    /// 按域编码筛选
    pub domain_code: Option<String>,
    /// 按应用编码筛选
    pub application_code: Option<String>,
    /// 按模块编码筛选
    pub module_code: Option<String>,
}

/// 从 API 层过滤条件转换为 cmx-plugin 层过滤条件
impl From<ApiPluginFilter> for cmx_plugin::domain::plugin::PluginFilter {
    fn from(api_filter: ApiPluginFilter) -> Self {
        Self {
            status: api_filter.status.as_ref().and_then(|s| {
                s.parse::<cmx_plugin::domain::plugin::PluginStatus>().ok()
            }),
            name: api_filter.name,
            domain_code: api_filter.domain_code,
            application_code: api_filter.application_code,
            module_code: api_filter.module_code,
        }
    }
}

/// 插件ID路径参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginIdPath {
    /// 插件ID
    pub plugin_id: String,
}

/// 插件查重查询参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginExistsQuery {
    /// 插件ID
    pub plugin_id: String,
}

/// 插件函数查询参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct PluginFunctionsQuery {
    /// 插件ID
    pub plugin_id: String,
}

/// 批量获取插件函数请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginFunctionsRequest {
    /// 插件ID列表
    pub plugin_ids: Vec<String>,
}
