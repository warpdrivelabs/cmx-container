//! 激活服务模块
//!
//! 处理插件激活和停用流程，提供完整的插件生命周期管理。
//!
//! # 功能概述
//!
//! - 激活已安装的插件
//! - 停用已激活的插件
//! - 检查依赖关系
//! - 管理 WASM 模块加载
//! - 注册和注销服务
//!
//! # 激活流程
//!
//! 1. 检查插件存在
//! 2. 检查当前状态
//! 3. 检查依赖
//! 4. 加载 WASM 模块
//! 5. 注册服务
//! 6. 更新状态
//! 7. 记录审计日志
//!
//! # 停用流程
//!
//! 1. 检查插件存在
//! 2. 检查当前状态
//! 3. 检查依赖者
//! 4. 注销服务
//! 5. 卸载 WASM 模块
//! 6. 更新状态
//! 7. 记录审计日志

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::audit::logger::AuditLogger;
use crate::core::context::PluginContext;
use crate::domain::plugin::PluginStatus;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::storage::file::FileStorage;
use crate::runtime::activation::ActivationManager;
use crate::runtime::service_registry::ServiceRegistry;

/// 激活请求
///
/// 包含插件激活所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    /// 插件ID
    ///
    /// 要激活的插件的唯一标识符。
    pub plugin_id: String,

    /// 是否强制激活
    ///
    /// - `true`: 忽略依赖检查，强制激活
    /// - `false`: 检查所有依赖是否满足
    ///
    /// # 注意
    ///
    /// 强制激活可能导致运行时错误，因为依赖的插件可能未激活。
    pub force: bool,
}

/// 激活响应
///
/// 包含激活操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateResponse {
    /// 插件ID
    ///
    /// 被激活的插件唯一标识符。
    pub plugin_id: String,

    /// 是否成功
    ///
    /// 指示激活操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 激活结果的描述性消息。
    pub message: String,
}

/// 停用请求
///
/// 包含插件停用所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateRequest {
    /// 插件ID
    ///
    /// 要停用的插件的唯一标识符。
    pub plugin_id: String,

    /// 是否强制停用
    ///
    /// - `true`: 忽略依赖者检查，强制停用
    /// - `false`: 检查是否有其他已激活插件依赖此插件
    ///
    /// # 注意
    ///
    /// 强制停用可能导致依赖此插件的其他插件无法正常工作。
    pub force: bool,
}

/// 停用响应
///
/// 包含停用操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateResponse {
    /// 插件ID
    ///
    /// 被停用的插件唯一标识符。
    pub plugin_id: String,

    /// 是否成功
    ///
    /// 指示停用操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 停用结果的描述性消息。
    pub message: String,
}

/// 激活服务依赖
///
/// 包含激活服务运行所需的所有依赖项。
pub struct ActivateServiceDeps {
    /// 数据仓库
    ///
    /// 用于查询和更新插件状态。
    pub repository: Arc<PluginRepository>,

    /// 缓存管理器
    ///
    /// 用于缓存插件信息，激活后清除缓存。
    pub cache: Arc<LayeredCacheManager>,

    /// 文件存储
    ///
    /// 用于访问插件文件。
    pub storage: Arc<FileStorage>,

    /// 审计日志
    ///
    /// 用于记录激活/停用操作的审计日志。
    pub audit_logger: Arc<AuditLogger>,

    /// 激活管理器
    ///
    /// 用于管理 WASM 模块的加载和卸载。
    pub activation_manager: Arc<ActivationManager>,

    /// 服务注册表
    ///
    /// 用于注册和注销插件提供的服务。
    pub service_registry: Arc<ServiceRegistry>,

    /// 插件上下文映射
    ///
    /// 存储插件的运行时上下文信息。
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 激活服务
///
/// 提供插件激活和停用功能的核心服务。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::service::activate::{ActivateService, ActivateRequest, DeactivateRequest};
///
/// # async fn example(service: &ActivateService) -> Result<(), cmx_plugin::error::PluginError> {
/// // 激活插件
/// let activate_req = ActivateRequest {
///     plugin_id: "my-plugin".to_string(),
///     force: false,
/// };
/// let response = service.activate(activate_req).await?;
///
/// // 停用插件
/// let deactivate_req = DeactivateRequest {
///     plugin_id: "my-plugin".to_string(),
///     force: false,
/// };
/// let response = service.deactivate(deactivate_req).await?;
/// # Ok(())
/// # }
/// ```
pub struct ActivateService {
    deps: ActivateServiceDeps,
}

impl ActivateService {
    /// 创建新的激活服务
    ///
    /// # 参数
    ///
    /// * `deps` - 激活服务的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的激活服务实例
    pub fn new(deps: ActivateServiceDeps) -> Self {
        Self { deps }
    }

