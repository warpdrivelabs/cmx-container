//! 插件市场业务服务
//!
//! 提供插件市场的核心业务逻辑，包括：
//! - 发布插件到市场
//! - 搜索和查询插件
//! - 获取插件详情
//! - 评分功能
//! - 分类查询
//!
//! 安装逻辑由 API handler 层通过 PluginManager 间接完成，
//! 本服务只负责市场数据操作。

use std::sync::Arc;

use tracing::{debug, info};
use uuid::Uuid;

use super::model::{
    CategoryInfo, MarketplaceFilter, MarketplacePlugin, MarketplacePluginVersion, MarketplaceRating,
};
use super::repository::MarketplaceRepository;
use super::stats::StatsService;
use crate::error::{PluginError, PluginResult};

/// 插件市场服务
///
/// 封装插件市场的核心业务逻辑，协调 Repository 和 StatsService 完成操作。
pub struct MarketplaceService {
    /// 市场数据仓库
    repo: Arc<MarketplaceRepository>,
    /// 统计服务
    stats_service: Arc<StatsService>,
}

impl MarketplaceService {
    /// 创建新的市场服务
    ///
    /// # 参数
    /// * `repo` - 市场数据仓库
    /// * `stats_service` - 统计服务
    pub fn new(repo: Arc<MarketplaceRepository>, stats_service: Arc<StatsService>) -> Self {
        Self { repo, stats_service }
    }

    /// 发布插件到市场
    ///
    /// 创建插件主记录和版本记录，状态设为 published。
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    /// * `name` - 插件名称
    /// * `description` - 插件描述
    /// * `short_description` - 简短描述
    /// * `category` - 分类
    /// * `tags` - 标签（JSON数组字符串）
    /// * `license_type` - 许可证类型
    /// * `vendor_name` - 供应商名称
    /// * `vendor_url` - 供应商URL
    /// * `vendor_contact` - 联系方式
    /// * `homepage_url` - 主页URL
    /// * `documentation_url` - 文档URL
    /// * `repository_url` - 仓库URL
    /// * `icon_url` - 图标URL
    /// * `domain_code` - 域编码
    /// * `application_code` - 应用编码
    /// * `module_code` - 模块编码
    /// * `plugin_type` - 插件类型
    /// * `version` - 版本号
    /// * `download_url` - 下载地址
    /// * `package_size` - 包大小
    /// * `checksum` - 校验和
    /// * `changelog` - 变更日志
    /// * `release_notes` - 发布说明
    /// * `min_platform_version` - 最低平台版本
    /// * `max_platform_version` - 最高平台版本
    /// * `create_by` - 创建人ID
    /// * `create_name` - 创建人姓名
    pub async fn publish_plugin(
        &self,
        plugin_id: String,
        name: Option<String>,
        description: Option<String>,
        short_description: Option<String>,
        category: Option<String>,
        tags: Option<String>,
        license_type: Option<String>,
        vendor_name: Option<String>,
        vendor_url: Option<String>,
        vendor_contact: Option<String>,
        homepage_url: Option<String>,
        documentation_url: Option<String>,
        repository_url: Option<String>,
        icon_url: Option<String>,
        domain_code: Option<String>,
        application_code: Option<String>,
        module_code: Option<String>,
        plugin_type: Option<String>,
        version: String,
        download_url: Option<String>,
        package_size: Option<i64>,
        checksum: Option<String>,
        changelog: Option<String>,
        release_notes: Option<String>,
        min_platform_version: Option<String>,
        max_platform_version: Option<String>,
        create_by: Option<String>,
        create_name: Option<String>,
    ) -> PluginResult<MarketplacePlugin> {
        info!("发布插件到市场: plugin_id={}, version={}", plugin_id, version);

        // 检查插件是否已存在
        let existing = self.repo.get_plugin_by_plugin_id(&plugin_id).await?;

        let plugin = if let Some(mut existing_plugin) = existing {
            // 插件已存在，更新信息
            self.repo.update_plugin(
                &plugin_id,
                name.as_deref(),
                description.as_deref(),
                short_description.as_deref(),
                category.as_deref(),
                tags.as_deref(),
                Some("published"),
                None,
                None,
                icon_url.as_deref(),
                license_type.as_deref(),
                homepage_url.as_deref(),
                documentation_url.as_deref(),
                repository_url.as_deref(),
                vendor_name.as_deref(),
                vendor_url.as_deref(),
                vendor_contact.as_deref(),
            ).await?;

            existing_plugin.name = name.or(existing_plugin.name);
            existing_plugin.status = Some("published".to_string());
            existing_plugin
        } else {
            // 新插件，创建记录
            let tags_value = tags.and_then(|s| serde_json::from_str(&s).ok());
            let plugin = MarketplacePlugin {
                id: Uuid::new_v4().to_string(),
                plugin_id: plugin_id.clone(),
                name,
                description,
                short_description,
                icon_url,
                category,
                tags: tags_value,
                vendor_name,
                vendor_url,
                vendor_contact,
                license_type,
                homepage_url,
                documentation_url,
                repository_url,
                status: Some("published".to_string()),
                is_featured: Some(0),
                is_official: Some(0),
                avg_rating: None,
                rating_count: Some(0),
                download_count: Some(0),
                install_count: Some(0),
                domain_code,
                application_code,
                module_code,
                plugin_type,
                archived: Some(0),
                create_time: None,
                update_time: None,
                create_by: create_by.clone(),
                create_name: create_name.clone(),
                update_by: create_by.clone(),
                update_name: create_name.clone(),
            };

            self.repo.insert_plugin(&plugin).await?;
            plugin
        };

        // 创建版本记录
        let version_record = MarketplacePluginVersion {
            id: Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            version: version.clone(),
            version_rank: Some(0),
            changelog,
            release_notes,
            download_url,
            package_size,
            checksum,
            min_platform_version,
            max_platform_version,
            dependencies: None,
            compatibility: None,
            status: Some("published".to_string()),
            is_latest: Some(1),
            is_stable: Some(1),
            download_count: Some(0),
            published_at: None,
            archived: Some(0),
            create_time: None,
            update_time: None,
            create_by,
            create_name,
            update_by: None,
            update_name: None,
        };

        self.repo.insert_version(&version_record).await?;

        Ok(plugin)
    }

