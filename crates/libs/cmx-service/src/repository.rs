//! 服务仓储层
//!
//! 提供服务定义的数据库访问能力。

use cmx_core::model::service::ServiceDefinition;
use cmx_database::DatabaseManager;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ServiceError;

/// 服务仓储
///
/// 提供服务定义的持久化操作，包括：
/// - 服务定义的增删改查
/// - 服务版本的保存和查询
/// - 编排配置的存储和获取
#[derive(Clone)]
pub struct ServiceRepository {
    /// 数据库管理器
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    default_db_id: String,
}

impl ServiceRepository {
    /// 创建服务仓储（同步版本，使用默认值）
    ///
    ///
    /// # 参数
    /// * `db_manager` - 数据库管理器
    /// * `default_db_id` - 默认数据库id
    pub fn new(db_manager: Arc<DatabaseManager>, default_db_id: String) -> Self {
        Self {
            db_manager,
            default_db_id,
        }
    }


    /// 设置默认数据库ID
    ///
    /// # 参数
    /// * `db_id` - 数据库ID
    pub fn with_db_id(mut self, db_id: impl Into<String>) -> Self {
        self.default_db_id = db_id.into();
        self
    }

    /// 获取默认数据库ID
    pub fn get_default_db_id(&self) -> &str {
        &self.default_db_id
    }

    /// 保存服务定义（UPSERT）
    ///
    /// 如果 service_key 已存在则更新，否则插入新记录
    ///
    /// # 参数
    /// * `service` - 服务定义
    pub async fn save_service(&self, service: &ServiceDefinition) -> Result<(), ServiceError> {
        self.save_service_with_txn(service, None).await
    }

