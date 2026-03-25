//! 部署记录仓库模块
//!
//! 提供插件节点部署记录的增删改查操作

use chrono::{DateTime, Utc};
use sea_query::{PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use modql::field::Fields;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::error::{PluginError, PluginResult};

/// 部署记录
#[derive(Debug, Clone, Serialize, Deserialize,Fields)]
pub struct DeploymentRecord {
    pub id: String,
    pub plugin_id: String,
    pub node_id: String,
    pub node_type: Option<String>,
    pub version: String,
    /// 部署状态
    pub status: String,
   ///  进度 (0-100)
    pub progress: i32,
    /// 错误消息
    pub error_message: Option<String>,
    /// 错误详情
    pub error_details: Option<String>,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 归档标志
    pub archived: i32,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 部署更新字段 通过plugin_id node_id version作为where条件更新
#[derive(Debug, Clone, Default,Serialize, Deserialize,Fields)]
pub struct DeploymentUpdateFields {
    pub plugin_id: String,
    pub node_id: String,
    pub version: String,
    pub status: Option<String>,
    pub progress: Option<i32>,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
    /// 归档标志
    pub archived: i32,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
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
        use sea_query::{PostgresQueryBuilder, Query};
        use sea_query_binder::SqlxBinder;

        let mut query = Query::insert();
        query
            .into_table("cmx_plugin_deployments")
            .columns(vec![
                "id", "plugin_id", "node_id", "node_type", "version",
                "status", "progress", "error_message", "error_details",
                "archived", "create_by", "create_name", "update_by", "update_name"
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.node_id.clone().into(),
                record.node_type.clone().into(),
                record.version.clone().into(),
                record.status.clone().into(),
                record.progress.into(),
                record.error_message.clone().into(),
                record.error_details.clone().into(),
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
        use sea_query::{Expr, PostgresQueryBuilder, Query};
        use sea_query_binder::SqlxBinder;

        let mut query = Query::update();
        query.table("cmx_plugin_deployments");

        if fields.version != String::default() {
            query.value("version", fields.version.clone());
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

        if let Some(ref update_by) = fields.update_by {
            query.value("update_by", update_by.clone());
        }

        if let Some(ref update_name) = fields.update_name {
            query.value("update_name", update_name.clone());
        }

        query.and_where(Expr::col("id").eq(id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新部署记录失败: {}", e)))?;

        Ok(())
    }

    /// 查询节点上的插件部署
    pub async fn find_deployment(&self, plugin_id: &str, node_id: &str, version: &str) -> PluginResult<Option<DeploymentRecord>> {
        let sql = "SELECT * FROM cmx_plugin_deployments WHERE plugin_id = $1 AND node_id = $2 AND version = $3 and archived =0";
        let params = serde_json::json!([plugin_id, node_id, version]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "cmx_plugin_deployments")
            .await
            .map_err(|e| PluginError::Database(format!("查询部署记录失败: {}", e)))?;

        Self::parse_deployment_record(&result).map(|r| r.into_iter().next())
    }

    /// 查询插件在所有节点的部署情况
    pub async fn list_plugin_deployments(&self, plugin_id: &str) -> PluginResult<Vec<DeploymentRecord>> {
        let sql = "SELECT * FROM cmx_plugin_deployments WHERE plugin_id = $1 and archived =0 ORDER BY create_time DESC";
        let params = serde_json::json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "deployment_list")
            .await
            .map_err(|e| PluginError::Database(format!("查询插件部署列表失败: {}", e)))?;

        Self::parse_deployment_record(&result)
    }

    /// 查询节点的所有部署
    pub async fn list_node_deployments(&self, node_id: &str) -> PluginResult<Vec<DeploymentRecord>> {
        let sql = "SELECT * FROM cmx_plugin_deployments WHERE node_id = $1 and archived =0 ORDER BY create_time DESC";
        let params = serde_json::json!([node_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "deployment_list")
            .await
            .map_err(|e| PluginError::Database(format!("查询节点部署列表失败: {}", e)))?;

        Self::parse_deployment_record(&result)
    }

    // /// 更新节点的部署版本
    // pub async fn update_node_version(&self, plugin_id: &str, node_id: &str, version: &str, txn_id: Option<&str>) -> PluginResult<()> {
    //     let mut query = Query::update();
    //     query.table("cmx_plugin_deployments");
    //     query.value("version", version);
    //     query.and_where(sea_query::Expr::col("plugin_id").eq(plugin_id));
    //     query.and_where(sea_query::Expr::col("node_id").eq(node_id));
    //
    //     let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
    //
    //     self.db_manager
    //         .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
    //         .await
    //         .map_err(|e| PluginError::Database(format!("更新节点版本失败: {}", e)))?;
    //
    //     Ok(())
    // }

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
            let get_datetime_default = |col_name: &str, default_fn: fn() -> DateTime<Utc>| -> DateTime<Utc> {
                row.get_by_name_as(schema, col_name).unwrap_or_else(default_fn)
            };

            let record = DeploymentRecord {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                node_id: row.get_by_name_as(schema, "node_id").unwrap_or_default(),
                node_type: row.get_by_name_as(schema, "node_type"),
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                status: row.get_by_name_as(schema, "status").unwrap_or_default(),
                progress: row.get_by_name_as(schema, "progress").unwrap_or(0),
                error_message: row.get_by_name_as(schema, "error_message"),
                error_details: row.get_by_name_as(schema, "error_details"),
                archived: row.get_by_name_as(schema, "archived").unwrap_or(0),
                create_by: row.get_by_name_as(schema, "create_by"),
                create_name: row.get_by_name_as(schema, "create_name"),
                update_by: row.get_by_name_as(schema, "update_by"),
                update_name: row.get_by_name_as(schema, "update_name"),
                create_time: get_datetime_default("create_time", Utc::now),
                update_time: get_datetime_default("update_time", Utc::now),
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
