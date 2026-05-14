//! 插件市场数据模型。
//!
//! 提供插件市场的所有数据结构定义，包括数据库实体、创建/更新 DTO、
//! 查询过滤器及表映射层。

use chrono::{DateTime, Utc};
use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::{Deserialize, Serialize};

use cmx_database::crud::DbBmc;

// ============================================================================
// 插件主表实体
// ============================================================================

/// 插件市场中的插件完整实体。
///
/// 对应数据库表 `cmx_marketplace_plugin`，包含插件的所有静态信息和统计数据。
/// 仅用于查询返回，不用于创建/更新操作。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePlugin {
    /// 主键 ID（雪花算法生成）。
    pub id: String,
    /// 插件唯一标识符（业务 ID，由插件开发者指定）。
    pub plugin_id: String,
    /// 插件名称。
    pub name: Option<String>,
    /// 插件详细描述。
    pub description: Option<String>,
    /// 插件简短描述（用于列表展示）。
    pub short_description: Option<String>,
    /// 插件图标 URL。
    pub icon_url: Option<String>,
    /// 插件分类（如：工具类、安全类、监控类等）。
    pub category: Option<String>,
    /// 插件标签列表（JSON 数组格式）。
    pub tags: Option<serde_json::Value>,
    /// 插件供应商/厂商名称。
    pub vendor_name: Option<String>,
    /// 插件供应商官网 URL。
    pub vendor_url: Option<String>,
    /// 插件供应商联系信息。
    pub vendor_contact: Option<String>,
    /// 开源许可证类型（如：MIT、Apache-2.0、GPL-3.0 等）。
    pub license_type: Option<String>,
    /// 插件主页 URL。
    pub homepage_url: Option<String>,
    /// 插件文档 URL。
    pub documentation_url: Option<String>,
    /// 插件代码仓库 URL。
    pub repository_url: Option<String>,
    /// 插件状态（draft/published/archived）。
    pub status: Option<String>,
    /// 是否推荐插件（1-推荐，0-普通）。
    pub is_featured: Option<i16>,
    /// 是否官方插件（1-官方，0-第三方）。
    pub is_official: Option<i16>,
    /// 插件平均评分（1-5 分）。
    pub avg_rating: Option<f64>,
    /// 评分次数。
    pub rating_count: Option<i32>,
    /// 插件总下载次数。
    pub download_count: Option<i64>,
    /// 插件总安装次数。
    pub install_count: Option<i64>,
    /// 所属域编码。
    pub domain_code: Option<String>,
    /// 所属应用编码。
    pub application_code: Option<String>,
    /// 所属模块编码。
    pub module_code: Option<String>,
    /// 插件类型（如：module、component、integration 等）。
    pub plugin_type: Option<String>,
    /// 归档状态（0-未归档，1-已归档）。
    pub archived: Option<i32>,
    /// 创建时间。
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间。
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人用户 ID。
    pub create_by: Option<String>,
    /// 创建人用户名。
    pub create_name: Option<String>,
    /// 更新人用户 ID。
    pub update_by: Option<String>,
    /// 更新人用户名。
    pub update_name: Option<String>,
}

/// 插件创建请求 DTO。
///
/// 用于发布新插件到市场，包含插件基本信息。
/// 不包含自动生成字段（id、create_time 等）和统计字段（download_count 等）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct MarketplacePluginForCreate {
    /// 插件唯一标识符（业务 ID，必须全局唯一）。
    pub plugin_id: String,
    /// 插件名称。
    pub name: Option<String>,
    /// 插件详细描述。
    pub description: Option<String>,
    /// 插件简短描述。
    pub short_description: Option<String>,
    /// 插件图标 URL。
    pub icon_url: Option<String>,
    /// 插件分类。
    pub category: Option<String>,
    /// 插件标签列表（JSON 字符串格式，如：`["tag1","tag2"]`）。
    pub tags: Option<String>,
    /// 插件供应商/厂商名称。
    pub vendor_name: Option<String>,
    /// 插件供应商官网 URL。
    pub vendor_url: Option<String>,
    /// 插件供应商联系信息。
    pub vendor_contact: Option<String>,
    /// 开源许可证类型。
    pub license_type: Option<String>,
    /// 插件主页 URL。
    pub homepage_url: Option<String>,
    /// 插件文档 URL。
    pub documentation_url: Option<String>,
    /// 插件代码仓库 URL。
    pub repository_url: Option<String>,
    /// 插件状态（发布时自动设为 published）。
    pub status: Option<String>,
    /// 是否推荐插件。
    pub is_featured: Option<i16>,
    /// 是否官方插件。
    pub is_official: Option<i16>,
    /// 所属域编码。
    pub domain_code: Option<String>,
    /// 所属应用编码。
    pub application_code: Option<String>,
    /// 所属模块编码。
    pub module_code: Option<String>,
    /// 插件类型。
    pub plugin_type: Option<String>,
}

