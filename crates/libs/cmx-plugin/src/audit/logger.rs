//! 审计日志模块
//!
//! 记录操作日志到数据库

use std::sync::Arc;
use chrono::{DateTime, Utc};
use sea_query::{PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::audit::record::{AuditRecord, OperationResult, OperationType};
use crate::error::{PluginError, PluginResult};

/// 审计日志配置
#[derive(Clone)]
pub struct AuditLoggerConfig {
    /// 数据库管理器
    pub db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    pub default_db_id: String,
    /// 节点ID
    pub node_id: String,
}

impl AuditLoggerConfig {
    /// 创建新的审计日志配置
    pub fn new(
        db_manager: Arc<DatabaseManager>,
        default_db_id: String,
        node_id: String,
    ) -> Self {
        Self {
            db_manager,
            default_db_id,
            node_id,
        }
    }
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        Self {
            db_manager: Arc::new(DatabaseManager::new(Default::default())),
            default_db_id: "default".to_string(),
            node_id: "default".to_string(),
        }
    }
}

/// 审计日志记录器
pub struct AuditLogger {
    /// 配置
    config: AuditLoggerConfig,
}

impl AuditLogger {
    /// 创建新的审计日志记录器
    pub fn new(config: AuditLoggerConfig) -> Self {
        Self { config }
    }

    /// 记录操作（持久化到数据库）
    pub async fn log(&self, mut record: AuditRecord) -> PluginResult<()> {
        // 设置完成时间和耗时
        let duration_ms = if let Some(duration) = record.duration_ms {
            duration
        } else {
            (Utc::now() - record.started_at).num_milliseconds()
        };
        record.completed_at = Some(Utc::now());
        record.duration_ms = Some(duration_ms);

        // 如果没有设置 node_id，使用默认的节点ID
        if record.node_id.is_none() {
            record.node_id = Some(self.config.node_id.clone());
        }

        // 持久化到数据库
        self.insert_record(&record).await
    }

