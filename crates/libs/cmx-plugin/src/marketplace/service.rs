//! 插件市场业务服务层。
//!
//! 封装插件市场的所有业务逻辑，包括插件发布、版本管理、评分评论、
//! 分类统计和热门推荐等。
//!
//! # 设计原则
//!
//! - 简单 CRUD 使用 `GenericCrudService` + `modql FilterNodes`
//! - 复杂业务逻辑（upsert、多表聚合）使用自定义 SQL
//! - 业务方法参数使用结构体传递，禁止平铺参数

use std::sync::Arc;

use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use tracing::{debug, info};

use super::model::{
    CategoryInfo, MarketplacePlugin, MarketplacePluginBmc, MarketplacePluginFilter,
    MarketplacePluginForCreate, MarketplacePluginForUpdate, MarketplacePluginVersion,
    MarketplacePluginVersionBmc,
    MarketplacePluginVersionForCreate, MarketplaceRating, MarketplaceRatingBmc,
    MarketplaceRatingFilter, MarketplaceRatingForCreate,
};
use super::repository::MarketplaceRepository;
use super::stats::StatsService;
use crate::error::{PluginError, PluginResult};

/// 插件市场服务。
///
/// 协调 Repository 层和 StatsService，提供插件市场的完整业务能力。
pub struct MarketplaceService {
    /// 数据仓库，用于复杂 SQL 查询。
    repo: Arc<MarketplaceRepository>,
    /// 统计服务，用于下载/安装统计。
    stats_service: Arc<StatsService>,
    /// 数据库管理器。
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库 ID。
    db_id: String,
}

impl MarketplaceService {
    pub fn repo(&self) -> &super::repository::MarketplaceRepository {
        &self.repo
    }

    /// 创建新的 MarketplaceService 实例。
    ///
    /// # Arguments
    ///
    /// * `repo` - 数据仓库实例。
    /// * `stats_service` - 统计服务实例。
    /// * `db_manager` - 数据库管理器。
    /// * `db_id` - 默认数据库 ID。
    ///
    /// # Returns
    ///
    /// 新的 MarketplaceService 实例。
    pub fn new(
        repo: Arc<MarketplaceRepository>,
        stats_service: Arc<StatsService>,
        db_manager: Arc<DatabaseManager>,
        db_id: String,
    ) -> Self {
        Self {
            repo,
            stats_service,
            db_manager,
            db_id,
        }
    }

    /// 发布插件到市场。
    ///
    /// 如果插件已存在（根据 plugin_id 判断），则更新插件信息并创建新版本；
    /// 如果插件不存在，则创建新插件记录。
    ///
    /// # Arguments
    ///
    /// * `plugin_req` - 插件基本信息。
    /// * `version_req` - 插件版本信息。
    ///
    /// # Returns
    ///
    /// 发布成功后的插件完整信息。
    ///
    /// # Errors
    ///
    /// 当插件不存在或数据库操作失败时返回错误。
    pub async fn publish_plugin(
        &self,
        plugin_req: MarketplacePluginForCreate,
        version_req: MarketplacePluginVersionForCreate,
    ) -> PluginResult<MarketplacePlugin> {
        info!(
            "发布插件到市场: plugin_id={}, version={}, allow_overwrite={}",
            plugin_req.plugin_id, version_req.version, version_req.allow_version_overwrite
        );

        let saved_plugin_id = plugin_req.plugin_id.clone();

        let existing_version = self
            .repo
            .get_version(&plugin_req.plugin_id, &version_req.version)
            .await?;

        if existing_version.is_some() && !version_req.allow_version_overwrite {
            return Err(PluginError::Plugin(format!(
                "版本 {} 已存在，如需覆盖发布请开启 allow_version_overwrite",
                version_req.version
            )));
        }

        let existing = self.repo.get_plugin_by_plugin_id(&plugin_req.plugin_id).await?;

        if existing.is_some() {
            let update_data = MarketplacePluginForUpdate {
                name: plugin_req.name,
                description: plugin_req.description,
                short_description: plugin_req.short_description,
                icon_url: plugin_req.icon_url,
                category: plugin_req.category,
                tags: plugin_req.tags,
                vendor_name: plugin_req.vendor_name,
                vendor_url: plugin_req.vendor_url,
                vendor_contact: plugin_req.vendor_contact,
                license_type: plugin_req.license_type,
                homepage_url: plugin_req.homepage_url,
                documentation_url: plugin_req.documentation_url,
                repository_url: plugin_req.repository_url,
                status: Some("published".to_string()),
                is_featured: plugin_req.is_featured,
                is_official: plugin_req.is_official,
                domain_code: plugin_req.domain_code,
                application_code: plugin_req.application_code,
                module_code: plugin_req.module_code,
                plugin_type: plugin_req.plugin_type,
            };
            self.repo.update_plugin_by_plugin_id(&plugin_req.plugin_id, &update_data).await?;
            let updated = self.repo.get_plugin_by_plugin_id(&plugin_req.plugin_id).await?;
            let plugin = updated.unwrap();
            self.upsert_version(&version_req).await?;
            return Ok(plugin);
        }

        let _dataset = GenericCrudService::<MarketplacePluginBmc>::create(
            &self.db_manager,
            &self.db_id,
            None,
            plugin_req,
        )
        .await
        .map_err(|e| PluginError::Database(format!("创建插件记录失败: {}", e)))?;

        self.upsert_version(&version_req).await?;

        let plugin = self
            .repo
            .get_plugin_by_plugin_id(&saved_plugin_id)
            .await?;
        let plugin = plugin.unwrap();
        Ok(plugin)
    }

