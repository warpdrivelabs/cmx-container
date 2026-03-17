//! 插件数据库实现 - 基于 cmx-database
//!
//! 实现 PluginDatabase trait，提供实际的数据库操作。

use async_trait::async_trait;
use chrono::Utc;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::db::{
    AuditDbRecord, DeploymentDbRecord, PluginDbError, PluginDbRecord,
    PluginDatabase, PluginUpdateFields, RollbackDbRecord, VersionDbRecord,
};

/// 插件数据库实现 - 基于 cmx-database
pub struct CmxPluginDatabase {
    db_manager: DatabaseManager,
}

impl CmxPluginDatabase {
    /// 创建新的插件数据库实例
    pub fn new(db_manager: DatabaseManager) -> Self {
        Self { db_manager }
    }

    /// 将 PluginDbRecord 转换为 JSON 参数
    fn record_to_json(record: &PluginDbRecord) -> serde_json::Value {
        serde_json::json!([
            record.plugin_id,
            record.name,
            record.version,
            record.status,
            record.wasm_path,
            record.install_path,
            record.config_path,
            record.db_id,
            record.is_system,
            record.is_locked,
            record.domain_code,
            record.application_code,
            record.module_code,
            record.vendor_name,
            record.vendor_url,
            record.vendor_contact,
            record.metadata,
            record.signature_algorithm,
            record.signer_key_id,
            record.created_at.to_rfc3339(),
            record.updated_at.to_rfc3339(),
        ])
    }

    /// 从 DataSet 解析 PluginDbRecord
    fn parse_record_from_dataset(_dataset: &DataSet, _row_idx: usize) -> Option<PluginDbRecord> {
        // TODO: 实现从 DataSet 解析记录
        None
    }
}

