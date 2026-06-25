//! Nacos 注册中心适配器。
//!
//! 该模块基于 `nacos-sdk` 实现 [`ServiceRegistry`] trait，
//! 提供与 Nacos 命名服务的注册、注销、发现能力。
//!
//! # 数据模型转换
//!
//! 通过两个内部函数 `convert_to_nacos_instance` / `convert_from_nacos_instance`
//! 实现 cmx-container 的 [`ServiceInstance`] 与 nacos-sdk 的 `NacosServiceInstance`
//! 之间的双向转换。

use async_trait::async_trait;
use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingServiceBuilder,
    ServiceInstance as NacosServiceInstance,
};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

use crate::config_model::NacosNamingConfig;
use crate::error::RegistryError;
use crate::utils::write_lock;

use super::instance_cache::ServiceInstanceCache;
use super::registry_traits::{InstanceChangeCallback, ServiceInstance, ServiceRegistry};

/// Nacos 服务实例变更监听器。
///
/// 实现 nacos-sdk 的 [`NamingEventListener`] trait，
/// 当收到实例变更事件时更新本地缓存。
struct NacosInstanceListener {
    service_name: String,
    cache: Arc<ServiceInstanceCache>,
}

impl NamingEventListener for NacosInstanceListener {
    fn event(&self, event: Arc<NamingChangeEvent>) {
        let cache = self.cache.clone();
        let service_name = self.service_name.clone();
        // 异步处理变更事件，避免阻塞 nacos-sdk 通知线程。
        // 实例转换和缓存更新（含订阅者回调）可能耗时，放入 tokio task 执行。
        // nacos-sdk 基于 tokio，其回调运行在 tokio 运行时上下文中，可安全 spawn。
        tokio::spawn(async move {
            let total = event.instances.as_ref().map(|v| v.len()).unwrap_or(0);
            let instances: Vec<ServiceInstance> = event
                .instances
                .as_ref()
                .map(|v| {
                    v.iter()
                        .filter(|i| i.healthy)
                        .filter_map(convert_from_nacos_instance)
                        .collect()
                })
                .unwrap_or_default();
            let healthy = instances.len();
            cache.update(&service_name, instances);
            info!(
                service_name = %service_name,
                total = total,
                healthy = healthy,
                "服务实例变更，缓存已更新"
            );
        });
    }
}

/// Nacos 注册中心实现。
///
/// 内部持有 `nacos-sdk` 的 `NamingService` 句柄，
/// 通过该句柄与 Nacos Server 通信完成注册/发现操作。
pub struct NacosRegistry {
    /// nacos-sdk 命名服务客户端。
    naming: nacos_sdk::api::naming::NamingService,
    /// 服务实例缓存。
    cache: Arc<ServiceInstanceCache>,
    /// 已注册 Nacos 监听器的服务名称集合。
    registered_listeners: RwLock<HashSet<String>>,
}

impl NacosRegistry {
    /// 创建 Nacos 注册中心实例。
    ///
    /// 构造 `ClientProps` 并构建 `NamingService` 客户端。
    /// 如配置了用户名和密码则启用认证。
    #[deprecated(since = "0.1.8", note = "请使用 new_with_cache() 以支持服务实例缓存")]
    pub async fn new(config: &NacosNamingConfig) -> Result<Self, RegistryError> {
        let cache = Arc::new(ServiceInstanceCache::new());
        Self::new_with_cache(config, cache).await
    }

    /// 创建带外部缓存的 Nacos 注册中心实例。
    ///
    /// 允许外部共享同一个缓存实例（例如通过 `GlobalServiceInstanceCache`）。
    pub async fn new_with_cache(
        config: &NacosNamingConfig,
        cache: Arc<ServiceInstanceCache>,
    ) -> Result<Self, RegistryError> {
        let client_props = crate::utils::build_nacos_client_props(
            &config.server_addr,
            &config.namespace,
            &config.app_name,
            &config.username,
            &config.password,
        );

        let naming = NamingServiceBuilder::new(client_props)
            .build()
            .await
            .map_err(|e| RegistryError::InitFailed(format!("命名服务初始化失败: {}", e)))?;

        info!("Nacos 命名服务初始化成功: {}", config.server_addr);

        Ok(Self {
            naming,
            cache,
            registered_listeners: RwLock::new(HashSet::new()),
        })
    }
}

/// 将 cmx-container 的 [`ServiceInstance`] 转换为 nacos-sdk 的 `NacosServiceInstance`。
fn convert_to_nacos_instance(instance: &ServiceInstance) -> NacosServiceInstance {
    NacosServiceInstance {
        ip: instance.ip.clone(),
        port: instance.port as i32,
        service_name: Some(instance.service_name.clone()),
        cluster_name: instance.cluster_name.clone(),
        weight: instance.weight,
        healthy: instance.healthy,
        ephemeral: instance.ephemeral,
        metadata: instance.metadata.clone(),
        ..Default::default()
    }
}