/// 插件更新请求 DTO。
///
/// 用于更新已存在的插件信息。所有字段均为可选，
/// 仅更新提供的非 None 字段。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct MarketplacePluginForUpdate {
    /// 插件名称。
    pub name: Option<String>,
    /// 插件详细描述。
    pub description: Option<String>,
    /// 插件简短描述。
    pub short_description: Option<String>,
    /// 插件图标 URL。
    pub icon_url: Option<String>,
    /// 插件分类。
    pub category: Option<String>,
    /// 插件标签列表（JSON 字符串格式）。
    pub tags: Option<String>,
    /// 插件供应商/厂商名称。
    pub vendor_name: Option<String>,
    /// 插件供应商官网 URL。
    pub vendor_url: Option<String>,
    /// 插件供应商联系信息。
    pub vendor_contact: Option<String>,
    /// 开源许可证类型。
    pub license_type: Option<String>,
    /// 插件主页 URL。
    pub homepage_url: Option<String>,
    /// 插件文档 URL。
    pub documentation_url: Option<String>,
    /// 插件代码仓库 URL。
    pub repository_url: Option<String>,
    /// 插件状态。
    pub status: Option<String>,
    /// 是否推荐插件。
    pub is_featured: Option<i16>,
    /// 是否官方插件。
    pub is_official: Option<i16>,
    /// 所属域编码。
    pub domain_code: Option<String>,
    /// 所属应用编码。
    pub application_code: Option<String>,
    /// 所属模块编码。
    pub module_code: Option<String>,
    /// 插件类型。
    pub plugin_type: Option<String>,
}

// ============================================================================
// 插件版本表实体
// ============================================================================

/// 插件版本实体。
///
/// 对应数据库表 `cmx_marketplace_plugin_version`，
/// 记录每个插件的版本历史信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginVersion {
    /// 主键 ID。
    pub id: String,
    /// 所属插件唯一标识符。
    pub plugin_id: String,
    /// 版本号（遵循语义化版本规范，如：1.0.0、2.1.3）。
    pub version: String,
    /// 版本排序值（用于版本列表排序，数值越大越新）。
    pub version_rank: Option<i32>,
    /// 变更日志。
    pub changelog: Option<String>,
    /// 发布说明。
    pub release_notes: Option<String>,
    /// 插件包下载 URL。
    pub download_url: Option<String>,
    /// 插件包大小（字节）。
    pub package_size: Option<i64>,
    /// 插件包校验和（SHA256，用于安全验证）。
    pub checksum: Option<String>,
    /// 最低兼容平台版本。
    pub min_platform_version: Option<String>,
    /// 最高兼容平台版本。
    pub max_platform_version: Option<String>,
    /// 插件依赖列表（JSON 对象格式）。
    pub dependencies: Option<serde_json::Value>,
    /// 插件兼容性信息（JSON 对象格式）。
    pub compatibility: Option<serde_json::Value>,
    /// 版本状态（draft/published/archived）。
    pub status: Option<String>,
    /// 是否为最新版本（1-最新，0-非最新）。
    pub is_latest: Option<i16>,
    /// 是否为稳定版本（1-稳定，0-测试版）。
    pub is_stable: Option<i16>,
    /// 该版本下载次数。
    pub download_count: Option<i64>,
    /// 发布时间。
    pub published_at: Option<DateTime<Utc>>,
    /// 归档状态。
    pub archived: Option<i32>,
    /// 创建时间。
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间。
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人用户 ID。
    pub create_by: Option<String>,
    /// 创建人用户名。
    pub create_name: Option<String>,
    /// 更新人用户 ID。
    pub update_by: Option<String>,
    /// 更新人用户名。
    pub update_name: Option<String>,
}

