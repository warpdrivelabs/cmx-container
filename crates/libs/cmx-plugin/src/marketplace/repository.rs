//! 插件市场数据仓库
//!
//! 提供 `cmx_marketplace_plugin`、`cmx_marketplace_plugin_version`、
//! `cmx_marketplace_download_stats`、`cmx_marketplace_rating` 四张表的数据库操作。
//!
//! 使用 `DatabaseManager` 执行 SQL，通过 `execute_sql_with_json` 和
//! `query_sql_with_json` 方法进行参数化查询。

use cmx_database::DatabaseManager;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::model::{
    CategoryInfo, MarketplaceFilter, MarketplacePlugin,
    MarketplacePluginVersion, MarketplaceRating,
};
use crate::error::{PluginError, PluginResult};

/// 插件市场数据仓库
///
/// 封装所有插件市场相关的数据库操作，包括插件的增删改查、
/// 版本管理、下载统计和评分记录。
pub struct MarketplaceRepository {
    /// 数据库管理器
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    default_db_id: String,
}

impl MarketplaceRepository {
    /// 创建新的数据仓库
    ///
    /// # 参数
    /// * `db_manager` - 数据库管理器
    /// * `default_db_id` - 默认数据库ID
    pub fn new(db_manager: Arc<DatabaseManager>, default_db_id: String) -> Self {
        Self {
            db_manager,
            default_db_id,
        }
    }

    /// 获取默认数据库ID
    pub fn default_db_id(&self) -> &str {
        &self.default_db_id
    }

    // ==================== 插件主表操作 ====================