/// 将 nacos-sdk 的 `NacosServiceInstance` 转换为 cmx-container 的 [`ServiceInstance`]。
///
/// 端口号超出 `u16` 范围时返回 `None`，调用方应过滤掉此类无效实例，
/// 避免端口 0 污染缓存导致后续连接失败。
fn convert_from_nacos_instance(nacos_instance: &NacosServiceInstance) -> Option<ServiceInstance> {
    // 从 serviceName 解析 group_name（Nacos 格式：group_name@@service_name）
    let (group_name, service_name) = match &nacos_instance.service_name {
        Some(name) if name.contains("@@") => {
            let parts: Vec<&str> = name.splitn(2, "@@").collect();
            (Some(parts[0].to_string()), parts[1].to_string())
        }
        other => (None, other.clone().unwrap_or_default()),
    };

    // 端口号必须能转换为 u16，否则视为无效实例跳过
    let port = match u16::try_from(nacos_instance.port) {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(
                ip = %nacos_instance.ip,
                port = nacos_instance.port,
                service_name = %service_name,
                "Nacos 实例端口号超出 u16 范围，已跳过该实例"
            );
            return None;
        }
    };

    Some(ServiceInstance {
        ip: nacos_instance.ip.clone(),
        port,
        service_name,
        group_name,
        cluster_name: nacos_instance.cluster_name.clone(),
        weight: nacos_instance.weight,
        healthy: nacos_instance.healthy,
        ephemeral: nacos_instance.ephemeral,
        metadata: nacos_instance.metadata.clone(),
    })
}

#[async_trait]
impl ServiceRegistry for NacosRegistry {
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        let nacos_instance = convert_to_nacos_instance(instance);
        self.naming
            .register_instance(
                instance.service_name.clone(),
                instance.group_name.clone(),
                nacos_instance,
            )
            .await
            .map_err(|e| RegistryError::RegisterFailed(e.to_string()))?;

        info!(
            "服务实例已注册到 Nacos: {}:{} ({}/{})",
            instance.ip,
            instance.port,
            instance.group_name.as_deref().unwrap_or("DEFAULT_GROUP"),
            instance.service_name
        );
        Ok(())
    }

    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        let nacos_instance = convert_to_nacos_instance(instance);
        self.naming
            .deregister_instance(
                instance.service_name.clone(),
                instance.group_name.clone(),
                nacos_instance,
            )
            .await
            .map_err(|e| RegistryError::DeregisterFailed(e.to_string()))?;

        info!("服务实例已从 Nacos 注销: {}:{}", instance.ip, instance.port);
        Ok(())
    }

    async fn query_instances(
        &self,
        service_name: &str,
        mut group_name: Option<&str>,
        clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, RegistryError> {
        if group_name.is_none() {
            //nacos 默认组名为 DEFAULT_GROUP
            group_name = Some(crate::config_model::DEFAULT_GROUP);
        }

        let result = self
            .naming
            .select_instances(
                service_name.to_string(),
                group_name.map(|s| s.to_string()),
                clusters,
                true,
                true,
            )
            .await;

        let instances = match result {
            Ok(v) => v,
            Err(e) => {
                // 查询失败直接返回错误。
                // 注意：此处不再移除 subscribe_instances 中设置的订阅占位，
                // 因为订阅本身可能已成功（Nacos 会持续推送变更），
                // 移除占位会导致并发场景下重复订阅。
                // 若确需重订阅，应由独立的健康检查机制处理。
                warn!(
                    "查询实例失败 (service={}): {}，保留订阅占位等待 Nacos 推送",
                    service_name, e
                );
                return Err(RegistryError::QueryFailed(e.to_string()));
            }
        };

        Ok(instances
            .iter()
            .filter_map(convert_from_nacos_instance)
            .collect())
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn subscribe_instances(
        &self,
        service_name: &str,
        callback: InstanceChangeCallback,
    ) -> Result<(), RegistryError> {
        // 注册 Nacos 监听器（每个 service_name 只注册一次）
        // 使用写锁原子检查+占位，避免并发 TOCTOU 导致重复订阅
        let already_registered = {
            let mut set = write_lock(&self.registered_listeners);
            if set.contains(service_name) {
                true
            } else {
                set.insert(service_name.to_string());
                false
            }
        };

        if !already_registered {
            let listener = Arc::new(NacosInstanceListener {
                service_name: service_name.to_string(),
                cache: self.cache.clone(),
            });
            if let Err(e) = self
                .naming
                .subscribe(service_name.to_string(), None, Vec::new(), listener)
                .await
            {
                // 订阅失败，回滚占位
                write_lock(&self.registered_listeners).remove(service_name);
                return Err(RegistryError::QueryFailed(e.to_string()));
            }

            // Nacos SDK 订阅成功后注册 cache callback，
            // 避免订阅失败时 callback 残留在 cache 中。
            // 仅首次订阅时注册，防止重复调用导致 callback 累积。
            self.cache.subscribe(service_name, callback);
        }

        // 首次拉取：失败时不回滚订阅占位。
        // 订阅本身已成功，Nacos 会通过 listener 推送变更更新缓存，
        // 移除占位会导致并发场景下重复订阅。
        match self.query_instances(service_name, None, Vec::new()).await {
            Ok(instances) => {
                self.cache.update(service_name, instances);
            }
            Err(e) => {
                warn!(
                    service_name = %service_name,
                    error = %e,
                    "首次拉取实例失败，等待 Nacos 推送更新缓存"
                );
            }
        }

        Ok(())
    }

    fn get_cached_instances(&self, service_name: &str) -> Option<Vec<ServiceInstance>> {
        self.cache.get(service_name)
    }

    async fn get_service_list(&self) -> Result<Vec<String>, RegistryError> {
        // 分页拉取所有服务，每页 1000 条，避免大规模部署遗漏服务。
        const PAGE_SIZE: i32 = 1000;
        let mut all_services = Vec::new();
        let mut page_no = 1;

        loop {
            let (services, _total) = self
                .naming
                .get_service_list(page_no, PAGE_SIZE, None)
                .await
                .map_err(|e| RegistryError::QueryFailed(e.to_string()))?;

            let fetched = services.len();
            all_services.extend(services);

            if (fetched as i32) < PAGE_SIZE {
                break;
            }
            page_no += 1;
        }

        Ok(all_services)
    }
}
