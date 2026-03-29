//! 版本历史仓库模块
//!
//! 提供插件版本历史的增删改查操作

use chrono::{DateTime, Utc};
use sea_query::{Alias, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::error::{PluginError, PluginResult};

/// 版本历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistoryRecord {
    pub id: String,
    pub plugin_id: String,
    pub version: String,
    pub install_path: String,
    pub wasm_path: String,
    pub is_current: bool,
    pub installed_at: DateTime<Utc>,
    pub uninstalled_at: Option<DateTime<Utc>>,
    pub zip_source_url: Option<String>,
    pub zip_source_type: Option<String>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub archived: i32,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 版本历史更新字段（不含 WHERE 条件字段）
#[derive(Debug, Clone, Default)]
pub struct VersionHistoryUpdateFields {
    pub install_path: Option<String>,
    pub wasm_path: Option<String>,
    pub is_current: Option<bool>,
    pub uninstalled_at: Option<DateTime<Utc>>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 版本历史仓库
pub struct VersionHistoryRepository {
    db_manager: Arc<DatabaseManager>,
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
    pub async fn insert_version(&self, record: &VersionHistoryRecord, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::insert();
        query
            .into_table("cmx_plugin_versions")
            .columns(vec![
                "id", "plugin_id", "version",
                "install_path", "wasm_path", "is_current",
                "installed_at", "uninstalled_at",
                "zip_source_url", "zip_source_type",
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
                record.uninstalled_at.clone().into(),
                record.zip_source_url.clone().into(),
                record.zip_source_type.clone().into(),
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
    /// - `record`: 版本历史记录
    /// - `txn_id`: 事务ID
    ///
    /// # 返回
    /// - `Ok(true)`: 新插入的记录
    /// - `Ok(false)`: 更新的记录
    pub async fn upsert_version(
        &self,
        record: &VersionHistoryRecord,
        txn_id: Option<&str>,
    ) -> PluginResult<bool> {
        let mut query = Query::insert();
        query
            .into_table(Alias::new("cmx_plugin_versions"))
            .columns(vec![
                Alias::new("id"),
                Alias::new("plugin_id"),
                Alias::new("version"),
                Alias::new("install_path"),
                Alias::new("wasm_path"),
                Alias::new("is_current"),
                Alias::new("installed_at"),
                Alias::new("uninstalled_at"),
                Alias::new("zip_source_url"),
                Alias::new("zip_source_type"),
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
                record.version.clone().into(),
                record.install_path.clone().into(),
                record.wasm_path.clone().into(),
                record.is_current.into(),
                record.installed_at.into(),
                record.uninstalled_at.clone().into(),
                record.zip_source_url.clone().into(),
                record.zip_source_type.clone().into(),
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
            Alias::new("version"),
        ])
        .update_columns(vec![
            Alias::new("install_path"),
            Alias::new("wasm_path"),
            Alias::new("is_current"),
            Alias::new("uninstalled_at"),
            Alias::new("zip_source_url"),
            Alias::new("zip_source_type"),
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

        if let Some(row) = result.iter().next() {
            if let Some(cmx_core::model::cell::DataValue::Bool(is_inserted)) = row.get(0) {
                return Ok(*is_inserted);
            }
        }

        Ok(false)
    }

    /// 更新版本历史记录（通过主键 ID）
    pub async fn update_version(&self, id: &str, fields: &VersionHistoryUpdateFields, txn_id: Option<&str>) -> PluginResult<()> {
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
            query.value("uninstalled_at", uninstalled_at.clone());
        }
        if let Some(ref update_time) = fields.uninstalled_at {
            query.value("update_time", update_time.clone());
        }

        query.value("update_by", fields.update_by.clone());
        query.value("update_name", fields.update_name.clone());

        query.and_where(sea_query::Expr::col("id").eq(id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新版本历史失败: {}", e)))?;

        Ok(())
    }

    /// 物理删除插件的所有版本历史记录
    pub async fn delete_versions_by_plugin_id(&self, plugin_id: &str, txn_id: Option<&str>) -> PluginResult<()> {
        let sql = "DELETE FROM cmx_plugin_versions WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, txn_id, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("删除版本历史记录失败: {}", e)))?;

        Ok(())
    }

    /// 查询插件的版本历史
    pub async fn list_versions(&self, plugin_id: &str) -> PluginResult<Vec<VersionHistoryRecord>> {
        let sql = "SELECT * FROM cmx_plugin_versions WHERE plugin_id = $1 ORDER BY installed_at DESC";
        let params = serde_json::json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "version_history_list")
            .await
            .map_err(|e| PluginError::Database(format!("查询版本历史失败: {}", e)))?;

        Self::parse_version_record(&result)
    }

    /// 查询指定版本
    pub async fn find_version(&self, plugin_id: &str, version: &str) -> PluginResult<Option<VersionHistoryRecord>> {
        let sql = "SELECT * FROM cmx_plugin_versions WHERE plugin_id = $1 AND version = $2";
        let params = serde_json::json!([plugin_id, version]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "version_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询指定版本失败: {}", e)))?;

        Self::parse_version_record(&result).map(|r| r.into_iter().next())
    }

    /// 获取当前基线版本（is_current=true 的版本）
    pub async fn get_current_baseline(&self, plugin_id: &str) -> PluginResult<Option<VersionHistoryRecord>> {
        let sql = "SELECT * FROM cmx_plugin_versions WHERE plugin_id = $1 AND is_current = true";
        let params = serde_json::json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "current_baseline_query")
            .await
            .map_err(|e| PluginError::Database(format!("获取当前基线版本失败: {}", e)))?;

        Self::parse_version_record(&result).map(|r| r.into_iter().next())
    }

    /// 将所有版本标记为非当前
    pub async fn mark_all_not_current(&self, plugin_id: &str, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_versions");
        query.value("is_current", false);
        query.and_where(sea_query::Expr::col("plugin_id").eq(plugin_id));

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
    ///
    /// # 参数
    /// - `plugin_id`: 插件ID
    /// - `version`: 版本号
    /// - `install_path`: 安装路径
    /// - `wasm_path`: WASM文件路径
    /// - `txn_id`: 事务ID
    pub async fn set_current_version(
        &self,
        plugin_id: &str,
        version: &str,
        install_path: &str,
        wasm_path: &str,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        // 1. 标记所有版本为非当前
        self.mark_all_not_current(plugin_id, txn_id).await?;

        // 2. 检查版本记录是否存在
        let existing = self.find_version(plugin_id, version).await?;

        if let Some(ref record) = existing {
            // 3a. 更新现有记录为当前版本
            let update_fields = VersionHistoryUpdateFields {
                install_path: Some(install_path.to_string()),
                wasm_path: Some(wasm_path.to_string()),
                is_current: Some(true),
                uninstalled_at: None,
                update_time: Utc::now(),
                create_by: None,
                create_name: None,
                update_by: None,
                update_name: None,
            };
            self.update_version(&record.id, &update_fields, txn_id).await?;
        } else {
            // 3b. 插入新版本记录（此时没有来源信息，来源信息应在插入时由调用方提供）
            let record = VersionHistoryRecord {
                id: uuid::Uuid::new_v4().to_string(),
                plugin_id: plugin_id.to_string(),
                version: version.to_string(),
                install_path: install_path.to_string(),
                wasm_path: wasm_path.to_string(),
                is_current: true,
                installed_at: Utc::now(),
                uninstalled_at: None,
                zip_source_url: None,
                zip_source_type: None,
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

    /// 解析版本历史记录
    fn parse_version_record(dataset: &DataSet) -> PluginResult<Vec<VersionHistoryRecord>> {
        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        for row in dataset.iter() {
            let get_datetime_default = |col_name: &str, default_fn: fn() -> DateTime<Utc>| -> DateTime<Utc> {
                row.get_by_name_as(schema, col_name).unwrap_or_else(default_fn)
            };

            let record = VersionHistoryRecord {
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
