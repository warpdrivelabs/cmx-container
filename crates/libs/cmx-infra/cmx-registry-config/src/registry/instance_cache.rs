//! 通用服务实例缓存。
//!
//! 提供注册中心无关的内存级缓存与变更订阅机制。
//! 不依赖任何具体注册中心 SDK，可被 `NacosRegistry`、`MockRegistry` 等共享使用。

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use super::trait_rs::ServiceInstance;
use crate::error::RegistryError;

/// 服务实例变更回调。
pub type InstanceChangeCallback = Arc<dyn Fn(&str, &[ServiceInstance]) + Send + Sync>;

/// 通用服务实例缓存（注册中心无关）。
///
/// 内部使用 `std::sync::RwLock` 保护 `HashMap`，提供 O(1) 读取。
/// 支持懒加载和变更订阅通知。
pub struct ServiceInstanceCache {
    cached: RwLock<HashMap<String, Vec<ServiceInstance>>>,
    subscribers: RwLock<HashMap<String, Vec<InstanceChangeCallback>>>,
}

impl ServiceInstanceCache {
    /// 创建空缓存实例。
    pub fn new() -> Self {
        Self {
            cached: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    /// 纯内存读取 O(1)。
    ///
    /// 返回指定服务的缓存实例列表，未缓存时返回 `None`。
    pub fn get(&self, service_name: &str) -> Option<Vec<ServiceInstance>> {
        self.cached.read().unwrap().get(service_name).cloned()
    }

    /// 懒加载：缓存命中直接返回，未命中时通过 `fetch_fn` 获取并缓存。
    pub async fn get_or_fetch<F, Fut>(
        &self,
        service_name: &str,
        fetch_fn: F,
    ) -> Result<Vec<ServiceInstance>, RegistryError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Vec<ServiceInstance>, RegistryError>>,
    {
        if let Some(instances) = self.get(service_name) {
            return Ok(instances);
        }
        let instances = fetch_fn().await?;
        self.update(service_name, instances.clone());
        Ok(instances)
    }

    /// 更新缓存并通知所有订阅者。
    pub fn update(&self, service_name: &str, instances: Vec<ServiceInstance>) {
        self.cached
            .write()
            .unwrap()
            .insert(service_name.to_string(), instances.clone());

        if let Some(subscribers) = self.subscribers.read().unwrap().get(service_name) {
            for cb in subscribers {
                cb(service_name, &instances);
            }
        }
    }

    /// 注册变更回调。
    pub fn subscribe(&self, service_name: &str, callback: InstanceChangeCallback) {
        self.subscribers
            .write()
            .unwrap()
            .entry(service_name.to_string())
            .or_default()
            .push(callback);
    }
}

impl Default for ServiceInstanceCache {
    fn default() -> Self {
        Self::new()
    }
}
