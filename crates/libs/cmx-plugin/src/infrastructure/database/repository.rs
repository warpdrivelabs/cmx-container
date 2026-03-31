//! 数据仓库模块
//!
//! 提供插件数据的增删改查操作

use chrono::{DateTime, Utc};
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_database::DatabaseManager;
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::schema::SchemaManager;
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};

/// 插件数据库记录
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct PluginDbRecord {
    /// 主键ID
    pub id: String,
    /// 插件唯一标识
    pub plugin_id: String,
    /// 显示名称
    pub name: String,
    /// 当前版本
    pub version: String,
    /// WASM 文件路径
    pub wasm_path: String,
    /// 安装根目录路径
    pub install_path: String,
    /// 数据库ID
    pub db_id: String,
    /// 状态
    pub status: String,
    /// 是否系统插件
    pub is_system: bool,
    /// 是否锁定
    pub is_locked: bool,
    /// 域编码
    pub domain_code: Option<String>,
    /// 应用编码
    pub application_code: Option<String>,
    /// 模块编码
    pub module_code: Option<String>,
    /// 域名称
    pub domain_name: Option<String>,
    /// 应用名称
    pub application_name: Option<String>,
    /// 模块名称
    pub module_name: Option<String>,

    /// 开发商名称
    pub vendor_name: Option<String>,
    /// 开发商URL
    pub vendor_url: Option<String>,
    /// 开发商联系方式
    pub vendor_contact: Option<String>,
    /// 扩展元数据
    pub metadata: Option<serde_json::Value>,
    /// 签名算法
    pub signature_algorithm: Option<String>,
    /// 签名密钥ID
    pub signer_key_id: Option<String>,
    /// 插件ZIP包来源地址
    pub zip_source_url: Option<String>,
    /// 插件来源类型: local, url, registry
    pub zip_source_type: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,

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

/// 插件更新字段 通过插件id和版本 作为where条件更新插件信息
#[derive(Debug, Clone, Default, Serialize, Deserialize, Fields)]
pub struct PluginUpdateFields {
    /// 插件唯一标识
    pub plugin_id: String,
    /// 显示名称
    pub name: String,
    /// 当前版本
    pub version: String,

    /// WASM 文件路径
    pub wasm_path: String,
    /// 安装根目录路径
    pub install_path: String,
    /// 数据库ID
    pub db_id: String,
    /// 状态
    pub status: String,
    /// 是否系统插件
    pub is_system: bool,
    /// 是否锁定
    pub is_locked: bool,
    /// 域编码
    pub domain_code: Option<String>,
    /// 应用编码
    pub application_code: Option<String>,
    /// 模块编码
    pub module_code: Option<String>,
    /// 开发商名称
    pub vendor_name: Option<String>,
    /// 开发商URL
    pub vendor_url: Option<String>,
    /// 开发商联系方式
    pub vendor_contact: Option<String>,
    /// 扩展元数据
    pub metadata: Option<serde_json::Value>,
    /// 签名算法
    pub signature_algorithm: Option<String>,
    /// 签名密钥ID
    pub signer_key_id: Option<String>,
    /// 插件ZIP包来源地址
    pub zip_source_url: Option<String>,
    /// 插件来源类型: local, url, registry
    pub zip_source_type: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,

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
        record: &PluginDbRecord,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        use sea_query::{PostgresQueryBuilder, Query};
        use sea_query_binder::SqlxBinder;

        let mut query = Query::insert();
        query
            .into_table("cmx_plugin")
            .columns(vec![
                "id",
                "plugin_id",
                "name",
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
                "create_time",
                "update_time",
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.name.clone().into(),
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
        fields: &PluginUpdateFields,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        use sea_query::{Expr, PostgresQueryBuilder, Query};
        use sea_query_binder::SqlxBinder;

        let mut query = Query::update();
        query.table("cmx_plugin");

        if fields.version != String::default() {
            query.value("version", fields.version.clone());
        }
        if fields.name != String::default() {
            query.value("name", fields.name.clone());
        }
        if fields.wasm_path != String::default() {
            query.value("wasm_path", fields.wasm_path.clone());
        }
        if fields.install_path != String::default() {
            query.value("install_path", fields.install_path.clone());
        }
        if fields.db_id != String::default() {
            query.value("db_id", fields.db_id.clone());
        }
        if fields.status != String::default() {
            query.value("status", fields.status.clone());
        }
        if fields.is_system != bool::default() {
            query.value("is_system", fields.is_system);
        }
        if fields.is_locked != bool::default() {
            query.value("is_locked", fields.is_locked);
        }
        if fields.domain_code.is_some() {
            query.value("domain_code", fields.domain_code.clone());
        }
        if fields.application_code.is_some() {
            query.value("application_code", fields.application_code.clone());
        }
        if fields.module_code.is_some() {
            query.value("module_code", fields.module_code.clone());
        }
        if fields.vendor_name.is_some() {
            query.value("vendor_name", fields.vendor_name.clone());
        }
        if fields.vendor_url.is_some() {
            query.value("vendor_url", fields.vendor_url.clone());
        }
        if fields.vendor_contact.is_some() {
            query.value("vendor_contact", fields.vendor_contact.clone());
        }
        if fields.metadata.is_some() {
            query.value("metadata", fields.metadata.clone());
        }
        if fields.signature_algorithm.is_some() {
            query.value("signature_algorithm", fields.signature_algorithm.clone());
        }
        if fields.signer_key_id.is_some() {
            query.value("signer_key_id", fields.signer_key_id.clone());
        }
        if fields.zip_source_url.is_some() {
            query.value("zip_source_url", fields.zip_source_url.clone());
        }
        if fields.zip_source_type.is_some() {
            query.value("zip_source_type", fields.zip_source_type.clone());
        }
        if fields.plugin_type.is_some() {
            query.value("plugin_type", fields.plugin_type.clone());
        }
        if fields.source_path.is_some() {
            query.value("source_path", fields.source_path.clone());
        }
        if fields.update_by.is_some() {
            query.value("update_by", fields.update_by.clone());
        }
        if fields.update_name.is_some() {
            query.value("update_name", fields.update_name.clone());
        }

        query.and_where(Expr::col("plugin_id").eq(plugin_id));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("更新插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 删除插件记录
    pub async fn delete_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        let sql = "DELETE FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("删除插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 插入或更新插件记录 (upsert)
    ///
    /// 使用 ON CONFLICT (plugin_id) DO UPDATE 实现 upsert 语义
    ///
    /// # 参数
    /// - `record`: 插件记录
    /// - `txn_id`: 事务ID
    ///
    /// # 返回
    /// - `Ok(true)`: 新插入的记录
    /// - `Ok(false)`: 更新的记录
    pub async fn upsert_plugin(
        &self,
        record: &PluginDbRecord,
        txn_id: Option<&str>,
    ) -> PluginResult<bool> {
        use sea_query::{Alias, PostgresQueryBuilder, Query};
        use sea_query_binder::SqlxBinder;

        // 使用 sea_query 构建带参数占位符的 SQL
        let mut query = Query::insert();
        query
            .into_table(Alias::new("cmx_plugin"))
            .columns(vec![
                Alias::new("id"),
                Alias::new("plugin_id"),
                Alias::new("name"),
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
                record.name.clone().into(),
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
                record.create_time.into(),
                record.update_time.into(),
                record.archived.into(),
                record.create_by.clone().into(),
                record.create_name.clone().into(),
                record.update_by.clone().into(),
                record.update_name.clone().into(),
            ]);

        // 构建简单的 ON CONFLICT 子句
        let on_conflict = sea_query::OnConflict::column(Alias::new("plugin_id"))
            .update_columns(vec![
                Alias::new("name"),
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
                Alias::new("update_time"),
                Alias::new("update_by"),
                Alias::new("update_name"),
            ])
            .to_owned();

        query.on_conflict(on_conflict);

        // 手动添加 RETURNING 子句
        let (mut sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        sql.push_str(" RETURNING (xmax = 0) AS is_inserted");

        let result = self
            .db_manager
            .query_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values, "upsert_plugin")
            .await
            .map_err(|e| PluginError::Database(format!("upsert插件记录失败: {}", e)))?;

        // 解析返回值判断是插入还是更新
        if let Some(row) = result.iter().next() {
            if let Some(cmx_core::model::cell::DataValue::Bool(is_inserted)) = row.get(0) {
                return Ok(*is_inserted);
            }
        }

        // 默认返回 false（更新）
        Ok(false)
    }

    /// 查询插件记录
    pub async fn find_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginDbRecord>> {
        let sql = "SELECT * FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "plugin_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询插件记录失败: {}", e)))?;

        Self::parse_plugin_record(&result).map(|r| r.into_iter().next())
    }

    /// 通过ID查询插件记录
    pub async fn find_plugin_by_id(&self, id: &str) -> PluginResult<Option<PluginDbRecord>> {
        let sql = "SELECT * FROM cmx_plugin WHERE id = $1";
        let params = serde_json::json!([id]);

        let result = self
            .db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "plugin_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询插件记录失败: {}", e)))?;

        Self::parse_plugin_record(&result).map(|r| r.into_iter().next())
    }

    /// 列出所有插件
    pub async fn list_plugins(&self, filter: &PluginFilter) -> PluginResult<Vec<PluginDbRecord>> {
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        let mut param_index = 1;

        if let Some(ref status) = filter.status {
            conditions.push(format!("status = ${}", param_index));
            params.push(serde_json::json!(status.to_string()));
            param_index += 1;
        }

        if let Some(ref name) = filter.name {
            conditions.push(format!("name LIKE ${}", param_index));
            params.push(serde_json::json!(format!("%{}%", name)));
            param_index += 1;
        }

        if let Some(ref domain_code) = filter.domain_code {
            conditions.push(format!("domain_code = ${}", param_index));
            params.push(serde_json::json!(domain_code));
            param_index += 1;
        }

        if let Some(ref application_code) = filter.application_code {
            conditions.push(format!("application_code = ${}", param_index));
            params.push(serde_json::json!(application_code));
            param_index += 1;
        }

        if let Some(ref module_code) = filter.module_code {
            conditions.push(format!("module_code = ${}", param_index));
            params.push(serde_json::json!(module_code));
            param_index += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT * FROM cmx_plugin {} ORDER BY create_time DESC",
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

    // /// 插入或更新插件记录
    // pub async fn upsert_plugin(&self, record: &PluginDbRecord, txn_id: Option<&str>) -> PluginResult<()> {
    //     let sql = r#"
    //         INSERT INTO cmx_plugin (
    //             id, plugin_id, name, version, wasm_path, install_path, config_path,
    //             db_id, status, is_system, is_locked, domain_code, application_code,
    //             module_code, vendor_name, vendor_url, vendor_contact, metadata,
    //             signature_algorithm, signer_key_id, activated_at, create_time, update_time,
    //             archived, create_by, create_name, update_by, update_name
    //         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28)
    //         ON CONFLICT (plugin_id) DO UPDATE SET
    //             name = EXCLUDED.name,
    //             version = EXCLUDED.version,
    //             wasm_path = EXCLUDED.wasm_path,
    //             install_path = EXCLUDED.install_path,
    //             config_path = EXCLUDED.config_path,
    //             status = EXCLUDED.status,
    //             is_locked = EXCLUDED.is_locked,
    //             metadata = EXCLUDED.metadata,
    //             activated_at = EXCLUDED.activated_at,
    //             update_time = EXCLUDED.update_time,
    //             update_by = EXCLUDED.update_by,
    //             update_name = EXCLUDED.update_name
    //     "#;
    //
    //     let params = serde_json::json!([
    //         record.id,
    //         record.plugin_id,
    //         record.name,
    //         record.version,
    //         record.wasm_path,
    //         record.install_path,
    //         record.config_path,
    //         record.db_id,
    //         record.status,
    //         record.is_system,
    //         record.is_locked,
    //         record.domain_code,
    //         record.application_code,
    //         record.module_code,
    //         record.vendor_name,
    //         record.vendor_url,
    //         record.vendor_contact,
    //         record.metadata,
    //         record.signature_algorithm,
    //         record.signer_key_id,
    //         record.activated_at,
    //         record.create_time,
    //         record.update_time,
    //         record.archived,
    //         record.create_by,
    //         record.create_name,
    //         record.update_by,
    //         record.update_name,
    //     ]);
    //
    //     self.db_manager
    //         .execute_sql_with_json(&self.default_db_id, txn_id, sql, params)
    //         .await
    //         .map_err(|e| PluginError::Database(format!("插入或更新插件记录失败: {}", e)))?;
    //
    //     Ok(())
    // }

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
    pub async fn update_plugin_status(&self, plugin_id: &str, status: &str) -> PluginResult<()> {
        let fields = PluginUpdateFields {
            status: status.to_string(),
            ..Default::default()
        };
        self.update_plugin(plugin_id, &fields, None).await
    }

    /// 解析插件记录
    fn parse_plugin_record(dataset: &DataSet) -> PluginResult<Vec<PluginDbRecord>> {
        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        for row in dataset.iter() {
            let get_datetime_default =
                |col_name: &str, default_fn: fn() -> DateTime<Utc>| -> DateTime<Utc> {
                    row.get_by_name_as(schema, col_name)
                        .unwrap_or_else(default_fn)
                };

            let record = PluginDbRecord {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                name: row.get_by_name_as(schema, "name").unwrap_or_default(),
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

    fn parse_metadata(row: &Row, schema: &Schema) -> Option<serde_json::Value> {
        row.get_by_name(schema, "metadata").and_then(|v| match v {
            DataValue::Json(s) => serde_json::from_str(s).ok(),
            DataValue::String(s) => serde_json::from_str(s).ok(),
            _ => None,
        })
    }

    /// 解析计数结果
    fn parse_count(dataset: &DataSet) -> Option<i64> {
        if dataset.row_count() > 0 {
            let row = dataset.iter().next()?;
            row.get_by_name(dataset.schema.as_ref(), "count")
                .and_then(|v| {
                    if let cmx_core::model::cell::DataValue::Int(n) = v {
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
