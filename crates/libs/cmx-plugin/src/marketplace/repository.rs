//! 插件市场数据仓库层。
//!
//! 封装插件市场所有数据库操作。
//!
//! # 设计原则
//!
//! - 简单 CRUD 操作委托给 `GenericCrudService`（在 Service 层处理）
//! - 复杂 SQL（多表 JOIN、UPSERT、聚合统计、子查询）在此层实现
//! - 仅包含无法用 GenericCrudService 表达的业务 SQL

use cmx_database::DatabaseManager;
use serde_json::json;
use std::sync::Arc;

use super::model::{
    CategoryInfo, MarketplacePlugin, MarketplacePluginForUpdate, MarketplacePluginVersion,
    MarketplacePluginVersionForCreate, MarketplaceRatingForCreate,
};
use crate::error::{PluginError, PluginResult};

/// 插件市场数据仓库。
///
/// 负责所有与数据库直接交互的操作，封装复杂 SQL 逻辑。
pub struct MarketplaceRepository {
    /// 数据库管理器。
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库 ID。
    default_db_id: String,
}

impl MarketplaceRepository {
    /// 创建新的 MarketplaceRepository 实例。
    ///
    /// # Arguments
    ///
    /// * `db_manager` - 数据库管理器。
    /// * `default_db_id` - 默认数据库 ID。
    ///
    /// # Returns
    ///
    /// 新的 MarketplaceRepository 实例。
    pub fn new(db_manager: Arc<DatabaseManager>, default_db_id: String) -> Self {
        Self {
            db_manager,
            default_db_id,
        }
    }