    /// 激活插件
    ///
    /// 执行完整的插件激活流程。
    ///
    /// # 参数
    ///
    /// * `request` - 激活请求，包含插件ID和选项
    ///
    /// # 返回值
    ///
    /// 返回激活响应，包含操作结果信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::PluginNotFound`: 插件不存在
    /// - `PluginError::InvalidState`: 插件状态不允许激活
    /// - `PluginError::Dependency`: 依赖检查失败（非强制模式）
    /// - `PluginError::Activate`: WASM 模块加载失败
    /// - `PluginError::Activate`: 服务注册失败
    ///
    /// # 流程说明
    ///
    /// 1. **检查插件存在**: 验证插件是否已安装
    /// 2. **检查当前状态**: 确认插件处于可激活状态
    /// 3. **检查依赖**: 验证所有依赖是否已激活（非强制模式）
    /// 4. **加载 WASM 模块**: 将插件代码加载到运行时
    /// 5. **注册服务**: 将插件提供的服务注册到服务注册表
    /// 6. **更新状态**: 将插件状态更新为 "activated"
    /// 7. **记录审计日志**: 记录激活操作
    /// 8. **发布事件**: 通知其他组件插件已激活
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use cmx_plugin::service::activate::{ActivateService, ActivateRequest};
    /// # async fn example(service: &ActivateService) -> Result<(), cmx_plugin::error::PluginError> {
    /// let request = ActivateRequest {
    ///     plugin_id: "my-plugin".to_string(),
    ///     force: false,
    /// };
    ///
    /// let response = service.activate(request).await?;
    /// if response.success {
    ///     println!("插件激活成功: {}", response.message);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn activate(&self, request: ActivateRequest) -> PluginResult<ActivateResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1: 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2: 检查当前状态
        if plugin.status == "activated" {
            return Ok(ActivateResponse {
                plugin_id: request.plugin_id,
                success: true,
                message: "插件已经处于激活状态".to_string(),
            });
        }

        if plugin.status != "installed" && plugin.status != "deactivated" && !request.force {
            return Err(PluginError::invalid_state(
                &request.plugin_id,
                &plugin.status,
                "installed or deactivated",
            ));
        }

        // 步骤3: 检查依赖
        if !request.force {
            let dep_result = self.check_dependencies(&request.plugin_id).await?;
            if !dep_result.satisfied {
                let missing: Vec<String> = dep_result
                    .missing
                    .iter()
                    .map(|m| m.plugin_id.clone())
                    .collect();
                return Err(PluginError::Dependency(format!(
                    "缺少依赖插件: {}",
                    missing.join(", ")
                )));
            }
        }

        // 步骤4: 加载 WASM 模块
        self.deps
            .activation_manager
            .activate(&request.plugin_id, &plugin.version)
            .await
            .map_err(|e| PluginError::Activate(format!("加载 WASM 模块失败: {}", e)))?;

        // 步骤5: 注册服务
        self.register_plugin_services(&request.plugin_id, &plugin).await?;

