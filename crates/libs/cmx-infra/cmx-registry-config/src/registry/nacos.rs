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
use nacos_sdk::api::props::ClientProps;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::info;

use crate::config::NacosNamingConfig;
use crate::error::RegistryError;

use super::instance_cache::ServiceInstanceCache;
use super::trait_rs::{InstanceChangeCallback, ServiceInstance, ServiceRegistry};

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
        let instances: Vec<ServiceInstance> = event
            .instances
            .as_ref()
            .map(|v| {
                v.iter()
                    .filter(|i| i.healthy)
                    .map(convert_from_nacos_instance)
                    .collect()
            })
            .unwrap_or_default();
        self.cache.update(&self.service_name, instances);
        info!(
            service_name = %self.service_name,
            count = event.instances.as_ref().map(|v| v.len()).unwrap_or(0),
            "服务实例变更，缓存已更新"
        );
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
        let mut client_props = ClientProps::new()
            .server_addr(&config.server_addr)
            .namespace(&config.namespace)
            .app_name(&config.app_name);

        // 同时配置用户名和密码时才启用认证。
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client_props = client_props.auth_username(username).auth_password(password);
        }

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
fn convert_from_nacos_instance(nacos_instance: &NacosServiceInstance) -> ServiceInstance {
    ServiceInstance {
        ip: nacos_instance.ip.clone(),
        port: nacos_instance.port as u16,
        service_name: nacos_instance.service_name.clone().unwrap_or_default(),
        group_name: None,
        cluster_name: nacos_instance.cluster_name.clone(),
        weight: nacos_instance.weight,
        healthy: nacos_instance.healthy,
        ephemeral: nacos_instance.ephemeral,
        metadata: nacos_instance.metadata.clone(),
    }
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
         group_name = Some("DEFAULT_GROUP");
        }

        let instances = self
            .naming
            .select_instances(
                service_name.to_string(),
                group_name.map(|s| s.to_string()),
                clusters,
                true,
                true,
            )
            .await
            .map_err(|e| RegistryError::QueryFailed(e.to_string()))?;

        Ok(instances.iter().map(convert_from_nacos_instance).collect())
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn subscribe_instances(
        &self,
        service_name: &str,
        callback: InstanceChangeCallback,
    ) -> Result<(), RegistryError> {
        self.cache.subscribe(service_name, callback);

        // 首次拉取
        let instances = self.query_instances(service_name, None, Vec::new()).await?;
        self.cache.update(service_name, instances);

        // 注册 Nacos 监听器（每个 service_name 只注册一次）
        if !self
            .registered_listeners
            .read()
            .unwrap()
            .contains(service_name)
        {
            let listener = Arc::new(NacosInstanceListener {
                service_name: service_name.to_string(),
                cache: self.cache.clone(),
            });
            self.naming
                .subscribe(service_name.to_string(), None, Vec::new(), listener)
                .await
                .map_err(|e| RegistryError::QueryFailed(e.to_string()))?;
            self.registered_listeners
                .write()
                .unwrap()
                .insert(service_name.to_string());
        }

        Ok(())
    }

    fn get_cached_instances(&self, service_name: &str) -> Option<Vec<ServiceInstance>> {
        self.cache.get(service_name)
    }
}