    /// 更新插件市场信息
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    /// * `name` - 插件名称
    /// * `description` - 描述
    /// * `short_description` - 简短描述
    /// * `category` - 分类
    /// * `tags` - 标签
    /// * `status` - 状态
    /// * `is_featured` - 是否推荐
    /// * `is_official` - 是否官方
    /// * `icon_url` - 图标URL
    /// * `license_type` - 许可证类型
    /// * `homepage_url` - 主页URL
    /// * `documentation_url` - 文档URL
    /// * `repository_url` - 仓库URL
    /// * `vendor_name` - 供应商名称
    /// * `vendor_url` - 供应商URL
    /// * `vendor_contact` - 联系方式
    pub async fn update_plugin(
        &self,
        plugin_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        short_description: Option<&str>,
        category: Option<&str>,
        tags: Option<&str>,
        status: Option<&str>,
        is_featured: Option<i16>,
        is_official: Option<i16>,
        icon_url: Option<&str>,
        license_type: Option<&str>,
        homepage_url: Option<&str>,
        documentation_url: Option<&str>,
        repository_url: Option<&str>,
        vendor_name: Option<&str>,
        vendor_url: Option<&str>,
        vendor_contact: Option<&str>,
    ) -> PluginResult<()> {
        debug!("更新插件市场信息: plugin_id={}", plugin_id);

        self.repo.update_plugin(
            plugin_id, name, description, short_description, category,
            tags, status, is_featured, is_official, icon_url,
            license_type, homepage_url, documentation_url, repository_url,
            vendor_name, vendor_url, vendor_contact,
        ).await?;

        Ok(())
    }