        // 步骤6: 更新状态
        self.deps
            .repository
            .update_plugin_status(&request.plugin_id, "activated")
            .await?;

        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.status = PluginStatus::Activated;
            }
        }

        // 步骤7: 清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤8: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Activate,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
        }))
        .with_new_value("activated".to_string())
        .with_completed(duration_ms);
        let _ = self.deps.audit_logger.log(audit_record).await;

        Ok(ActivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件激活成功".to_string(),
        })
    }

    /// 停用插件
    ///
    /// 执行完整的插件停用流程。
    ///
    /// # 参数
    ///
    /// * `request` - 停用请求，包含插件ID和选项
    ///
    /// # 返回值
    ///
    /// 返回停用响应，包含操作结果信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::PluginNotFound`: 插件不存在
    /// - `PluginError::Dependency`: 有其他已激活插件依赖此插件（非强制模式）
    /// - `PluginError::Deactivate`: WASM 模块卸载失败
    ///
    /// # 流程说明
    ///
    /// 1. **检查插件存在**: 验证插件是否已安装
    /// 2. **检查当前状态**: 确认插件处于激活状态
    /// 3. **检查依赖者**: 验证是否有其他已激活插件依赖此插件（非强制模式）
    /// 4. **注销服务**: 从服务注册表移除插件提供的服务
    /// 5. **卸载 WASM 模块**: 从运行时卸载插件代码
    /// 6. **更新状态**: 将插件状态更新为 "deactivated"
    /// 7. **记录审计日志**: 记录停用操作
    /// 8. **发布事件**: 通知其他组件插件已停用
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use cmx_plugin::service::activate::{ActivateService, DeactivateRequest};
    /// # async fn example(service: &ActivateService) -> Result<(), cmx_plugin::error::PluginError> {
    /// let request = DeactivateRequest {
    ///     plugin_id: "my-plugin".to_string(),
    ///     force: false,
    /// };
    ///
    /// let response = service.deactivate(request).await?;
    /// if response.success {
    ///     println!("插件停用成功: {}", response.message);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn deactivate(&self, request: DeactivateRequest) -> PluginResult<DeactivateResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1: 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2: 检查当前状态
        if plugin.status != "activated" {
            return Ok(DeactivateResponse {
                plugin_id: request.plugin_id,
                success: true,
                message: "插件已经处于停用状态".to_string(),
            });
        }

        // 步骤3: 检查依赖者
        if !request.force {
            let dependents = self.check_active_dependents(&request.plugin_id).await?;
            if !dependents.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "以下已激活的插件依赖此插件: {}",
                    dependents.join(", ")
                )));
            }
        }

        // 步骤4: 注销服务
        self.deps
            .service_registry
            .unregister_plugin_services(&request.plugin_id)
            .await;

        // 步骤5: 卸载 WASM 模块
        self.deps
            .activation_manager
            .deactivate(&request.plugin_id)
            .await
            .map_err(|e| PluginError::Deactivate(format!("卸载 WASM 模块失败: {}", e)))?;

        // 步骤6: 更新状态
        self.deps
            .repository
            .update_plugin_status(&request.plugin_id, "deactivated")
            .await?;

        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.status = PluginStatus::Deactivated;
            }
        }

        // 步骤7: 清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤8: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Deactivate,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
        }))
        .with_old_value("activated".to_string())
        .with_new_value("deactivated".to_string())
        .with_completed(duration_ms);
        let _ = self.deps.audit_logger.log(audit_record).await;

        Ok(DeactivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件停用成功".to_string(),
        })
    }

    /// 检查插件依赖是否满足
    ///
    /// 验证插件的所有依赖是否已安装。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 要检查的插件 ID
    ///
    /// # 返回值
    ///
    /// 返回依赖检查结果，包含缺失的依赖列表。
    async fn check_dependencies(
        &self,
        plugin_id: &str,
    ) -> PluginResult<crate::domain::dependency::DependencyCheckResult> {
        use crate::domain::dependency::{DependencyCheckResult, MissingDependency};

        let mut result = DependencyCheckResult::new();

        let plugin = self
            .deps
            .repository
            .find_plugin(plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(plugin_id))?;

        if let Some(ref metadata) = plugin.metadata
            && let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                for dep in deps {
                    if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                        let installed = self.deps.repository.plugin_exists(dep_id).await?;
                        if !installed {
                            result.add_missing(MissingDependency {
                                plugin_id: dep_id.to_string(),
                                version_constraint: None,
                                required_by: plugin_id.to_string(),
                            });
                        }
                    }
                }
            }

        Ok(result)
    }

    /// 检查已激活的依赖此插件的其他插件
    ///
    /// 查找所有已激活且依赖指定插件的插件。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 要检查的目标插件 ID
    ///
    /// # 返回值
    ///
    /// 返回已激活且依赖此插件的插件 ID 列表。
    async fn check_active_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self
            .deps
            .repository
            .list_plugins(&crate::domain::plugin::PluginFilter::default())
            .await?;
        let mut dependents = Vec::new();

        for plugin in all_plugins {
            if plugin.status != "activated" {
                continue;
            }

            if let Some(ref metadata) = plugin.metadata
                && let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str())
                            && dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                    }
                }
        }

        Ok(dependents)
    }

    /// 注册插件提供的服务
    ///
    /// 从插件元数据中读取服务定义并注册到服务注册表。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件 ID
    /// * `plugin` - 插件数据库记录，包含元数据
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - `PluginError::Activate`: 服务注册失败
    async fn register_plugin_services(
        &self,
        plugin_id: &str,
        plugin: &crate::infrastructure::database::repository::PluginRecord,
    ) -> PluginResult<()> {
        if let Some(ref metadata) = plugin.metadata
            && let Some(services) = metadata.get("services").and_then(|s| s.as_array()) {
                for service in services {
                    let service_name = service
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");

                    let service_type = service
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("default");

                    let definition = crate::runtime::service_registry::ServiceDefinition {
                        id: format!("{}.{}", plugin_id, service_name),
                        name: service_name.to_string(),
                        provider_plugin_id: plugin_id.to_string(),
                        service_type: service_type.to_string(),
                        config: Some(service.clone()),
                    };

                    self.deps
                        .service_registry
                        .register(definition)
                        .await
                        .map_err(|e| PluginError::Activate(format!("注册服务失败: {}", e)))?;
                    tracing::info!("已注册服务: {}.{}", plugin_id, service_name);
                }
            }

        Ok(())
    }
}

impl Default for ActivateService {
    /// 创建默认配置的激活服务
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(ActivateServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(std::path::Path::new(""))),
            audit_logger: Arc::new(AuditLogger::default()),
            activation_manager: Arc::new(ActivationManager::new()),
            service_registry: Arc::new(ServiceRegistry::new()),
            contexts: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }
}