    /// 插入记录到数据库
    async fn insert_record(&self, record: &AuditRecord) -> PluginResult<()> {
        let mut query = Query::insert();
        query
            .into_table("cmx_plugin_audit_log")
            .columns(vec![
                "id", "plugin_id", "node_id", "version_id", "deployment_id",
                "operation_type", "operation_status", "operator", "operator_ip",
                "operator_session", "request_id", "correlation_id", "details",
                "old_value", "new_value", "error_code", "error_message",
                "stack_trace", "started_at", "completed_at", "duration_ms",
                "archived", "create_by", "create_name", "update_by", "update_name"
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.node_id.clone().into(),
                record.version_id.clone().into(),
                record.deployment_id.clone().into(),
                record.operation.to_string().into(),
                record.result.to_string().into(),
                record.operator.clone().into(),
                record.operator_ip.clone().into(),
                record.operator_session.clone().into(),
                record.request_id.clone().into(),
                record.correlation_id.clone().into(),
                record.details.clone().into(),
                record.old_value.clone().into(),
                record.new_value.clone().into(),
                record.error_code.clone().into(),
                record.error_message.clone().into(),
                record.stack_trace.clone().into(),
                record.started_at.into(),
                record.completed_at.clone().into(),
                record.duration_ms.into(),
                0.into(),
                record.operator.clone().into(),
                record.operator.clone().into(),
                record.operator.clone().into(),
                record.operator.clone().into(),
            ])
            .map_err(|e| PluginError::Database(format!("构建插入语句失败: {}", e)))?;

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.config.db_manager
            .execute_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("插入审计日志失败: {}", e)))?;

        Ok(())
    }

    /// 获取所有记录
    pub async fn get_all(&self, limit: Option<usize>, offset: Option<usize>) -> PluginResult<Vec<AuditRecord>> {
        let mut query = Query::select();
        query
            .from("cmx_plugin_audit_log")
            .columns(vec![
                "id", "plugin_id", "node_id", "version_id", "deployment_id",
                "operation_type", "operation_status", "operator", "operator_ip",
                "operator_session", "request_id", "correlation_id", "details",
                "old_value", "new_value", "error_code", "error_message",
                "stack_trace", "started_at", "completed_at", "duration_ms"
            ])
            .order_by("started_at", sea_query::Order::Desc);

        if let Some(limit_val) = limit {
            query.limit(limit_val as u64);
        }

        if let Some(offset_val) = offset {
            query.offset(offset_val as u64);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.config.db_manager
            .query_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values, "audit_get_all")
            .await
            .map_err(|e| PluginError::Database(format!("查询审计日志失败: {}", e)))?;

        Self::parse_records(&result)
    }

    /// 获取指定插件的记录
    pub async fn get_by_plugin(&self, plugin_id: &str, limit: Option<usize>) -> PluginResult<Vec<AuditRecord>> {
        let mut query = Query::select();
        query
            .from("cmx_plugin_audit_log")
            .columns(vec![
                "id", "plugin_id", "node_id", "version_id", "deployment_id",
                "operation_type", "operation_status", "operator", "operator_ip",
                "operator_session", "request_id", "correlation_id", "details",
                "old_value", "new_value", "error_code", "error_message",
                "stack_trace", "started_at", "completed_at", "duration_ms"
            ])
            .and_where(sea_query::Expr::col("plugin_id").eq(plugin_id))
            .order_by("started_at", sea_query::Order::Desc);

        if let Some(limit_val) = limit {
            query.limit(limit_val as u64);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.config.db_manager
            .query_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values, "audit_get_by_plugin")
            .await
            .map_err(|e| PluginError::Database(format!("查询审计日志失败: {}", e)))?;

        Self::parse_records(&result)
    }

    /// 获取指定时间范围的记录
    pub async fn get_by_time_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> PluginResult<Vec<AuditRecord>> {
        let mut query = Query::select();
        query
            .from("cmx_plugin_audit_log")
            .columns(vec![
                "id", "plugin_id", "node_id", "version_id", "deployment_id",
                "operation_type", "operation_status", "operator", "operator_ip",
                "operator_session", "request_id", "correlation_id", "details",
                "old_value", "new_value", "error_code", "error_message",
                "stack_trace", "started_at", "completed_at", "duration_ms"
            ])
            .and_where(sea_query::Expr::col("started_at").gte(start))
            .and_where(sea_query::Expr::col("started_at").lte(end))
            .order_by("started_at", sea_query::Order::Desc);

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.config.db_manager
            .query_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values, "audit_get_by_time_range")
            .await
            .map_err(|e| PluginError::Database(format!("查询审计日志失败: {}", e)))?;

        Self::parse_records(&result)
    }

    /// 获取指定节点的记录
    pub async fn get_by_node(&self, node_id: &str, limit: Option<usize>) -> PluginResult<Vec<AuditRecord>> {
        let mut query = Query::select();
        query
            .from("cmx_plugin_audit_log")
            .columns(vec![
                "id", "plugin_id", "node_id", "version_id", "deployment_id",
                "operation_type", "operation_status", "operator", "operator_ip",
                "operator_session", "request_id", "correlation_id", "details",
                "old_value", "new_value", "error_code", "error_message",
                "stack_trace", "started_at", "completed_at", "duration_ms"
            ])
            .and_where(sea_query::Expr::col("node_id").eq(node_id))
            .order_by("started_at", sea_query::Order::Desc);

        if let Some(limit_val) = limit {
            query.limit(limit_val as u64);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.config.db_manager
            .query_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values, "audit_get_by_node")
            .await
            .map_err(|e| PluginError::Database(format!("查询审计日志失败: {}", e)))?;

        Self::parse_records(&result)
    }

    /// 清空所有记录（软删除，设置为 archived = 1）
    pub async fn clear(&self) -> PluginResult<()> {
        let mut query = Query::update();
        query.table("cmx_plugin_audit_log");
        query.value("archived", 1);

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.config.db_manager
            .execute_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("清空审计日志失败: {}", e)))?;

        Ok(())
    }

    /// 获取记录数量
    pub async fn len(&self) -> PluginResult<i64> {
        let mut query = Query::select();
        query
            .from("cmx_plugin_audit_log")
            .expr(sea_query::Expr::col("id").count())
            .and_where(sea_query::Expr::col("archived").eq(0));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.config.db_manager
            .query_sql_with_sqlxvalues(&self.config.default_db_id, None, &sql, sql_values, "audit_count")
            .await
            .map_err(|e| PluginError::Database(format!("统计审计日志失败: {}", e)))?;

        Self::parse_count(&result)
    }

    /// 检查是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await.map(|c| c == 0).unwrap_or(true)
    }

    /// 解析记录列表
    fn parse_records(dataset: &DataSet) -> PluginResult<Vec<AuditRecord>> {
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

            let get_i64 = |col_name: &str| -> Option<i64> {
                row.get_by_name(schema, col_name)
                    .and_then(|v| if let DataValue::Int(n) = v { Some(*n) } else { None })
            };

            let get_opt_json = |col_name: &str| -> Option<serde_json::Value> {
                row.get_by_name(schema, col_name).and_then(|v| {
                    if let DataValue::Json(s) = v {
                        serde_json::from_str(s).ok()
                    } else if let DataValue::String(s) = v {
                        serde_json::from_str(s).ok()
                    } else {
                        None
                    }
                })
            };

            let operation_str = get_string("operation_type").unwrap_or_default();
            let operation = match operation_str.as_str() {
                "install" => OperationType::Install,
                "uninstall" => OperationType::Uninstall,
                "activate" => OperationType::Activate,
                "deactivate" => OperationType::Deactivate,
                "upgrade" => OperationType::Upgrade,
                "downgrade" => OperationType::Downgrade,
                "rollback" => OperationType::Rollback,
                "config_update" => OperationType::ConfigUpdate,
                _ => OperationType::Install,
            };

            let result_str = get_string("operation_status").unwrap_or_default();
            let result = match result_str.as_str() {
                "success" => OperationResult::Success,
                "failure" => OperationResult::Failure,
                _ => OperationResult::Success,
            };

            let record = AuditRecord {
                id: get_string("id").unwrap_or_default(),
                plugin_id: get_string("plugin_id").unwrap_or_default(),
                node_id: get_opt_string("node_id"),
                version_id: get_opt_string("version_id"),
                deployment_id: get_opt_string("deployment_id"),
                operation,
                result,
                operator: get_opt_string("operator"),
                operator_ip: get_opt_string("operator_ip"),
                operator_session: get_opt_string("operator_session"),
                request_id: get_opt_string("request_id"),
                correlation_id: get_opt_string("correlation_id"),
                details: get_opt_json("details"),
                old_value: get_opt_string("old_value"),
                new_value: get_opt_string("new_value"),
                error_code: get_opt_string("error_code"),
                error_message: get_opt_string("error_message"),
                stack_trace: get_opt_string("stack_trace"),
                started_at: get_datetime("started_at"),
                completed_at: get_opt_datetime("completed_at"),
                duration_ms: get_i64("duration_ms"),
            };

            records.push(record);
        }

        Ok(records)
    }

    /// 解析计数结果
    fn parse_count(dataset: &DataSet) -> PluginResult<i64> {
        if dataset.row_count() > 0 {
            let row = dataset.iter().next();
            if let Some(row) = row {
                return row.get_by_name(dataset.schema.as_ref(), "count")
                    .and_then(|v| {
                        if let DataValue::Int(n) = v {
                            Some(*n)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| PluginError::Database("解析计数结果失败".to_string()));
            }
        }
        Ok(0)
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self {
            config: AuditLoggerConfig::default(),
        }
    }
}