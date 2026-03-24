//! 版本历史仓库模块
//!
//! 提供插件版本历史的增删改查操作

use chrono::{DateTime, Utc};
use sea_query::{PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::error::{PluginError, PluginResult};

/// 版本历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistoryRecord {
    pub id: String,
    pub plugin_id: String,
    pub version: String,
    pub version_type: String,
    pub from_version: Option<String>,
    pub install_path: String,
    pub wasm_path: String,
    pub backup_path: Option<String>,
    pub is_current: bool,
    pub installed_at: DateTime<Utc>,
    pub uninstalled_at: Option<DateTime<Utc>>,
    pub installed_by: Option<String>,
    pub install_reason: Option<String>,
    pub archived: i32,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 版本历史更新字段
#[derive(Debug, Clone, Default)]
pub struct VersionHistoryUpdateFields {
    pub from_version: Option<String>,
    pub install_path: Option<String>,
    pub wasm_path: Option<String>,
    pub backup_path: Option<String>,
    pub is_current: Option<bool>,
    pub uninstalled_at: Option<DateTime<Utc>>,
    pub installed_by: Option<String>,
    pub install_reason: Option<String>,
    pub archived: Option<i32>,
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
                "id", "plugin_id", "version", "version_type", "from_version",
                "install_path", "wasm_path", "backup_path", "is_current",
                "installed_at", "uninstalled_at", "installed_by", "install_reason",
                "archived", "create_by", "create_name", "update_by", "update_name"
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.version.clone().into(),
                record.version_type.clone().into(),
                record.from_version.clone().into(),
                record.install_path.clone().into(),
                record.wasm_path.clone().into(),
                record.backup_path.clone().into(),
                record.is_current.into(),
                record.installed_at.into(),
                record.uninstalled_at.clone().into(),
                record.installed_by.clone().into(),
                record.install_reason.clone().into(),
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

    /// 更新版本历史记录
    pub async fn update_version(&self, id: &str, fields: &VersionHistoryUpdateFields, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_versions");

        if let Some(ref from_version) = fields.from_version {
            query.value("from_version", from_version.clone());
        }

        if let Some(ref install_path) = fields.install_path {
            query.value("install_path", install_path.clone());
        }

        if let Some(ref wasm_path) = fields.wasm_path {
            query.value("wasm_path", wasm_path.clone());
        }

        if let Some(ref backup_path) = fields.backup_path {
            query.value("backup_path", backup_path.clone());
        }

        if let Some(is_current) = fields.is_current {
            query.value("is_current", is_current);
        }

        if let Some(ref uninstalled_at) = fields.uninstalled_at {
            query.value("uninstalled_at", uninstalled_at.clone());
        }

        if let Some(ref installed_by) = fields.installed_by {
            query.value("installed_by", installed_by.clone());
        }

        if let Some(ref install_reason) = fields.install_reason {
            query.value("install_reason", install_reason.clone());
        }

        if let Some(archived) = fields.archived {
            query.value("archived", archived);
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

    /// 解析版本历史记录
    fn parse_version_record(dataset: &DataSet) -> PluginResult<Vec<VersionHistoryRecord>> {
        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        for row in dataset.iter() {
            let get_string = |col_name: &str| -> Option<String> {
                row.get_by_name(schema, col_name)
                    .and_then(|v| if let DataValue::String(s) = v { Some(s.clone()) } else { None })
            };

            let get_opt_string = |col_name: &str| -> Option<String> {
                row.get_by_name(schema, col_name).and_then(|v| match v {
                    DataValue::Null => None,
                    DataValue::String(s) => Some(s.clone()),
                    _ => None,
                })
            };

            let get_bool = |col_name: &str| -> bool {
                row.get_by_name(schema, col_name)
                    .map(|v| if let DataValue::Bool(b) = v { *b } else { false })
                    .unwrap_or(false)
            };

            let get_i32 = |col_name: &str| -> i32 {
                row.get_by_name(schema, col_name)
                    .and_then(|v| if let DataValue::Int(n) = v { Some(*n as i32) } else { None })
                    .unwrap_or(0)
            };

            let get_opt_datetime = |col_name: &str| -> Option<DateTime<Utc>> {
                row.get_by_name(schema, col_name).and_then(|v| {
                    if let DataValue::String(s) = v {
                        DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
                    } else {
                        None
                    }
                })
            };

            let get_datetime = |col_name: &str| -> DateTime<Utc> {
                row.get_by_name(schema, col_name)
                    .and_then(|v| {
                        if let DataValue::String(s) = v {
                            DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(Utc::now)
            };

            let record = VersionHistoryRecord {
                id: get_string("id").unwrap_or_default(),
                plugin_id: get_string("plugin_id").unwrap_or_default(),
                version: get_string("version").unwrap_or_default(),
                version_type: get_string("version_type").unwrap_or_default(),
                from_version: get_opt_string("from_version"),
                install_path: get_string("install_path").unwrap_or_default(),
                wasm_path: get_string("wasm_path").unwrap_or_default(),
                backup_path: get_opt_string("backup_path"),
                is_current: get_bool("is_current"),
                installed_at: get_datetime("installed_at"),
                uninstalled_at: get_opt_datetime("uninstalled_at"),
                installed_by: get_opt_string("installed_by"),
                install_reason: get_opt_string("install_reason"),
                archived: get_i32("archived"),
                create_by: get_opt_string("create_by"),
                create_name: get_opt_string("create_name"),
                update_by: get_opt_string("update_by"),
                update_name: get_opt_string("update_name"),
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