//! 插件市场 API 响应结构体
//!
//! 定义插件市场相关接口的响应参数，包括插件信息、版本信息、
//! 评分信息、安装结果等。

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// 插件市场插件响应
///
/// 返回插件的基本信息和统计数据
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginResponse {
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
    /// 分类
    pub category: Option<String>,
    /// 标签列表（JSON数组）
    pub tags: Option<serde_json::Value>,
    /// 供应商名称
    pub vendor_name: Option<String>,
    /// 供应商主页
    pub vendor_url: Option<String>,
    /// 联系方式
    pub vendor_contact: Option<String>,
    /// 许可证类型
    pub license_type: Option<String>,
    /// 插件主页
    pub homepage_url: Option<String>,
    /// 文档地址
    pub documentation_url: Option<String>,
    /// 代码仓库地址
    pub repository_url: Option<String>,
    /// 状态
    pub status: Option<String>,
    /// 是否推荐
    pub is_featured: Option<i16>,
    /// 是否官方插件
    pub is_official: Option<i16>,
    /// 平均评分
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

/// 插件市场版本响应
///
/// 返回版本的详细信息和下载地址
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceVersionResponse {
    /// 主键
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 版本号
    pub version: String,
    /// 版本排序值
    pub version_rank: Option<i32>,
    /// 变更日志
    pub changelog: Option<String>,
    /// 发布说明
    pub release_notes: Option<String>,
    /// 下载地址
    pub download_url: Option<String>,
    /// 包大小（字节）
    pub package_size: Option<i64>,
    /// 校验和
    pub checksum: Option<String>,
    /// 最低平台版本要求
    pub min_platform_version: Option<String>,
    /// 最高平台版本要求
    pub max_platform_version: Option<String>,
    /// 依赖列表（JSON数组）
    pub dependencies: Option<serde_json::Value>,
    /// 兼容性信息（JSON对象）
    pub compatibility: Option<serde_json::Value>,
    /// 状态
    pub status: Option<String>,
    /// 是否最新版本
    pub is_latest: Option<i16>,
    /// 是否稳定版
    pub is_stable: Option<i16>,
    /// 版本下载量
    pub download_count: Option<i64>,
    /// 发布时间
    pub published_at: Option<DateTime<Utc>>,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
}

/// 插件市场详情响应
///
/// 返回插件的完整信息，包括基本信息、最新版本和版本列表
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginDetailResponse {
    /// 插件基本信息
    pub plugin: MarketplacePluginResponse,
    /// 最新版本
    pub latest_version: Option<MarketplaceVersionResponse>,
    /// 所有版本列表
    pub versions: Vec<MarketplaceVersionResponse>,
}

/// 从市场安装响应
///
/// 返回安装结果信息
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketInstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: Option<String>,
    /// 安装的版本
    pub version: Option<String>,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}

/// 评分响应
///
/// 返回评分记录信息
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceRatingResponse {
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
    /// 状态
    pub status: Option<String>,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人姓名
    pub create_name: Option<String>,
}

/// 分类信息响应
///
/// 返回分类名称和该分类下的插件数量
#[derive(Debug, Serialize, ToSchema)]
pub struct CategoryResponse {
    /// 分类编码
    pub category: String,
    /// 该分类下的插件数量
    pub count: i64,
}
