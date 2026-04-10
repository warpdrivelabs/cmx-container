//! 服务查询实现
//!
//! 实现 cmx_traits::ServiceQuery trait。

use std::sync::Arc;
use async_trait::async_trait;
use cmx_core::model::service::{ServiceInfo, ServiceOrchestration};
use cmx_traits::{ServiceQuery, TraitError};

use crate::registry::ServiceRegistry;
use crate::repository::ServiceRepository;

/// 服务查询实现
#[derive(Clone)]
pub struct ServiceQueryImpl {
    repository: Arc<ServiceRepository>,
    registry: Arc<ServiceRegistry>,
}

impl ServiceQueryImpl {
    /// 创建服务查询实现
    pub fn new(repository: Arc<ServiceRepository>, registry: Arc<ServiceRegistry>) -> Self {
        Self { repository, registry }
    }
}

#[async_trait]
impl ServiceQuery for ServiceQueryImpl {
    async fn get_service(&self, service_key: &str) -> Result<Option<ServiceInfo>, TraitError> {
        if let Some(service) = self.registry.get(service_key).await {
            return Ok(Some(service));
        }

        let service_def = self.repository.get_service(service_key).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        Ok(service_def.map(|def| ServiceInfo::from(def)))
    }

    async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceInfo>, TraitError> {
        let services = self.registry.get_by_plugin(plugin_id).await;
        if !services.is_empty() {
            return Ok(services);
        }

        let service_defs = self.repository.get_services_by_plugin(plugin_id).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        Ok(service_defs.into_iter().map(ServiceInfo::from).collect())
    }

    async fn list_active_services(&self) -> Result<Vec<ServiceInfo>, TraitError> {
        let all_services = self.repository.list_services().await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        let active: Vec<ServiceInfo> = all_services
            .into_iter()
            .filter(|s| s.status == 1)
            .map(ServiceInfo::from)
            .collect();

        Ok(active)
    }

    async fn get_orchestration(&self, service_key: &str) -> Result<Option<ServiceOrchestration>, TraitError> {
        if let Some(orch_value) = self.registry.get_orchestration(service_key).await {
            match serde_json::from_value::<ServiceOrchestration>(orch_value) {
                Ok(orch) => return Ok(Some(orch)),
                Err(e) => {
                    tracing::warn!("解析编排 JSON 失败: {}", e);
                }
            }
        }

        let service = self.repository.get_service(service_key).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        match service {
            Some(svc) => {
                let versions = self.repository.get_service_versions(&svc.service_key).await
                    .map_err(|e| TraitError::Internal(e.to_string()))?;

                if let Some((version, _)) = versions.first() {
                    if let Some(config) = self.repository.get_service_config(&svc.service_key, version).await
                        .map_err(|e| TraitError::Internal(e.to_string()))? {
                        let orch: ServiceOrchestration = serde_json::from_str(&config)
                            .map_err(|e| TraitError::Internal(e.to_string()))?;
                        return Ok(Some(orch));
                    }
                }
                Ok(None)
            }
            None => Ok(None),
        }
    }
}
