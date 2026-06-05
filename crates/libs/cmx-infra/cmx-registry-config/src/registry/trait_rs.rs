//! 注册中心 trait 定义。
//!
//! 该模块定义服务注册中心的抽象接口。
//! 所有具体实现（Nacos、Mock、未来的 Consul/etcd）都必须实现 [`ServiceRegistry`]。

use async_trait::async_trait;
use std::collections::HashMap;

use crate::error::RegistryError;

/// 服务实例信息。
///
/// 描述一个注册到注册中心的服务实例的完整属性。
/// 该结构体是 cmx-container 与具体注册中心实现之间的统一数据模型。
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    /// IP 地址。
    pub ip: String,

    /// 端口号。
    pub port: u16,

    /// 服务名称。
    pub service_name: String,

    /// 分组名称，`None` 时使用注册中心默认值。
    pub group_name: Option<String>,

    /// 集群名称，`None` 时使用注册中心默认值。
    pub cluster_name: Option<String>,

    /// 实例权重，范围通常为 `0.0 ~ 1.0`，默认 `1.0`。
    pub weight: f64,

    /// 实例元数据。
    pub metadata: HashMap<String, String>,

    /// 是否健康。
    pub healthy: bool,

    /// 是否为临时实例（进程退出后自动注销）。
    pub ephemeral: bool,
}

/// 服务注册中心 trait。
///
/// 抽象微服务实例的注册、注销和发现能力。
/// 实现：`NacosRegistry`、`MockRegistry`、(未来) `ConsulRegistry` 等。
///
/// 所有方法都是 `async`，因为与注册中心的交互通常是网络 IO。
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// 注册服务实例。
    ///
    /// # Arguments
    ///
    /// * `instance` - 待注册的服务实例信息。
    ///
    /// # Errors
    ///
    /// 当与注册中心通信失败时返回 [`RegistryError`]。
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError>;

    /// 注销服务实例。
    ///
    /// # Arguments
    ///
    /// * `instance` - 待注销的服务实例信息。
    ///
    /// # Errors
    ///
    /// 当与注册中心通信失败时返回 [`RegistryError`]。
    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError>;

    /// 查询健康的服务实例列表。
    ///
    /// # Arguments
    ///
    /// * `service_name` - 目标服务名称。
    /// * `group_name` - 可选的分组过滤条件。
    /// * `clusters` - 可选的集群过滤条件列表。
    ///
    /// # Returns
    ///
    /// 返回健康实例的完整列表，调用方按需应用负载均衡策略。
    ///
    /// # Errors
    ///
    /// 当与注册中心通信失败时返回 [`RegistryError`]。
    async fn query_instances(
        &self,
        service_name: &str,
        group_name: Option<&str>,
        clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, RegistryError>;

    /// 检查注册中心是否已启用。
    ///
    /// # Returns
    ///
    /// * `true` - 注册中心功能已启用，可执行注册/发现。
    /// * `false` - 注册中心被禁用，所有操作应为 no-op。
    fn is_enabled(&self) -> bool;
}
