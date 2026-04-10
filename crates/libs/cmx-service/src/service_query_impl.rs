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
///
/// 组合 ServiceRepository 和 ServiceRegistry 提供服务查询能力：
/// - 优先从内存缓存查询
/// - 缓存未命中时从数据库查询
#[derive(Clone)]
pub struct ServiceQueryImpl {
    /// 服务仓储（数据库访问）
    repository: Arc<ServiceRepository>,
    /// 服务注册中心（内存缓存）
    registry: Arc<ServiceRegistry>,
}

impl ServiceQueryImpl {
    /// 创建服务查询实现
    ///
    /// # 参数
    /// * `repository` - 服务仓储
    /// * `registry` - 服务注册中心
    pub fn new(repository: Arc<ServiceRepository>, registry: Arc<ServiceRegistry>) -> Self {
        Self { repository, registry }
    }
}

#[async_trait]
impl ServiceQuery for ServiceQueryImpl {
    /// 获取服务信息
    ///
    /// 优先从内存缓存查询，未命中时从数据库查询
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回服务信息，如果不存在则返回 None
    async fn get_service(&self, service_key: &str) -> Result<Option<ServiceInfo>, TraitError> {
        if let Some(service) = self.registry.get(service_key).await {
            return Ok(Some(service));
        }

        let service_def = self.repository.get_service(service_key).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        Ok(service_def.map(|def| ServiceInfo::from(def)))
    }

    /// 根据插件ID获取所有服务
    ///
    /// 优先从内存缓存查询，未命中时从数据库查询
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    ///
    /// # 返回值
    /// 返回该插件下所有服务信息列表
    async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceInfo>, TraitError> {
        let services = self.registry.get_by_plugin(plugin_id).await;
        if !services.is_empty() {
            return Ok(services);
        }

        let service_defs = self.repository.get_services_by_plugin(plugin_id).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        Ok(service_defs.into_iter().map(ServiceInfo::from).collect())
    }

    /// 获取所有启用的服务
    ///
    /// # 返回值
    /// 返回所有状态为启用的服务信息列表
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

    /// 获取服务编排定义
    ///
    /// 优先从内存缓存查询，未命中时从数据库查询最新版本的编排配置
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回服务编排定义，如果不存在则返回 None
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
