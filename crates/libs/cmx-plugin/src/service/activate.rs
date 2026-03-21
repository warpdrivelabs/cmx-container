//! 激活服务模块
//!
//! 处理插件激活和停用流程

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::PluginStatus;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::audit::logger::AuditLogger;
use crate::runtime::activation::ActivationManager;
use crate::runtime::service_registry::ServiceRegistry;
use crate::core::context::PluginContext;

/// 激活请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制激活（忽略依赖检查）
    pub force: bool,
}

/// 激活响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 停用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制停用（忽略依赖检查）
    pub force: bool,
}

/// 停用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 是否成功
    pub success: bool,
    /// 普消息
    pub message: String,
}

/// 激活服务依赖
pub struct ActivateServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 激活管理器
    pub activation_manager: Arc<ActivationManager>,
    /// 服务注册表
    pub service_registry: Arc<ServiceRegistry>,
    /// 插件上下文
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 激活服务
pub struct ActivateService {
    deps: ActivateServiceDeps,
}

impl ActivateService {
    /// 创建新的激活服务
    pub fn new(deps: ActivateServiceDeps) -> Self {
        Self { deps }
    }

    /// 激活插件
    ///
    /// 执行完整的插件激活流程：
    /// 1. 检查插件存在
    /// 2. 检查当前状态
    /// 3. 检查依赖
    /// 4. 加载 WASM 模块
    /// 5. 注册服务
    /// 6. 更新状态
    /// 7. 记录审计日志
    pub async fn activate(&self, request: ActivateRequest) -> PluginResult<ActivateResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1：检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2：检查当前状态
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

        // 步骤3：检查依赖
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

        // 步骤4：加载 WASM 模块
        self.deps
            .activation_manager
            .activate(&request.plugin_id, &plugin.version)
            .await
            .map_err(|e| PluginError::Activate(format!("加载 WASM 模块失败: {}", e)))?;

        // 步骤5：注册服务
        self.register_plugin_services(&request.plugin_id, &plugin).await?;

        // 步骤6：更新状态
        self.deps
            .repository
            .update_plugin_status(&request.plugin_id, "activated")
            .await?;

        // 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.status = PluginStatus::Activated;
            }
        }

        // 更新缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤7：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Activate,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

        // 发布事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginActivated,
                request.plugin_id.clone(),
                serde_json::json!({
                    "version": plugin.version,
                }),
            ))
            .await;

        Ok(ActivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件激活成功".to_string(),
        })
    }

    /// 停用插件
    ///
    /// 执行完整的插件停用流程：
    /// 1. 检查插件存在
    /// 2. 检查当前状态
    /// 3. 检查是否有其他插件依赖此插件
    /// 4. 注销服务
    /// 5. 卸载 WASM 模块
    /// 6. 更新状态
    /// 7. 记录审计日志
    pub async fn deactivate(&self, request: DeactivateRequest) -> PluginResult<DeactivateResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1：检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2：检查当前状态
        if plugin.status != "activated" {
            return Ok(DeactivateResponse {
                plugin_id: request.plugin_id,
                success: true,
                message: "插件已经处于停用状态".to_string(),
            });
        }

        // 步骤3：检查是否有其他插件依赖此插件（非强制模式）
        if !request.force {
            let dependents = self.check_active_dependents(&request.plugin_id).await?;
            if !dependents.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "以下已激活的插件依赖此插件: {}",
                    dependents.join(", ")
                )));
            }
        }

        // 步骤4：注销服务
        self.deps
            .service_registry
            .unregister_plugin_services(&request.plugin_id)
            .await;

        // 步骤5：卸载 WASM 模块
        self.deps
            .activation_manager
            .deactivate(&request.plugin_id)
            .await
            .map_err(|e| PluginError::Deactivate(format!("卸载 WASM 模块失败: {}", e)))?;

        // 步骤6：更新状态
        self.deps
            .repository
            .update_plugin_status(&request.plugin_id, "deactivated")
            .await?;

        // 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.status = PluginStatus::Deactivated;
            }
        }

        // 更新缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤7：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Deactivate,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

        // 发布事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginDeactivated,
                request.plugin_id.clone(),
                serde_json::json!({
                    "version": plugin.version,
                }),
            ))
            .await;

        Ok(DeactivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件停用成功".to_string(),
        })
    }

    /// 检查插件依赖是否满足
    async fn check_dependencies(
        &self,
        plugin_id: &str,
    ) -> PluginResult<crate::domain::dependency::DependencyCheckResult> {
        use crate::domain::dependency::{DependencyCheckResult, MissingDependency};

        let mut result = DependencyCheckResult::new();

        // 获取插件信息
        let plugin = self
            .deps
            .repository
            .find_plugin(plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(plugin_id))?;

        // 从元数据中获取依赖信息
        if let Some(ref metadata) = plugin.metadata {
            if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                for dep in deps {
                    if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                        // 检查依赖插件是否已安装
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
        }

        Ok(result)
    }

    /// 检查已激活的依赖此插件的其他插件
    async fn check_active_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self
            .deps
            .repository
            .list_plugins(&crate::domain::plugin::PluginFilter::default())
            .await?;
        let mut dependents = Vec::new();

        for plugin in all_plugins {
            // 只检查已激活的插件
            if plugin.status != "activated" {
                continue;
            }

            // 从元数据中获取依赖信息
            if let Some(ref metadata) = plugin.metadata {
                if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                            if dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(dependents)
    }

    /// 注册插件提供的服务
    async fn register_plugin_services(
        &self,
        plugin_id: &str,
        plugin: &crate::infrastructure::database::repository::PluginDbRecord,
    ) -> PluginResult<()> {
        // 从元数据中获取服务定义
        if let Some(ref metadata) = plugin.metadata {
            if let Some(services) = metadata.get("services").and_then(|s| s.as_array()) {
                for service in services {
                    let service_name = service
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");

                    let service_type = service
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("default");

                    // 注册服务
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
        }

        Ok(())
    }
}

impl Default for ActivateService {
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(ActivateServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(std::path::Path::new(""))),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            activation_manager: Arc::new(ActivationManager::new()),
            service_registry: Arc::new(ServiceRegistry::new()),
            contexts: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }
}
