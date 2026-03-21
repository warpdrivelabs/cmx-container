//! 服务注册工具模块
//!
//! 提供插件服务注册等通用操作。
//!
//! # 功能概述
//!
//! - 注册插件服务到服务注册表
//! - 从插件定义注册服务
//! - 注销插件服务

use std::path::Path;
use std::sync::Arc;

use crate::error::{PluginError, PluginResult};
use crate::runtime::service_registry::{ServiceDefinition, ServiceRegistry};

/// 服务注册工具依赖
///
/// 包含服务注册工具运行所需的所有依赖项。
pub struct ServiceUtilsDeps {
    /// 服务注册表
    ///
    /// 用于存储和管理插件提供的服务。
    /// 服务注册后可被其他组件查询和调用。
    pub service_registry: Arc<ServiceRegistry>,
}

/// 服务注册工具
///
/// 提供插件服务注册和注销的统一接口。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::common::{ServiceUtils, ServiceUtilsDeps};
/// use std::sync::Arc;
/// # use cmx_plugin::runtime::service_registry::ServiceRegistry;
///
/// # fn example(service_registry: Arc<ServiceRegistry>) {
/// let utils = ServiceUtils::new(ServiceUtilsDeps {
///     service_registry,
/// });
/// # }
/// ```
pub struct ServiceUtils {
    deps: ServiceUtilsDeps,
}

impl ServiceUtils {
    /// 创建新的服务注册工具
    ///
    /// # 参数
    ///
    /// * `deps` - 服务注册工具的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的服务注册工具实例
    pub fn new(deps: ServiceUtilsDeps) -> Self {
        Self { deps }
    }

    /// 注册插件服务
    ///
    /// 从插件安装目录读取 manifest.json 并注册其中定义的服务。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件 ID，用于标识服务提供者
    /// * `install_path` - 插件安装目录路径，应包含 manifest.json 文件
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - `PluginError::Activate`: 当读取 manifest.json 失败时
    /// - `PluginError::Activate`: 当解析插件定义失败时
    ///
    /// # 说明
    ///
    /// 如果安装目录不存在 manifest.json 文件，方法将静默返回成功，
    /// 并输出调试日志表示跳过服务注册。
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// # async fn example(utils: &cmx_plugin::common::ServiceUtils) -> Result<(), cmx_plugin::error::PluginError> {
    /// let install_path = Path::new("./plugins/my-plugin");
    /// utils.register_plugin_services("my-plugin", install_path).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_plugin_services(
        &self,
        plugin_id: &str,
        install_path: &Path,
    ) -> PluginResult<()> {
        let manifest_path = install_path.join("manifest.json");

        if !manifest_path.exists() {
            tracing::debug!("插件 {} 没有 manifest.json 文件，跳过服务注册", plugin_id);
            return Ok(());
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Activate(format!("读取插件定义失败: {}", e)))?;

        let plugin_def: cmx_core::model::meta::plugin::PluginDefinition =
            serde_json::from_str(&content)
                .map_err(|e| PluginError::Activate(format!("解析插件定义失败: {}", e)))?;

        self.register_services_from_definition(plugin_id, &plugin_def).await
    }

    /// 从插件定义注册服务
    ///
    /// 遍历插件定义中的服务列表，逐个注册到服务注册表。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件 ID，作为服务提供者标识
    /// * `plugin_def` - 插件定义，包含服务声明
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 说明
    ///
    /// 此方法会遍历 `plugin_def.services` 列表，为每个服务：
    /// 1. 创建 `ServiceDefinition` 对象
    /// 2. 设置服务类型为 "wasm"
    /// 3. 将入口点、版本、描述等信息存储到配置中
    /// 4. 注册到服务注册表
    ///
    /// 如果某个服务注册失败，会记录警告日志但不会中断整个流程。
    /// 这样可以确保其他服务仍然能够正常注册。
    ///
    /// # 服务定义结构
    ///
    /// 注册的服务包含以下信息：
    /// - `id`: 服务 ID（来自 `service_id` 字段）
    /// - `name`: 服务名称
    /// - `provider_plugin_id`: 提供者插件 ID
    /// - `service_type`: 固定为 "wasm"
    /// - `config`: 包含入口点、版本、描述的 JSON 对象
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use cmx_core::model::meta::plugin::PluginDefinition;
    /// # async fn example(
    /// #     utils: &cmx_plugin::common::ServiceUtils,
    /// #     plugin_def: &PluginDefinition
    /// # ) -> Result<(), cmx_plugin::error::PluginError> {
    /// utils.register_services_from_definition("my-plugin", plugin_def).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_services_from_definition(
        &self,
        plugin_id: &str,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
    ) -> PluginResult<()> {
        if plugin_def.services.is_empty() {
            tracing::debug!("插件 {} 没有定义服务，跳过服务注册", plugin_id);
            return Ok(());
        }

        for service in &plugin_def.services {
            let service_def = ServiceDefinition {
                id: service.service_id.clone(),
                name: service.name.clone(),
                provider_plugin_id: plugin_id.to_string(),
                service_type: "wasm".to_string(),
                config: Some(serde_json::json!({
                    "entry_point": service.entry_point,
                    "version": service.version,
                    "description": service.description,
                })),
            };

            if let Err(e) = self.deps.service_registry.register(service_def).await {
                tracing::warn!("注册服务 {} 失败: {}", service.service_id, e);
            } else {
                tracing::info!("成功注册服务: {} (插件: {})", service.service_id, plugin_id);
            }
        }

        Ok(())
    }

    /// 注销插件服务
    ///
    /// 从服务注册表中移除插件提供的所有服务。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 要注销服务的插件 ID
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 说明
    ///
    /// 此方法会移除指定插件注册的所有服务。
    /// 通常在插件卸载或停用时调用。
    ///
    /// 即使插件没有注册任何服务，调用此方法也是安全的，
    /// 它会静默返回成功。
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # async fn example(utils: &cmx_plugin::common::ServiceUtils) -> Result<(), cmx_plugin::error::PluginError> {
    /// utils.unregister_plugin_services("my-plugin").await?;
    /// println!("已注销插件的所有服务");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn unregister_plugin_services(&self, plugin_id: &str) -> PluginResult<()> {
        self.deps.service_registry.unregister_plugin_services(plugin_id).await;
        tracing::info!("已注销插件 {} 的所有服务", plugin_id);
        Ok(())
    }
}
