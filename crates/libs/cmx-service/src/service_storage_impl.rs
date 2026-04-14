//! 服务存储实现
//!
//! 实现 cmx_traits::ServiceStorage trait。

use std::sync::Arc;
use async_trait::async_trait;
use cmx_core::model::service::ServiceDefinition;
use cmx_traits::{ServiceStorage, TraitError};

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
    /// * `service_key` - 服务唯一标识
    /// * `version` - 服务版本号
    /// * `plugin_id` - 所属插件ID
    /// * `plugin_version` - 所属插件版本
    /// * `config` - 编排配置 JSON 字符串
    /// * `db_id` - 数据库ID
    /// * `txn_id` - 事务ID（可选）
    async fn save_service_version(
        &self,
        service_key: &str,
        version: &str,
        plugin_id: &str,
        plugin_version: &str,
        config: &str,
        txn_id: Option<&str>,
    ) -> Result<(), TraitError> {
        self.repository
            .save_service_version_with_txn(
                service_key,
                version,
                plugin_id,
                plugin_version,
                config,
                txn_id,
            )
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 删除服务定义及其所有版本（物理删除）
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    async fn delete_service(&self, service_key: &str,txn_id: Option<&str>, version: Option<&str>) -> Result<(), TraitError> {
        self.repository
            .delete_service(service_key,txn_id,version)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 根据插件ID删除所有服务（物理删除）
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    async fn delete_services_by_plugin(&self, plugin_id: &str) -> Result<(), TraitError> {
        self.repository
            .delete_services_by_plugin(plugin_id)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 获取服务编排配置
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `version` - 服务版本号
    ///
    /// # 返回值
    /// 返回编排配置 JSON 字符串，如果不存在则返回 None
    async fn get_service_config(
        &self,
        service_key: &str,
        version: &str,
    ) -> Result<Option<String>, TraitError> {
        self.repository
            .get_service_config(service_key, version)
            .await
            .map_err(|e| TraitError::Internal(e.to_string()))
    }
}
