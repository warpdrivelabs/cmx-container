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
use nacos_sdk::api::naming::{NamingServiceBuilder, ServiceInstance as NacosServiceInstance};
use nacos_sdk::api::props::ClientProps;
use tracing::info;

use crate::config::NacosNamingConfig;
use crate::error::RegistryError;

use super::trait_rs::{ServiceInstance, ServiceRegistry};

/// Nacos 注册中心实现。
///
/// 内部持有 `nacos-sdk` 的 `NamingService` 句柄，
/// 通过该句柄与 Nacos Server 通信完成注册/发现操作。
pub struct NacosRegistry {
    /// nacos-sdk 命名服务客户端。
    naming: nacos_sdk::api::naming::NamingService,
}

impl NacosRegistry {
    /// 创建 Nacos 注册中心实例。
    ///
    /// 构造 `ClientProps` 并构建 `NamingService` 客户端。
    /// 如配置了用户名和密码则启用认证。
    ///
    /// # Arguments
    ///
    /// * `config` - Nacos 命名服务配置。
    ///
    /// # Returns
    ///
    /// * `Ok(NacosRegistry)` - 初始化成功。
    /// * `Err(RegistryError::InitFailed)` - nacos-sdk 客户端构建失败。
    pub fn new(config: &NacosNamingConfig) -> Result<Self, RegistryError> {
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
            .map_err(|e| RegistryError::InitFailed(format!("命名服务初始化失败: {}", e)))?;

        info!("Nacos 命名服务初始化成功: {}", config.server_addr);

        Ok(Self { naming })
    }
}

/// 将 cmx-container 的 [`ServiceInstance`] 转换为 nacos-sdk 的 `NacosServiceInstance`。
///
/// 注意：Nacos SDK 的 `port` 字段为 `i32`，需要从 `u16` 转换。
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
/// 注意：Nacos SDK 不提供 `group_name` 字段，转换后该字段为 `None`；
/// `port` 从 `i32` 截断为 `u16`。
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
    /// 注册服务实例到 Nacos。
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

    /// 从 Nacos 注销服务实例。
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

    /// 查询健康的服务实例列表。
    ///
    /// 使用 `select_instances` 时启用健康过滤（`healthy = true`）和订阅模式（`subscribe = true`）。
    async fn query_instances(
        &self,
        service_name: &str,
        group_name: Option<&str>,
        clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, RegistryError> {
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

    /// Nacos 实现始终视为已启用。
    fn is_enabled(&self) -> bool {
        true
    }
}
