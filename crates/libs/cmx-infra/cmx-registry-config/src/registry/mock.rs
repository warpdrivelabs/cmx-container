//! Mock 注册中心实现。
//!
//! 该模块提供 [`ServiceRegistry`] 的内存级实现，仅用于本地开发与单元测试环境。
//! 所有操作均在内存中维护，不涉及任何网络 IO。

use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::info;

use crate::error::RegistryError;
use crate::utils::{read_lock, write_lock};

use super::instance_cache::ServiceInstanceCache;
use super::registry_traits::{InstanceChangeCallback, ServiceInstance, ServiceRegistry};

/// Mock 注册中心。
///
/// 内存级实现，使用 `std::sync::RwLock<Vec<ServiceInstance>>` 维护已注册实例列表。
/// 适用于本地开发和单元测试，不持久化、重启即丢失。
pub struct MockRegistry {
    /// 已注册实例列表，多个 Mock 共享同一份数据通过 `Arc` 共享。
    registered: RwLock<Vec<ServiceInstance>>,
    /// 服务实例缓存。
    cache: Arc<ServiceInstanceCache>,
    /// 已订阅的服务名集合（避免重复 subscribe 导致 callback 累积）。
    subscribed_services: RwLock<HashSet<String>>,
}

impl MockRegistry {
    /// 创建 Mock 注册中心。
    #[deprecated(since = "0.1.8", note = "请使用 new_with_cache() 以支持服务实例缓存")]
    pub fn new() -> Self {
        Self {
            registered: RwLock::new(Vec::new()),
            cache: Arc::new(ServiceInstanceCache::new()),
            subscribed_services: RwLock::new(HashSet::new()),
        }
    }

    /// 创建带外部缓存的 Mock 注册中心。
    ///
    /// 允许外部共享同一个缓存实例（例如通过 `GlobalServiceInstanceCache`）。
    pub fn new_with_cache(cache: Arc<ServiceInstanceCache>) -> Self {
        Self {
            registered: RwLock::new(Vec::new()),
            cache,
            subscribed_services: RwLock::new(HashSet::new()),
        }
    }
}

impl Default for MockRegistry {
    /// 返回默认（空）Mock 注册中心。
    fn default() -> Self {
        Self::new_with_cache(Arc::new(ServiceInstanceCache::new()))
    }
}

#[async_trait]
impl ServiceRegistry for MockRegistry {
    /// 注册服务实例：追加到内部列表尾部，并更新缓存。
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        write_lock(&self.registered).push(instance.clone());
        info!(
            "[MockRegistry] 注册服务: {}:{} ({})",
            instance.ip, instance.port, instance.service_name
        );
        self.refresh_cache(&instance.service_name);
        Ok(())
    }

    /// 注销服务实例：移除 `service_name`、`ip` 和 `port` 同时匹配的实例，并更新缓存。
    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        {
            let mut registered = write_lock(&self.registered);
            registered.retain(|i| {
                !(i.service_name == instance.service_name
                    && i.ip == instance.ip
                    && i.port == instance.port)
            });
        }
        info!(
            "[MockRegistry] 注销服务: {}:{} ({})",
            instance.ip, instance.port, instance.service_name
        );
        let service_name = instance.service_name.clone();
        self.refresh_cache(&service_name);
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
        let registered = read_lock(&self.registered);
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

    /// 订阅服务实例变更通知。
    ///
    /// 每个 service_name 只注册一次 callback（通过 `subscribed_services` 去重），
    /// 避免重复调用导致 callback 累积。
    async fn subscribe_instances(
        &self,
        service_name: &str,
        callback: InstanceChangeCallback,
    ) -> Result<(), RegistryError> {
        // 检查是否已订阅，避免重复注册 callback
        let already_subscribed = {
            let mut set = write_lock(&self.subscribed_services);
            if set.contains(service_name) {
                true
            } else {
                set.insert(service_name.to_string());
                false
            }
        };

        if !already_subscribed {
            self.cache.subscribe(service_name, callback);
        }

        // 首次拉取
        let instances = self.query_instances(service_name, None, Vec::new()).await?;
        self.cache.update(service_name, instances);
        Ok(())
    }

    /// 获取缓存的服务实例列表（纯内存，无网络请求）。
    fn get_cached_instances(&self, service_name: &str) -> Option<Vec<ServiceInstance>> {
        self.cache.get(service_name)
    }

    async fn get_service_list(&self) -> Result<Vec<String>, RegistryError> {
        let registered = read_lock(&self.registered);
        let mut names: Vec<String> = registered.iter().map(|i| i.service_name.clone()).collect();
        names.sort();
        names.dedup();
        Ok(names)
    }
}

impl MockRegistry {
    /// 根据内部 registered 列表刷新指定服务的缓存。
    fn refresh_cache(&self, service_name: &str) {
        let registered = read_lock(&self.registered);
        let instances: Vec<ServiceInstance> = registered
            .iter()
            .filter(|i| i.service_name == service_name)
            .cloned()
            .collect();
        self.cache.update(service_name, instances);
    }
}
