//! 服务存储实现
//!
//! 实现 cmx_traits::ServiceStorage trait。

use std::sync::Arc;
use async_trait::async_trait;
use cmx_core::model::service::ServiceDefinition;
use cmx_traits::{ServiceStorage, TraitError};

use crate::repository::ServiceRepository;

/// 服务存储实现
#[derive(Clone)]
pub struct ServiceStorageImpl {
    repository: Arc<ServiceRepository>,
}

impl ServiceStorageImpl {
    /// 创建服务存储实现
    pub fn new(repository: Arc<ServiceRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl ServiceStorage for ServiceStorageImpl {
    async fn save_service(&self, service: &ServiceDefinition) -> Result<(), TraitError> {
        self.repository.save_service(service).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn save_service_version(
        &self,
        service_key: &str,
        version: &str,
        plugin_id: &str,
        plugin_version: &str,
        config: &str,
    ) -> Result<(), TraitError> {
        self.repository.save_service_version(
            service_key, version, plugin_id, plugin_version, config
        ).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn delete_service(&self, service_key: &str) -> Result<(), TraitError> {
        self.repository.delete_service(service_key).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn delete_services_by_plugin(&self, plugin_id: &str) -> Result<(), TraitError> {
        self.repository.delete_services_by_plugin(plugin_id).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn get_service_config(&self, service_key: &str, version: &str) -> Result<Option<String>, TraitError> {
        self.repository.get_service_config(service_key, version).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        Ok(self.repository.get_service_config(service_key, version).await.ok().flatten())
    }
}
