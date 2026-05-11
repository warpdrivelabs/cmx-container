//! 插件市场 API 请求结构体
//!
//! 定义插件市场相关接口的请求参数，包括过滤条件、发布请求、
//! 安装请求、评分请求等。

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// 插件市场过滤条件
///
/// 用于分页查询接口的过滤参数
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplacePluginFilter {
    /// 关键词搜索（名称/描述）
    pub keyword: Option<String>,
    /// 分类过滤
    pub category: Option<String>,
    /// 标签过滤
    pub tags: Option<String>,
    /// 状态过滤（默认 published）
    pub status: Option<String>,
    /// 域编码过滤
    pub domain_code: Option<String>,
    /// 应用编码过滤
    pub application_code: Option<String>,
    /// 模块编码过滤
    pub module_code: Option<String>,
    /// 排序字段（download_count/avg_rating/create_time/update_time）
    pub sort_by: Option<String>,
    /// 排序方向（asc/desc）
    pub sort_order: Option<String>,
}

/// 插件市场版本过滤条件
///
/// 用于版本列表查询的过滤参数
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplaceVersionFilter {
    /// 插件ID
    pub plugin_id: Option<String>,
    /// 状态过滤
    pub status: Option<String>,
}

/// 插件市场评分过滤条件
///
/// 用于评分列表查询的过滤参数
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplaceRatingFilter {
    /// 插件ID
    pub plugin_id: Option<String>,
    /// 状态过滤
    pub status: Option<String>,
}

/// 从市场安装请求
///
/// 指定要安装的插件和版本信息
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarketInstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 版本号（默认最新稳定版）
    pub version: Option<String>,
    /// 目标数据库ID
    pub db_id: Option<String>,
    /// 是否自动激活
    #[serde(default = "default_true")]
    pub auto_activate: bool,
}

/// 发布插件到市场请求
///
/// 包含插件基本信息和首个版本信息
#[derive(Debug, Deserialize, ToSchema)]
pub struct PublishPluginRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 插件名称
    pub name: Option<String>,
    /// 插件描述
    pub description: Option<String>,
    /// 简短描述
    pub short_description: Option<String>,
    /// 分类
    pub category: Option<String>,
    /// 标签（JSON数组字符串）
    pub tags: Option<String>,
    /// 许可证类型
    pub license_type: Option<String>,
    /// 供应商名称
    pub vendor_name: Option<String>,
    /// 供应商URL
    pub vendor_url: Option<String>,
    /// 供应商联系方式
    pub vendor_contact: Option<String>,
    /// 插件主页URL
    pub homepage_url: Option<String>,
    /// 文档URL
    pub documentation_url: Option<String>,
    /// 代码仓库URL
    pub repository_url: Option<String>,
    /// 图标URL
    pub icon_url: Option<String>,
    /// 域编码
    pub domain_code: Option<String>,
    /// 应用编码
    pub application_code: Option<String>,
    /// 模块编码
    pub module_code: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 版本号
    pub version: String,
    /// 下载地址
    pub download_url: Option<String>,
    /// 包大小
    pub package_size: Option<i64>,
    /// 校验和
    pub checksum: Option<String>,
    /// 变更日志
    pub changelog: Option<String>,
    /// 发布说明
    pub release_notes: Option<String>,
    /// 最低平台版本
    pub min_platform_version: Option<String>,
    /// 最高平台版本
    pub max_platform_version: Option<String>,
}

/// 更新插件市场信息请求
///
/// 更新插件的基本信息，所有字段均为可选，仅更新提供的字段
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMarketplacePluginRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 插件名称
    pub name: Option<String>,
    /// 插件描述
    pub description: Option<String>,
    /// 简短描述
    pub short_description: Option<String>,
    /// 分类
    pub category: Option<String>,
    /// 标签（JSON数组字符串）
    pub tags: Option<String>,
    /// 状态
    pub status: Option<String>,
    /// 是否推荐
    pub is_featured: Option<i16>,
    /// 是否官方
    pub is_official: Option<i16>,
    /// 图标URL
    pub icon_url: Option<String>,
    /// 许可证类型
    pub license_type: Option<String>,
    /// 主页URL
    pub homepage_url: Option<String>,
    /// 文档URL
    pub documentation_url: Option<String>,
    /// 仓库URL
    pub repository_url: Option<String>,
    /// 供应商名称
    pub vendor_name: Option<String>,
    /// 供应商URL
    pub vendor_url: Option<String>,
    /// 供应商联系方式
    pub vendor_contact: Option<String>,
}

/// 删除插件市场信息请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteMarketplacePluginRequest {
    /// 插件ID
    pub plugin_id: String,
}

/// 评分请求
///
/// 对指定插件进行评分和评论
#[derive(Debug, Deserialize, ToSchema)]
pub struct RatePluginRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 评分（1-5）
    pub rating: i32,
    /// 评论内容
    pub review: Option<String>,
}

/// 热门插件过滤条件
///
/// 用于热门插件列表查询
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct TrendingFilter {
    /// 统计天数（默认7天）
    pub days: Option<i64>,
    /// 返回数量限制（默认10）
    pub limit: Option<i64>,
}

/// 分类过滤条件
///
/// 用于分类列表查询
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct MarketplaceCategoryFilter {}

/// 查询参数（get_by_id 使用）
///
/// 用于通过 id 或 plugin_id 查询单条记录
#[derive(Debug, Deserialize, IntoParams)]
pub struct MarketplacePluginGetParams {
    /// 主键ID
    pub id: Option<String>,
    /// 插件plugin_id
    pub plugin_id: Option<String>,
}

/// 版本详情查询参数
#[derive(Debug, Deserialize, IntoParams)]
pub struct MarketplaceVersionGetParams {
    /// 版本主键ID
    pub id: Option<String>,
}

/// 默认值函数：返回 true
fn default_true() -> bool {
    true
}