    /// 获取默认数据库 ID。
    ///
    /// # Returns
    ///
    /// 默认数据库 ID 字符串引用。
    pub fn default_db_id(&self) -> &str {
        &self.default_db_id
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
        let sql = r#"
            SELECT id, plugin_id, name, description, short_description,
                   icon_url, category, tags, vendor_name, vendor_url,
                   vendor_contact, license_type, homepage_url, documentation_url,
                   repository_url, status, is_featured, is_official,
                   avg_rating, rating_count, download_count, install_count,
                   domain_code, application_code, module_code, plugin_type,
                   archived, create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin
            WHERE plugin_id = $1
        "#;

        let params = json!([plugin_id]);
        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "get_marketplace_plugin",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询市场插件失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_plugin(row, schema)))
    }

    /// 根据 plugin_id 更新插件信息。
    ///
    /// 使用 COALESCE 实现部分更新：传入的 None 字段保持原值不动。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    /// * `data` - 待更新的插件信息，仅非 None 字段会被更新。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn update_plugin_by_plugin_id(
        &self,
        plugin_id: &str,
        data: &MarketplacePluginForUpdate,
    ) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin SET
                name = COALESCE($2::varchar, name),
                description = COALESCE($3::text, description),
                short_description = COALESCE($4::varchar, short_description),
                icon_url = COALESCE($5::varchar, icon_url),
                category = COALESCE($6::varchar, category),
                tags = COALESCE($7::jsonb, tags),
                vendor_name = COALESCE($8::varchar, vendor_name),
                vendor_url = COALESCE($9::varchar, vendor_url),
                vendor_contact = COALESCE($10::varchar, vendor_contact),
                license_type = COALESCE($11::varchar, license_type),
                homepage_url = COALESCE($12::varchar, homepage_url),
                documentation_url = COALESCE($13::varchar, documentation_url),
                repository_url = COALESCE($14::varchar, repository_url),
                status = COALESCE($15::varchar, status),
                is_featured = COALESCE($16::smallint, is_featured),
                is_official = COALESCE($17::smallint, is_official),
                domain_code = COALESCE($18::varchar, domain_code),
                application_code = COALESCE($19::varchar, application_code),
                module_code = COALESCE($20::varchar, module_code),
                plugin_type = COALESCE($21::varchar, plugin_type),
                update_time = NOW()
            WHERE plugin_id = $1::varchar
        "#;

        let params = json!([
            plugin_id,
            data.name,
            data.description,
            data.short_description,
            data.icon_url,
            data.category,
            data.tags,
            data.vendor_name,
            data.vendor_url,
            data.vendor_contact,
            data.license_type,
            data.homepage_url,
            data.documentation_url,
            data.repository_url,
            data.status,
            data.is_featured,
            data.is_official,
            data.domain_code,
            data.application_code,
            data.module_code,
            data.plugin_type,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新市场插件失败: {}", e)))?;

        Ok(())
    }

    /// 删除市场插件。
    ///
    /// 执行逻辑删除，将插件的 archived 字段设为 1，同时更新 update_time。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn delete_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin SET archived = 1, update_time = NOW()
            WHERE plugin_id = $1
        "#;

        let params = json!([plugin_id]);
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("删除市场插件失败: {}", e)))?;

        Ok(())
    }

    /// 查询插件的所有版本。
    ///
    /// 仅返回未归档的版本，按 version_rank 降序排列。
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
    pub async fn list_versions_by_plugin_id(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Vec<MarketplacePluginVersion>> {
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, storage_file_id, package_size, checksum,
                   min_platform_version, max_platform_version, dependencies,
                   compatibility, status, is_latest, is_stable,
                   download_count, published_at, archived,
                   create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE plugin_id = $1 AND archived = 0
            ORDER BY version_rank DESC
        "#;

        let params = json!([plugin_id]);
        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "cmx_marketplace_plugin_version",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询市场版本列表失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut versions = Vec::new();
        for row in result.iter() {
            versions.push(Self::row_to_version(row, schema));
        }

        Ok(versions)
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
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, storage_file_id, package_size, checksum,
                   min_platform_version, max_platform_version, dependencies,
                   compatibility, status, is_latest, is_stable,
                   download_count, published_at, archived,
                   create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE id = $1
        "#;

        let params = json!([id]);
        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "get_marketplace_version",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询市场版本失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_version(row, schema)))
    }

    /// 获取插件的最新稳定版本。
    ///
    /// 优先返回已发布且 is_stable=1 的版本。
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
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, storage_file_id, package_size, checksum,
                   min_platform_version, max_platform_version, dependencies,
                   compatibility, status, is_latest, is_stable,
                   download_count, published_at, archived,
                   create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE plugin_id = $1 AND archived = 0 AND status = 'published'
            ORDER BY is_stable DESC, version_rank DESC
            LIMIT 1
        "#;

        let params = json!([plugin_id]);
        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "get_latest_stable_version",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询最新稳定版本失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_version(row, schema)))
    }

    /// 获取插件的指定版本。
    ///
    /// 通过 plugin_id + version 精确匹配。
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
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, storage_file_id, package_size, checksum,
                   min_platform_version, max_platform_version, dependencies,
                   compatibility, status, is_latest, is_stable,
                   download_count, published_at, archived,
                   create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE plugin_id = $1 AND version = $2 AND archived = 0
        "#;

        let params = json!([plugin_id, version]);
        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "get_marketplace_version_by_number",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询市场版本失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_version(row, schema)))
    }

    /// 更新版本记录。
    ///
    /// 根据 plugin_id + version 定位已有版本记录，使用 COALESCE 部分更新非 None 字段。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件业务 ID。
    /// * `version` - 版本号。
    /// * `data` - 版本创建请求（字段复用，非 None 字段参与更新）。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn update_version_by_plugin_id_and_version(
        &self,
        plugin_id: &str,
        version: &str,
        data: &MarketplacePluginVersionForCreate,
    ) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin_version SET
                version_rank = COALESCE($3::int4, version_rank),
                changelog = COALESCE($4::text, changelog),
                release_notes = COALESCE($5::text, release_notes),
                download_url = COALESCE($6::varchar, download_url),
                storage_file_id = COALESCE($7::varchar, storage_file_id),
                package_size = COALESCE($8::int8, package_size),
                checksum = COALESCE($9::varchar, checksum),
                min_platform_version = COALESCE($10::varchar, min_platform_version),
                max_platform_version = COALESCE($11::varchar, max_platform_version),
                dependencies = COALESCE($12::jsonb, dependencies),
                compatibility = COALESCE($13::jsonb, compatibility),
                status = COALESCE($14::varchar, status),
                is_latest = COALESCE($15::smallint, is_latest),
                is_stable = COALESCE($16::smallint, is_stable),
                published_at = COALESCE($17::timestamp, published_at),
                update_time = NOW()
            WHERE plugin_id = $1::varchar AND version = $2::varchar AND archived = 0
        "#;

        let params = json!([
            plugin_id,
            version,
            data.version_rank,
            data.changelog,
            data.release_notes,
            data.download_url,
            data.storage_file_id,
            data.package_size,
            data.checksum,
            data.min_platform_version,
            data.max_platform_version,
            data.dependencies,
            data.compatibility,
            data.status,
            data.is_latest,
            data.is_stable,
            data.published_at,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新市场版本记录失败: {}", e)))?;

        Ok(())
    }

    /// 插入或更新评分记录。
    ///
    /// 使用 ON CONFLICT 实现 UPSERT：同一用户对同一插件重复评分时更新记录。
    ///
    /// # Arguments
    ///
    /// * `req` - 评分创建请求，包含 plugin_id、user_id、rating 等。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn upsert_rating(&self, req: &MarketplaceRatingForCreate) -> PluginResult<()> {
        let sql = r#"
            INSERT INTO cmx_marketplace_rating (
                id, plugin_id, user_id, rating, review, status,
                archived, create_time, update_time
            ) VALUES (
                gen_random_uuid()::text, $1, $2, $3, $4, $5,
                0, NOW(), NOW()
            )
            ON CONFLICT (plugin_id, user_id) DO UPDATE SET
                rating = EXCLUDED.rating,
                review = EXCLUDED.review,
                status = EXCLUDED.status,
                update_time = NOW()
        "#;

        let params = json!([
            req.plugin_id,
            req.user_id,
            req.rating,
            req.review,
            req.status,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("插入/更新评分失败: {}", e)))?;

        Ok(())
    }

    /// 更新插件的评分汇总。
    ///
    /// 从评分表聚合计算 avg_rating 和 rating_count。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn update_rating_summary(&self, plugin_id: &str) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin
            SET avg_rating = (
                    SELECT AVG(CAST(rating AS DECIMAL))
                    FROM cmx_marketplace_rating
                    WHERE plugin_id = $1 AND status = 'approved'
                ),
                rating_count = (
                    SELECT COUNT(*)
                    FROM cmx_marketplace_rating
                    WHERE plugin_id = $1 AND status = 'approved'
                ),
                update_time = NOW()
            WHERE plugin_id = $1
        "#;

        let params = json!([plugin_id]);
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新评分汇总失败: {}", e)))?;

        Ok(())
    }

    /// 增加插件总下载量。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn increment_download_count(&self, plugin_id: &str) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin
            SET download_count = COALESCE(download_count, 0) + 1,
                update_time = NOW()
            WHERE plugin_id = $1
        "#;

        let params = json!([plugin_id]);
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新下载量失败: {}", e)))?;

        Ok(())
    }

    /// 增加插件总安装量。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn increment_install_count(&self, plugin_id: &str) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin
            SET install_count = COALESCE(install_count, 0) + 1,
                update_time = NOW()
            WHERE plugin_id = $1
        "#;

        let params = json!([plugin_id]);
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新安装量失败: {}", e)))?;

        Ok(())
    }

    /// 插入或累加日下载统计。
    ///
    /// 按 (plugin_id, version, download_date, source_type) 去重，
    /// 重复下载时累加 download_count。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件的业务唯一标识。
    /// * `version` - 版本号。
    /// * `download_date` - 下载日期（格式：YYYY-MM-DD）。
    /// * `source_type` - 下载来源类型。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn upsert_download_stat(
        &self,
        plugin_id: &str,
        version: &str,
        download_date: &str,
        source_type: &str,
    ) -> PluginResult<()> {
        let sql = r#"
            INSERT INTO cmx_marketplace_download_stats (
                id, plugin_id, version, download_date, download_count,
                install_count, source_type, archived, create_time, update_time
            ) VALUES (gen_random_uuid()::text, $1, $2, $3::date, 1, 0, $4, 0, NOW(), NOW())
            ON CONFLICT (plugin_id, version, download_date, source_type) DO UPDATE SET
                download_count = cmx_marketplace_download_stats.download_count + 1,
                update_time = NOW()
        "#;

        let params = json!([plugin_id, version, download_date, source_type]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新下载统计失败: {}", e)))?;

        Ok(())
    }

    /// 查询热门插件。
    ///
    /// 基于日期范围内下载量统计，关联统计表按总下载量排序。
    ///
    /// # Arguments
    ///
    /// * `since_date` - 统计起始日期（格式：YYYY-MM-DD）。
    /// * `limit` - 返回的插件数量上限。
    ///
    /// # Returns
    ///
    /// 热门插件列表，按下载量降序排列。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_trending_since(
        &self,
        since_date: &str,
        limit: i64,
    ) -> PluginResult<Vec<MarketplacePlugin>> {
        let sql = r#"
            SELECT p.id, p.plugin_id, p.name, p.description, p.short_description,
                   p.icon_url, p.category, p.tags, p.vendor_name, p.vendor_url,
                   p.vendor_contact, p.license_type, p.homepage_url, p.documentation_url,
                   p.repository_url, p.status, p.is_featured, p.is_official,
                   p.avg_rating, p.rating_count, p.download_count, p.install_count,
                   p.domain_code, p.application_code, p.module_code, p.plugin_type,
                   p.archived, p.create_time, p.update_time, p.create_by, p.create_name,
                   p.update_by, p.update_name
            FROM cmx_marketplace_plugin p
            INNER JOIN (
                SELECT plugin_id, SUM(download_count) as recent_downloads
                FROM cmx_marketplace_download_stats
                WHERE download_date >= $1::date
                GROUP BY plugin_id
                ORDER BY recent_downloads DESC
                LIMIT $2
            ) ds ON p.plugin_id = ds.plugin_id
            WHERE p.archived = 0 AND p.status = 'published'
            ORDER BY ds.recent_downloads DESC
        "#;

        let params = json!([since_date, limit]);
        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "get_trending_plugins",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询热门插件失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut plugins = Vec::new();
        for row in result.iter() {
            plugins.push(Self::row_to_plugin(row, schema));
        }

        Ok(plugins)
    }

    /// 查询分类统计。
    ///
    /// 统计各分类下已发布且未归档的插件数量，按数量降序。
    ///
    /// # Returns
    ///
    /// 分类信息列表，包含分类名称和插件数量。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn list_categories(&self) -> PluginResult<Vec<CategoryInfo>> {
        let sql = r#"
            SELECT category, COUNT(*) as count
            FROM cmx_marketplace_plugin
            WHERE archived = 0 AND status = 'published' AND category IS NOT NULL
            GROUP BY category
            ORDER BY count DESC
        "#;

        let result = self
            .db_manager
            .query_sql(
                &self.default_db_id,
                None,
                sql,
                "list_marketplace_categories",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询分类列表失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut categories = Vec::new();
        for row in result.iter() {
            categories.push(CategoryInfo {
                category: row.get_by_name_as(schema, "category").unwrap_or_default(),
                count: row.get_by_name_as::<i64>(schema, "count").unwrap_or(0),
            });
        }

        Ok(categories)
    }

    /// 重置指定插件所有版本的 `is_latest` 标志为 0。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件业务 ID。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn reset_is_latest(&self, plugin_id: &str) -> PluginResult<()> {
        let sql = r#"
            UPDATE cmx_marketplace_plugin_version
            SET is_latest = 0, update_time = NOW()
            WHERE plugin_id = $1
        "#;
        let params = json!([plugin_id]);
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("重置is_latest失败: {}", e)))?;
        Ok(())
    }

    /// 批量查询多个插件的最新发布版本。
    ///
    /// 使用 `DISTINCT ON` 按 `plugin_id` 去重，返回每个插件的最新版本。
    ///
    /// # Arguments
    ///
    /// * `plugin_ids` - 插件业务 ID 列表。
    ///
    /// # Returns
    ///
    /// 以 `plugin_id` 为键、最新版本信息为值的哈希表。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_latest_versions_batch(
        &self,
        plugin_ids: &[String],
    ) -> PluginResult<std::collections::HashMap<String, MarketplacePluginVersion>> {
        if plugin_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let sql = r#"
            SELECT DISTINCT ON (plugin_id)
                id, plugin_id, version, version_rank, changelog,
                release_notes, download_url, storage_file_id, package_size, checksum,
                min_platform_version, max_platform_version, dependencies,
                compatibility, status, is_latest, is_stable,
                download_count, published_at, archived,
                create_time, update_time, create_by, create_name,
                update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE plugin_id = ANY($1) AND status = 'published' AND archived = 0
            ORDER BY plugin_id, version_rank DESC
        "#;
        let params = json![plugin_ids];
        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_latest_versions_batch")
            .await
            .map_err(|e| PluginError::Database(format!("批量查询最新版本失败: {}", e)))?;
        let schema = result.schema.as_ref();
        let mut map = std::collections::HashMap::new();
        for row in result.iter() {
            let version = Self::row_to_version(row, schema);
            map.insert(version.plugin_id.clone(), version);
        }
        Ok(map)
    }

    // =========================================================================
    // 私有辅助方法
    // =========================================================================

    /// 将数据库行映射为 MarketplacePlugin 实体。
    ///
    /// 处理 tags 字段从 JSON 字符串到 serde_json::Value 的自动转换。
    ///
    /// # Arguments
    ///
    /// * `row` - 数据库行数据。
    /// * `schema` - 数据集 schema。
    ///
    /// # Returns
    ///
    /// 映射后的 MarketplacePlugin 实体，所有字段使用 `unwrap_or_default()` 保证非空安全。
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

    /// 将数据库行映射为 MarketplacePluginVersion 实体。
    ///
    /// 处理 dependencies 和 compatibility 字段从 JSON 字符串到 serde_json::Value 的自动转换。
    ///
    /// # Arguments
    ///
    /// * `row` - 数据库行数据。
    /// * `schema` - 数据集 schema。
    ///
    /// # Returns
    ///
    /// 映射后的 MarketplacePluginVersion 实体。
    fn row_to_version(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> MarketplacePluginVersion {
        let deps_str: Option<String> = row.get_by_name_as(schema, "dependencies");
        let dependencies = deps_str.and_then(|s| serde_json::from_str(&s).ok());
        let compat_str: Option<String> = row.get_by_name_as(schema, "compatibility");
        let compatibility = compat_str.and_then(|s| serde_json::from_str(&s).ok());

        MarketplacePluginVersion {
            id: row.get_by_name_as(schema, "id").unwrap_or_default(),
            plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
            version: row.get_by_name_as(schema, "version").unwrap_or_default(),
            version_rank: row.get_by_name_as(schema, "version_rank"),
            changelog: row.get_by_name_as(schema, "changelog"),
            release_notes: row.get_by_name_as(schema, "release_notes"),
            download_url: row.get_by_name_as(schema, "download_url"),
            storage_file_id: row.get_by_name_as(schema, "storage_file_id"),
            package_size: row.get_by_name_as(schema, "package_size"),
            checksum: row.get_by_name_as(schema, "checksum"),
            min_platform_version: row.get_by_name_as(schema, "min_platform_version"),
            max_platform_version: row.get_by_name_as(schema, "max_platform_version"),
            dependencies,
            compatibility,
            status: row.get_by_name_as(schema, "status"),
            is_latest: row.get_by_name_as::<i64>(schema, "is_latest").map(|v| v as i16),
            is_stable: row.get_by_name_as::<i64>(schema, "is_stable").map(|v| v as i16),
            download_count: row.get_by_name_as(schema, "download_count"),
            published_at: row.get_by_name_as(schema, "published_at"),
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
