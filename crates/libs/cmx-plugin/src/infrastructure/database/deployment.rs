//! 部署记录仓库模块
//!
//! 提供插件节点部署记录的增删改查操作

use chrono::{DateTime, Utc};
use sea_query::{PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::error::{PluginError, PluginResult};

/// 部署记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: String,
    pub plugin_id: String,
    pub node_id: String,
    pub node_name: Option<String>,
    pub node_type: Option<String>,
    pub version: String,
    pub deployment_type: String,
    pub status: String,
    pub progress: i32,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
    pub sync_token: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub deployed_at: DateTime<Utc>,
    pub validated_at: Option<DateTime<Utc>>,
    pub archived: i32,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 部署更新字段
#[derive(Debug, Clone, Default)]
pub struct DeploymentUpdateFields {
    pub version: Option<String>,
    pub deployment_type: Option<String>,
    pub status: Option<String>,
    pub progress: Option<i32>,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
    pub sync_token: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub validated_at: Option<DateTime<Utc>>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 部署记录仓库
pub struct DeploymentRepository {
    db_manager: Arc<DatabaseManager>,
    default_db_id: String,
}

impl DeploymentRepository {
    /// 创建新的部署仓库
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

    /// 插入部署记录
    pub async fn insert_deployment(&self, record: &DeploymentRecord, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::insert();
        query
            .into_table("cmx_plugin_deployments")
            .columns(vec![
                "id", "plugin_id", "node_id", "node_name", "node_type", "version",
                "deployment_type", "status", "progress", "error_message", "error_details",
                "sync_token", "last_sync_at", "deployed_at", "validated_at",
                "archived", "create_by", "create_name", "update_by", "update_name"
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.node_id.clone().into(),
                record.node_name.clone().into(),
                record.node_type.clone().into(),
                record.version.clone().into(),
                record.deployment_type.clone().into(),
                record.status.clone().into(),
                record.progress.into(),
                record.error_message.clone().into(),
                record.error_details.clone().into(),
                record.sync_token.clone().into(),
                record.last_sync_at.clone().into(),
                record.deployed_at.into(),
                record.validated_at.clone().into(),
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
            .map_err(|e| PluginError::Database(format!("插入部署记录失败: {}", e)))?;

        Ok(())
    }

    /// 更新部署记录
    pub async fn update_deployment(&self, id: &str, fields: &DeploymentUpdateFields, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_deployments");

        if let Some(ref version) = fields.version {
            query.value("version", version.clone());
        }

        if let Some(ref deployment_type) = fields.deployment_type {
            query.value("deployment_type", deployment_type.clone());
        }

        if let Some(ref status) = fields.status {
            query.value("status", status.clone());
        }

        if let Some(progress) = fields.progress {
            query.value("progress", progress);
        }

        if let Some(ref error_message) = fields.error_message {
            query.value("error_message", error_message.clone());
        }

        if let Some(ref error_details) = fields.error_details {
            query.value("error_details", error_details.clone());
        }

        if let Some(ref sync_token) = fields.sync_token {
            query.value("sync_token", sync_token.clone());
        }

        if let Some(ref last_sync_at) = fields.last_sync_at {
            query.value("last_sync_at", last_sync_at.clone());
        }

        if let Some(ref validated_at) = fields.validated_at {
            query.value("validated_at", validated_at.clone());
        }

        query.value("update_by", fields.update_by.clone());
        query.value("update_name", fields.update_name.clone());

        query.and_where(sea_query::Expr::col("id").eq(id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新部署记录失败: {}", e)))?;

        Ok(())
    }

    /// 查询节点上的插件部署
    pub async fn find_deployment(&self, plugin_id: &str, node_id: &str) -> PluginResult<Option<DeploymentRecord>> {
        let sql = "SELECT * FROM cmx_plugin_deployments WHERE plugin_id = $1 AND node_id = $2";
        let params = serde_json::json!([plugin_id, node_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "deployment_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询部署记录失败: {}", e)))?;

        Self::parse_deployment_record(&result).map(|r| r.into_iter().next())
    }

    /// 查询插件在所有节点的部署情况
    pub async fn list_plugin_deployments(&self, plugin_id: &str) -> PluginResult<Vec<DeploymentRecord>> {
        let sql = "SELECT * FROM cmx_plugin_deployments WHERE plugin_id = $1 ORDER BY deployed_at DESC";
        let params = serde_json::json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "deployment_list")
            .await
            .map_err(|e| PluginError::Database(format!("查询插件部署列表失败: {}", e)))?;

        Self::parse_deployment_record(&result)
    }

    /// 查询节点的所有部署
    pub async fn list_node_deployments(&self, node_id: &str) -> PluginResult<Vec<DeploymentRecord>> {
        let sql = "SELECT * FROM cmx_plugin_deployments WHERE node_id = $1 ORDER BY deployed_at DESC";
        let params = serde_json::json!([node_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "deployment_list")
            .await
            .map_err(|e| PluginError::Database(format!("查询节点部署列表失败: {}", e)))?;

        Self::parse_deployment_record(&result)
    }

    /// 更新节点的部署版本
    pub async fn update_node_version(&self, plugin_id: &str, node_id: &str, version: &str, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_deployments");
        query.value("version", version);
        query.value("last_sync_at", Utc::now());
        query.and_where(sea_query::Expr::col("plugin_id").eq(plugin_id));
        query.and_where(sea_query::Expr::col("node_id").eq(node_id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新节点版本失败: {}", e)))?;

        Ok(())
    }

    /// 删除节点上的插件部署
    pub async fn delete_deployment(&self, plugin_id: &str, node_id: &str, txn_id: Option<&str>) -> PluginResult<()> {
        let mut query = Query::delete();
        query.from_table("cmx_plugin_deployments");
        query.and_where(sea_query::Expr::col("plugin_id").eq(plugin_id));
        query.and_where(sea_query::Expr::col("node_id").eq(node_id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("删除部署记录失败: {}", e)))?;

        Ok(())
    }

    /// 解析部署记录
    fn parse_deployment_record(dataset: &DataSet) -> PluginResult<Vec<DeploymentRecord>> {
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

            let record = DeploymentRecord {
                id: get_string("id").unwrap_or_default(),
                plugin_id: get_string("plugin_id").unwrap_or_default(),
                node_id: get_string("node_id").unwrap_or_default(),
                node_name: get_opt_string("node_name"),
                node_type: get_opt_string("node_type"),
                version: get_string("version").unwrap_or_default(),
                deployment_type: get_string("deployment_type").unwrap_or_default(),
                status: get_string("status").unwrap_or_default(),
                progress: get_i32("progress"),
                error_message: get_opt_string("error_message"),
                error_details: get_opt_string("error_details"),
                sync_token: get_opt_string("sync_token"),
                last_sync_at: get_opt_datetime("last_sync_at"),
                deployed_at: get_datetime("deployed_at"),
                validated_at: get_opt_datetime("validated_at"),
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

impl Default for DeploymentRepository {
    fn default() -> Self {
        Self::new(
            Arc::new(DatabaseManager::new(Default::default())),
            "default".to_string(),
        )
    }
}
