//! 服务存储实现
//!
//! 实现 cmx_traits::ServiceStorage trait。

use std::sync::Arc;
use async_trait::async_trait;
use cmx_core::model::service::ServiceDefinition;
use cmx_traits::{ServiceStorage, SaveServiceVersionParams, TraitError};

use crate::repository::ServiceRepository;

/// 服务存储实现
///
/// 通过 ServiceRepository 提供服务持久化能力
#[derive(Clone)]
pub struct ServiceStorageImpl {
    /// 服务仓储（数据库访问）
    repository: Arc<ServiceRepository>,
}

impl ServiceStorageImpl {
    /// 创建服务存储实现
    ///
    /// # 参数
    /// * `repository` - 服务仓储
    pub fn new(repository: Arc<ServiceRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl ServiceStorage for ServiceStorageImpl {
    /// 保存服务定义
    ///
    /// 如果 service_key 已存在则更新，否则插入新记录
    ///
    /// # 参数
    /// * `service` - 服务定义
    /// * `db_id` - 数据库ID
    /// * `txn_id` - 事务ID（可选）
    async fn save_service(
        &self,
        service: &ServiceDefinition,
        txn_id: Option<&str>,
    ) -> Result<(), TraitError> {
        self.repository
            .save_service_with_txn(service, txn_id)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 保存服务版本
    ///
    /// # 参数
    /// * `params` - 保存参数
    async fn save_service_version(
        &self,
        params: SaveServiceVersionParams<'_>,
    ) -> Result<(), TraitError> {
        self.repository
            .save_service_version_with_txn(
                params.service_key,
                params.app_id,
                params.version,
                params.plugin_id,
                params.plugin_version,
                params.config,
                params.txn_id,
            )
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 删除服务定义及其所有版本（物理删除）
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `app_id` - 应用隔离标识
    async fn delete_service(&self, service_key: &str, app_id: &str, txn_id: Option<&str>, version: Option<&str>) -> Result<(), TraitError> {
        self.repository
            .delete_service(service_key, app_id, txn_id, version)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 根据插件ID删除所有服务（物理删除）
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `app_id` - 应用隔离标识
    async fn delete_services_by_plugin(&self, plugin_id: &str, app_id: &str, txn_id: Option<&str>) -> Result<(), TraitError> {
        self.repository
            .delete_services_by_plugin(plugin_id, app_id, txn_id)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 获取服务编排配置
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `version` - 服务版本号
    /// * `app_id` - 应用隔离标识
    ///
    /// # 返回值
    /// 返回编排配置 JSON 字符串，如果不存在则返回 None
    async fn get_service_config(
        &self,
        service_key: &str,
        version: &str,
        app_id: &str,
    ) -> Result<Option<String>, TraitError> {
        self.repository
            .get_service_config(service_key, version, app_id)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))
    }
}
