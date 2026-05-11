//! Nacos 命名服务封装
//!
//! 封装 nacos_sdk::api::naming 的服务注册、发现功能

use nacos_sdk::api::naming::{NamingService, ServiceInstance};

use crate::config::NamingConfig;
use crate::error::NacosError;

/// 命名服务客户端
///
/// 封装 Nacos 命名服务，提供服务注册、注销和查询功能
pub struct NamingClient {
    /// Nacos 命名服务实例
    naming_service: NamingService,

    /// 命名服务配置
    naming_config: NamingConfig,
}

impl NamingClient {
    /// 创建命名服务客户端
    ///
    /// # 参数
    /// - `naming_service`: Nacos 命名服务实例
    /// - `naming_config`: 命名服务配置
    pub fn new(naming_service: NamingService, naming_config: NamingConfig) -> Self {
        Self {
            naming_service,
            naming_config,
        }
    }

    /// 注册服务实例
    ///
    /// # 参数
    /// - `ip`: 服务实例 IP 地址
    /// - `port`: 服务实例端口
    pub async fn register_instance(&self, ip: &str, port: u16) -> Result<(), NacosError> {
        let instance = ServiceInstance {
            ip: ip.to_string(),
            port: port as i32,
            service_name: Some(self.naming_config.service_name.clone()),
            cluster_name: Some(self.naming_config.cluster_name.clone()),
            weight: self.naming_config.weight,
            enabled: self.naming_config.enabled,
            healthy: true,
            ephemeral: true,
            metadata: self.naming_config.metadata.clone(),
            ..Default::default()
        };

        self.naming_service
            .register_instance(
                self.naming_config.service_name.clone(),
                Some(self.naming_config.group_name.clone()),
                instance,
            )
            .await
            .map_err(|e| NacosError::RegisterFailed(e.to_string()))?;

        tracing::info!(
            "服务实例已注册到 Nacos: {}:{} ({}/{})",
            ip,
            port,
            self.naming_config.group_name,
            self.naming_config.service_name
        );
        Ok(())
    }

    /// 注销服务实例
    ///
    /// # 参数
    /// - `ip`: 服务实例 IP 地址
    /// - `port`: 服务实例端口
    pub async fn deregister_instance(&self, ip: &str, port: u16) -> Result<(), NacosError> {
        let instance = ServiceInstance {
            ip: ip.to_string(),
            port: port as i32,
            service_name: Some(self.naming_config.service_name.clone()),
            cluster_name: Some(self.naming_config.cluster_name.clone()),
            ..Default::default()
        };

        self.naming_service
            .deregister_instance(
                self.naming_config.service_name.clone(),
                Some(self.naming_config.group_name.clone()),
                instance,
            )
            .await
            .map_err(|e| NacosError::DeregisterFailed(e.to_string()))?;

        tracing::info!("服务实例已从 Nacos 注销: {}:{}", ip, port);
        Ok(())
    }

    /// 查询健康的服务实例列表
    ///
    /// # 参数
    /// - `service_name`: 服务名称
    /// - `group_name`: 分组名称（可选）
    /// - `clusters`: 集群名称列表
    pub async fn select_instances(
        &self,
        service_name: &str,
        group_name: Option<&str>,
        clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, NacosError> {
        let instances = self
            .naming_service
            .select_instances(
                service_name.to_string(),
                group_name.map(|s| s.to_string()),
                clusters,
                true,
                true,
            )
            .await
            .map_err(|e| NacosError::QueryFailed(e.to_string()))?;
        Ok(instances)
    }
}