#[async_trait]
impl PluginDatabase for CmxPluginDatabase {
    /// 插入插件记录
    async fn insert_plugin(&self, record: &PluginDbRecord) -> Result<(), PluginDbError> {
        let sql = r#"
            INSERT INTO cmx_plugin (
                plugin_id, name, version, status, wasm_path, install_path, config_path,
                db_id, is_system, is_locked, domain_code, application_code, module_code,
                vendor_name, vendor_url, vendor_contact, metadata, signature_algorithm,
                signer_key_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
        "#;

        let params = Self::record_to_json(record);

        self.db_manager
            .execute_sql_with_params(&record.db_id, None, sql, params)
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 更新插件记录
    async fn update_plugin(&self, db_id: &str, plugin_id: &str, updates: &PluginUpdateFields) -> Result<(), PluginDbError> {
        let mut set_clauses = Vec::new();
        let mut params: Vec<serde_json::Value> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref version) = updates.version {
            set_clauses.push(format!("version = ${}", param_idx));
            params.push(serde_json::json!(version));
            param_idx += 1;
        }

        if let Some(ref status) = updates.status {
            set_clauses.push(format!("status = ${}", param_idx));
            params.push(serde_json::json!(status));
            param_idx += 1;
        }

        if let Some(ref wasm_path) = updates.wasm_path {
            set_clauses.push(format!("wasm_path = ${}", param_idx));
            params.push(serde_json::json!(wasm_path));
            param_idx += 1;
        }

        if let Some(ref install_path) = updates.install_path {
            set_clauses.push(format!("install_path = ${}", param_idx));
            params.push(serde_json::json!(install_path));
            param_idx += 1;
        }

        if let Some(activated_at) = updates.activated_at {
            set_clauses.push(format!("activated_at = ${}", param_idx));
            params.push(serde_json::json!(activated_at.to_rfc3339()));
            param_idx += 1;
        }

        // 添加 updated_at
        set_clauses.push(format!("updated_at = ${}", param_idx));
        params.push(serde_json::json!(Utc::now().to_rfc3339()));
        param_idx += 1;

        // 添加 plugin_id 条件
        params.push(serde_json::json!(plugin_id));

        let sql = format!(
            "UPDATE cmx_plugin SET {} WHERE plugin_id = ${}",
            set_clauses.join(", "),
            param_idx
        );

        self.db_manager
            .execute_sql_with_params(db_id, None, &sql, serde_json::json!(params))
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 删除插件记录
    async fn delete_plugin(&self, db_id: &str, plugin_id: &str) -> Result<(), PluginDbError> {
        let sql = "DELETE FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        self.db_manager
            .execute_sql_with_params(db_id, None, sql, params)
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 根据 plugin_id 查询插件
    async fn get_plugin_by_id(&self, db_id: &str, plugin_id: &str) -> Result<Option<PluginDbRecord>, PluginDbError> {
        let sql = "SELECT * FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        let dataset: DataSet = self.db_manager
            .query_sql_with_params(db_id, None, sql, params, "plugin")
            .await
            .map_err(|e| PluginDbError::Query(e.to_string()))?;

        if dataset.rows.is_empty() {
            return Ok(None);
        }

        Ok(Self::parse_record_from_dataset(&dataset, 0))
    }

    /// 查询所有插件
    async fn get_all_plugins(&self, db_id: &str) -> Result<Vec<PluginDbRecord>, PluginDbError> {
        let sql = "SELECT * FROM cmx_plugin ORDER BY created_at DESC";
        let dataset: DataSet = self.db_manager
            .query_sql(db_id, None, sql, "plugins")
            .await
            .map_err(|e| PluginDbError::Query(e.to_string()))?;

        let mut records = Vec::new();
        for i in 0..dataset.rows.len() {
            if let Some(record) = Self::parse_record_from_dataset(&dataset, i) {
                records.push(record);
            }
        }

        Ok(records)
    }

    /// 插入版本记录
    async fn insert_version(&self, db_id: &str, record: &VersionDbRecord) -> Result<(), PluginDbError> {
        let sql = r#"
            INSERT INTO cmx_plugin_versions (
                plugin_id, version, version_type, from_version, install_path, wasm_path,
                backup_path, is_current, installed_at, uninstalled_at, installed_by, install_reason
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#;

        let params = serde_json::json!([
            record.plugin_id,
            record.version,
            record.version_type,
            record.from_version,
            record.install_path,
            record.wasm_path,
            record.backup_path,
            record.is_current,
            record.installed_at.to_rfc3339(),
            record.uninstalled_at.map(|dt| dt.to_rfc3339()),
            record.installed_by,
            record.install_reason,
        ]);

        self.db_manager
            .execute_sql_with_params(db_id, None, sql, params)
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 插入审计日志记录
    async fn insert_audit_log(&self, db_id: &str, record: &AuditDbRecord) -> Result<(), PluginDbError> {
        let sql = r#"
            INSERT INTO cmx_plugin_audit_log (
                plugin_id, operation_type, operator, status, details,
                error_message, client_ip, user_agent, timestamp
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#;

        let params = serde_json::json!([
            record.plugin_id,
            record.operation_type,
            record.operator,
            record.status,
            record.details,
            record.error_message,
            record.client_ip,
            record.user_agent,
            record.timestamp.to_rfc3339(),
        ]);

        self.db_manager
            .execute_sql_with_params(db_id, None, sql, params)
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 查询审计日志
    async fn query_audit_logs(
        &self,
        db_id: &str,
        plugin_id: Option<&str>,
        operation_type: Option<&str>,
        limit: u64,
    ) -> Result<Vec<AuditDbRecord>, PluginDbError> {
        let mut sql = String::from("SELECT * FROM cmx_plugin_audit_log WHERE 1=1");
        let mut params: Vec<serde_json::Value> = Vec::new();
        let mut param_idx = 1;

        if let Some(pid) = plugin_id {
            sql.push_str(&format!(" AND plugin_id = ${}", param_idx));
            params.push(serde_json::json!(pid));
            param_idx += 1;
        }

        if let Some(op_type) = operation_type {
            sql.push_str(&format!(" AND operation_type = ${}", param_idx));
            params.push(serde_json::json!(op_type));
            param_idx += 1;
        }

        sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT ${}", param_idx));
        params.push(serde_json::json!(limit));

        let dataset: DataSet = self.db_manager
            .query_sql_with_params(db_id, None, &sql, serde_json::json!(params), "audit_logs")
            .await
            .map_err(|e| PluginDbError::Query(e.to_string()))?;

        // TODO: 解析 dataset 为 AuditDbRecord 列表
        let _ = dataset;
        Ok(Vec::new())
    }

    /// 插入部署记录
    async fn insert_deployment(&self, db_id: &str, record: &DeploymentDbRecord) -> Result<(), PluginDbError> {
        let sql = r#"
            INSERT INTO cmx_plugin_deployments (
                plugin_id, node_id, version, status, deployed_at, error_message
            ) VALUES ($1, $2, $3, $4, $5, $6)
        "#;

        let params = serde_json::json!([
            record.plugin_id,
            record.node_id,
            record.version,
            record.status,
            record.deployed_at.to_rfc3339(),
            record.error_message,
        ]);

        self.db_manager
            .execute_sql_with_params(db_id, None, sql, params)
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }

    /// 插入回滚记录
    async fn insert_rollback(&self, db_id: &str, record: &RollbackDbRecord) -> Result<(), PluginDbError> {
        let sql = r#"
            INSERT INTO cmx_plugin_rollback (
                operation_id, plugin_id, from_version, to_version, backup_path, status
            ) VALUES ($1, $2, $3, $4, $5, $6)
        "#;

        let params = serde_json::json!([
            record.operation_id,
            record.plugin_id,
            record.from_version,
            record.to_version,
            record.backup_path,
            record.status,
        ]);

        self.db_manager
            .execute_sql_with_params(db_id, None, sql, params)
            .await
            .map_err(|e| PluginDbError::Operation(e.to_string()))?;

        Ok(())
    }
}