    /// 删除插件（逻辑删除）
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    pub async fn delete_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        info!("删除市场插件: plugin_id={}", plugin_id);
        self.repo.delete_plugin(plugin_id).await
    }

    /// 分页查询插件
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `page` - 页码
    /// * `size` - 每页大小
    pub async fn page_plugins(
        &self,
        filter: &MarketplaceFilter,
        page: u64,
        size: u64,
    ) -> PluginResult<(Vec<MarketplacePlugin>, u64)> {
        debug!("分页查询市场插件: page={}, size={}", page, size);
        self.repo.page_plugins(filter, page, size).await
    }

    /// 根据 plugin_id 获取插件详情
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    pub async fn get_plugin_by_plugin_id(&self, plugin_id: &str) -> PluginResult<Option<MarketplacePlugin>> {
        debug!("查询市场插件详情: plugin_id={}", plugin_id);
        self.repo.get_plugin_by_plugin_id(plugin_id).await
    }

    /// 根据 id 获取插件详情
    ///
    /// # 参数
    /// * `id` - 主键
    pub async fn get_plugin_by_id(&self, id: &str) -> PluginResult<Option<MarketplacePlugin>> {
        debug!("查询市场插件详情: id={}", id);
        self.repo.get_plugin_by_id(id).await
    }

    /// 获取插件版本列表
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    pub async fn get_plugin_versions(&self, plugin_id: &str) -> PluginResult<Vec<MarketplacePluginVersion>> {
        debug!("查询插件版本列表: plugin_id={}", plugin_id);
        self.repo.list_versions_by_plugin_id(plugin_id).await
    }

    /// 获取版本详情
    ///
    /// # 参数
    /// * `id` - 版本主键
    pub async fn get_version_by_id(&self, id: &str) -> PluginResult<Option<MarketplacePluginVersion>> {
        debug!("查询版本详情: id={}", id);
        self.repo.get_version_by_id(id).await
    }

    /// 获取最新稳定版本
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    pub async fn get_latest_stable_version(&self, plugin_id: &str) -> PluginResult<Option<MarketplacePluginVersion>> {
        self.repo.get_latest_stable_version(plugin_id).await
    }

    /// 获取指定版本
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `version` - 版本号
    pub async fn get_version(&self, plugin_id: &str, version: &str) -> PluginResult<Option<MarketplacePluginVersion>> {
        self.repo.get_version(plugin_id, version).await
    }

    /// 评分
    ///
    /// 创建或更新评分记录，并更新插件评分汇总。
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `user_id` - 用户ID
    /// * `rating` - 评分（1-5）
    /// * `review` - 评论内容
    /// * `create_by` - 创建人ID
    /// * `create_name` - 创建人姓名
    pub async fn rate_plugin(
        &self,
        plugin_id: &str,
        user_id: &str,
        rating: i32,
        review: Option<&str>,
        create_by: Option<&str>,
        create_name: Option<&str>,
    ) -> PluginResult<()> {
        info!("插件评分: plugin_id={}, user_id={}, rating={}", plugin_id, user_id, rating);

        // 校验评分范围
        if !(1..=5).contains(&rating) {
            return Err(PluginError::Plugin(format!("评分必须在1-5之间，当前值: {}", rating)));
        }

        // 检查插件是否存在
        let plugin = self.repo.get_plugin_by_plugin_id(plugin_id).await?;
        if plugin.is_none() {
            return Err(PluginError::NotFound(format!("插件 '{}' 不存在", plugin_id)));
        }

        let rating_record = MarketplaceRating {
            id: Uuid::new_v4().to_string(),
            plugin_id: plugin_id.to_string(),
            user_id: user_id.to_string(),
            rating: Some(rating),
            review: review.map(|s| s.to_string()),
            status: Some("approved".to_string()),
            archived: Some(0),
            create_time: None,
            update_time: None,
            create_by: create_by.map(|s| s.to_string()),
            create_name: create_name.map(|s| s.to_string()),
            update_by: create_by.map(|s| s.to_string()),
            update_name: create_name.map(|s| s.to_string()),
        };

        self.repo.upsert_rating(&rating_record).await?;

        // 更新评分汇总
        self.repo.update_rating_summary(plugin_id).await?;

        Ok(())
    }

    /// 获取评分列表
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `status` - 评分状态过滤
    pub async fn list_ratings(
        &self,
        plugin_id: &str,
        status: Option<&str>,
    ) -> PluginResult<Vec<MarketplaceRating>> {
        self.repo.list_ratings(plugin_id, status).await
    }

    /// 获取分类列表
    pub async fn get_categories(&self) -> PluginResult<Vec<CategoryInfo>> {
        self.repo.list_categories().await
    }

    /// 获取热门插件
    ///
    /// # 参数
    /// * `days` - 统计天数
    /// * `limit` - 返回数量限制
    pub async fn get_trending_plugins(&self, days: i64, limit: i64) -> PluginResult<Vec<MarketplacePlugin>> {
        self.stats_service.get_trending(days, limit).await
    }

    /// 记录下载事件
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `version` - 版本号
    /// * `source_type` - 来源类型
    pub async fn record_download(
        &self,
        plugin_id: &str,
        version: &str,
        source_type: &str,
    ) -> PluginResult<()> {
        self.stats_service.record_download(plugin_id, version, source_type).await
    }

    /// 记录安装事件
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    pub async fn record_install(&self, plugin_id: &str) -> PluginResult<()> {
        self.repo.increment_install_count(plugin_id).await
    }
}
