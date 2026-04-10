//! 服务查询接口
//!
//! 定义服务信息的查询接口，供其他模块查询服务定义和编排信息。
//! cmx-service 模块实现此 trait。

use crate::error::TraitError;
use cmx_core::model::service::{ServiceInfo, ServiceOrchestration};

/// 服务查询 trait
///
/// 定义服务信息的查询接口，实现模块解耦。
/// cmx-service 模块实现此 trait，供其他模块（cmx-api）查询服务信息。
#[async_trait::async_trait]
pub trait ServiceQuery: Send + Sync {
    /// 根据 service_key 查询服务信息
    ///
    /// # Arguments
    /// * `service_key` - 服务唯一标识
    ///
    /// # Returns
    /// * `Ok(Some(ServiceInfo))` - 找到服务
    /// * `Ok(None)` - 服务不存在
    /// * `Err(TraitError)` - 查询失败
    async fn get_service(&self, service_key: &str) -> Result<Option<ServiceInfo>, TraitError>;

    /// 根据插件ID查询所有服务
    ///
    /// # Arguments
    /// * `plugin_id` - 插件唯一标识
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceInfo>)` - 该插件下的所有服务列表
    /// * `Err(TraitError)` - 查询失败
    async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceInfo>, TraitError>;

    /// 查询所有启用的服务
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceInfo>)` - 所有启用状态的服务列表
    /// * `Err(TraitError)` - 查询失败
    async fn list_active_services(&self) -> Result<Vec<ServiceInfo>, TraitError>;

    /// 获取服务的编排定义
    ///
    /// # Arguments
    /// * `service_key` - 服务唯一标识
    ///
    /// # Returns
    /// * `Ok(Some(ServiceOrchestration))` - 找到编排定义
    /// * `Ok(None)` - 编排定义不存在
    /// * `Err(TraitError)` - 查询失败
    async fn get_orchestration(&self, service_key: &str) -> Result<Option<ServiceOrchestration>, TraitError>;
}
