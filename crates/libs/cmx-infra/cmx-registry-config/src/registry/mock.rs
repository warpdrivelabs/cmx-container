//! Mock 注册中心实现

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::RegistryError;

use super::trait_rs::{ServiceInstance, ServiceRegistry};

/// Mock 注册中心
///
/// 内存级实现，用于开发和测试环境。
pub struct MockRegistry {
    registered: Arc<RwLock<Vec<ServiceInstance>>>,
}

impl MockRegistry {
    /// 创建 Mock 注册中心
    pub fn new() -> Self {
        Self {
            registered: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for MockRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceRegistry for MockRegistry {
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        self.registered.write().await.push(instance.clone());
        info!(
            "[MockRegistry] 注册服务: {}:{} ({})",
            instance.ip, instance.port, instance.service_name
        );
        Ok(())
    }

    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        let mut registered = self.registered.write().await;
        registered.retain(|i| !(i.ip == instance.ip && i.port == instance.port));
        info!(
            "[MockRegistry] 注销服务: {}:{} ({})",
            instance.ip, instance.port, instance.service_name
        );
        Ok(())
    }

    async fn query_instances(
        &self,
        service_name: &str,
        _group_name: Option<&str>,
        _clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, RegistryError> {
        let registered = self.registered.read().await;
        let result: Vec<ServiceInstance> = registered
            .iter()
            .filter(|i| i.service_name == service_name)
            .cloned()
            .collect();
        Ok(result)
    }

    fn is_enabled(&self) -> bool {
        true
    }
}
