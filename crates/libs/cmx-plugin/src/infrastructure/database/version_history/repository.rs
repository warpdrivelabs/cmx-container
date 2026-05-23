//! 版本历史仓库
//!
//! 提供 `cmx_plugin_versions` 表的增删改查操作

use chrono::{DateTime, Utc};
use sea_query::{Alias, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use std::sync::Arc;

use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use super::model::{VersionCreateParams, VersionRecord, VersionUpdateParams};
use crate::error::{PluginError, PluginResult};

/// 版本历史仓库
pub struct VersionHistoryRepository {
    /// 数据库管理器
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    default_db_id: String,
}

impl VersionHistoryRepository {
    /// 创建新的版本历史仓库
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

    /// 插入版本历史记录
     async fn insert_version(
        &self,
        record: &VersionCreateParams,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let mut query = Query::insert();
        query
            .into_table("cmx_plugin_versions")
            .columns(vec![
                "id", "plugin_id", "version",
                "install_path", "wasm_path", "is_current",
                "installed_at", "uninstalled_at",
                "zip_source_url", "zip_source_type",
                "plugin_type", "source_path",
                "marketplace_source_id",
                "create_time", "update_time",
                "archived", "create_by", "create_name", "update_by", "update_name"
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.version.clone().into(),
                record.install_path.clone().into(),
                record.wasm_path.clone().into(),
                record.is_current.into(),
                record.installed_at.into(),
                record.uninstalled_at.into(),
                record.zip_source_url.clone().into(),
                record.zip_source_type.clone().into(),
                record.plugin_type.clone().into(),
                record.source_path.clone().into(),
                record.marketplace_source_id.clone().into(),
                record.create_time.into(),
                record.update_time.into(),
                record.archived.into(),
                record.create_by.clone().into(),
                record.create_name.clone().into(),
                record.update_by.clone().into(),
                record.update_name.clone().into(),
            ])
            .map_err(|e| PluginError::Database(format!("构建插入语句失败: {}", e)))?;

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("插入版本历史失败: {}", e)))?;

        Ok(())
    }

    /// 插入或更新版本历史记录 (upsert)
    ///
    /// 使用 ON CONFLICT (plugin_id, version) DO UPDATE 实现 upsert 语义
    ///
    /// # 参数
    /// - `record`: 版本历史创建参数
    /// - `txn_id`: 事务ID
    ///
    /// # 返回
    /// - `Ok(true)`: 新插入的记录
    /// - `Ok(false)`: 更新的记录
    pub async fn upsert_version(
        &self,
        record: &VersionCreateParams,
        txn_id: Option<&str>,
    ) -> PluginResult<bool> {
        let mut query = Query::insert();
        query
            .into_table(Alias::new("cmx_plugin_versions"))
            .columns(vec![
                Alias::new("id"),
                Alias::new("plugin_id"),
                Alias::new("app_id"),
                Alias::new("version"),
                Alias::new("install_path"),
                Alias::new("wasm_path"),
                Alias::new("is_current"),
                Alias::new("installed_at"),
                Alias::new("uninstalled_at"),
                Alias::new("zip_source_url"),
                Alias::new("zip_source_type"),
                Alias::new("plugin_type"),
                Alias::new("source_path"),
                Alias::new("build_type"),
                Alias::new("marketplace_source_id"),
                Alias::new("create_time"),
                Alias::new("update_time"),
                Alias::new("archived"),
                Alias::new("create_by"),
                Alias::new("create_name"),
                Alias::new("update_by"),
                Alias::new("update_name"),
            ])
            .values_panic(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.app_id.clone().into(),
                record.version.clone().into(),
                record.install_path.clone().into(),
                record.wasm_path.clone().into(),
                record.is_current.into(),
                record.installed_at.into(),
                record.uninstalled_at.into(),
                record.zip_source_url.clone().into(),
                record.zip_source_type.clone().into(),
                record.plugin_type.clone().into(),
                record.source_path.clone().into(),
                record.build_type.clone().into(),
                record.marketplace_source_id.clone().into(),
                record.create_time.into(),
                record.update_time.into(),
                record.archived.into(),
                record.create_by.clone().into(),
                record.create_name.clone().into(),
                record.update_by.clone().into(),
                record.update_name.clone().into(),
            ]);

        let on_conflict = sea_query::OnConflict::columns(vec![
            Alias::new("plugin_id"),
            Alias::new("app_id"),
            Alias::new("version"),
        ])
        .update_columns(vec![
            Alias::new("install_path"),
            Alias::new("wasm_path"),
            Alias::new("is_current"),
            Alias::new("uninstalled_at"),
            Alias::new("zip_source_url"),
            Alias::new("zip_source_type"),
            Alias::new("plugin_type"),
            Alias::new("source_path"),
            Alias::new("build_type"),
            Alias::new("marketplace_source_id"),
            Alias::new("update_time"),
            Alias::new("update_by"),
            Alias::new("update_name"),
        ])
        .to_owned();

        query.on_conflict(on_conflict);

        let (mut sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        sql.push_str(" RETURNING (xmax = 0) AS is_inserted");

        let result = self
            .db_manager
            .query_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values, "upsert_version")
            .await
            .map_err(|e| PluginError::Database(format!("upsert版本历史失败: {}", e)))?;

        if let Some(row) = result.iter().next()
            && let Some(cmx_core::model::cell::DataValue::Bool(is_inserted)) = row.get(0) {
                return Ok(*is_inserted);
            }

        Ok(false)
    }

    /// 更新版本历史记录（通过主键 ID 和 app_id）
    ///
    /// # Arguments
    ///
    /// * `id` - 版本记录主键 ID
    /// * `app_id` - 应用隔离标识，用于多租户隔离
    /// * `fields` - 要更新的字段
    /// * `txn_id` - 事务 ID（可选）
    ///
    /// # Errors
    ///
    /// 数据库执行失败时返回 `PluginError::Database`
    pub async fn update_version(
        &self,
        id: &str,
        app_id: &str,
        fields: &VersionUpdateParams,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_versions");

        if let Some(ref install_path) = fields.install_path {
            query.value("install_path", install_path.clone());
        }

        if let Some(ref wasm_path) = fields.wasm_path {
            query.value("wasm_path", wasm_path.clone());
        }

        if let Some(is_current) = fields.is_current {
            query.value("is_current", is_current);
        }

        if let Some(ref uninstalled_at) = fields.uninstalled_at {
            query.value("uninstalled_at", *uninstalled_at);
        }

        if let Some(ref update_time) = fields.update_time {
            query.value("update_time", *update_time);
        }

        if let Some(ref update_by) = fields.update_by {
            query.value("update_by", update_by.clone());
        }

        if let Some(ref update_name) = fields.update_name {
            query.value("update_name", update_name.clone());
        }

        query.and_where(sea_query::Expr::col("id").eq(id));
        query.and_where(sea_query::Expr::col("app_id").eq(app_id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新版本历史失败: {}", e)))?;

        Ok(())
    }

    /// 物理删除插件的所有版本历史记录（按 plugin_id 和 app_id）
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件唯一标识
    /// * `app_id` - 应用隔离标识，用于多租户隔离
    /// * `txn_id` - 事务 ID（可选）
    ///
    /// # Errors
    ///
    /// 数据库执行失败时返回 `PluginError::Database`
    pub async fn delete_versions_by_plugin_id(
        &self,
        plugin_id: &str,
        app_id: &str,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let sql = "DELETE FROM cmx_plugin_versions WHERE plugin_id = $1 AND app_id = $2";
        let params = serde_json::json!([plugin_id, app_id]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, txn_id, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("删除版本历史记录失败: {}", e)))?;

        Ok(())
    }

    /// 查询插件的版本历史
    pub async fn list_versions(&self, plugin_id: &str) -> PluginResult<Vec<VersionRecord>> {
        let sql = "SELECT * FROM cmx_plugin_versions WHERE plugin_id = $1 ORDER BY installed_at DESC";
        let params = serde_json::json!([plugin_id]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "version_history_list")
            .await
            .map_err(|e| PluginError::Database(format!("查询版本历史失败: {}", e)))?;

        Self::parse_version_record(&result)
    }

    /// 查询指定版本
    pub async fn find_version(
        &self,
        plugin_id: &str,
        app_id: &str,
        version: &str,
        txn_id: Option<&str>,
    ) -> PluginResult<Option<VersionRecord>> {
        let sql = "SELECT * FROM cmx_plugin_versions WHERE plugin_id = $1 AND app_id = $2 AND version = $3";
        let params = serde_json::json!([plugin_id, app_id, version]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, txn_id, sql, params, "version_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询指定版本失败: {}", e)))?;

        Self::parse_version_record(&result).map(|r| r.into_iter().next())
    }

    /// 获取当前基线版本（is_current=true 的版本）
    pub async fn get_current_baseline(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Option<VersionRecord>> {
        let sql = "SELECT * FROM cmx_plugin_versions WHERE plugin_id = $1 AND is_current = true";
        let params = serde_json::json!([plugin_id]);

        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "current_baseline_query",
            )
            .await
            .map_err(|e| PluginError::Database(format!("获取当前基线版本失败: {}", e)))?;

        Self::parse_version_record(&result).map(|r| r.into_iter().next())
    }

    /// 将所有版本标记为非当前
    pub async fn mark_all_not_current(
        &self,
        plugin_id: &str,
        app_id: &str,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_versions");
        query.value("is_current", false);
        query.and_where(sea_query::Expr::col("plugin_id").eq(plugin_id));
        query.and_where(sea_query::Expr::col("app_id").eq(app_id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("标记非当前版本失败: {}", e)))?;

        Ok(())
    }

    /// 设置当前版本（原子操作）
    ///
    /// 1. 将所有版本标记为非当前
    /// 2. 插入或更新指定版本为当前
    pub async fn set_current_version(
        &self,
        plugin_id: &str,
        app_id: &str,
        version: &str,
        install_path: &str,
        wasm_path: &str,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        self.mark_all_not_current(plugin_id, app_id, txn_id).await?;

        // dbg!(plugin_id,version);
        let existing = self.find_version(plugin_id, app_id, version, txn_id).await?;
        // dbg!(&existing);

        if let Some(ref record) = existing {
            let update_fields = VersionUpdateParams {
                install_path: Some(install_path.to_string()),
                wasm_path: Some(wasm_path.to_string()),
                is_current: Some(true),
                uninstalled_at: None,
                update_time: Some(Utc::now()),
                create_by: None,
                create_name: None,
                update_by: None,
                update_name: None,
            };
            self.update_version(&record.id, app_id, &update_fields, txn_id).await?;
        } else {
            let record = VersionCreateParams {
                id: uuid::Uuid::new_v4().to_string(),
                plugin_id: plugin_id.to_string(),
                app_id: app_id.to_string(),
                version: version.to_string(),
                install_path: install_path.to_string(),
                wasm_path: wasm_path.to_string(),
                is_current: true,
                installed_at: Utc::now(),
                uninstalled_at: None,
                zip_source_url: None,
                zip_source_type: None,
                plugin_type: None,
                source_path: None,
                build_type: "release".to_string(),
                marketplace_source_id: None,
                create_time: Utc::now(),
                update_time: Utc::now(),
                archived: 0,
                create_by: None,
                create_name: None,
                update_by: None,
                update_name: None,
            };
            self.insert_version(&record, txn_id).await?;
        }

        Ok(())
    }

    /// 解析版本历史记录（从 DataSet 转换为 VersionRecord）
    fn parse_version_record(dataset: &DataSet) -> PluginResult<Vec<VersionRecord>> {
        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        for row in dataset.iter() {
            let get_datetime_default =
                |col_name: &str, default_fn: fn() -> DateTime<Utc>| -> DateTime<Utc> {
                    row.get_by_name_as(schema, col_name)
                        .unwrap_or_else(default_fn)
                };

            let record = VersionRecord {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                install_path: row.get_by_name_as(schema, "install_path").unwrap_or_default(),
                wasm_path: row.get_by_name_as(schema, "wasm_path").unwrap_or_default(),
                is_current: row.get_by_name_as(schema, "is_current").unwrap_or(false),
                installed_at: get_datetime_default("installed_at", Utc::now),
                uninstalled_at: row.get_by_name_as(schema, "uninstalled_at"),
                zip_source_url: row.get_by_name_as(schema, "zip_source_url"),
                zip_source_type: row.get_by_name_as(schema, "zip_source_type"),
                plugin_type: row.get_by_name_as(schema, "plugin_type"),
                source_path: row.get_by_name_as(schema, "source_path"),
                marketplace_source_id: row.get_by_name_as(schema, "marketplace_source_id"),
                create_time: get_datetime_default("create_time", Utc::now),
                update_time: get_datetime_default("update_time", Utc::now),
                archived: row.get_by_name_as(schema, "archived").unwrap_or(0),
                create_by: row.get_by_name_as(schema, "create_by"),
                create_name: row.get_by_name_as(schema, "create_name"),
                update_by: row.get_by_name_as(schema, "update_by"),
                update_name: row.get_by_name_as(schema, "update_name"),
            };

            records.push(record);
        }

        Ok(records)
    }
}

impl Default for VersionHistoryRepository {
    fn default() -> Self {
        Self::new(
            Arc::new(DatabaseManager::new(Default::default())),
            "default".to_string(),
        )
    }
}
