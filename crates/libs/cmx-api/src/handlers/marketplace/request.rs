//! 插件市场 API 请求结构体。
//!
//! 定义插件市场所有接口的请求参数，包括发布、更新、删除、安装、评分等请求，
//! 以及过滤器和查询参数。

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// 发布插件到市场的请求。
///
/// 包含插件基本信息和首个版本信息，用于创建新插件或更新已存在插件。
#[derive(Debug, Deserialize, ToSchema)]
pub struct PublishPluginRequest {
    /// 插件唯一标识符（业务 ID，必须全局唯一）。
    pub plugin_id: String,
    /// 插件名称。
    pub name: Option<String>,
    /// 插件详细描述。
    pub description: Option<String>,
    /// 插件简短描述（用于列表展示）。
    pub short_description: Option<String>,
    /// 插件分类。
    pub category: Option<String>,
    /// 插件标签列表（JSON 字符串格式，如：`["tag1","tag2"]`）。
    pub tags: Option<String>,
    /// 开源许可证类型。
    pub license_type: Option<String>,
    /// 插件供应商/厂商名称。
    pub vendor_name: Option<String>,
    /// 插件供应商官网 URL。
    pub vendor_url: Option<String>,
    /// 插件供应商联系信息。
    pub vendor_contact: Option<String>,
    /// 插件主页 URL。
    pub homepage_url: Option<String>,
    /// 插件文档 URL。
    pub documentation_url: Option<String>,
    /// 插件代码仓库 URL。
    pub repository_url: Option<String>,
    /// 插件图标 URL。
    pub icon_url: Option<String>,
    /// 所属域编码。
    pub domain_code: Option<String>,
    /// 所属应用编码。
    pub application_code: Option<String>,
    /// 所属模块编码。
    pub module_code: Option<String>,
    /// 插件类型。
    pub plugin_type: Option<String>,
    /// 插件版本号（必填，遵循语义化版本规范）。
    pub version: String,
    /// 变更日志。
    pub changelog: Option<String>,
    /// 发布说明。
    pub release_notes: Option<String>,
    /// 最低兼容平台版本。
    pub min_platform_version: Option<String>,
    /// 最高兼容平台版本。
    pub max_platform_version: Option<String>,
}

/// 更新市场插件信息的请求。
///
/// 所有字段均为可选，仅更新提供的非 None 字段。
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMarketplacePluginRequest {
    /// 插件唯一标识符（用于定位要更新的插件）。
    pub plugin_id: String,
    /// 插件名称。
    pub name: Option<String>,
    /// 插件详细描述。
    pub description: Option<String>,
    /// 插件简短描述。
    pub short_description: Option<String>,
    /// 插件分类。
    pub category: Option<String>,
    /// 插件标签列表。
    pub tags: Option<String>,
    /// 插件状态。
    pub status: Option<String>,
    /// 是否推荐插件。
    pub is_featured: Option<i16>,
    /// 是否官方插件。
    pub is_official: Option<i16>,
    /// 插件图标 URL。
    pub icon_url: Option<String>,
    /// 开源许可证类型。
    pub license_type: Option<String>,
    /// 插件主页 URL。
    pub homepage_url: Option<String>,
    /// 插件文档 URL。
    pub documentation_url: Option<String>,
    /// 插件代码仓库 URL。
    pub repository_url: Option<String>,
    /// 插件供应商/厂商名称。
    pub vendor_name: Option<String>,
    /// 插件供应商官网 URL。
    pub vendor_url: Option<String>,
    /// 插件供应商联系信息。
    pub vendor_contact: Option<String>,
    /// 所属域编码。
    pub domain_code: Option<String>,
    /// 所属应用编码。
    pub application_code: Option<String>,
    /// 所属模块编码。
    pub module_code: Option<String>,
    /// 插件类型。
    pub plugin_type: Option<String>,
}

/// 删除市场插件的请求。
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteMarketplacePluginRequest {
    /// 要删除的插件唯一标识符。
    pub plugin_id: String,
}

/// 从市场安装插件的请求。
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarketInstallRequest {
    /// 要安装的插件唯一标识符。
    pub plugin_id: String,
    /// 要安装的版本号（不填则安装最新稳定版）。
    pub version: Option<String>,
    /// 安装到的目标数据库 ID（不填则使用默认数据库）。
    pub db_id: Option<String>,
    /// 安装后是否自动激活（默认为 true）。
    #[serde(default = "default_true")]
    pub auto_activate: bool,
}