/// 插件版本创建请求 DTO。
///
/// 用于创建新的插件版本。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct MarketplacePluginVersionForCreate {
    /// 所属插件唯一标识符。
    pub plugin_id: String,
    /// 版本号。
    pub version: String,
    /// 版本排序值。
    pub version_rank: Option<i32>,
    /// 变更日志。
    pub changelog: Option<String>,
    /// 发布说明。
    pub release_notes: Option<String>,
    /// 插件包下载 URL。
    pub download_url: Option<String>,
    /// 插件包大小（字节）。
    pub package_size: Option<i64>,
    /// 插件包校验和（SHA256）。
    pub checksum: Option<String>,
    /// 最低兼容平台版本。
    pub min_platform_version: Option<String>,
    /// 最高兼容平台版本。
    pub max_platform_version: Option<String>,
    /// 插件依赖列表（JSON 字符串格式）。
    pub dependencies: Option<String>,
    /// 插件兼容性信息（JSON 字符串格式）。
    pub compatibility: Option<String>,
    /// 版本状态。
    pub status: Option<String>,
    /// 是否为最新版本。
    pub is_latest: Option<i16>,
    /// 是否为稳定版本。
    pub is_stable: Option<i16>,
}

// ============================================================================
// 下载统计表实体
// ============================================================================

/// 插件下载统计实体。
///
/// 对应数据库表 `cmx_marketplace_download_stats`，
/// 按日期、来源类型记录插件的下载和安装统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceDownloadStats {
    /// 主键 ID。
    pub id: String,
    /// 插件唯一标识符。
    pub plugin_id: String,
    /// 插件版本号。
    pub version: Option<String>,
    /// 统计日期（格式：YYYY-MM-DD）。
    pub download_date: Option<String>,
    /// 当日下载次数。
    pub download_count: Option<i32>,
    /// 当日安装次数。
    pub install_count: Option<i32>,
    /// 下载来源类型（如：marketplace、url、api 等）。
    pub source_type: Option<String>,
    /// 下载地区。
    pub region: Option<String>,
    /// 归档状态。
    pub archived: Option<i32>,
    /// 创建时间。
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间。
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人用户 ID。
    pub create_by: Option<String>,
    /// 创建人用户名。
    pub create_name: Option<String>,
    /// 更新人用户 ID。
    pub update_by: Option<String>,
    /// 更新人用户名。
    pub update_name: Option<String>,
}

// ============================================================================
// 评分表实体
// ============================================================================

/// 插件评分实体。
///
/// 对应数据库表 `cmx_marketplace_rating`，
/// 记录用户对插件的评分和评价。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceRating {
    /// 主键 ID。
    pub id: String,
    /// 插件唯一标识符。
    pub plugin_id: String,
    /// 评分用户 ID。
    pub user_id: String,
    /// 评分值（1-5 分）。
    pub rating: Option<i32>,
    /// 用户评价内容。
    pub review: Option<String>,
    /// 评分状态（pending/approved/rejected）。
    pub status: Option<String>,
    /// 归档状态。
    pub archived: Option<i32>,
    /// 创建时间。
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间。
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人用户 ID。
    pub create_by: Option<String>,
    /// 创建人用户名。
    pub create_name: Option<String>,
    /// 更新人用户 ID。
    pub update_by: Option<String>,
    /// 更新人用户名。
    pub update_name: Option<String>,
}

/// 插件评分创建请求 DTO。
///
/// 用于用户对插件进行评分。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct MarketplaceRatingForCreate {
    /// 插件唯一标识符。
    pub plugin_id: String,
    /// 评分用户 ID。
    pub user_id: String,
    /// 评分值（1-5 分）。
    pub rating: Option<i32>,
    /// 用户评价内容。
    pub review: Option<String>,
    /// 评分状态。
    pub status: Option<String>,
}

// ============================================================================
// 分类统计
// ============================================================================

