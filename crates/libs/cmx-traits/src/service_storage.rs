//! 服务存储接口
//!
//! 定义服务定义的存储接口，供其他模块（cmx-plugin）调用 cmx-service 存储服务定义。
//! cmx-service 模块实现此 trait。

use crate::error::TraitError;
use cmx_core::model::service::ServiceDefinition;

/// 服务存储 trait
///
/// 定义服务定义的存储接口，用于 cmx-plugin 存储插件安装时解析出的服务定义。
/// cmx-service 模块实现此 trait。
#[async_trait::async_trait]
pub trait ServiceStorage: Send + Sync {
    /// 保存服务定义
    ///
    /// # Arguments
    /// * `service` - 服务定义
    ///
    /// # Returns
    /// * `Ok(())` - 保存成功
    /// * `Err(TraitError)` - 保存失败
    async fn save_service(&self, service: &ServiceDefinition) -> Result<(), TraitError>;

    /// 保存服务版本
    ///
    /// # Arguments
    /// * `service_key` - 服务唯一标识
    /// * `version` - 版本号
    /// * `plugin_id` - 插件ID
    /// * `plugin_version` - 插件版本
    /// * `config` - 编排配置 JSON
    ///
    /// # Returns
    /// * `Ok(())` - 保存成功
    /// * `Err(TraitError)` - 保存失败
    async fn save_service_version(
        &self,
        service_key: &str,
        version: &str,
        plugin_id: &str,
        plugin_version: &str,
        config: &str,
    ) -> Result<(), TraitError>;

    /// 删除服务定义及其所有版本（物理删除）
    ///
    /// # Arguments
    /// * `service_key` - 服务唯一标识
    ///
    /// # Returns
    /// * `Ok(())` - 删除成功
    /// * `Err(TraitError)` - 删除失败
    async fn delete_service(&self, service_key: &str) -> Result<(), TraitError>;

    /// 根据插件ID删除所有服务
    ///
    /// # Arguments
    /// * `plugin_id` - 插件唯一标识
    ///
    /// # Returns
    /// * `Ok(())` - 删除成功
    /// * `Err(TraitError)` - 删除失败
    async fn delete_services_by_plugin(&self, plugin_id: &str) -> Result<(), TraitError>;

    /// 获取服务编排配置
    ///
    /// # Arguments
    /// * `service_key` - 服务唯一标识
    /// * `version` - 版本号
    ///
    /// # Returns
    /// * `Ok(Some(config))` - 找到配置
    /// * `Ok(None)` - 配置不存在
    /// * `Err(TraitError)` - 查询失败
    async fn get_service_config(&self, service_key: &str, version: &str) -> Result<Option<String>, TraitError>;
}
