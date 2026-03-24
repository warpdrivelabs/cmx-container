//! 数据仓库模块
//!
//! 提供插件数据的增删改查操作

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use super::schema::SchemaManager;
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};

/// 插件数据库记录
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 配置文件路径
    pub config_path: Option<String>,
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
    /// 激活时间
    pub activated_at: Option<DateTime<Utc>>,
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

/// 插件更新字段
#[derive(Debug, Clone, Default)]
pub struct PluginUpdateFields {
    /// 名称
    pub name: Option<String>,
    /// 版本
    pub version: Option<String>,
    /// 状态
    pub status: Option<String>,
    /// 是否锁定
    pub is_locked: Option<bool>,
    /// 元数据
    pub metadata: Option<serde_json::Value>,
    /// 激活时间
    pub activated_at: Option<DateTime<Utc>>,
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
            let statements: Vec<&str> = sql.split(';')
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
    pub async fn insert_plugin(&self, record: &PluginDbRecord,txn_id:Option<&str>) -> PluginResult<()> {
        use sea_query::{PostgresQueryBuilder, Query};
        use sea_query_binder::SqlxBinder;

        let mut query = Query::insert();
        query
            .into_table("cmx_plugin")
            .columns(vec![
                "id", "plugin_id", "name", "version", "wasm_path", "install_path", "config_path",
                "db_id", "status", "is_system", "is_locked", "domain_code", "application_code",
                "module_code", "vendor_name", "vendor_url", "vendor_contact", "metadata",
                "signature_algorithm", "signer_key_id", "activated_at", "create_time", "update_time"
            ])
            .values(vec![
                record.id.clone().into(),
                record.plugin_id.clone().into(),
                record.name.clone().into(),
                record.version.clone().into(),
                record.wasm_path.clone().into(),
                record.install_path.clone().into(),
                record.config_path.clone().into(),
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
                record.activated_at.clone().into(),
                record.create_time.into(),
                record.update_time.into()
            ])
            .map_err(|e| PluginError::Database(format!("构建插入语句失败: {}", e)))?;

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("插入插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 更新插件记录
    pub async fn update_plugin(&self, plugin_id: &str, fields: &PluginUpdateFields) -> PluginResult<()> {
        let mut updates = Vec::new();
        let mut param_index = 1;
        let mut params = Vec::new();

        if let Some(ref name) = fields.name {
            updates.push(format!("name = ${}", param_index));
            params.push(serde_json::json!(name));
            param_index += 1;
        }

        if let Some(ref version) = fields.version {
            updates.push(format!("version = ${}", param_index));
            params.push(serde_json::json!(version));
            param_index += 1;
        }

        if let Some(ref status) = fields.status {
            updates.push(format!("status = ${}", param_index));
            params.push(serde_json::json!(status));
            param_index += 1;
        }

        if let Some(is_locked) = fields.is_locked {
            updates.push(format!("is_locked = ${}", param_index));
            params.push(serde_json::json!(is_locked));
            param_index += 1;
        }

        if let Some(ref metadata) = fields.metadata {
            updates.push(format!("metadata = ${}", param_index));
            params.push(serde_json::json!(metadata));
            param_index += 1;
        }

        if let Some(ref activated_at) = fields.activated_at {
            updates.push(format!("activated_at = ${}", param_index));
            params.push(serde_json::json!(activated_at));
            param_index += 1;
        }

        if updates.is_empty() {
            return Ok(());
        }

        updates.push(format!("update_time = ${}", param_index));
        params.push(serde_json::json!(Utc::now()));
        param_index += 1;

        params.push(serde_json::json!(plugin_id));

        let sql = format!(
            "UPDATE cmx_plugin SET {} WHERE plugin_id = ${}",
            updates.join(", "),
            param_index
        );

        self.db_manager
            .execute_sql_with_json(
                &self.default_db_id,
                None,
                &sql,
                serde_json::json!(params),
            )
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

    /// 查询插件记录
    pub async fn find_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginDbRecord>> {
        let sql = "SELECT * FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "plugin_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询插件记录失败: {}", e)))?;

        Self::parse_plugin_record(&result).map(|r| r.into_iter().next())
    }

    /// 通过ID查询插件记录
    pub async fn find_plugin_by_id(&self, id: &str) -> PluginResult<Option<PluginDbRecord>> {
        let sql = "SELECT * FROM cmx_plugin WHERE id = $1";
        let params = serde_json::json!([id]);

        let result = self.db_manager
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

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!("SELECT * FROM cmx_plugin {} ORDER BY create_time DESC", where_clause);

        let result = self.db_manager
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

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "count_query")
            .await
            .map_err(|e| PluginError::Database(format!("检查插件存在失败: {}", e)))?;

        let count = Self::parse_count(&result).unwrap_or(0);
        Ok(count > 0)
    }

    /// 获取插件总数
    pub async fn count_plugins(&self) -> PluginResult<u64> {
        let sql = "SELECT COUNT(*) as count FROM cmx_plugin";

        let result = self.db_manager
            .query_sql(&self.default_db_id, None, sql, "count_query")
            .await
            .map_err(|e| PluginError::Database(format!("获取插件总数失败: {}", e)))?;

        Ok(Self::parse_count(&result).unwrap_or(0) as u64)
    }

    /// 插入或更新插件记录
    pub async fn upsert_plugin(&self, record: &PluginDbRecord, txn_id: Option<&str>) -> PluginResult<()> {
        let sql = r#"
            INSERT INTO cmx_plugin (
                id, plugin_id, name, version, wasm_path, install_path, config_path,
                db_id, status, is_system, is_locked, domain_code, application_code,
                module_code, vendor_name, vendor_url, vendor_contact, metadata,
                signature_algorithm, signer_key_id, activated_at, create_time, update_time,
                archived, create_by, create_name, update_by, update_name
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28)
            ON CONFLICT (plugin_id) DO UPDATE SET
                name = EXCLUDED.name,
                version = EXCLUDED.version,
                wasm_path = EXCLUDED.wasm_path,
                install_path = EXCLUDED.install_path,
                config_path = EXCLUDED.config_path,
                status = EXCLUDED.status,
                is_locked = EXCLUDED.is_locked,
                metadata = EXCLUDED.metadata,
                activated_at = EXCLUDED.activated_at,
                update_time = EXCLUDED.update_time,
                update_by = EXCLUDED.update_by,
                update_name = EXCLUDED.update_name
        "#;

        let params = serde_json::json!([
            record.id,
            record.plugin_id,
            record.name,
            record.version,
            record.wasm_path,
            record.install_path,
            record.config_path,
            record.db_id,
            record.status,
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
            record.activated_at,
            record.create_time,
            record.update_time,
            record.archived,
            record.create_by,
            record.create_name,
            record.update_by,
            record.update_name,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, txn_id, sql, params)
            .await
            .map_err(|e| PluginError::Database(format!("插入或更新插件记录失败: {}", e)))?;

        Ok(())
    }

    /// 查询插件基线版本
    pub async fn get_baseline_version(&self, plugin_id: &str) -> PluginResult<Option<String>> {
        let sql = "SELECT version FROM cmx_plugin WHERE plugin_id = $1";
        let params = serde_json::json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "baseline_version_query")
            .await
            .map_err(|e| PluginError::Database(format!("查询基线版本失败: {}", e)))?;

        if result.row_count() > 0 {
            let row = result.iter().next();
            if let Some(row) = row {
                let version = row.get_by_name(result.schema.as_ref(), "version")
                    .and_then(|v| if let DataValue::String(s) = v { Some(s.clone()) } else { None });
                return Ok(version);
            }
        }
        Ok(None)
    }

    /// 更新插件状态
    pub async fn update_plugin_status(&self, plugin_id: &str, status: &str) -> PluginResult<()> {
        let fields = PluginUpdateFields {
            status: Some(status.to_string()),
            ..Default::default()
        };
        self.update_plugin(plugin_id, &fields).await
    }

    /// 解析插件记录
    fn parse_plugin_record(dataset: &DataSet) -> PluginResult<Vec<PluginDbRecord>> {
        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        for row in dataset.iter() {
            let get_string = |col_name: &str| -> Option<String> {
                row.get_by_name(schema, col_name)
                    .and_then(|v| if let DataValue::String(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    })
            };

            let get_opt_string = |col_name: &str| -> Option<String> {
                row.get_by_name(schema, col_name).and_then(|v| {
                    match v {
                        DataValue::Null => None,
                        DataValue::String(s) => Some(s.clone()),
                        _ => None,
                    }
                })
            };

            let get_bool = |col_name: &str| -> bool {
                row.get_by_name(schema, col_name)
                    .map(|v| {
                        match v {
                            DataValue::Bool(b) => *b,
                            _ => false,
                        }
                    })
                    .unwrap_or(false)
            };

            let get_opt_json = |col_name: &str| -> Option<serde_json::Value> {
                row.get_by_name(schema, col_name).and_then(|v| {
                    match v {
                        DataValue::Null => None,
                        DataValue::Json(s) => {
                            serde_json::from_str(s).ok()
                        }
                        _ => None,
                    }
                })
            };

            let get_datetime = |col_name: &str| -> DateTime<Utc> {
                row.get_by_name(schema, col_name)
                    .and_then(|v| {
                        if let DataValue::String(s) = v {
                            DateTime::parse_from_rfc3339(s).ok()
                                .map(|dt| dt.with_timezone(&Utc))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(Utc::now)
            };

            let get_i32 = |col_name: &str| -> i32 {
                row.get_by_name(schema, col_name)
                    .and_then(|v| if let DataValue::Int(n) = v { Some(*n as i32) } else { None })
                    .unwrap_or(0)
            };

            let get_opt_datetime = |col_name: &str| -> Option<DateTime<Utc>> {
                row.get_by_name(schema, col_name)
                    .and_then(|v| {
                        if let DataValue::String(s) = v {
                            DateTime::parse_from_rfc3339(s).ok()
                                .map(|dt| dt.with_timezone(&Utc))
                        } else {
                            None
                        }
                    })
            };

            let record = PluginDbRecord {
                id: get_string("id").unwrap_or_default(),
                plugin_id: get_string("plugin_id").unwrap_or_default(),
                name: get_string("name").unwrap_or_default(),
                version: get_string("version").unwrap_or_default(),
                wasm_path: get_string("wasm_path").unwrap_or_default(),
                install_path: get_string("install_path").unwrap_or_default(),
                config_path: get_opt_string("config_path"),
                db_id: get_string("db_id").unwrap_or_else(|| "default".to_string()),
                status: get_string("status").unwrap_or_else(|| "installed".to_string()),
                is_system: get_bool("is_system"),
                is_locked: get_bool("is_locked"),
                domain_code: get_opt_string("domain_code"),
                application_code: get_opt_string("application_code"),
                module_code: get_opt_string("module_code"),
                vendor_name: get_opt_string("vendor_name"),
                vendor_url: get_opt_string("vendor_url"),
                vendor_contact: get_opt_string("vendor_contact"),
                metadata: get_opt_json("metadata"),
                signature_algorithm: get_opt_string("signature_algorithm"),
                signer_key_id: get_opt_string("signer_key_id"),
                activated_at: get_opt_datetime("activated_at"),
                create_time: get_datetime("create_time"),
                update_time: get_datetime("update_time"),
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

    // /// 在事务中执行操作
    // pub async fn with_transaction<F, T>(&self, db_id: &str, f: F) -> PluginResult<T>
    // where
    //     F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = PluginResult<T>> + Send>>,
    // {
    //     let txn_context = self.db_manager.get_transaction_context();
    //     let txn_id = txn_context.begin(db_id, TransactionOptions::default()).await
    //         .map_err(|e| PluginError::Transaction(format!("开始事务失败: {}", e)))?;
    //
    //     match f().await {
    //         Ok(result) => {
    //             txn_context.commit(&txn_id).await
    //                 .map_err(|e| PluginError::Transaction(format!("提交事务失败: {}", e)))?;
    //             Ok(result)
    //         }
    //         Err(e) => {
    //             let _ = txn_context.rollback(&txn_id).await;
    //             Err(e)
    //         }
    //     }
    // }
}

impl Default for PluginRepository {
    fn default() -> Self {
        Self::new(
            Arc::new(DatabaseManager::new(Default::default())),
            "default".to_string(),
        )
    }
}
