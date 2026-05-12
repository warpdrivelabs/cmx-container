//! Nacos 客户端统一入口
//!
//! 整合命名服务和配置中心，提供统一的 NacosClient API

use std::sync::Arc;

use nacos_sdk::api::config::{ConfigChangeListener, ConfigServiceBuilder};
use nacos_sdk::api::naming::{NamingServiceBuilder, ServiceInstance};
use nacos_sdk::api::props::ClientProps;

use crate::config::NacosConfig;
use crate::config_source::NacosConfigSource;
use crate::error::NacosError;

/// Nacos 客户端
///
/// 统一封装命名服务和配置中心功能，提供服务注册、发现、配置获取等能力
pub struct NacosClient {
    /// 命名服务客户端（可选，未启用时为 None）
    naming: Option<NamingServiceWrapper>,

    /// 配置中心客户端（可选，未启用时为 None）
    config: Option<ConfigServiceWrapper>,

    /// Nacos 配置
    nacos_config: NacosConfig,
}

/// 命名服务包装（内部使用，隐藏 nacos_sdk 类型）
struct NamingServiceWrapper {
    /// Nacos 命名服务实例
    service: nacos_sdk::api::naming::NamingService,
}

/// 配置中心包装（内部使用，隐藏 nacos_sdk 类型）
struct ConfigServiceWrapper {
    /// Nacos 配置服务实例
    service: nacos_sdk::api::config::ConfigService,
}

impl NacosClient {
    /// 从配置创建 NacosClient
    ///
    /// 根据 NacosConfig 中的 enabled、naming.enabled、config.enabled 标志
    /// 决定是否初始化命名服务和配置中心。
    ///
    /// # 参数
    /// - `nacos_config`: Nacos 配置
    ///
    /// # 返回值
    /// 成功返回 NacosClient 实例，失败返回 NacosError
    pub fn new(nacos_config: NacosConfig) -> Result<Self, NacosError> {
        if !nacos_config.enabled {
            tracing::info!("Nacos 集成已禁用");
            return Ok(Self {
                naming: None,
                config: None,
                nacos_config,
            });
        }

        // 构建 ClientProps
        let mut client_props = ClientProps::new()
            .server_addr(&nacos_config.server_addr)
            .namespace(&nacos_config.namespace)
            .app_name(&nacos_config.app_name);

        // 设置认证信息
        if let (Some(username), Some(password)) =
            (&nacos_config.username, &nacos_config.password)
        {
            client_props = client_props.auth_username(username).auth_password(password);
        }

        // 初始化命名服务
        let naming = if nacos_config.naming.enabled {
            let service = NamingServiceBuilder::new(client_props.clone())
                .build()
                .map_err(|e| NacosError::InitFailed(format!("命名服务初始化失败: {}", e)))?;
            tracing::info!(
                "Nacos 命名服务初始化成功: {}/{}",
                nacos_config.naming.group_name,
                nacos_config.naming.service_name
            );
            Some(NamingServiceWrapper { service })
        } else {
            tracing::info!("Nacos 命名服务未启用");
            None
        };

        // 初始化配置中心
        let config = if nacos_config.config.enabled {
            let service = ConfigServiceBuilder::new(client_props)
                .build()
                .map_err(|e| NacosError::InitFailed(format!("配置中心初始化失败: {}", e)))?;
            tracing::info!("Nacos 配置中心初始化成功");
            Some(ConfigServiceWrapper { service })
        } else {
            tracing::info!("Nacos 配置中心未启用");
            None
        };

        Ok(Self {
            naming,
            config,
            nacos_config,
        })
    }

    /// 注册服务实例到 Nacos
    ///
    /// # 参数
    /// - `ip`: 服务实例 IP 地址
    /// - `port`: 服务实例端口
    pub async fn register_service(&self, ip: &str, port: u16) -> Result<(), NacosError> {
        let naming = self
            .naming
            .as_ref()
            .ok_or(NacosError::NamingDisabled)?;
        let naming_config = &self.nacos_config.naming;

        let instance = ServiceInstance {
            ip: ip.to_string(),
            port: port as i32,
            service_name: Some(naming_config.service_name.clone()),
            cluster_name: Some(naming_config.cluster_name.clone()),
            weight: naming_config.weight,
            enabled: naming_config.enabled,
            healthy: true,
            ephemeral: true,
            metadata: naming_config.metadata.clone(),
            ..Default::default()
        };

        naming
            .service
            .register_instance(
                naming_config.service_name.clone(),
                Some(naming_config.group_name.clone()),
                instance,
            )
            .await
            .map_err(|e| NacosError::RegisterFailed(e.to_string()))?;

        tracing::info!(
            "服务实例已注册到 Nacos: {}:{} ({}/{})",
            ip,
            port,
            naming_config.group_name,
            naming_config.service_name
        );
        Ok(())
    }

