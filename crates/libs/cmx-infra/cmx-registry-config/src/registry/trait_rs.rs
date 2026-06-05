//! 注册中心 trait 定义

use async_trait::async_trait;
use std::collections::HashMap;

use crate::error::RegistryError;

/// 服务实例信息
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    /// IP 地址
    pub ip: String,
    /// 端口号
    pub port: u16,
    /// 服务名称
    pub service_name: String,
    /// 分组名称
    pub group_name: Option<String>,
    /// 集群名称
    pub cluster_name: Option<String>,
    /// 实例权重
    pub weight: f64,
    /// 实例元数据
    pub metadata: HashMap<String, String>,
    /// 是否健康
    pub healthy: bool,
    /// 是否为临时实例
    pub ephemeral: bool,
}

/// 服务注册中心 trait
///
/// 抽象微服务实例的注册、注销和发现能力。
/// 实现：NacosRegistry、MockRegistry、(未来) ConsulRegistry 等。
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// 注册服务实例
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError>;

    /// 注销服务实例
    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError>;

    /// 查询健康的服务实例列表
    async fn query_instances(
        &self,
        service_name: &str,
        group_name: Option<&str>,
        clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, RegistryError>;

    /// 检查注册中心是否已启用
    fn is_enabled(&self) -> bool;
}