/// 插件分类统计信息。
///
/// 用于返回各分类下的插件数量统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    /// 分类名称。
    pub category: String,
    /// 该分类下的插件数量。
    pub count: i64,
}

// ============================================================================
// 查询过滤器（用于 modql + GenericCrudService）
// ============================================================================

/// 插件查询过滤器。
///
/// 支持多条件组合查询，使用 modql 的 FilterNodes 实现。
/// 各字段之间为 AND 关系，字段内多个值之间为 OR 关系。
///
/// # Examples
///
/// ```ignore
/// let filter = MarketplacePluginFilter {
///     name: Some(OpValsString(vec![OpValString::Contains("test".to_string())])),
///     status: Some(OpValsString(vec![OpValString::Eq("published".to_string())])),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct MarketplacePluginFilter {
    /// 插件唯一标识符（精确匹配）。
    pub plugin_id: Option<OpValsString>,
    /// 插件名称（模糊匹配）。
    pub name: Option<OpValsString>,
    /// 插件分类（精确匹配）。
    pub category: Option<OpValsString>,
    /// 插件状态（精确匹配）。
    pub status: Option<OpValsString>,
    /// 所属域编码（精确匹配）。
    pub domain_code: Option<OpValsString>,
    /// 所属应用编码（精确匹配）。
    pub application_code: Option<OpValsString>,
    /// 所属模块编码（精确匹配）。
    pub module_code: Option<OpValsString>,
    /// 插件类型（精确匹配）。
    pub plugin_type: Option<OpValsString>,
    /// 归档状态（精确匹配，默认查询未归档）。
    pub archived: Option<OpValsInt64>,
}

/// 插件版本查询过滤器。
///
/// # Examples
///
/// ```ignore
/// let filter = MarketplacePluginVersionFilter {
///     plugin_id: Some(OpValsString(vec![OpValString::Eq("my-plugin".to_string())])),
///     version: Some(OpValsString(vec![OpValString::Eq("1.0.0".to_string())])),
///     status: Some(OpValsString(vec![OpValString::Eq("published".to_string())])),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct MarketplacePluginVersionFilter {
    /// 所属插件唯一标识符（精确匹配）。
    pub plugin_id: Option<OpValsString>,
    /// 版本号（精确匹配）。
    pub version: Option<OpValsString>,
    /// 版本状态（精确匹配）。
    pub status: Option<OpValsString>,
    /// 归档状态（精确匹配）。
    pub archived: Option<OpValsInt64>,
}

/// 插件评分查询过滤器。
///
/// # Examples
///
/// ```ignore
/// let filter = MarketplaceRatingFilter {
///     plugin_id: Some(OpValsString(vec![OpValString::Eq("my-plugin".to_string())])),
///     user_id: Some(OpValsString(vec![OpValString::Eq("user123".to_string())])),
///     status: Some(OpValsString(vec![OpValString::Eq("approved".to_string())])),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct MarketplaceRatingFilter {
    /// 所属插件唯一标识符（精确匹配）。
    pub plugin_id: Option<OpValsString>,
    /// 评分用户 ID（精确匹配）。
    pub user_id: Option<OpValsString>,
    /// 评分状态（精确匹配）。
    pub status: Option<OpValsString>,
    /// 归档状态（精确匹配）。
    pub archived: Option<OpValsInt64>,
}

// ============================================================================
// 表映射层（实现 DbBmc trait）
// ============================================================================

/// 插件主表映射。
pub struct MarketplacePluginBmc;

impl DbBmc for MarketplacePluginBmc {
    const TABLE: &'static str = "cmx_marketplace_plugin";
    const PK_COLUMN: &'static str = "id";
}

/// 插件版本表映射。
pub struct MarketplacePluginVersionBmc;

impl DbBmc for MarketplacePluginVersionBmc {
    const TABLE: &'static str = "cmx_marketplace_plugin_version";
    const PK_COLUMN: &'static str = "id";
}

/// 插件评分表映射。
pub struct MarketplaceRatingBmc;

impl DbBmc for MarketplaceRatingBmc {
    const TABLE: &'static str = "cmx_marketplace_rating";
    const PK_COLUMN: &'static str = "id";
}