    /// 注销服务实例
    ///
    /// # 参数
    /// - `ip`: 服务实例 IP 地址
    /// - `port`: 服务实例端口
    pub async fn deregister_service(&self, ip: &str, port: u16) -> Result<(), NacosError> {
        let naming = self
            .naming
            .as_ref()
            .ok_or(NacosError::NamingDisabled)?;
        let naming_config = &self.nacos_config.naming;

        let instance = ServiceInstance {
            ip: ip.to_string(),
            port: port as i32,
            service_name: Some(naming_config.service_name.clone()),
            cluster_name: Some(naming_config.cluster_name.clone()),
            ..Default::default()
        };

        naming
            .service
            .deregister_instance(
                naming_config.service_name.clone(),
                Some(naming_config.group_name.clone()),
                instance,
            )
            .await
            .map_err(|e| NacosError::DeregisterFailed(e.to_string()))?;

        tracing::info!("服务实例已从 Nacos 注销: {}:{}", ip, port);
        Ok(())
    }

    /// 获取远程配置内容
    ///
    /// # 参数
    /// - `data_id`: 配置标识
    /// - `group`: 配置分组
    pub async fn get_config(&self, data_id: &str, group: &str) -> Result<String, NacosError> {
        let config = self.config.as_ref().ok_or(NacosError::ConfigDisabled)?;

        let response = config
            .service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .map_err(|e| NacosError::ConfigGetFailed(e.to_string()))?;

        Ok(response.content().to_string())
    }

    /// 获取远程配置并转换为 NacosConfigSource
    ///
    /// 远程配置内容应为 TOML 格式，会被解析为 config::Value 树，
    /// 可通过 ConfigBuilder::add_source() 注入配置系统。
    ///
    /// # 参数
    /// - `data_id`: 配置标识
    /// - `group`: 配置分组
    pub async fn get_config_source(
        &self,
        data_id: &str,
        group: &str,
    ) -> Result<NacosConfigSource, NacosError> {
        let content = self.get_config(data_id, group).await?;
        tracing::info!("已获取的远程配置信息为: {}/{}", group, content);
        NacosConfigSource::from_toml_str(&content)
    }

    /// 添加配置变更监听器
    ///
    /// # 参数
    /// - `data_id`: 配置标识
    /// - `group`: 配置分组
    /// - `listener`: 配置变更监听器（Arc 包装）
    pub async fn listen_config(
        &self,
        data_id: &str,
        group: &str,
        listener: Arc<dyn ConfigChangeListener>,
    ) -> Result<(), NacosError> {
        let config = self.config.as_ref().ok_or(NacosError::ConfigDisabled)?;

        config
            .service
            .add_listener(data_id.to_string(), group.to_string(), listener)
            .await
            .map_err(|e| NacosError::ConfigListenFailed(e.to_string()))?;

        tracing::info!("已添加配置监听: {}/{}", group, data_id);
        Ok(())
    }

    /// 查询健康的服务实例列表
    ///
    /// # 参数
    /// - `service_name`: 服务名称
    /// - `group_name`: 分组名称（可选）
    /// - `clusters`: 集群名称列表
    pub async fn query_instances(
        &self,
        service_name: &str,
        group_name: Option<&str>,
        clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, NacosError> {
        let naming = self
            .naming
            .as_ref()
            .ok_or(NacosError::NamingDisabled)?;

        let instances = naming
            .service
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

    /// 获取 Nacos 配置引用
    pub fn nacos_config(&self) -> &NacosConfig {
        &self.nacos_config
    }

    /// 检查命名服务是否已启用
    pub fn is_naming_enabled(&self) -> bool {
        self.naming.is_some()
    }

    /// 检查配置中心是否已启用
    pub fn is_config_enabled(&self) -> bool {
        self.config.is_some()
    }
}