    /// 保存服务定义（支持事务）
    ///
    /// 如果 service_key 已存在则更新，否则插入新记录
    ///
    /// # 参数
    /// * `service` - 服务定义
    /// * `db_id` - 数据库ID
    /// * `txn_id` - 事务ID（可选）
    pub async fn save_service_with_txn(
        &self,
        service: &ServiceDefinition,
        txn_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let sql = r#"
            INSERT INTO cmx_service_define (
                id, service_key, service_name, description, plugin_id,
                status, version, domain_code, application_code, module_code,
                create_time, update_time
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
            ON CONFLICT (service_key) DO UPDATE SET
                service_name = EXCLUDED.service_name,
                description = EXCLUDED.description,
                plugin_id = EXCLUDED.plugin_id,
                status = EXCLUDED.status,
                version = EXCLUDED.version,
                domain_code = EXCLUDED.domain_code,
                application_code = EXCLUDED.application_code,
                module_code = EXCLUDED.module_code,
                update_time = NOW()
        "#;

        let params = json!([
            service.id,
            service.service_key,
            service.service_name,
            service.description,
            service.plugin_id,
            service.status,
            service.version,
            service.domain_code,
            service.application_code,
            service.module_code,
        ]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, txn_id, sql, params)
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 根据 service_key 获取服务定义
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回服务定义（包含最新版本的 config），如果不存在则返回 None
    pub async fn get_service(&self, service_key: &str) -> Result<Option<ServiceDefinition>, ServiceError> {
        let sql = r#"
            SELECT d.id, d.service_key, d.service_name, d.description, d.plugin_id,
                   d.status, d.version, d.domain_code, d.application_code, d.module_code,
                   d.create_time, d.update_time, v.config
            FROM cmx_service_define d
            LEFT JOIN cmx_service_define_version v ON d.service_key = v.service_key and d.version = v.plugin_version
            WHERE d.service_key = $1
            ORDER BY v.create_time DESC
            LIMIT 1
        "#;

        let params = json!([service_key]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "cmx_service_define")
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next();
        match row {
            Some(r) => {
                let schema = result.schema.as_ref();
                Ok(Some(ServiceDefinition {
                    id: r.get_by_name_as(schema, "id").unwrap_or_default(),
                    service_key: r.get_by_name_as(schema, "service_key").unwrap_or_default(),
                    service_name: r.get_by_name_as(schema, "service_name").unwrap_or_default(),
                    description: r.get_by_name_as(schema, "description").unwrap_or_default(),
                    plugin_id: r.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                    status: r.get_by_name_as(schema, "status").unwrap_or(1) as i32,
                    version: r.get_by_name_as(schema, "version").unwrap_or_default(),
                    config: r.get_by_name_as(schema, "config"),
                    domain_code: r.get_by_name_as(schema, "domain_code").unwrap_or_default(),
                    application_code: r.get_by_name_as(schema, "application_code").unwrap_or_default(),
                    module_code: r.get_by_name_as(schema, "module_code").unwrap_or_default(),
                    domain_name: String::new(),
                    application_name: String::new(),
                    module_name: String::new(),
                    plugin_name:String::new(),
                }))
            }
            None => Ok(None)
        }
    }

    /// 获取所有服务定义
    ///
    /// # 返回值
    /// 返回所有服务定义列表，按更新时间降序排列
    pub async fn list_services(&self) -> Result<Vec<ServiceDefinition>, ServiceError> {
        let sql = r#"
            SELECT id, service_key, service_name, description, plugin_id,
                   status, version, domain_code, application_code, module_code,
                   create_time, update_time
            FROM cmx_service_define
            ORDER BY update_time DESC
        "#;

        let result = self.db_manager
            .query_sql(&self.default_db_id, None, sql, "list_services")
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        let schema = result.schema.as_ref();
        let mut services = Vec::new();
        for row in result.iter() {
            services.push(ServiceDefinition {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                service_key: row.get_by_name_as(schema, "service_key").unwrap_or_default(),
                service_name: row.get_by_name_as(schema, "service_name").unwrap_or_default(),
                description: row.get_by_name_as(schema, "description").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                status: row.get_by_name_as(schema, "status").unwrap_or(1) as i32,
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                config: None,
                domain_code: row.get_by_name_as(schema, "domain_code").unwrap_or_default(),
                application_code: row.get_by_name_as(schema, "application_code").unwrap_or_default(),
                module_code: row.get_by_name_as(schema, "module_code").unwrap_or_default(),
                domain_name: String::new(),
                application_name: String::new(),
                module_name: String::new(),
                plugin_name:String::new(),
            });
        }

        Ok(services)
    }

    /// 根据插件ID获取所有服务
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    ///
    /// # 返回值
    /// 返回该插件下所有服务定义列表（包含最新版本的 config）
    pub async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceDefinition>, ServiceError> {
        let sql = r#"
            SELECT d.id, d.service_key, d.service_name, d.description, d.plugin_id,
                   d.status, d.version, d.domain_code, d.application_code, d.module_code,
                   d.create_time, d.update_time, dv.config
            FROM cmx_service_define d
            LEFT JOIN cmx_service_define_version dv ON d.service_key = dv.service_key
             and d.version = dv.plugin_version
            WHERE d.plugin_id = $1
            ORDER BY d.create_time DESC
        "#;

        let params = json!([plugin_id]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_services_by_plugin")
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        let schema = result.schema.as_ref();
        let mut services = Vec::new();
        for row in result.iter() {
            services.push(ServiceDefinition {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                service_key: row.get_by_name_as(schema, "service_key").unwrap_or_default(),
                service_name: row.get_by_name_as(schema, "service_name").unwrap_or_default(),
                description: row.get_by_name_as(schema, "description").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                status: row.get_by_name_as(schema, "status").unwrap_or(1) as i32,
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                config: row.get_by_name_as(schema, "config"),
                domain_code: row.get_by_name_as(schema, "domain_code").unwrap_or_default(),
                application_code: row.get_by_name_as(schema, "application_code").unwrap_or_default(),
                module_code: row.get_by_name_as(schema, "module_code").unwrap_or_default(),
                domain_name: String::new(),
                application_name: String::new(),
                module_name: String::new(),
                plugin_name:String::new(),
            });
        }

        Ok(services)
    }

    /// 删除服务定义及其所有版本（物理删除）
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `txn_id` - 事务id
    /// * `version` - 服务版本
    pub async fn delete_service(&self, service_key: &str,_txn_id: Option<&str>, _version: Option<&str>) -> Result<(), ServiceError> {
        let sql_version = r#"
            DELETE FROM cmx_service_define_version WHERE service_key = $1
        "#;
        let params = json!([service_key]);
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql_version, params.clone())
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        let sql_define = r#"
            DELETE FROM cmx_service_define WHERE service_key = $1
        "#;
        self.db_manager
            .execute_sql_with_json(&self.default_db_id, None, sql_define, params)
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 根据插件ID删除所有服务（物理删除）
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    pub async fn delete_services_by_plugin(&self, plugin_id: &str,txn_id: Option<&str>) -> Result<(), ServiceError> {
        let services = self.get_services_by_plugin(plugin_id).await?;
        for service in services {
            self.delete_service(&service.service_key, txn_id, None).await?;
        }
        Ok(())
    }

    /// 保存服务版本
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `version` - 服务版本号
    /// * `plugin_id` - 所属插件ID
    /// * `plugin_version` - 所属插件版本
    /// * `config` - 编排配置 JSON 字符串
    pub async fn save_service_version(
        &self,
        service_key: &str,
        version: &str,
        plugin_id: &str,
        plugin_version: &str,
        config: &str,
    ) -> Result<(), ServiceError> {
        self.save_service_version_with_txn(
            service_key,
            version,
            plugin_id,
            plugin_version,
            config,
            None,
        )
        .await
    }

    /// 保存服务版本（支持事务）
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `version` - 服务版本号
    /// * `plugin_id` - 所属插件ID
    /// * `plugin_version` - 所属插件版本
    /// * `config` - 编排配置 JSON 字符串
    /// * `db_id` - 数据库ID
    /// * `txn_id` - 事务ID（可选）
    pub async fn save_service_version_with_txn(
        &self,
        service_key: &str,
        version: &str,
        plugin_id: &str,
        plugin_version: &str,
        config: &str,
        txn_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let sql = r#"
            INSERT INTO cmx_service_define_version (
                id, service_key, version, plugin_id, plugin_version,
                config, create_time, update_time
            ) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
        "#;

        let id = Uuid::new_v4().to_string();
        let params = json!([id, service_key, version, plugin_id, plugin_version, config]);

        self.db_manager
            .execute_sql_with_json(&self.default_db_id, txn_id, sql, params)
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 获取服务版本列表
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回 (version, plugin_version) 元组列表，按创建时间降序排列
    pub async fn get_service_versions(&self, service_key: &str) -> Result<Vec<(String, String)>, ServiceError> {
        let sql = r#"
            SELECT version, plugin_version
            FROM cmx_service_define_version
            WHERE service_key = $1
            ORDER BY create_time DESC
        "#;

        let params = json!([service_key]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_service_versions")
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        let schema = result.schema.as_ref();
        let mut versions = Vec::new();
        for row in result.iter() {
            versions.push((
                row.get_by_name_as(schema, "version").unwrap_or_default(),
                row.get_by_name_as(schema, "plugin_version").unwrap_or_default(),
            ));
        }

        Ok(versions)
    }

    /// 获取服务编排配置
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `version` - 服务版本号
    ///
    /// # 返回值
    /// 返回编排配置 JSON 字符串，如果不存在则返回 None
    pub async fn get_service_config(&self, service_key: &str, version: &str) -> Result<Option<String>, ServiceError> {
        let sql = r#"
            SELECT config
            FROM cmx_service_define_version
            WHERE service_key = $1 AND version = $2
        "#;

        let params = json!([service_key, version]);

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, sql, params, "get_service_config")
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        if result.row_count() == 0 {
            return Ok(None);
        }

        let row = result.iter().next();
        match row {
            Some(r) => {
                let schema = result.schema.as_ref();
                Ok(r.get_by_name_as(schema, "config"))
            }
            None => Ok(None)
        }
    }

    /// 分页查询服务列表
    ///
    /// 支持多条件组合查询，service_key 和 service_name 支持模糊匹配
    ///
    /// # 参数
    /// * `filter` - 查询过滤器
    /// * `page` - 页码（从 1 开始）
    /// * `size` - 每页大小
    ///
    /// # 返回值
    /// 返回 (服务列表, 总数)
    pub async fn page_services(
        &self,
        filter: &cmx_traits::ServicePageFilter,
        page: u64,
        size: u64,
    ) -> Result<(Vec<ServiceDefinition>, u64), ServiceError> {
        let offset = (page.saturating_sub(1)) * size;

        let mut where_clauses: Vec<String> = Vec::new();
        let mut params: Vec<serde_json::Value> = Vec::new();
        let mut param_index = 1;

        if let Some(ref keyword) = filter.keyword
            && !keyword.is_empty() {
                where_clauses.push(format!("(s.service_key LIKE ${} OR S.service_name LIKE ${} )", param_index,param_index+1));
                params.push(json!(format!("%{}%", keyword)));
                params.push(json!(format!("%{}%", keyword)));
                param_index += 2;

            }



        if let Some(ref plugin_id) = filter.plugin_id {
            where_clauses.push(format!("s.plugin_id = ${}", param_index));
            params.push(json!(plugin_id));
            param_index += 1;
        }

        if let Some(ref domain_code) = filter.domain_code {
            where_clauses.push(format!("s.domain_code = ${}", param_index));
            params.push(json!(domain_code));
            param_index += 1;
        }

        if let Some(ref application_code) = filter.application_code {
            where_clauses.push(format!("s.application_code = ${}", param_index));
            params.push(json!(application_code));
            param_index += 1;
        }

        if let Some(ref module_code) = filter.module_code {
            where_clauses.push(format!("s.module_code = ${}", param_index));
            params.push(json!(module_code));
            param_index += 1;
        }

        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let count_sql = format!(
            "SELECT COUNT(*) as total FROM cmx_service_define s {}",
            where_clause
        );

        let count_result = self.db_manager
            .query_sql_with_json(
                &self.default_db_id,
                None,
                &count_sql,
                json!(params.clone()),
                "count_services",
            )
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        let total: u64 = count_result
            .iter()
            .next()
            .map(|r| {
                let schema = count_result.schema.as_ref();
                r.get_by_name_as::<i64>(schema, "total").unwrap_or(0) as u64
            })
            .unwrap_or(0);

        let data_sql = format!(
            r#"
            SELECT s.id, s.service_key, s.service_name, s.description, s.plugin_id,
                   s.status, s.version, s.domain_code, s.application_code, s.module_code,
                   s.create_time, s.update_time,
                   d.name as domain_name,
                   a.name as application_name,
                   m.name as module_name,
                   p.name as plugin_name
            FROM cmx_service_define s
            LEFT JOIN cmx_domain d ON s.domain_code = d.code
            LEFT JOIN cmx_application a ON s.application_code = a.code
            LEFT JOIN cmx_module m ON s.module_code = m.code
            LEFT JOIN cmx_plugin p ON s.plugin_id = p.plugin_id
            {}
            ORDER BY s.update_time DESC
            LIMIT ${} OFFSET ${}
            "#,
            where_clause, param_index, param_index + 1
        );

        params.push(json!(size));
        params.push(json!(offset));

        let result = self.db_manager
            .query_sql_with_json(&self.default_db_id, None, &data_sql, json!(params), "page_services")
            .await
            .map_err(|e| ServiceError::DatabaseError(e.to_string()))?;

        let schema = result.schema.as_ref();
        let mut services = Vec::new();
        for row in result.iter() {
            services.push(ServiceDefinition {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                service_key: row.get_by_name_as(schema, "service_key").unwrap_or_default(),
                service_name: row.get_by_name_as(schema, "service_name").unwrap_or_default(),
                description: row.get_by_name_as(schema, "description").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                status: row.get_by_name_as(schema, "status").unwrap_or(1) as i32,
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                config: None,
                domain_code: row.get_by_name_as(schema, "domain_code").unwrap_or_default(),
                application_code: row.get_by_name_as(schema, "application_code").unwrap_or_default(),
                module_code: row.get_by_name_as(schema, "module_code").unwrap_or_default(),
                domain_name: row.get_by_name_as(schema, "domain_name").unwrap_or_default(),
                application_name: row.get_by_name_as(schema, "application_name").unwrap_or_default(),
                module_name: row.get_by_name_as(schema, "module_name").unwrap_or_default(),
                plugin_name: row.get_by_name_as(schema, "plugin_name").unwrap_or_default(),
            });
        }

        Ok((services, total))
    }
}
