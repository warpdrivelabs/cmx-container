//! 服务查询实现
//!
//! 实现 cmx_traits::ServiceQuery trait。

use std::sync::Arc;
use async_trait::async_trait;
use cmx_core::model::service::{ServiceDefinition, ServiceOrchestration};
use cmx_traits::{ServiceQuery, ServicePageFilter, ServicePageResult, TraitError};

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
    /// 应用隔离标识
    app_id: String,
}

impl ServiceQueryImpl {
    /// 创建服务查询实现
    ///
    /// # 参数
    /// * `repository` - 服务仓储
    /// * `registry` - 服务注册中心
    /// * `app_id` - 应用隔离标识
    pub fn new(repository: Arc<ServiceRepository>, registry: Arc<ServiceRegistry>, app_id: String) -> Self {
        Self { repository, registry, app_id }
    }
}

#[async_trait]
impl ServiceQuery for ServiceQueryImpl {
    /// 获取服务信息
    ///
    /// 优先从内存缓存查询，未命中时从数据库查询并回写到缓存。
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回服务信息，如果不存在则返回 None
    async fn get_service(&self, service_key: &str) -> Result<Option<ServiceDefinition>, TraitError> {
        if let Some(service) = self.registry.get(service_key).await {
            return Ok(Some(service));
        }

        let service_def = self.repository.get_service(service_key, &self.app_id).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        if let Some(def) = &service_def {
            let orchestration = serde_json::from_str::<serde_json::Value>(
                def.config.as_ref().unwrap()
            )
                .map_err(|e| TraitError::Internal(e.to_string()))?;
            self.registry.register(def.clone(), Some(orchestration)).await;
        }

        Ok(service_def)
    }

    /// 根据插件ID获取所有服务
    ///
    /// 优先从内存缓存查询，未命中时从数据库查询并回写到缓存。
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    ///
    /// # 返回值
    /// 返回该插件下所有服务信息列表
    async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceDefinition>, TraitError> {
        let cached_services = self.registry.get_by_plugin(plugin_id).await;
        if !cached_services.is_empty() {
            return Ok(cached_services);
        }

        let service_defs = self.repository.get_services_by_plugin(plugin_id, &self.app_id).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        for def in &service_defs {
            let orchestration = serde_json::from_str::<serde_json::Value>(
                def.config.as_ref().unwrap()
            )
                .map_err(|e| TraitError::Internal(e.to_string()))?;
            self.registry.register(def.clone(), Some(orchestration)).await;
        }

        Ok(service_defs)
    }

    /// 获取所有启用的服务
    ///
    /// 优先从缓存获取，如果缓存中没有则从数据库查询。
    ///
    /// # 返回值
    /// 返回所有状态为启用的服务信息列表
    async fn list_active_services(&self) -> Result<Vec<ServiceDefinition>, TraitError> {
        let all_keys = self.registry.get_all_keys().await;

        if all_keys.is_empty() {
            let all_services = self.repository.list_services(&self.app_id).await
                .map_err(|e| TraitError::Internal(e.to_string()))?;

            let mut active = Vec::new();
            for def in all_services {
                if def.status == 1 {
                    let orchestration = serde_json::from_str::<serde_json::Value>(
                        def.config.as_ref().unwrap()
                    )
                        .map_err(|e| TraitError::Internal(e.to_string()))?;
                    self.registry.register(def.clone(), Some(orchestration)).await;

                    active.push(def);
                }
            }

            return Ok(active);
        }

        let mut active = Vec::new();
        for key in all_keys {
            if let Some(service) = self.registry.get(&key).await
                && service.status == 1 {
                    active.push(service);
                }
        }

        Ok(active)
    }

    /// 获取服务编排定义
    ///
    /// 优先从内存缓存查询，未命中时从数据库查询最新版本的编排配置。
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

        let service = self.repository.get_service(service_key, &self.app_id).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        match service {
            Some(svc) => {
                let versions = self.repository.get_service_versions(&svc.service_key, &self.app_id).await
                    .map_err(|e| TraitError::Internal(e.to_string()))?;

                if let Some((version, _)) = versions.first()
                    && let Some(config) = self.repository.get_service_config(&svc.service_key, version, &self.app_id).await
                        .map_err(|e| TraitError::Internal(e.to_string()))? {
                        let orch: ServiceOrchestration = serde_json::from_str(&config)
                            .map_err(|e| TraitError::Internal(e.to_string()))?;
                        return Ok(Some(orch));
                    }
                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// 分页查询服务列表
    ///
    /// 支持多条件组合查询，service_key 和 service_name 支持模糊匹配。
    /// 注意：分页查询直接查数据库，不走缓存。
    ///
    /// # 参数
    /// * `filter` - 查询过滤器
    /// * `page` - 页码（从 1 开始）
    /// * `size` - 每页大小
    ///
    /// # 返回值
    /// 返回分页结果
    async fn page_services(
        &self,
        mut filter: ServicePageFilter,
        page: u64,
        size: u64,
    ) -> Result<ServicePageResult, TraitError> {
        if filter.app_id.is_none() {
            filter.app_id = Some(self.app_id.clone());
        }
        let (items, total) = self.repository.page_services(&filter, page, size).await
            .map_err(|e| TraitError::Internal(e.to_string()))?;

        Ok(ServicePageResult {
            items,
            total,
        })
    }
}
