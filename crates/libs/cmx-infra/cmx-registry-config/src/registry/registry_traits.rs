//! 注册中心 trait 定义。
//!
//! 该模块定义服务注册中心的抽象接口。
//! 所有具体实现（Nacos、Mock、未来的 Consul/etcd）都必须实现 [`ServiceRegistry`]。

use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

use crate::error::RegistryError;

/// 服务实例变更回调类型。
///
/// 使用 owned 类型（`String`、`Vec<ServiceInstance>`）作为参数，
/// 便于调用方在异步上下文中跨 `await` 传递，避免引用生命周期约束。
pub type InstanceChangeCallback = Arc<dyn Fn(String, Vec<ServiceInstance>) + Send + Sync>;

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
///
/// # Metadata 适配要求
///
/// [`ServiceInstance::metadata`] 是跨注册中心的统一元数据载体，用于传递附加信息
/// （如 `grpc_port`、`version` 等）。每个注册中心实现必须确保：
///
/// 1. **注册时**：将 `metadata` 完整写入注册中心的原生元数据字段
/// 2. **查询/订阅时**：从注册中心读取原生元数据，完整还原到 `metadata`
/// 3. **注销时**：能通过 `ip + port + service_name` 准确定位并删除实例
///
/// 各注册中心的适配方式：
///
/// | 注册中心    | 注册时 metadata 写入                           | 查询时 metadata 读取                            |
/// |-----------|----------------------------------------------|-----------------------------------------------|
/// | Nacos     | `metadata` → `NacosServiceInstance.metadata` | `NacosServiceInstance.metadata` → `metadata`  |
/// | Consul    | `metadata` → `Service.Meta`                  | `Service.Meta` → `metadata`                   |
/// | etcd      | `ServiceInstance` 整体序列化为 JSON value        | JSON value 反序列化为 `ServiceInstance`          |
/// | ZooKeeper | `ServiceInstance` 整体序列化为 JSON znode data   | JSON znode data 反序列化为 `ServiceInstance`     |
///
/// # 标准 Metadata Key
///
/// | Key          | 说明               | 示例          |
/// |-------------|-------------------|--------------|
/// | `grpc_port` | gRPC 服务端口       | `"9090"`     |
/// | `version`   | 服务版本号（预留）    | `"1.0.0"`    |
/// | `protocol`  | 支持的协议列表（预留） | `"http,grpc"` |
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
    /// 订阅服务实例变更通知。
    ///
    /// 默认实现为空操作（no-op），具体注册中心实现可覆盖以提供真实的推送能力。
    async fn subscribe_instances(
        &self,
        service_name: &str,
        callback: InstanceChangeCallback,
    ) -> Result<(), RegistryError> {
        let _ = (service_name, callback);
        Ok(())
    }

    /// 获取缓存的服务实例列表（纯内存，无网络请求）。
    ///
    /// 默认实现返回 `None`，具体注册中心实现可覆盖以提供本地缓存能力。
    fn get_cached_instances(&self, service_name: &str) -> Option<Vec<ServiceInstance>> {
        let _ = service_name;
        None
    }

    /// 获取注册中心中的服务名列表。
    ///
    /// 默认实现返回空列表，具体注册中心实现可覆盖。
    async fn get_service_list(&self) -> Result<Vec<String>, RegistryError> {
        Ok(vec![])
    }
}
