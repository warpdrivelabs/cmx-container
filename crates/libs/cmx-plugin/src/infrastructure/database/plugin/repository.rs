//! 插件数据仓库
//!
//! 提供 `cmx_plugin` 表的增删改查操作

use chrono::{DateTime, Utc};
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_database::DatabaseManager;
use std::sync::Arc;

use super::super::schema::SchemaManager;
use super::model::{PluginCreateParams, PluginRecord, PluginUpdateParams};
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};

/// 插件数据仓库
pub struct PluginRepository {
    /// 数据库管理器
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    default_db_id: String,
}

impl PluginRepository {
    /// 创建新的数据仓库
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

    /// 初始化系统表
    pub async fn init_system_tables(&self) -> PluginResult<()> {
        let sqls = SchemaManager::get_create_system_tables_sql();

        for sql in sqls {
            let statements: Vec<&str> = sql
                .split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.starts_with("--"))
                .collect();

            for statement in statements {
                if !statement.is_empty() {
                    self.db_manager
                        .execute_sql(&self.default_db_id, None, statement)
                        .await
                        .map_err(|e| PluginError::Database(format!("初始化系统表失败: {}", e)))?;
                }
            }
        }

        Ok(())
    }

    /// 插入插件记录
    pub async fn insert_plugin(
        &self,
        record: &PluginCreateParams,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        use sea_query::{PostgresQueryBuilder, Query};
        use sea_query_sqlx::SqlxBinder;

        let mut query = Query::insert();
        query
            .into_table("cmx_plugin")
            .columns(vec![
                "id",
                "app_id",
                "plugin_id",
                "name",
                "description",
                "version",
                "wasm_path",
                "install_path",
                "db_id",
                "status",
                "is_system",
                "is_locked",
                "domain_code",
                "application_code",
                "module_code",
                "vendor_name",
                "vendor_url",
                "vendor_contact",
                "metadata",
                "signature_algorithm",
                "signer_key_id",
                "zip_source_url",
                "zip_source_type",
                "plugin_type",
                "source_path",
                "marketplace_source_id",
                "storage_key",
                "storage_checksum",
                "create_time",
                "update_time",
            ])
            .values(vec![
                record.id.clone().into(),
                record.app_id.clone().into(),
                record.plugin_id.clone().into(),
                record.name.clone().into(),
                record.description.clone().into(),
                record.version.clone().into(),
                record.wasm_path.clone().into(),
                record.install_path.clone().into(),
                record.db_id.clone().into(),
                record.status.clone().into(),
                record.is_system.into(),
                record.is_locked.into(),
                record.domain_code.clone().into(),
                record.application_code.clone().into(),
                record.module_code.clone().into(),
                record.vendor_name.clone().into(),
                record.vendor_url.clone().into(),
                record.vendor_contact.clone().into(),
                record.metadata.clone().into(),
                record.signature_algorithm.clone().into(),
                record.signer_key_id.clone().into(),
                record.zip_source_url.clone().into(),
                record.zip_source_type.clone().into(),
                record.plugin_type.clone().into(),
                record.source_path.clone().into(),
                record.marketplace_source_id.clone().into(),
                record.storage_key.clone().into(),
                record.storage_checksum.clone().into(),
                record.create_time.into(),
                record.update_time.into(),
            ])
            .map_err(|e| PluginError::Database(format!("构建插入语句失败: {}", e)))?;

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("插入插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 更新插件记录，通过 plugin_id 作为 where 条件
    pub async fn update_plugin(
        &self,
        plugin_id: &str,
        app_id: &str,
        fields: &PluginUpdateParams,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        use sea_query::{Expr, ExprTrait, PostgresQueryBuilder, Query};
        use sea_query_sqlx::SqlxBinder;

        let mut query = Query::update();
        query.table("cmx_plugin");

        if let Some(ref v) = fields.version {
            query.value("version", v.clone());
        }
        if let Some(ref v) = fields.name {
            query.value("name", v.clone());
        }
        if let Some(ref v) = fields.description {
            query.value("description", v.clone());
        }

        if let Some(ref v) = fields.wasm_path {
            query.value("wasm_path", v.clone());
        }
        if let Some(ref v) = fields.install_path {
            query.value("install_path", v.clone());
        }
        if let Some(ref v) = fields.db_id {
            query.value("db_id", v.clone());
        }
        if let Some(ref v) = fields.status {
            query.value("status", v.clone());
        }
        if let Some(v) = fields.is_system {
            query.value("is_system", v);
        }
        if let Some(v) = fields.is_locked {
            query.value("is_locked", v);
        }
        if let Some(ref v) = fields.domain_code {
            query.value("domain_code", v.clone());
        }
        if let Some(ref v) = fields.application_code {
            query.value("application_code", v.clone());
        }
        if let Some(ref v) = fields.module_code {
            query.value("module_code", v.clone());
        }
        if let Some(ref v) = fields.vendor_name {
            query.value("vendor_name", v.clone());
        }
        if let Some(ref v) = fields.vendor_url {
            query.value("vendor_url", v.clone());
        }
        if let Some(ref v) = fields.vendor_contact {
            query.value("vendor_contact", v.clone());
        }
        if let Some(ref v) = fields.metadata {
            query.value("metadata", v.clone());
        }
        if let Some(ref v) = fields.signature_algorithm {
            query.value("signature_algorithm", v.clone());
        }
        if let Some(ref v) = fields.signer_key_id {
            query.value("signer_key_id", v.clone());
        }
        if let Some(ref v) = fields.zip_source_url {
            query.value("zip_source_url", v.clone());
        }
        if let Some(ref v) = fields.zip_source_type {
            query.value("zip_source_type", v.clone());
        }
        if let Some(ref v) = fields.plugin_type {
            query.value("plugin_type", v.clone());
        }
        if let Some(ref v) = fields.source_path {
            query.value("source_path", v.clone());
        }
        if let Some(ref v) = fields.marketplace_source_id {
            query.value("marketplace_source_id", v.clone());
        }
        if let Some(ref v) = fields.app_id {
            query.value("app_id", v.clone());
        }
        if let Some(ref v) = fields.storage_key {
            query.value("storage_key", v.clone());
        }
        if let Some(ref v) = fields.storage_checksum {
            query.value("storage_checksum", v.clone());
        }
        if let Some(ref v) = fields.update_by {
            query.value("update_by", v.clone());
        }
        if let Some(ref v) = fields.update_name {
            query.value("update_name", v.clone());
        }

        query.and_where(Expr::col("plugin_id").eq(plugin_id));
        query.and_where(Expr::col("app_id").eq(app_id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 删除插件记录（按 plugin_id 和 app_id 精确匹配）
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件唯一标识
    /// * `app_id` - 应用隔离标识，用于多租户隔离
    ///
    /// # Errors
    ///
    /// 数据库执行失败时返回 `PluginError::Database`
    pub async fn delete_plugin(&self, plugin_id: &str, app_id: &str) -> PluginResult<()> {
        let sql = "DELETE FROM cmx_plugin WHERE plugin_id = $1 AND app_id = $2";
        let params = serde_json::json!([plugin_id, app_id]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("删除插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 插入或更新插件记录 (upsert)
    ///
    /// 使用 ON CONFLICT (app_id, plugin_id) DO UPDATE 实现 upsert 语义
    ///
    /// # 参数
    /// - `record`: 插件创建参数
    /// - `txn_id`: 事务ID
    ///
    /// # 返回
    /// - `Ok(true)`: 新插入的记录
    /// - `Ok(false)`: 更新的记录
    pub async fn upsert_plugin(
        &self,
        record: &PluginCreateParams,
        txn_id: Option<&str>,
    ) -> PluginResult<bool> {
        use sea_query::{Alias, PostgresQueryBuilder, Query};
        use sea_query_sqlx::SqlxBinder;

        let mut query = Query::insert();
        query
            .into_table(Alias::new("cmx_plugin"))
            .columns(vec![
                Alias::new("id"),
                Alias::new("app_id"),
                Alias::new("plugin_id"),
                Alias::new("name"),
                Alias::new("description"),
                Alias::new("version"),
                Alias::new("wasm_path"),
                Alias::new("install_path"),
                Alias::new("db_id"),
                Alias::new("status"),
                Alias::new("is_system"),
                Alias::new("is_locked"),
                Alias::new("domain_code"),
                Alias::new("application_code"),
                Alias::new("module_code"),
                Alias::new("vendor_name"),
                Alias::new("vendor_url"),
                Alias::new("vendor_contact"),
                Alias::new("metadata"),
                Alias::new("signature_algorithm"),
                Alias::new("signer_key_id"),
                Alias::new("zip_source_url"),
                Alias::new("zip_source_type"),
                Alias::new("plugin_type"),
                Alias::new("source_path"),
                Alias::new("marketplace_source_id"),
                Alias::new("storage_key"),
                Alias::new("storage_checksum"),
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
                record.app_id.clone().into(),
                record.plugin_id.clone().into(),
                record.name.clone().into(),
                record.description.clone().into(),
                record.version.clone().into(),
                record.wasm_path.clone().into(),
                record.install_path.clone().into(),
                record.db_id.clone().into(),
                record.status.clone().into(),
                record.is_system.into(),
                record.is_locked.into(),
                record.domain_code.clone().into(),
                record.application_code.clone().into(),
                record.module_code.clone().into(),
                record.vendor_name.clone().into(),
                record.vendor_url.clone().into(),
                record.vendor_contact.clone().into(),
                record.metadata.clone().into(),
                record.signature_algorithm.clone().into(),
                record.signer_key_id.clone().into(),
                record.zip_source_url.clone().into(),
                record.zip_source_type.clone().into(),
                record.plugin_type.clone().into(),
                record.source_path.clone().into(),
                record.marketplace_source_id.clone().into(),
                record.storage_key.clone().into(),
                record.storage_checksum.clone().into(),
                record.create_time.into(),
                record.update_time.into(),
                record.archived.into(),
                record.create_by.clone().into(),
                record.create_name.clone().into(),
                record.update_by.clone().into(),
                record.update_name.clone().into(),
            ]);

        let on_conflict =
            sea_query::OnConflict::columns(vec![Alias::new("app_id"), Alias::new("plugin_id")])
                .update_columns(vec![
                    Alias::new("name"),
                    Alias::new("description"),
                    Alias::new("version"),
                    Alias::new("wasm_path"),
                    Alias::new("install_path"),
                    Alias::new("db_id"),
                    Alias::new("status"),
                    Alias::new("is_system"),
                    Alias::new("is_locked"),
                    Alias::new("domain_code"),
                    Alias::new("application_code"),
                    Alias::new("module_code"),
                    Alias::new("vendor_name"),
                    Alias::new("vendor_url"),
                    Alias::new("vendor_contact"),
                    Alias::new("signature_algorithm"),
                    Alias::new("signer_key_id"),
                    Alias::new("zip_source_url"),
                    Alias::new("zip_source_type"),
                    Alias::new("plugin_type"),
                    Alias::new("source_path"),
                    Alias::new("marketplace_source_id"),
                    Alias::new("storage_key"),
                    Alias::new("storage_checksum"),
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
            .query_sql_with_sqlxvalues(
                &self.default_db_id,
                txn_id,
                &sql,
                sql_values,
                "upsert_plugin",
            )
            .await
            .map_err(|e| PluginError::Database(format!("upsert插件记录失败: {}", e)))?;

        if let Some(row) = result.iter().next()
            && let Some(DataValue::Bool(is_inserted)) = row.get(0)
        {
            return Ok(*is_inserted);
        }

        Ok(false)
    }

    /// 查询插件记录（带 JOIN 域/应用/模块名称）
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `app_id` - 应用ID，用于多租户隔离
    pub async fn find_plugin(
        &self,
        plugin_id: &str,
        app_id: &str,
    ) -> PluginResult<Option<PluginRecord>> {
        let sql = r#"
            SELECT p.*,
                   d.name AS domain_name,
                   a.name AS application_name,
                   m.name AS module_name
            FROM cmx_plugin p
            LEFT JOIN cmx_domain d ON p.domain_code = d.code
            LEFT JOIN cmx_application a ON p.application_code = a.code
            LEFT JOIN cmx_module m ON p.module_code = m.code
            WHERE p.plugin_id = $1 AND p.app_id = $2
        "#;
        let params = serde_json::json!([plugin_id, app_id]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "plugin_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询插件记录失败: {}", e)))?;

        Self::parse_plugin_record(&result).map(|r| r.into_iter().next())
    }

    // /// 通过ID查询插件记录（带 JOIN 域/应用/模块名称）
    // pub async fn find_plugin_by_id(&self, id: &str) -> PluginResult<Option<PluginRecord>> {
    //     let sql = r#"
    //         SELECT p.*,
    //                d.name AS domain_name,
    //                a.name AS application_name,
    //                m.name AS module_name
    //         FROM cmx_plugin p
    //         LEFT JOIN cmx_domain d ON p.domain_code = d.code
    //         LEFT JOIN cmx_application a ON p.application_code = a.code
    //         LEFT JOIN cmx_module m ON p.module_code = m.code
    //         WHERE p.id = $1
    //     "#;
    //     let params = serde_json::json!([id]);
    //
    //     let result = self
    //         .db_manager
    //         .query_sql_with_json(&self.default_db_id, None, sql, params, "plugin_query")
    //         .await
    //         .map_err(|e| PluginError::Database(format!("查询插件记录失败: {}", e)))?;
    //
    //     Self::parse_plugin_record(&result).map(|r| r.into_iter().next())
    // }

    /// 列出所有插件（带 JOIN 域/应用/模块名称）
    pub async fn list_plugins(&self, filter: &PluginFilter) -> PluginResult<Vec<PluginRecord>> {
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        let mut param_index = 1;

        if let Some(ref app_id) = filter.app_id {
            conditions.push(format!("p.app_id = ${}", param_index));
            params.push(serde_json::json!(app_id));
            param_index += 1;
        }

        if let Some(ref status) = filter.status {
            conditions.push(format!("p.status = ${}", param_index));
            params.push(serde_json::json!(status.to_string()));
            param_index += 1;
        }

        if let Some(ref name) = filter.name {
            conditions.push(format!("p.name LIKE ${}", param_index));
            params.push(serde_json::json!(format!("%{}%", name)));
            param_index += 1;
        }

        if let Some(ref domain_code) = filter.domain_code {
            conditions.push(format!("p.domain_code = ${}", param_index));
            params.push(serde_json::json!(domain_code));
            param_index += 1;
        }

        if let Some(ref application_code) = filter.application_code {
            conditions.push(format!("p.application_code = ${}", param_index));
            params.push(serde_json::json!(application_code));
            param_index += 1;
        }

        if let Some(ref module_code) = filter.module_code {
            conditions.push(format!("p.module_code = ${}", param_index));
            params.push(serde_json::json!(module_code));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            r#"
            SELECT p.*,
                   d.name AS domain_name,
                   a.name AS application_name,
                   m.name AS module_name
            FROM cmx_plugin p
            LEFT JOIN cmx_domain d ON p.domain_code = d.code
            LEFT JOIN cmx_application a ON p.application_code = a.code
            LEFT JOIN cmx_module m ON p.module_code = m.code
            {} ORDER BY p.create_time DESC
            "#,
            where_clause
        );

        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                &sql,
                serde_json::json!(params),
                "plugin_list",
            )
            .await
            .map_err(|e| PluginError::Database(format!("列出插件失败: {}", e)))?;

        Self::parse_plugin_record(&result)
    }

    /// 检查插件是否存在
    pub async fn plugin_exists(&self, plugin_id: &str) -> PluginResult<bool> {
        let sql = "SELECT COUNT(*) as count FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "count_query")
            .await
            .map_err(|e| PluginError::Database(format!("检查插件存在失败: {}", e)))?;

        let count = Self::parse_count(&result).unwrap_or(0);
        Ok(count > 0)
    }

    /// 获取插件总数
    pub async fn count_plugins(&self) -> PluginResult<u64> {
        let sql = "SELECT COUNT(*) as count FROM cmx_plugin";

        let result = self
            .db_manager
            .query_sql(&self.default_db_id, None, sql, "count_query")
            .await
            .map_err(|e| PluginError::Database(format!("获取插件总数失败: {}", e)))?;

        Ok(Self::parse_count(&result).unwrap_or(0) as u64)
    }

    /// 查询插件基线版本
    pub async fn get_baseline_version(&self, plugin_id: &str) -> PluginResult<Option<String>> {
        let sql = "SELECT version FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        let result = self
            .db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                sql,
                params,
                "baseline_version_query",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询基线版本失败: {}", e)))?;

        if result.row_count() > 0 {
            let row = result.iter().next();
            if let Some(row) = row {
                let version = row
                    .get_by_name(result.schema.as_ref(), "version")
                    .and_then(|v| {
                        if let DataValue::String(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    });
                return Ok(version);
            }
        }
        Ok(None)
    }

    /// 更新插件状态
    pub async fn update_plugin_status(
        &self,
        plugin_id: &str,
        app_id: &str,
        status: &str,
    ) -> PluginResult<()> {
        let fields = PluginUpdateParams {
            status: Some(status.to_string()),
            ..Default::default()
        };
        self.update_plugin(plugin_id, app_id, &fields, None).await
    }

    /// 检查插件 DDL 执行状态。
    ///
    /// 查询 `cmx_meta_table_define` 中指定插件的所有表定义，
    /// 如果全部为 `completed` 则返回 `true`，否则返回 `false`。
    pub async fn check_ddl_completed(&self, plugin_id: &str) -> PluginResult<bool> {
        let sql = r#"
            SELECT COUNT(*) as total,
                   COUNT(*) FILTER (WHERE ddl_status = 'completed') as completed
            FROM cmx_meta_table_define
            WHERE plugin_id = $1
        "#;
        let params = serde_json::json!([plugin_id]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "ddl_status_check")
            .await
            .map_err(|e| PluginError::Database(format!("查询 ddl_status 失败: {}", e)))?;

        if result.row_count() == 0 {
            return Ok(true);
        }

        let row = result.iter().next().unwrap();
        let schema = result.schema.as_ref();
        let total: i64 = row.get_by_name_as(schema, "total").unwrap_or(0);
        let completed: i64 = row.get_by_name_as(schema, "completed").unwrap_or(0);

        Ok(total == 0 || total == completed)
    }

    /// 解析插件记录（从 DataSet 转换为 PluginRecord）
    fn parse_plugin_record(dataset: &DataSet) -> PluginResult<Vec<PluginRecord>> {
        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        for row in dataset.iter() {
            let get_datetime_default =
                |col_name: &str, default_fn: fn() -> DateTime<Utc>| -> DateTime<Utc> {
                    row.get_by_name_as(schema, col_name)
                        .unwrap_or_else(default_fn)
                };

            let record = PluginRecord {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                app_id: row.get_by_name_as(schema, "app_id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                name: row.get_by_name_as(schema, "name").unwrap_or_default(),
                description: row.get_by_name_as(schema, "description"),
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                wasm_path: row.get_by_name_as(schema, "wasm_path").unwrap_or_default(),
                install_path: row
                    .get_by_name_as(schema, "install_path")
                    .unwrap_or_default(),
                db_id: row
                    .get_by_name_as(schema, "db_id")
                    .unwrap_or_else(|| "default".to_string()),
                status: row
                    .get_by_name_as(schema, "status")
                    .unwrap_or_else(|| "installed".to_string()),
                is_system: row.get_by_name_as(schema, "is_system").unwrap_or(false),
                is_locked: row.get_by_name_as(schema, "is_locked").unwrap_or(false),
                domain_code: row.get_by_name_as(schema, "domain_code"),
                application_code: row.get_by_name_as(schema, "application_code"),
                module_code: row.get_by_name_as(schema, "module_code"),
                domain_name: row.get_by_name_as(schema, "domain_name"),
                application_name: row.get_by_name_as(schema, "application_name"),
                module_name: row.get_by_name_as(schema, "module_name"),
                vendor_name: row.get_by_name_as(schema, "vendor_name"),
                vendor_url: row.get_by_name_as(schema, "vendor_url"),
                vendor_contact: row.get_by_name_as(schema, "vendor_contact"),
                metadata: Self::parse_metadata(row, schema),
                signature_algorithm: row.get_by_name_as(schema, "signature_algorithm"),
                signer_key_id: row.get_by_name_as(schema, "signer_key_id"),
                zip_source_url: row.get_by_name_as(schema, "zip_source_url"),
                zip_source_type: row.get_by_name_as(schema, "zip_source_type"),
                plugin_type: row.get_by_name_as(schema, "plugin_type"),
                source_path: row.get_by_name_as(schema, "source_path"),
                marketplace_source_id: row.get_by_name_as(schema, "marketplace_source_id"),
                storage_key: row.get_by_name_as(schema, "storage_key"),
                storage_checksum: row.get_by_name_as(schema, "storage_checksum"),
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

    /// 解析 metadata JSONB 字段
    fn parse_metadata(row: &Row, schema: &Schema) -> Option<serde_json::Value> {
        row.get_by_name(schema, "metadata").and_then(|v| match v {
            DataValue::Json(s) => serde_json::from_str(s).ok(),
            DataValue::String(s) => serde_json::from_str(s).ok(),
            _ => None,
        })
    }

    /// 解析 COUNT(*) 查询结果
    fn parse_count(dataset: &DataSet) -> Option<i64> {
        if dataset.row_count() > 0 {
            let row = dataset.iter().next()?;
            row.get_by_name(dataset.schema.as_ref(), "count")
                .and_then(|v| {
                    if let DataValue::Int(n) = v {
                        Some(*n)
                    } else {
                        None
                    }
                })
        } else {
            None
        }
    }
}

impl Default for PluginRepository {
    fn default() -> Self {
        Self::new(
            Arc::new(DatabaseManager::new(Default::default())),
            "default".to_string(),
        )
    }
}
