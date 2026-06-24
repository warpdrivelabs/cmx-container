//! 通用服务实例缓存。
//!
//! 提供注册中心无关的内存级缓存与变更订阅机制。
//! 不依赖任何具体注册中心 SDK，可被 `NacosRegistry`、`MockRegistry` 等共享使用。

use std::{
    collections::HashMap,
    sync::RwLock,
};

use tracing::{debug, info};

use super::trait_rs::{InstanceChangeCallback, ServiceInstance};
use crate::error::RegistryError;
use crate::utils::{read_lock, write_lock};

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
    /// 锁 poisoned 时返回 `None` 并打印警告。
    pub fn get(&self, service_name: &str) -> Option<Vec<ServiceInstance>> {
        read_lock(&self.cached).get(service_name).cloned()
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
        debug!(
            service_name = %service_name,
            count = instances.len(),
            "缓存更新"
        );

        {
            let mut cached = write_lock(&self.cached);
            cached.insert(service_name.to_string(), instances.clone());
        }

        // 先 clone 出订阅者列表，释放锁后再调用回调，避免回调内部操作 subscribers 导致死锁
        let subscribers_snapshot: Vec<InstanceChangeCallback> = {
            let subscribers = read_lock(&self.subscribers);
            subscribers.get(service_name).cloned().unwrap_or_default()
        };

        for cb in &subscribers_snapshot {
            cb(service_name, &instances);
        }
    }

    /// 注册变更回调。
    ///
    /// 允许同一 service_name 注册多个回调（如 discover 的变更通知回调 + 业务层回调）。
    /// `cache.update` 时所有回调都会被调用。
    pub fn subscribe(&self, service_name: &str, callback: InstanceChangeCallback) {
        info!(service_name = %service_name, "注册实例变更订阅");
        let mut subscribers = write_lock(&self.subscribers);
        subscribers
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
