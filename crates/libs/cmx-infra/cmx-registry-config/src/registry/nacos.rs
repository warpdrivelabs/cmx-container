//! Nacos 注册中心适配器

use async_trait::async_trait;
use nacos_sdk::api::naming::{NamingServiceBuilder, ServiceInstance as NacosServiceInstance};
use nacos_sdk::api::props::ClientProps;
use tracing::info;

use crate::config::NacosNamingConfig;
use crate::error::RegistryError;

use super::trait_rs::{ServiceInstance, ServiceRegistry};

/// Nacos 注册中心实现
pub struct NacosRegistry {
    naming: nacos_sdk::api::naming::NamingService,
}

impl NacosRegistry {
    /// 创建 Nacos 注册中心实例
    pub fn new(config: &NacosNamingConfig) -> Result<Self, RegistryError> {
        let mut client_props = ClientProps::new()
            .server_addr(&config.server_addr)
            .namespace(&config.namespace)
            .app_name(&config.app_name);

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

    fn is_enabled(&self) -> bool {
        true
    }
}