/// 对插件评分的请求。
#[derive(Debug, Deserialize, ToSchema)]
pub struct RatePluginRequest {
    /// 要评分的插件唯一标识符。
    pub plugin_id: String,
    /// 评分值（1-5 分）。
    pub rating: i32,
    /// 用户评价内容（可选）。
    pub review: Option<String>,
}

/// 从市场升级插件的请求。
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarketUpgradeRequest {
    /// 要升级的插件业务 ID。
    pub plugin_id: String,
    /// 目标版本号，为 `None` 时升级到最新稳定版。
    pub target_version: Option<String>,
    /// 是否强制升级（忽略版本检查）。
    #[serde(default)]
    pub force: bool,
}

/// 检查插件更新的请求。
#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckUpdatesRequest {
    /// 要检查的插件 ID 列表，为 `None` 时检查所有已安装插件。
    pub plugin_ids: Option<Vec<String>>,
}

/// 插件包下载查询参数。
#[derive(Debug, Deserialize, IntoParams)]
pub struct MarketDownloadParams {
    /// 插件业务 ID。
    pub plugin_id: String,
    /// 版本号，为 `None` 时下载最新稳定版。
    pub version: Option<String>,
}

/// 热门插件查询过滤条件。
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct TrendingFilter {
    /// 统计最近 N 天的下载数据（默认 7 天）。
    pub days: Option<i64>,
    /// 返回的插件数量上限（默认 10 个）。
    pub limit: Option<i64>,
}

/// 插件分页查询过滤器。
///
/// 用于插件列表的分页查询，对应 cmx-plugin 中的 MarketplacePluginFilter。
/// 自动将 API 层类型转换为 modql FilterNodes 进行数据库查询。
///
/// - name 字段：模糊匹配
/// - 其他字段：精确匹配
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplacePluginFilterDoc {
    /// 插件唯一标识符（精确匹配）。
    pub plugin_id: Option<String>,
    /// 插件名称（模糊匹配）。
    pub name: Option<String>,
    /// 插件分类（精确匹配）。
    pub category: Option<String>,
    /// 插件状态（精确匹配）。
    pub status: Option<String>,
    /// 所属域编码（精确匹配）。
    pub domain_code: Option<String>,
    /// 所属应用编码（精确匹配）。
    pub application_code: Option<String>,
    /// 所属模块编码（精确匹配）。
    pub module_code: Option<String>,
    /// 插件类型（精确匹配）。
    pub plugin_type: Option<String>,
}

impl From<MarketplacePluginFilterDoc> for cmx_plugin::MarketplacePluginFilter {
    fn from(doc: MarketplacePluginFilterDoc) -> Self {
        use modql::filter::{OpValString, OpValsString};
        Self {
            plugin_id: doc.plugin_id.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            name: doc.name.map(|v| OpValsString(vec![OpValString::Contains(v)])),
            category: doc.category.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            status: doc.status.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            domain_code: doc.domain_code.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            application_code: doc.application_code.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            module_code: doc.module_code.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            plugin_type: doc.plugin_type.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            archived: None,
        }
    }
}

/// 插件版本查询过滤器。
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplacePluginVersionFilterDoc {
    /// 所属插件唯一标识符。
    pub plugin_id: Option<String>,
    /// 版本号。
    pub version: Option<String>,
    /// 版本状态。
    pub status: Option<String>,
}

/// 插件评分查询过滤器。
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplaceRatingFilterDoc {
    /// 所属插件唯一标识符。
    pub plugin_id: Option<String>,
    /// 评分用户 ID。
    pub user_id: Option<String>,
    /// 评分状态。
    pub status: Option<String>,
}

impl From<MarketplaceRatingFilterDoc> for cmx_plugin::MarketplaceRatingFilter {
    fn from(doc: MarketplaceRatingFilterDoc) -> Self {
        use modql::filter::{OpValString, OpValsString};
        Self {
            plugin_id: doc.plugin_id.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            user_id: doc.user_id.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            status: doc.status.map(|v| OpValsString(vec![OpValString::Eq(v)])),
            archived: None,
        }
    }
}

/// 插件详情查询参数。
///
/// 支持通过 `id`（主键）或 `plugin_id`（业务 ID）查询插件详情。
#[derive(Debug, Deserialize, IntoParams)]
pub struct MarketplacePluginGetParams {
    /// 数据库主键 ID。
    pub id: Option<String>,
    /// 插件业务唯一标识符。
    pub plugin_id: Option<String>,
}

/// 插件版本详情查询参数。
#[derive(Debug, Deserialize, IntoParams)]
pub struct MarketplaceVersionGetParams {
    /// 版本主键 ID。
    pub id: Option<String>,
}

/// 返回 true 的默认值（用于 serde default）。
fn default_true() -> bool {
    true
}