    /// 创建插件版本记录。
    ///
    /// 使用 GenericCrudService 创建版本记录，自动处理时间戳和审计字段。
    ///
    /// # Arguments
    ///
    /// * `req` - 版本创建请求，包含版本号、下载地址等。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn create_version(
        &self,
        req: MarketplacePluginVersionForCreate,
    ) -> PluginResult<()> {
        debug!("创建版本: plugin_id={}, version={}", req.plugin_id, req.version);
        GenericCrudService::<MarketplacePluginVersionBmc>::create(
            &self.db_manager,
            &self.db_id,
            None,
            req,
        )
        .await
        .map_err(|e| PluginError::Database(format!("创建版本记录失败: {}", e)))?;
        Ok(())
    }

    /// 插入或更新版本记录。
    ///
    /// 根据 `allow_version_overwrite` 标志决定行为：
    /// - 版本不存在 → 直接插入
    /// - 版本已存在 + `allow_version_overwrite=true` → 更新已有记录
    /// - 版本已存在 + `allow_version_overwrite=false` → 返回错误
    ///
    /// # Arguments
    ///
    /// * `req` - 版本创建请求，包含版本号、下载地址等。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn upsert_version(
        &self,
        req: &MarketplacePluginVersionForCreate,
    ) -> PluginResult<()> {
        let existing = self.repo.get_version(&req.plugin_id, &req.version).await?;

        if existing.is_some() {
            if !req.allow_version_overwrite {
                return Err(PluginError::Plugin(format!(
                    "版本 {} 已存在，如需覆盖发布请开启 allow_version_overwrite",
                    req.version
                )));
            }
            debug!(
                "覆盖更新版本: plugin_id={}, version={}",
                req.plugin_id, req.version
            );
            self.repo
                .update_version_by_plugin_id_and_version(&req.plugin_id, &req.version, req)
                .await?;
            return Ok(());
        }

        self.create_version(req.clone()).await
    }

    /// 更新插件市场信息。
    ///
    /// 根据 plugin_id 更新插件的基本信息，仅更新提供的非 None 字段。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    /// * `data` - 待更新的插件信息，仅非 None 字段会被更新。
    ///
    /// # Errors
    ///
    /// 当插件不存在或数据库操作失败时返回错误。
    pub async fn update_plugin(
        &self,
        plugin_id: &str,
        data: MarketplacePluginForUpdate,
    ) -> PluginResult<()> {
        debug!("更新插件市场信息: plugin_id={}", plugin_id);
        self.repo.update_plugin_by_plugin_id(plugin_id, &data).await
    }

    /// 删除市场插件。
    ///
    /// 执行逻辑删除，将插件的 archived 字段设为 1。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Errors
    ///
    /// 当插件不存在或数据库操作失败时返回错误。
    pub async fn delete_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        info!("删除市场插件: plugin_id={}", plugin_id);
        self.repo.delete_plugin(plugin_id).await
    }

    /// 分页查询市场插件。
    ///
    /// 支持多条件过滤和分页排序，默认只查询已发布且未归档的插件。
    ///
    /// # Arguments
    ///
    /// * `filters` - 查询过滤条件，使用 modql FilterNodes 构建。
    /// * `list_options` - 分页和排序选项。
    ///
    /// # Returns
    ///
    /// 插件列表和总数。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn page_plugins(
        &self,
        filters: Option<Vec<MarketplacePluginFilter>>,
        list_options: ListOptions,
    ) -> PluginResult<(Vec<MarketplacePlugin>, u64)> {
        debug!("分页查询市场插件");

        let default_filter = MarketplacePluginFilter {
            status: Some(modql::filter::OpValsString(vec![
                modql::filter::OpValString::Eq("published".to_string()),
            ])),
            archived: Some(modql::filter::OpValsInt64(vec![
                modql::filter::OpValInt64::Eq(0),
            ])),
            ..Default::default()
        };

        let final_filters = match filters {
            Some(mut fs) => {
                for f in &mut fs {
                    if f.status.is_none() {
                        f.status = default_filter.status.clone();
                    }
                    if f.archived.is_none() {
                        f.archived = default_filter.archived.clone();
                    }
                }
                Some(fs)
            }
            None => Some(vec![default_filter]),
        };

        let (dataset, total) = GenericCrudService::<MarketplacePluginBmc, MarketplacePluginFilter>::page(
            &self.db_manager,
            &self.db_id,
            None,
            final_filters,
            list_options,
        )
        .await
        .map_err(|e| PluginError::Database(format!("分页查询插件失败: {}", e)))?;

        let plugins = self.datasets_to_plugins(&dataset);
        Ok((plugins, total as u64))
    }

    /// 根据 plugin_id 查询插件详情。
    ///
    /// plugin_id 是插件的业务唯一标识。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Returns
    ///
    /// 找到时返回插件详情，未找到时返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_plugin_by_plugin_id(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Option<MarketplacePlugin>> {
        debug!("查询市场插件详情: plugin_id={}", plugin_id);
        self.repo.get_plugin_by_plugin_id(plugin_id).await
    }

    /// 根据主键 ID 查询插件详情。
    ///
    /// id 是数据库主键（雪花算法生成）。
    ///
    /// # Arguments
    ///
    /// * `id` - 插件的数据库主键 ID。
    ///
    /// # Returns
    ///
    /// 找到时返回插件详情，未找到时返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_plugin_by_id(&self, id: &str) -> PluginResult<Option<MarketplacePlugin>> {
        debug!("查询市场插件详情: id={}", id);
        let result = GenericCrudService::<MarketplacePluginBmc>::get(
            &self.db_manager,
            &self.db_id,
            None,
            id.into(),
        )
        .await;

        match result {
            Ok(dataset) => Ok(Some(self.dataset_to_plugin(&dataset))),
            Err(_) => Ok(None),
        }
    }

    /// 查询插件的所有版本。
    ///
    /// 按 version_rank 降序排列，最新版本排在最前。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Returns
    ///
    /// 插件的所有未归档版本列表。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_plugin_versions(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Vec<MarketplacePluginVersion>> {
        debug!("查询插件版本列表: plugin_id={}", plugin_id);
        self.repo.list_versions_by_plugin_id(plugin_id).await
    }

    /// 根据主键 ID 查询版本详情。
    ///
    /// # Arguments
    ///
    /// * `id` - 版本的数据库主键 ID。
    ///
    /// # Returns
    ///
    /// 找到时返回版本详情，未找到时返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_version_by_id(
        &self,
        id: &str,
    ) -> PluginResult<Option<MarketplacePluginVersion>> {
        debug!("查询版本详情: id={}", id);
        self.repo.get_version_by_id(id).await
    }

    /// 获取插件的最新稳定版本。
    ///
    /// 优先返回 status='published' 且 is_stable=1 的版本。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Returns
    ///
    /// 找到时返回最新稳定版本，未找到时返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_latest_stable_version(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Option<MarketplacePluginVersion>> {
        self.repo.get_latest_stable_version(plugin_id).await
    }

    /// 获取插件的指定版本。
    ///
    /// 通过 plugin_id 和 version 两个条件精确匹配。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    /// * `version` - 版本号。
    ///
    /// # Returns
    ///
    /// 找到时返回版本详情，未找到时返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> PluginResult<Option<MarketplacePluginVersion>> {
        self.repo.get_version(plugin_id, version).await
    }

    /// 对插件进行评分。
    ///
    /// 评分范围 1-5 分，评分后自动更新插件的平均评分和评分次数。
    ///
    /// # Arguments
    ///
    /// * `req` - 评分请求，包含 plugin_id、rating、review 等。
    ///
    /// # Errors
    ///
    /// 当评分不在有效范围内、插件不存在或数据库操作失败时返回错误。
    pub async fn rate_plugin(&self, req: MarketplaceRatingForCreate) -> PluginResult<()> {
        info!(
            "插件评分: plugin_id={}, rating={:?}",
            req.plugin_id, req.rating
        );

        if let Some(rating) = req.rating
            && !(1..=5).contains(&rating)
        {
            return Err(PluginError::Plugin(format!(
                "评分必须在1-5之间，当前值: {}",
                rating
            )));
        }

        let plugin = self.repo.get_plugin_by_plugin_id(&req.plugin_id).await?;
        if plugin.is_none() {
            return Err(PluginError::NotFound(format!(
                "插件 '{}' 不存在",
                req.plugin_id
            )));
        }

        self.repo.upsert_rating(&req).await?;
        self.repo.update_rating_summary(&req.plugin_id).await?;

        Ok(())
    }

    /// 查询评分列表。
    ///
    /// 支持多条件过滤和分页。
    ///
    /// # Arguments
    ///
    /// * `filters` - 查询过滤条件。
    /// * `list_options` - 分页和排序选项。
    ///
    /// # Returns
    ///
    /// 符合条件的评分列表。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn list_ratings(
        &self,
        filters: Option<Vec<MarketplaceRatingFilter>>,
        list_options: Option<ListOptions>,
    ) -> PluginResult<Vec<MarketplaceRating>> {
        let dataset = GenericCrudService::<MarketplaceRatingBmc, MarketplaceRatingFilter>::list(
            &self.db_manager,
            &self.db_id,
            None,
            filters,
            list_options,
        )
        .await
        .map_err(|e| PluginError::Database(format!("查询评分列表失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let mut ratings = Vec::new();
        for row in dataset.iter() {
            ratings.push(Self::row_to_rating(row, schema));
        }
        Ok(ratings)
    }

    /// 获取插件分类统计。
    ///
    /// 返回各分类下的插件数量，按数量降序排列。
    ///
    /// # Returns
    ///
    /// 分类信息列表，包含分类名称和插件数量。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_categories(&self) -> PluginResult<Vec<CategoryInfo>> {
        self.repo.list_categories().await
    }

    /// 获取热门插件。
    ///
    /// 根据最近 N 天的下载量统计，返回最热门的插件列表。
    ///
    /// # Arguments
    ///
    /// * `days` - 统计最近 N 天的数据。
    /// * `limit` - 返回的插件数量上限。
    ///
    /// # Returns
    ///
    /// 热门插件列表，按下载量降序排列。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_trending_plugins(
        &self,
        days: i64,
        limit: i64,
    ) -> PluginResult<Vec<MarketplacePlugin>> {
        self.stats_service.get_trending(days, limit).await
    }

    /// 记录插件下载事件。
    ///
    /// 更新日统计表和插件总下载量。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    /// * `version` - 下载的版本号。
    /// * `source_type` - 下载来源类型。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn record_download(
        &self,
        plugin_id: &str,
        version: &str,
        source_type: &str,
    ) -> PluginResult<()> {
        self.stats_service
            .record_download(plugin_id, version, source_type)
            .await
    }

    /// 记录插件安装事件。
    ///
    /// 更新插件总安装量。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn record_install(&self, plugin_id: &str) -> PluginResult<()> {
        self.repo.increment_install_count(plugin_id).await
    }

    /// 从插件市场安装插件。
    ///
    /// 查询市场版本信息，构建 `PluginSource`（`storage_file_id` 优先，`download_url` 降级），
    /// 调用 `InstallService` 执行实际安装，并记录下载和安装统计。
    ///
    /// # Arguments
    ///
    /// * `req` - 市场安装请求，包含 `plugin_id`、`version`、`db_id`、`auto_activate`。
    ///
    /// # Returns
    ///
    /// 安装成功时返回 `InstallResponse`，包含插件 ID、安装路径和版本信息。
    ///
    /// # Errors
    ///
    /// * `PluginError::NotFound` - 市场版本不存在。
    /// * `PluginError::Plugin` - 版本缺少 `storage_file_id` 和 `download_url`。
    /// * `PluginError::Install` - 安装过程失败。
    pub async fn install_from_marketplace(
        &self,
        req: &super::model::MarketInstallRequest,
    ) -> PluginResult<crate::service::install::InstallResponse> {
        use crate::domain::plugin::PluginSource;
        use crate::GlobalPluginManager;

        let version_info = if let Some(ref version) = req.version {
            self.get_version(&req.plugin_id, version).await?
        } else {
            self.get_latest_stable_version(&req.plugin_id).await?
        };

        let version_info = version_info.ok_or_else(|| {
            PluginError::NotFound(format!(
                "市场版本不存在: plugin_id={}, version={:?}",
                req.plugin_id, req.version
            ))
        })?;

        let source = if let Some(ref file_id) = version_info.storage_file_id {
            PluginSource::Storage {
                file_id: file_id.clone(),
                checksum: version_info.checksum.clone(),
            }
        } else if let Some(ref url) = version_info.download_url {
            PluginSource::Remote {
                url: url.clone(),
                checksum: version_info.checksum.clone(),
            }
        } else {
            return Err(PluginError::Plugin(format!(
                "市场版本 '{}' 缺少 storage_file_id 和 download_url",
                version_info.version
            )));
        };

        let manager = GlobalPluginManager::get();
        let install_req = crate::service::install::InstallRequest {
            source,
            db_id: req.db_id.clone(),
            auto_activate: req.auto_activate.unwrap_or(false),
            version_constraint: None,
            build_type: None,
            marketplace_source_id: Some(version_info.id.clone()),
            app_id: None,
            send_event: true,
        };

        let result = manager.install(install_req).await?;

        if let Err(e) = self
            .record_download(&req.plugin_id, &version_info.version, "marketplace")
            .await
        {
            tracing::warn!("记录下载统计失败: {}", e);
        }
        if let Err(e) = self.record_install(&req.plugin_id).await {
            tracing::warn!("记录安装统计失败: {}", e);
        }

        Ok(result)
    }

    /// 从插件市场升级插件。
    ///
    /// 查询市场版本信息，构建 `PluginSource`，调用 `UpgradeService` 执行升级，
    /// 并记录下载统计。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 要升级的插件业务 ID。
    /// * `target_version` - 目标版本号，为 `None` 时升级到最新稳定版。
    /// * `force` - 是否强制升级（忽略版本检查）。
    ///
    /// # Returns
    ///
    /// 升级成功时返回 `UpgradeResponse`，包含旧版本和新版本信息。
    ///
    /// # Errors
    ///
    /// * `PluginError::NotFound` - 市场版本不存在。
    /// * `PluginError::Plugin` - 版本缺少 `storage_file_id` 和 `download_url`。
    /// * `PluginError::Upgrade` - 升级过程失败。
    pub async fn upgrade_from_marketplace(
        &self,
        plugin_id: &str,
        target_version: Option<&str>,
        force: bool,
    ) -> PluginResult<crate::service::upgrade::UpgradeResponse> {
        use crate::domain::plugin::PluginSource;
        use crate::GlobalPluginManager;

        let version_info = if let Some(version) = target_version {
            self.get_version(plugin_id, version).await?
        } else {
            self.get_latest_stable_version(plugin_id).await?
        };

        let version_info = version_info.ok_or_else(|| {
            PluginError::NotFound(format!("市场版本不存在: plugin_id={}", plugin_id))
        })?;

        let source = if let Some(ref file_id) = version_info.storage_file_id {
            PluginSource::Storage {
                file_id: file_id.clone(),
                checksum: version_info.checksum.clone(),
            }
        } else if let Some(ref url) = version_info.download_url {
            PluginSource::Remote {
                url: url.clone(),
                checksum: version_info.checksum.clone(),
            }
        } else {
            return Err(PluginError::Plugin(format!(
                "市场版本 '{}' 缺少 storage_file_id 和 download_url",
                version_info.version
            )));
        };

        let manager = GlobalPluginManager::get();
        let upgrade_req = crate::service::upgrade::UpgradeRequest {
            plugin_id: plugin_id.to_string(),
            source,
            version_constraint: None,
            force,
            operator: None,
            build_type: None,
            marketplace_source_id: Some(version_info.id.clone()),
            app_id: None,
            send_event: true,
        };

        let result = manager.upgrade(upgrade_req).await?;

        if let Err(e) = self
            .record_download(plugin_id, &version_info.version, "marketplace")
            .await
        {
            tracing::warn!("记录下载统计失败: {}", e);
        }

        Ok(result)
    }

    /// 检查已安装插件在市场中的更新。
    ///
    /// 批量查询市场最新版本，使用 `SemanticVersion` 逐个比较版本号，
    /// 返回有更新可用的插件列表。
    ///
    /// # Arguments
    ///
    /// * `installed_plugins` - 已安装插件记录列表。
    ///
    /// # Returns
    ///
    /// 有更新可用的插件信息列表，包含当前版本和最新市场版本。
    ///
    /// # Errors
    ///
    /// * `PluginError::Database` - 数据库查询失败。
    pub async fn check_updates(
        &self,
        installed_plugins: &[crate::infrastructure::database::plugin::model::PluginRecord],
    ) -> PluginResult<Vec<super::model::PluginUpdateInfo>> {
        use crate::domain::version::SemanticVersion;

        let plugin_ids: Vec<String> =
            installed_plugins.iter().map(|p| p.plugin_id.clone()).collect();
        let latest_versions = self.repo.get_latest_versions_batch(&plugin_ids).await?;

        let mut updates = Vec::new();
        for plugin in installed_plugins {
            if let Some(latest) = latest_versions.get(&plugin.plugin_id) {
                let current = match SemanticVersion::parse(&plugin.version) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let latest_ver = match SemanticVersion::parse(&latest.version) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let version_outdated = latest_ver > current;
                let source_mismatch = match (&plugin.marketplace_source_id, &latest.id) {
                    (Some(current_id), latest_id) => current_id != latest_id,
                    (None, _) => true,
                };
                let has_update = version_outdated || source_mismatch;
                updates.push(super::model::PluginUpdateInfo {
                    plugin_id: plugin.plugin_id.clone(),
                    plugin_name: Some(plugin.name.clone()),
                    current_version: plugin.version.clone(),
                    current_marketplace_source_id: plugin.marketplace_source_id.clone(),
                    latest_version: latest.version.clone(),
                    latest_version_info: latest.clone(),
                    has_update,
                });
            }
        }

        Ok(updates)
    }

    // =========================================================================
    // 私有辅助方法
    // =========================================================================

    /// 将 DataSet 单行转换为 MarketplacePlugin。
    fn dataset_to_plugin(&self, dataset: &DataSet) -> MarketplacePlugin {
        if let Some(row) = dataset.iter().next() {
            let schema = dataset.schema.as_ref();
            Self::row_to_plugin(row, schema)
        } else {
            MarketplacePlugin::default()
        }
    }

    /// 将 DataSet 多行转换为 Vec<MarketplacePlugin>。
    fn datasets_to_plugins(&self, dataset: &DataSet) -> Vec<MarketplacePlugin> {
        let schema = dataset.schema.as_ref();
        dataset.iter().map(|row| Self::row_to_plugin(row, schema)).collect()
    }

    /// 将数据库行映射为 MarketplacePlugin 实体。
    ///
    /// 处理 tags 字段的 JSON 字符串到 JSON Value 的转换。
    ///
    /// # Arguments
    ///
    /// * `row` - 数据库行数据。
    /// * `schema` - 数据集 schema，用于按名称获取列值。
    ///
    /// # Returns
    ///
    /// 映射后的 MarketplacePlugin 实体。
    fn row_to_plugin(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> MarketplacePlugin {
        let tags_str: Option<String> = row.get_by_name_as(schema, "tags");
        let tags = tags_str.and_then(|s| serde_json::from_str(&s).ok());

        MarketplacePlugin {
            id: row.get_by_name_as(schema, "id").unwrap_or_default(),
            plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
            name: row.get_by_name_as(schema, "name"),
            description: row.get_by_name_as(schema, "description"),
            short_description: row.get_by_name_as(schema, "short_description"),
            icon_url: row.get_by_name_as(schema, "icon_url"),
            category: row.get_by_name_as(schema, "category"),
            tags,
            vendor_name: row.get_by_name_as(schema, "vendor_name"),
            vendor_url: row.get_by_name_as(schema, "vendor_url"),
            vendor_contact: row.get_by_name_as(schema, "vendor_contact"),
            license_type: row.get_by_name_as(schema, "license_type"),
            homepage_url: row.get_by_name_as(schema, "homepage_url"),
            documentation_url: row.get_by_name_as(schema, "documentation_url"),
            repository_url: row.get_by_name_as(schema, "repository_url"),
            status: row.get_by_name_as(schema, "status"),
            is_featured: row.get_by_name_as::<i32>(schema, "is_featured").map(|v| v as i16),
            is_official: row.get_by_name_as::<i32>(schema, "is_official").map(|v| v as i16),
            avg_rating: row.get_by_name_as(schema, "avg_rating"),
            rating_count: row.get_by_name_as(schema, "rating_count"),
            download_count: row.get_by_name_as(schema, "download_count"),
            install_count: row.get_by_name_as(schema, "install_count"),
            domain_code: row.get_by_name_as(schema, "domain_code"),
            application_code: row.get_by_name_as(schema, "application_code"),
            module_code: row.get_by_name_as(schema, "module_code"),
            plugin_type: row.get_by_name_as(schema, "plugin_type"),
            archived: row.get_by_name_as(schema, "archived"),
            create_time: row.get_by_name_as(schema, "create_time"),
            update_time: row.get_by_name_as(schema, "update_time"),
            create_by: row.get_by_name_as(schema, "create_by"),
            create_name: row.get_by_name_as(schema, "create_name"),
            update_by: row.get_by_name_as(schema, "update_by"),
            update_name: row.get_by_name_as(schema, "update_name"),
        }
    }

    /// 将数据库行映射为 MarketplaceRating 实体。
    ///
    /// # Arguments
    ///
    /// * `row` - 数据库行数据。
    /// * `schema` - 数据集 schema。
    ///
    /// # Returns
    ///
    /// 映射后的 MarketplaceRating 实体。
    fn row_to_rating(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> MarketplaceRating {
        MarketplaceRating {
            id: row.get_by_name_as(schema, "id").unwrap_or_default(),
            plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
            user_id: row.get_by_name_as(schema, "user_id").unwrap_or_default(),
            rating: row.get_by_name_as(schema, "rating"),
            review: row.get_by_name_as(schema, "review"),
            status: row.get_by_name_as(schema, "status"),
            archived: row.get_by_name_as(schema, "archived"),
            create_time: row.get_by_name_as(schema, "create_time"),
            update_time: row.get_by_name_as(schema, "update_time"),
            create_by: row.get_by_name_as(schema, "create_by"),
            create_name: row.get_by_name_as(schema, "create_name"),
            update_by: row.get_by_name_as(schema, "update_by"),
            update_name: row.get_by_name_as(schema, "update_name"),
        }
    }
}

pub async fn get_marketplace_service() -> MarketplaceService {
    let db_manager = cmx_database::get_default_db_manager();
    let default_db_id = db_manager.get_default_db_id().await;

    let repo = Arc::new(super::repository::MarketplaceRepository::new(
        db_manager.clone(),
        default_db_id.clone(),
    ));
    let stats_service = Arc::new(super::stats::StatsService::new(repo.clone()));
    MarketplaceService::new(repo, stats_service, db_manager.clone(), default_db_id)
}
