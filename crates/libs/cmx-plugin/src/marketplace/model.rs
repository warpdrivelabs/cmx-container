//! 插件市场数据模型
//!
//! 定义插件市场的核心数据结构，对应数据库中的4张表：
//! - `cmx_marketplace_plugin`: 插件主表
//! - `cmx_marketplace_plugin_version`: 版本表
//! - `cmx_marketplace_download_stats`: 下载统计表
//! - `cmx_marketplace_rating`: 评分表

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件市场 - 插件主表模型
///
/// 对应数据库表 `cmx_marketplace_plugin`，存储插件的基本信息、
/// 分类、供应商、统计数据等。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePlugin {
    /// 主键
    pub id: String,
    /// 插件唯一标识
    pub plugin_id: String,
    /// 插件名称
    pub name: Option<String>,
    /// 插件详细描述
    pub description: Option<String>,
    /// 简短描述
    pub short_description: Option<String>,
    /// 图标URL
    pub icon_url: Option<String>,
    /// 分类（如：数据集成、业务逻辑、工具类）
    pub category: Option<String>,
    /// 标签列表（JSON数组）
    pub tags: Option<serde_json::Value>,
    /// 供应商名称
    pub vendor_name: Option<String>,
    /// 供应商主页
    pub vendor_url: Option<String>,
    /// 联系方式
    pub vendor_contact: Option<String>,
    /// 许可证类型（MIT/Apache/Commercial/Free）
    pub license_type: Option<String>,
    /// 插件主页
    pub homepage_url: Option<String>,
    /// 文档地址
    pub documentation_url: Option<String>,
    /// 代码仓库地址
    pub repository_url: Option<String>,
    /// 状态（draft/published/deprecated/archived）
    pub status: Option<String>,
    /// 是否推荐（1是/0否）
    pub is_featured: Option<i16>,
    /// 是否官方插件（1是/0否）
    pub is_official: Option<i16>,
    /// 平均评分（1.00-5.00）
    pub avg_rating: Option<f64>,
    /// 评分数量
    pub rating_count: Option<i32>,
    /// 总下载量
    pub download_count: Option<i64>,
    /// 总安装量
    pub install_count: Option<i64>,
    /// 所属域编码
    pub domain_code: Option<String>,
    /// 所属应用编码
    pub application_code: Option<String>,
    /// 所属模块编码
    pub module_code: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 归档标记（0未归档/1已归档）
    pub archived: Option<i32>,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人姓名
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人姓名
    pub update_name: Option<String>,
}

/// 插件市场 - 版本表模型
///
/// 对应数据库表 `cmx_marketplace_plugin_version`，存储插件的版本信息、
/// 下载地址、兼容性等。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginVersion {
    /// 主键
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 版本号（语义化版本）
    pub version: String,
    /// 版本排序值（用于版本比较）
    pub version_rank: Option<i32>,
    /// 变更日志
    pub changelog: Option<String>,
    /// 发布说明
    pub release_notes: Option<String>,
    /// 下载地址
    pub download_url: Option<String>,
    /// 包大小（字节）
    pub package_size: Option<i64>,
    /// 校验和（SHA256）
    pub checksum: Option<String>,
    /// 最低平台版本要求
    pub min_platform_version: Option<String>,
    /// 最高平台版本要求
    pub max_platform_version: Option<String>,
    /// 依赖列表（JSON数组）
    pub dependencies: Option<serde_json::Value>,
    /// 兼容性信息（JSON对象）
    pub compatibility: Option<serde_json::Value>,
    /// 状态（draft/published/deprecated）
    pub status: Option<String>,
    /// 是否最新版本（1是/0否）
    pub is_latest: Option<i16>,
    /// 是否稳定版（1是/0否）
    pub is_stable: Option<i16>,
    /// 版本下载量
    pub download_count: Option<i64>,
    /// 发布时间
    pub published_at: Option<DateTime<Utc>>,
    /// 归档标记（0未归档/1已归档）
    pub archived: Option<i32>,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人姓名
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人姓名
    pub update_name: Option<String>,
}

/// 插件市场 - 下载统计表模型
///
/// 对应数据库表 `cmx_marketplace_download_stats`，按日期和来源记录下载/安装量。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceDownloadStats {
    /// 主键
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 版本号
    pub version: Option<String>,
    /// 下载日期
    pub download_date: Option<String>,
    /// 当日下载量
    pub download_count: Option<i32>,
    /// 当日安装量
    pub install_count: Option<i32>,
    /// 来源类型（api/cli/marketplace）
    pub source_type: Option<String>,
    /// 地区
    pub region: Option<String>,
    /// 归档标记（0未归档/1已归档）
    pub archived: Option<i32>,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人姓名
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人姓名
    pub update_name: Option<String>,
}

/// 插件市场 - 评分表模型
///
/// 对应数据库表 `cmx_marketplace_rating`，存储用户对插件的评分和评论。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceRating {
    /// 主键
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 用户ID
    pub user_id: String,
    /// 评分（1-5）
    pub rating: Option<i32>,
    /// 评论内容
    pub review: Option<String>,
    /// 状态（pending/approved/rejected）
    pub status: Option<String>,
    /// 归档标记（0未归档/1已归档）
    pub archived: Option<i32>,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人姓名
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人姓名
    pub update_name: Option<String>,
}

/// 插件市场 - 过滤条件
///
/// 用于搜索和分页查询的过滤参数。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MarketplaceFilter {
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

/// 插件市场 - 分类信息
///
/// 用于返回分类列表的统计信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    /// 分类编码
    pub category: String,
    /// 该分类下的插件数量
    pub count: i64,
}