    /// 插入插件记录
    ///
    /// # 参数
    /// * `plugin` - 插件数据
    pub async fn insert_plugin(&self, plugin: &MarketplacePlugin) -> PluginResult<()> {
        let sql = r#"
            INSERT INTO cmx_marketplace_plugin (
                id, plugin_id, name, description, short_description,
                icon_url, category, tags, vendor_name, vendor_url,
                vendor_contact, license_type, homepage_url, documentation_url,
                repository_url, status, is_featured, is_official,
                avg_rating, rating_count, download_count, install_count,
                domain_code, application_code, module_code, plugin_type,
                archived, create_time, update_time, create_by, create_name,
                update_by, update_name
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14,
                $15, $16, $17, $18,
                $19, $20, $21, $22,
                $23, $24, $25, $26,
                $27, NOW(), NOW(), $28, $29,
                $30, $31
            )
        "#;

        let tags_json = plugin.tags.as_ref().map(|v| v.to_string());
        let params = json!([
            plugin.id,
            plugin.plugin_id,
            plugin.name,
            plugin.description,
            plugin.short_description,
            plugin.icon_url,
            plugin.category,
            tags_json,
            plugin.vendor_name,
            plugin.vendor_url,
            plugin.vendor_contact,
            plugin.license_type,
            plugin.homepage_url,
            plugin.documentation_url,
            plugin.repository_url,
            plugin.status,
            plugin.is_featured,
            plugin.is_official,
            plugin.avg_rating,
            plugin.rating_count,
            plugin.download_count,
            plugin.install_count,
            plugin.domain_code,
            plugin.application_code,
            plugin.module_code,
            plugin.plugin_type,
            plugin.archived.unwrap_or(0),
            plugin.create_by,
            plugin.create_name,
            plugin.update_by,
            plugin.update_name,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("插入市场插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 根据 plugin_id 获取插件
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    pub async fn get_plugin_by_plugin_id(&self, plugin_id: &str) -> PluginResult<Option<MarketplacePlugin>> {
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
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_marketplace_plugin")
            .await
            .map_err(|e| PluginError::Database(format!("查询市场插件失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_plugin(row, schema)))
    }

    /// 根据 id 获取插件
    ///
    /// # 参数
    /// * `id` - 主键
    pub async fn get_plugin_by_id(&self, id: &str) -> PluginResult<Option<MarketplacePlugin>> {
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
            WHERE id = $1
        "#;

        let params = json!([id]);
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_marketplace_plugin_by_id")
            .await
            .map_err(|e| PluginError::Database(format!("查询市场插件失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_plugin(row, schema)))
    }

    /// 更新插件信息
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
        let sql = r#"
            UPDATE cmx_marketplace_plugin SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                short_description = COALESCE($4, short_description),
                category = COALESCE($5, category),
                tags = COALESCE($6::jsonb, tags),
                status = COALESCE($7, status),
                is_featured = COALESCE($8, is_featured),
                is_official = COALESCE($9, is_official),
                icon_url = COALESCE($10, icon_url),
                license_type = COALESCE($11, license_type),
                homepage_url = COALESCE($12, homepage_url),
                documentation_url = COALESCE($13, documentation_url),
                repository_url = COALESCE($14, repository_url),
                vendor_name = COALESCE($15, vendor_name),
                vendor_url = COALESCE($16, vendor_url),
                vendor_contact = COALESCE($17, vendor_contact),
                update_time = NOW()
            WHERE plugin_id = $1
        "#;

        let params = json!([
            plugin_id,
            name,
            description,
            short_description,
            category,
            tags,
            status,
            is_featured,
            is_official,
            icon_url,
            license_type,
            homepage_url,
            documentation_url,
            repository_url,
            vendor_name,
            vendor_url,
            vendor_contact,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新市场插件失败: {}", e)))?;

        Ok(())
    }

    /// 删除插件（逻辑删除，设置 archived=1）
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
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

    /// 分页查询插件列表
    ///
    /// 支持关键词搜索、分类过滤、标签过滤、状态过滤、排序等。
    ///
    /// # 参数
    /// * `filter` - 过滤条件
    /// * `page` - 页码（从 1 开始）
    /// * `size` - 每页大小
    pub async fn page_plugins(
        &self,
        filter: &MarketplaceFilter,
        page: u64,
        size: u64,
    ) -> PluginResult<(Vec<MarketplacePlugin>, u64)> {
        let offset = (page.saturating_sub(1)) * size;

        let mut where_clauses: Vec<String> = vec!["archived = 0".to_string()];
        let mut params: Vec<serde_json::Value> = Vec::new();
        let mut param_index = 1;

        // 关键词搜索
        if let Some(ref keyword) = filter.keyword
            && !keyword.is_empty()
        {
            where_clauses.push(format!("(name LIKE ${} OR description LIKE ${})", param_index, param_index + 1));
            params.push(json!(format!("%{}%", keyword)));
            params.push(json!(format!("%{}%", keyword)));
            param_index += 2;
        }

        if let Some(ref category) = filter.category
            && !category.is_empty()
        {
            where_clauses.push(format!("category = ${}", param_index));
            params.push(json!(category));
            param_index += 1;
        }

        if let Some(ref status) = filter.status
            && !status.is_empty()
        {
            where_clauses.push(format!("status = ${}", param_index));
            params.push(json!(status));
            param_index += 1;
        }

        if let Some(ref domain_code) = filter.domain_code
            && !domain_code.is_empty()
        {
            where_clauses.push(format!("domain_code = ${}", param_index));
            params.push(json!(domain_code));
            param_index += 1;
        }

        if let Some(ref application_code) = filter.application_code
            && !application_code.is_empty()
        {
            where_clauses.push(format!("application_code = ${}", param_index));
            params.push(json!(application_code));
            param_index += 1;
        }

        if let Some(ref module_code) = filter.module_code
            && !module_code.is_empty()
        {
            where_clauses.push(format!("module_code = ${}", param_index));
            params.push(json!(module_code));
            param_index += 1;
        }

        let where_clause = where_clauses.join(" AND ");

        // 排序
        let sort_by = filter.sort_by.as_deref().unwrap_or("update_time");
        let sort_order = filter.sort_order.as_deref().unwrap_or("desc");
        let order_clause = match sort_by {
            "download_count" => format!("download_count {}", sort_order),
            "avg_rating" => format!("avg_rating {}", sort_order),
            "create_time" => format!("create_time {}", sort_order),
            _ => format!("update_time {}", sort_order),
        };

        // 查询总数
        let count_sql = format!(
            "SELECT COUNT(*) as total FROM cmx_marketplace_plugin WHERE {}",
            where_clause
        );
        let count_result = self.db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                &count_sql,
                json!(params.clone()),
                "count_marketplace_plugins",
            )
            .await
            .map_err(|e| PluginError::Database(format!("统计市场插件数量失败: {}", e)))?;

        let total: u64 = count_result
            .iter()
            .next()
            .map(|r| {
                let schema = count_result.schema.as_ref();
                r.get_by_name_as::<i64>(schema, "total").unwrap_or(0) as u64
            })
            .unwrap_or(0);

        // 查询数据
        let data_sql = format!(
            r#"
            SELECT id, plugin_id, name, description, short_description,
                   icon_url, category, tags, vendor_name, vendor_url,
                   vendor_contact, license_type, homepage_url, documentation_url,
                   repository_url, status, is_featured, is_official,
                   avg_rating, rating_count, download_count, install_count,
                   domain_code, application_code, module_code, plugin_type,
                   archived, create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin
            WHERE {}
            ORDER BY {}
            LIMIT ${} OFFSET ${}
            "#,
            where_clause, order_clause, param_index, param_index + 1
        );

        params.push(json!(size as i64));
        params.push(json!(offset as i64));

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, &data_sql, json!(params), "page_marketplace_plugins")
            .await
            .map_err(|e| PluginError::Database(format!("分页查询市场插件失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut plugins = Vec::new();
        for row in result.iter() {
            plugins.push(Self::row_to_plugin(row, schema));
        }

        Ok((plugins, total))
    }

    /// 将数据库行转换为 MarketplacePlugin 模型
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

    // ==================== 版本表操作 ====================

    /// 插入版本记录
    ///
    /// # 参数
    /// * `version` - 版本数据
    pub async fn insert_version(&self, version: &MarketplacePluginVersion) -> PluginResult<()> {
        let sql = r#"
            INSERT INTO cmx_marketplace_plugin_version (
                id, plugin_id, version, version_rank, changelog,
                release_notes, download_url, package_size, checksum,
                min_platform_version, max_platform_version, dependencies,
                compatibility, status, is_latest, is_stable,
                download_count, published_at, archived,
                create_time, update_time, create_by, create_name,
                update_by, update_name
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19,
                NOW(), NOW(), $20, $21,
                $22, $23
            )
        "#;

        let deps_json = version.dependencies.as_ref().map(|v| v.to_string());
        let compat_json = version.compatibility.as_ref().map(|v| v.to_string());

        let params = json!([
            version.id,
            version.plugin_id,
            version.version,
            version.version_rank,
            version.changelog,
            version.release_notes,
            version.download_url,
            version.package_size,
            version.checksum,
            version.min_platform_version,
            version.max_platform_version,
            deps_json,
            compat_json,
            version.status,
            version.is_latest,
            version.is_stable,
            version.download_count,
            version.published_at,
            version.archived.unwrap_or(0),
            version.create_by,
            version.create_name,
            version.update_by,
            version.update_name,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("插入市场版本记录失败: {}", e)))?;

        Ok(())
    }

    /// 根据 id 获取版本
    ///
    /// # 参数
    /// * `id` - 主键
    pub async fn get_version_by_id(&self, id: &str) -> PluginResult<Option<MarketplacePluginVersion>> {
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, package_size, checksum,
                   min_platform_version, max_platform_version, dependencies,
                   compatibility, status, is_latest, is_stable,
                   download_count, published_at, archived,
                   create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE id = $1
        "#;

        let params = json!([id]);
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_marketplace_version")
            .await
            .map_err(|e| PluginError::Database(format!("查询市场版本失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_version(row, schema)))
    }

    /// 获取插件的所有版本列表
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    pub async fn list_versions_by_plugin_id(&self, plugin_id: &str) -> PluginResult<Vec<MarketplacePluginVersion>> {
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, package_size, checksum,
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
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "list_marketplace_versions")
            .await
            .map_err(|e| PluginError::Database(format!("查询市场版本列表失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut versions = Vec::new();
        for row in result.iter() {
            versions.push(Self::row_to_version(row, schema));
        }

        Ok(versions)
    }

    /// 获取插件的最新稳定版本
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    pub async fn get_latest_stable_version(&self, plugin_id: &str) -> PluginResult<Option<MarketplacePluginVersion>> {
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, package_size, checksum,
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
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_latest_stable_version")
            .await
            .map_err(|e| PluginError::Database(format!("查询最新稳定版本失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_version(row, schema)))
    }

    /// 获取插件的指定版本
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `version` - 版本号
    pub async fn get_version(&self, plugin_id: &str, version: &str) -> PluginResult<Option<MarketplacePluginVersion>> {
        let sql = r#"
            SELECT id, plugin_id, version, version_rank, changelog,
                   release_notes, download_url, package_size, checksum,
                   min_platform_version, max_platform_version, dependencies,
                   compatibility, status, is_latest, is_stable,
                   download_count, published_at, archived,
                   create_time, update_time, create_by, create_name,
                   update_by, update_name
            FROM cmx_marketplace_plugin_version
            WHERE plugin_id = $1 AND version = $2 AND archived = 0
        "#;

        let params = json!([plugin_id, version]);
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_marketplace_version_by_number")
            .await
            .map_err(|e| PluginError::Database(format!("查询市场版本失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        Ok(Some(Self::row_to_version(row, schema)))
    }

    /// 将数据库行转换为 MarketplacePluginVersion 模型
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
            package_size: row.get_by_name_as(schema, "package_size"),
            checksum: row.get_by_name_as(schema, "checksum"),
            min_platform_version: row.get_by_name_as(schema, "min_platform_version"),
            max_platform_version: row.get_by_name_as(schema, "max_platform_version"),
            dependencies,
            compatibility,
            status: row.get_by_name_as(schema, "status"),
            is_latest: row.get_by_name_as::<i32>(schema, "is_latest").map(|v| v as i16),
            is_stable: row.get_by_name_as::<i32>(schema, "is_stable").map(|v| v as i16),
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

    // ==================== 下载统计操作 ====================

    /// UPSERT 下载统计记录
    ///
    /// 如果当天该插件+版本+来源的记录已存在，则累加计数；否则插入新记录。
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `version` - 版本号
    /// * `download_date` - 下载日期（YYYY-MM-DD）
    /// * `source_type` - 来源类型
    pub async fn upsert_download_stat(
        &self,
        plugin_id: &str,
        version: &str,
        download_date: &str,
        source_type: &str,
    ) -> PluginResult<()> {
        let id = Uuid::new_v4().to_string();
        let sql = r#"
            INSERT INTO cmx_marketplace_download_stats (
                id, plugin_id, version, download_date, download_count,
                install_count, source_type, archived, create_time, update_time
            ) VALUES ($1, $2, $3, $4::date, 1, 0, $5, 0, NOW(), NOW())
            ON CONFLICT (plugin_id, version, download_date, source_type) DO UPDATE SET
                download_count = cmx_marketplace_download_stats.download_count + 1,
                update_time = NOW()
        "#;

        let params = json!([id, plugin_id, version, download_date, source_type]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("更新下载统计失败: {}", e)))?;

        Ok(())
    }

    /// 增加插件主表的总下载量
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
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

    /// 增加插件主表的总安装量
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
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

    /// 获取热门插件（按指定天数内的下载量排序）
    ///
    /// # 参数
    /// * `since_date` - 起始日期（YYYY-MM-DD）
    /// * `limit` - 返回数量限制
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
        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_trending_plugins")
            .await
            .map_err(|e| PluginError::Database(format!("查询热门插件失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut plugins = Vec::new();
        for row in result.iter() {
            plugins.push(Self::row_to_plugin(row, schema));
        }

        Ok(plugins)
    }

    // ==================== 评分操作 ====================

    /// 插入或更新评分记录
    ///
    /// 如果用户已对该插件评过分，则更新评分和评论；否则插入新记录。
    ///
    /// # 参数
    /// * `rating` - 评分数据
    pub async fn upsert_rating(&self, rating: &MarketplaceRating) -> PluginResult<()> {
        let sql = r#"
            INSERT INTO cmx_marketplace_rating (
                id, plugin_id, user_id, rating, review, status,
                archived, create_time, update_time, create_by, create_name,
                update_by, update_name
            ) VALUES ($1, $2, $3, $4, $5, $6, 0, NOW(), NOW(), $7, $8, $9, $10)
            ON CONFLICT (plugin_id, user_id) DO UPDATE SET
                rating = EXCLUDED.rating,
                review = EXCLUDED.review,
                status = EXCLUDED.status,
                update_time = NOW(),
                update_by = EXCLUDED.update_by,
                update_name = EXCLUDED.update_name
        "#;

        let params = json!([
            rating.id,
            rating.plugin_id,
            rating.user_id,
            rating.rating,
            rating.review,
            rating.status,
            rating.create_by,
            rating.create_name,
            rating.update_by,
            rating.update_name,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("插入/更新评分失败: {}", e)))?;

        Ok(())
    }

    /// 获取插件的评分列表
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `status` - 评分状态过滤（可选）
    pub async fn list_ratings(
        &self,
        plugin_id: &str,
        status: Option<&str>,
    ) -> PluginResult<Vec<MarketplaceRating>> {
        let sql = if status.is_some() {
            r#"
                SELECT id, plugin_id, user_id, rating, review, status,
                       archived, create_time, update_time, create_by, create_name,
                       update_by, update_name
                FROM cmx_marketplace_rating
                WHERE plugin_id = $1 AND status = $2 AND archived = 0
                ORDER BY create_time DESC
            "#
        } else {
            r#"
                SELECT id, plugin_id, user_id, rating, review, status,
                       archived, create_time, update_time, create_by, create_name,
                       update_by, update_name
                FROM cmx_marketplace_rating
                WHERE plugin_id = $1 AND archived = 0
                ORDER BY create_time DESC
            "#
        };

        let params = if let Some(s) = status {
            json!([plugin_id, s])
        } else {
            json!([plugin_id])
        };

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "list_marketplace_ratings")
            .await
            .map_err(|e| PluginError::Database(format!("查询评分列表失败: {}", e)))?;

        let schema = result.schema.as_ref();
        let mut ratings = Vec::new();
        for row in result.iter() {
            ratings.push(Self::row_to_rating(row, schema));
        }

        Ok(ratings)
    }

    /// 更新插件评分汇总（平均评分和评分数量）
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
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

    /// 将数据库行转换为 MarketplaceRating 模型
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

    // ==================== 分类操作 ====================

    /// 获取分类列表（包含每个分类的插件数量）
    pub async fn list_categories(&self) -> PluginResult<Vec<CategoryInfo>> {
        let sql = r#"
            SELECT category, COUNT(*) as count
            FROM cmx_marketplace_plugin
            WHERE archived = 0 AND status = 'published' AND category IS NOT NULL
            GROUP BY category
            ORDER BY count DESC
        "#;

        let result = self.db_manager
            .query_sql(&self.default_db_id, None, sql, "list_marketplace_categories")
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
}
