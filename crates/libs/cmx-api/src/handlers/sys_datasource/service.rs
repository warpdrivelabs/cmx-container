//! SysDatasource 实体的自定义 Service
//!
//! 实现数据源的 CRUD 操作，并动态管理数据库连接池

use crate::error::{Error, Result};
use cmx_core::model::data::dataset::{DataSet, Schema};
use cmx_database::{DatabaseManager, DbConfig, PoolConfig};
use cmx_database::config::DbType;
use modql::filter::{OpValString, OpValsString};
use serde_json::Value;
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info, warn};
use cmx_core::UpdatePayload;
use cmx_database::crud::GenericCrudService;
use super::{SysDatasourceBmc, SysDatasourceFilter, SysDatasourceForCreate, SysDatasourceForUpdate};

/// SysDatasource 自定义服务
///
/// 继承 GenericCrudService 并添加数据源动态管理功能
pub struct SysDatasourceService;

impl SysDatasourceService {
    /// 创建数据源
    ///
    /// # 流程
    /// 1. 验证数据源配置
    /// 2. 保存到数据库
    /// 3. 动态注册到 DatabaseManager
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: SysDatasourceForCreate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - SysDatasourceService::create - db_id: {}",
            "SERVICE", data.db_id
        );

        let db_type = data.db_type.to_lowercase();
        if !["postgres", "postgresql", "mysql", "sqlite", "sqlite3"].contains(&db_type.as_str()) {
            return Err(Error::bad_request(format!(
                "不支持的数据库类型: {}",
                data.db_type
            )));
        }

        let result = GenericCrudService::<SysDatasourceBmc>::create(mm, db_id, data.clone()).await?;

        let db_config = Self::to_db_config(&data);
        match mm.register_data_source(db_config).await {
            Ok(_) => info!("数据源注册成功: {}", data.db_id),
            Err(e) => warn!("数据源注册失败: {}, 错误: {}", data.db_id, e),
        }

        Ok(result)
    }

    /// 批量创建数据源
    pub async fn create_many(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<SysDatasourceForCreate>,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - SysDatasourceService::create_many - count: {}",
            "SERVICE",
            items.len()
        );

        let result = GenericCrudService::<SysDatasourceBmc>::create_many(mm, db_id, items.clone()).await?;

        for item in items {
            let db_config = Self::to_db_config(&item);
            match mm.register_data_source(db_config).await {
                Ok(_) => info!("数据源注册成功: {}", item.db_id),
                Err(e) => warn!("数据源注册失败: {}, 错误: {}", item.db_id, e),
            }
        }

        Ok(result)
    }

    /// 更新数据源
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: &str,
        data: SysDatasourceForUpdate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - SysDatasourceService::update - id: {}",
            "SERVICE", id
        );

        let old_data = GenericCrudService::<SysDatasourceBmc>::get(mm, db_id, Value::String(id.to_string())).await?;

        let result = GenericCrudService::<SysDatasourceBmc>::update(
            mm,
            db_id,
            Value::String(id.to_string()),
            data,
        ).await?;

        if let Some(old_db_id) = Self::get_field_from_dataset(&old_data, "db_id") {
            let _ = mm.unregister_data_source(&old_db_id).await;
        }

        if let Some(new_config) = Self::build_db_config_from_dataset(&result) {
            match mm.register_data_source(new_config).await {
                Ok(_) => info!("数据源重新注册成功"),
                Err(e) => warn!("数据源重新注册失败: {}", e),
            }
        }

        let schema = Arc::new(Schema::new("empty", vec![]));
        let result= DataSet::empty("result", schema);

        Ok(result)
    }

    /// 批量更新数据源
    pub async fn update_many(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<UpdatePayload<SysDatasourceForUpdate>>,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - SysDatasourceService::update_many - count: {}",
            "SERVICE",
            items.len()
        );

        let result = GenericCrudService::<SysDatasourceBmc>::update_many(mm, db_id, items.clone()).await?;

        for item in items {
            if let Ok(old_data) = GenericCrudService::<SysDatasourceBmc>::get(mm, db_id, item.id.clone()).await {
                if let Some(old_db_id) = Self::get_field_from_dataset(&old_data, "db_id") {
                    let _ = mm.unregister_data_source(&old_db_id).await;
                }
            }
        }

        Ok(result)
    }

    /// 删除数据源
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<String>) -> Result<DataSet> {
        info!(
            "{:<12} - SysDatasourceService::delete - count: {}",
            "SERVICE",
            ids.len()
        );

        for id in &ids {
            if let Ok(dataset) = GenericCrudService::<SysDatasourceBmc>::get(mm, db_id, Value::String(id.clone())).await {
                if let Some(ds_db_id) = Self::get_field_from_dataset(&dataset, "db_id") {
                    match mm.unregister_data_source(&ds_db_id).await {
                        Ok(_) => info!("数据源注销成功: {}", ds_db_id),
                        Err(e) => warn!("数据源注销失败: {}, 错误: {}", ds_db_id, e),
                    }
                }
            }
        }

        let ids_value: Vec<Value> = ids.into_iter().map(Value::String).collect();
        GenericCrudService::<SysDatasourceBmc>::delete(mm, db_id, ids_value)
            .await
            .map_err(Error::from)
    }

    /// 按 db_id 查询数据源
    pub async fn get_by_db_id(
        mm: &DatabaseManager,
        db_id: &str,
        target_db_id: &str,
    ) -> Result<DataSet> {
        debug!(
            "{:<12} - SysDatasourceService::get_by_db_id - target: {}",
            "SERVICE", target_db_id
        );

        let filter = SysDatasourceFilter {
            id: None,
            db_id: Some(OpValsString(vec![OpValString::Eq(target_db_id.to_string())])),
            db_type: None,
            default_flag: None,
            status: None,
            archived: None,
        };

        GenericCrudService::<SysDatasourceBmc, SysDatasourceFilter>::list(mm, db_id, Some(filter), None)
            .await
            .map_err(Error::from)
    }

    /// 测试数据源连接
    pub async fn test_connection(mm: &DatabaseManager, db_id: &str) -> Result<bool> {
        debug!(
            "{:<12} - SysDatasourceService::test_connection - db_id: {}",
            "SERVICE", db_id
        );

        mm.health_check(db_id).await.map_err(|e| {
            Error::internal_error(format!("数据源连接测试失败: {}", e))
        })
    }

    /// 列出所有已注册的数据源
    pub fn list_registered(mm: &DatabaseManager) -> Vec<String> {
        mm.list_data_sources()
    }

    /// 将 SysDatasourceForCreate 转换为 DbConfig
    fn to_db_config(data: &SysDatasourceForCreate) -> DbConfig {
        let db_type = DbType::from_str(&data.db_type).unwrap_or(DbType::Postgres);

        DbConfig {
            db_type,
            db_url: data.db_url.clone(),
            db_id: data.db_id.clone(),
            db_schema: data.db_schema.clone(),
            default: data.default_flag.unwrap_or(0) == 1,
            pool_config: PoolConfig {
                max_connections: data.max_connections.unwrap_or(10) as usize,
                min_connections: data.min_connections.unwrap_or(2) as usize,
                connect_timeout: data.connect_timeout.unwrap_or(30) as u64,
                idle_timeout: data.idle_timeout.unwrap_or(600) as u64,
                max_lifetime: data.max_lifetime.unwrap_or(1800) as u64,
            },
            health_check_interval: data.health_check_interval.unwrap_or(60) as u64,
            health_check_timeout: data.health_check_timeout.unwrap_or(5) as u64,
        }
    }

    /// 从 DataSet 中获取字段值
    fn get_field_from_dataset(dataset: &DataSet, field_name: &str) -> Option<String> {
        let row = dataset.iter().next()?;
        let value = row.get_by_name(&dataset.schema, field_name)?;
        String::try_from(value.clone()).ok()
    }

    /// 从 DataSet 构建 DbConfig
    fn build_db_config_from_dataset(dataset: &DataSet) -> Option<DbConfig> {
        let row = dataset.iter().next()?;
        let schema = &dataset.schema;

        let db_id = String::try_from(row.get_by_name(schema, "db_id")?.clone()).ok()?;
        let db_type_str = String::try_from(row.get_by_name(schema, "db_type")?.clone()).ok()?;
        let db_url = String::try_from(row.get_by_name(schema, "db_url")?.clone()).ok()?;
        let db_type = DbType::from_str(&db_type_str).ok()?;

        let db_schema = row.get_by_name(schema, "db_schema")
            .and_then(|v| String::try_from(v.clone()).ok());
        let default_flag = row.get_by_name(schema, "default_flag")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(0);

        let max_connections = row.get_by_name(schema, "max_connections")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(10) as usize;
        let min_connections = row.get_by_name(schema, "min_connections")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(2) as usize;
        let connect_timeout = row.get_by_name(schema, "connect_timeout")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(30) as u64;
        let idle_timeout = row.get_by_name(schema, "idle_timeout")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(600) as u64;
        let max_lifetime = row.get_by_name(schema, "max_lifetime")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(1800) as u64;
        let health_check_interval = row.get_by_name(schema, "health_check_interval")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(60) as u64;
        let health_check_timeout = row.get_by_name(schema, "health_check_timeout")
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(5) as u64;

        Some(DbConfig {
            db_type,
            db_url,
            db_id,
            db_schema,
            default: default_flag == 1,
            pool_config: PoolConfig {
                max_connections,
                min_connections,
                connect_timeout,
                idle_timeout,
                max_lifetime,
            },
            health_check_interval,
            health_check_timeout,
        })
    }
}
