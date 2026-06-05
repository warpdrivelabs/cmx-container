//! Mock 注册中心实现。
//!
//! 该模块提供 [`ServiceRegistry`] 的内存级实现，仅用于本地开发与单元测试环境。
//! 所有操作均在内存中维护，不涉及任何网络 IO。

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::RegistryError;

use super::trait_rs::{ServiceInstance, ServiceRegistry};

/// Mock 注册中心。
///
/// 内存级实现，使用 `tokio::sync::RwLock<Vec<ServiceInstance>>` 维护已注册实例列表。
/// 适用于本地开发和单元测试，不持久化、重启即丢失。
pub struct MockRegistry {
    /// 已注册实例列表，多个 Mock 共享同一份数据通过 `Arc` 共享。
    registered: Arc<RwLock<Vec<ServiceInstance>>>,
}

impl MockRegistry {
    /// 创建 Mock 注册中心。
    ///
    /// # Returns
    ///
    /// 返回初始为空的 `MockRegistry` 实例。
    pub fn new() -> Self {
        Self {
            registered: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for MockRegistry {
    /// 返回默认（空）Mock 注册中心。
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceRegistry for MockRegistry {
    /// 注册服务实例：追加到内部列表尾部。
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        self.registered.write().await.push(instance.clone());
        info!(
            "[MockRegistry] 注册服务: {}:{} ({})",
            instance.ip, instance.port, instance.service_name
        );
        Ok(())
    }

    /// 注销服务实例：移除 `ip` 和 `port` 同时匹配的实例。
    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        let mut registered = self.registered.write().await;
        registered.retain(|i| !(i.ip == instance.ip && i.port == instance.port));
        info!(
            "[MockRegistry] 注销服务: {}:{} ({})",
            instance.ip, instance.port, instance.service_name
        );
        Ok(())
    }

    /// 查询指定 `service_name` 的所有已注册实例。
    ///
    /// `group_name` 和 `clusters` 在 Mock 实现中被忽略。
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

    /// Mock 实现始终视为已启用。
    fn is_enabled(&self) -> bool {
        true
    }
}
