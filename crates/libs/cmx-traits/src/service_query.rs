//! 服务查询接口
//!
//! 定义服务信息的查询接口，供其他模块查询服务定义和编排信息。
//! cmx-service 模块实现此 trait。

use crate::error::TraitError;
use cmx_core::model::service::{ServiceInfo, ServiceOrchestration};
use serde::Deserialize;

/// 服务分页查询过滤器
///
/// 支持多条件组合查询服务列表
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServicePageFilter {
    pub app_id: Option<String>,
    /// 服务 key 或者服务名模糊查询
    pub keyword: Option<String>,
    /// 插件 ID 精确匹配
    pub plugin_id: Option<String>,
    /// 域代码精确匹配
    pub domain_code: Option<String>,
    /// 应用代码精确匹配
    pub application_code: Option<String>,
    /// 模块代码精确匹配
    pub module_code: Option<String>,
}

/// 服务分页结果
///
/// 包含分页数据总数
#[derive(Debug, Clone)]
pub struct ServicePageResult {
    /// 服务列表
    pub items: Vec<ServiceInfo>,
    /// 总数
    pub total: u64,
}

/// 服务查询 trait
///
/// 定义服务信息的查询接口，实现模块解耦。
/// cmx-service 模块实现此 trait，供其他模块（cmx-api）查询服务信息。
#[async_trait::async_trait]
pub trait ServiceQuery: Send + Sync {
    /// 根据 service_key 查询服务信息
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// * `Ok(Some(ServiceInfo))` - 找到服务
    /// * `Ok(None)` - 服务不存在
    /// * `Err(TraitError)` - 查询失败
    async fn get_service(&self, service_key: &str) -> Result<Option<ServiceInfo>, TraitError>;

    /// 根据插件ID查询所有服务
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    ///
    /// # 返回值
    /// * `Ok(Vec<ServiceInfo>)` - 该插件下的所有服务列表
    /// * `Err(TraitError)` - 查询失败
    async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceInfo>, TraitError>;

    /// 查询所有启用的服务
    ///
    /// # 返回值
    /// * `Ok(Vec<ServiceInfo>)` - 所有启用状态的服务列表
    /// * `Err(TraitError)` - 查询失败
    async fn list_active_services(&self) -> Result<Vec<ServiceInfo>, TraitError>;

    /// 获取服务的编排定义
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// * `Ok(Some(ServiceOrchestration))` - 找到编排定义
    /// * `Ok(None)` - 编排定义不存在
    /// * `Err(TraitError)` - 查询失败
    async fn get_orchestration(&self, service_key: &str) -> Result<Option<ServiceOrchestration>, TraitError>;

    /// 分页查询服务列表
    ///
    /// 支持多条件组合查询，service_key 和 service_name 支持模糊匹配
    ///
    /// # 参数
    /// * `filter` - 查询过滤器
    /// * `page` - 页码（从 1 开始）
    /// * `size` - 每页大小
    ///
    /// # 返回值
    /// * `Ok(ServicePageResult)` - 分页结果
    /// * `Err(TraitError)` - 查询失败
    async fn page_services(
        &self,
        filter: ServicePageFilter,
        page: u64,
        size: u64,
    ) -> Result<ServicePageResult, TraitError>;
}
