//! 服务存储接口
//!
//! 定义服务定义的存储接口，供其他模块（cmx-plugin）调用 cmx-service 存储服务定义。
//! cmx-service 模块实现此 trait。

use crate::error::TraitError;
use cmx_core::model::service::ServiceDefinition;

/// 保存服务版本的参数
///
/// 用于在插件安装/升级时保存服务版本信息到 cmx_service_define_version 表。
#[derive(Debug, Clone)]
pub struct SaveServiceVersionParams {
    /// 服务唯一标识
    pub service_key: String,
    /// 应用隔离标识，用于多租户/多应用场景隔离
    pub app_id: String,
    /// 服务版本号（通常等于插件版本号）
    pub version: String,
    /// 所属插件 ID
    pub plugin_id: String,
    /// 所属插件版本号
    pub plugin_version: String,
    /// 服务编排配置 JSON 字符串
    pub config: String,
    /// 服务接口文档 JSON 字符串（可选，由 api_doc_generator 生成）
    pub api_doc: Option<String>,
    /// 事务 ID（可选，用于跨表事务一致性）
    pub txn_id: Option<String>,
}

/// 服务存储 trait
///
/// 定义服务定义的存储接口，用于 cmx-plugin 存储插件安装时解析出的服务定义。
/// cmx-service 模块实现此 trait。
#[async_trait::async_trait]
pub trait ServiceStorage: Send + Sync {
    /// 保存服务定义
    ///
    /// # 参数
    /// * `service` - 服务定义
    /// * `db_id` - 数据库ID
    /// * `txn_id` - 事务ID（可选）
    ///
    /// # 返回值
    /// * `Ok(())` - 保存成功
    /// * `Err(TraitError)` - 保存失败
    async fn save_service(
        &self,
        service: &ServiceDefinition,
        txn_id: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 保存服务版本
    ///
    /// # 参数
    /// * `params` - 保存参数，包含 service_key, app_id, version, plugin_id, plugin_version, config, txn_id
    ///
    /// # 返回值
    /// * `Ok(())` - 保存成功
    /// * `Err(TraitError)` - 保存失败
    async fn save_service_version(
        &self,
        params: SaveServiceVersionParams,
    ) -> Result<(), TraitError>;

    /// 删除服务定义及其所有版本（物理删除）
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `app_id` - 应用隔离标识
    /// * `txn_id` - 事务id
    /// * `version` - 服务版本
    ///
    /// # 返回值
    /// * `Ok(())` - 删除成功
    /// * `Err(TraitError)` - 删除失败
    async fn delete_service(&self, service_key: &str, app_id: &str, txn_id: Option<&str>, version: Option<&str>) -> Result<(), TraitError>;

    /// 根据插件ID删除所有服务
    ///
    /// # 参数
    /// * `plugin_id` - 插件唯一标识
    /// * `app_id` - 应用隔离标识
    ///
    /// # 返回值
    /// * `Ok(())` - 删除成功
    /// * `Err(TraitError)` - 删除失败
    async fn delete_services_by_plugin(&self, plugin_id: &str, app_id: &str, txn_id: Option<&str>) -> Result<(), TraitError>;

    /// 获取服务编排配置
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `version` - 版本号
    /// * `app_id` - 应用隔离标识
    ///
    /// # 返回值
    /// * `Ok(Some(config))` - 找到配置
    /// * `Ok(None)` - 配置不存在
    /// * `Err(TraitError)` - 查询失败
    async fn get_service_config(&self, service_key: &str, version: &str, app_id: &str) -> Result<Option<String>, TraitError>;
}
