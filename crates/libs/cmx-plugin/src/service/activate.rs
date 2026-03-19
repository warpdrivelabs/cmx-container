//! 激活服务模块
//! 
//! 处理插件激活/停用流程

use std::sync::Arc;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::{PluginRepository, PluginUpdateFields};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::audit::logger::AuditLogger;
use crate::audit::record::{AuditRecord, OperationType};
use crate::runtime::activation::ActivationManager;
use crate::runtime::service_registry::ServiceRegistry;

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
    /// 消息
    pub message: String,
}

/// 激活服务
pub struct ActivateService {
    /// 数据仓库
    repository: Arc<PluginRepository>,
    /// 缓存管理器
    cache: Arc<LayeredCacheManager>,
    /// 事件总线
    event_bus: Arc<EventBus>,
    /// 审计日志
    audit_logger: Arc<AuditLogger>,
    /// 激活管理器
    activation_manager: Arc<ActivationManager>,
    /// 服务注册表
    service_registry: Arc<ServiceRegistry>,
}

impl ActivateService {
    /// 创建新的激活服务
    pub fn new(
        repository: Arc<PluginRepository>,
        cache: Arc<LayeredCacheManager>,
        event_bus: Arc<EventBus>,
        audit_logger: Arc<AuditLogger>,
        activation_manager: Arc<ActivationManager>,
        service_registry: Arc<ServiceRegistry>,
    ) -> Self {
        Self {
            repository,
            cache,
            event_bus,
            audit_logger,
            activation_manager,
            service_registry,
        }
    }
    
    /// 激活插件
    /// 
    /// 完整的激活流程：
    /// 1. 检查插件存在
    /// 2. 检查当前状态
    /// 3. 检查依赖是否已激活
    /// 4. 加载 WASM 模块
    /// 5. 注册服务
    /// 6. 更新状态
    /// 7. 记录审计日志
    pub async fn activate(&self, request: ActivateRequest) -> PluginResult<ActivateResponse> {
        let start_time = std::time::Instant::now();
        
        // 步骤1：检查插件存在
        let plugin = self.repository.find_plugin(&request.plugin_id).await?
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
                "activate",
            ));
        }
        
        // 步骤3：检查依赖是否已激活（非强制模式）
        if !request.force {
            let inactive_deps = self.check_dependencies_activated(&request.plugin_id).await?;
            if !inactive_deps.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "以下依赖尚未激活: {}",
                    inactive_deps.join(", ")
                )));
            }
        }
        
        // 步骤4：加载 WASM 模块
        self.activation_manager.activate(&request.plugin_id, &plugin.version).await
            .map_err(|e| PluginError::Activate(format!("加载 WASM 模块失败: {}", e)))?;
        
        // 步骤5：注册服务（如果插件提供服务）
        self.register_plugin_services(&request.plugin_id, &plugin).await?;
        
        // 步骤6：更新状态
        let mut fields = PluginUpdateFields {
            status: Some("activated".to_string()),
            ..Default::default()
        };
        fields.activated_at = Some(Utc::now());
        self.repository.update_plugin(&request.plugin_id, &fields).await?;
        
        // 更新缓存
        self.cache.delete(&format!("plugin:{}", request.plugin_id)).await;
        
        // 步骤7：记录审计日志
        let audit_record = AuditRecord::success(
            request.plugin_id.clone(),
            OperationType::Activate,
        ).with_details(serde_json::json!({
            "version": plugin.version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginActivated,
            request.plugin_id.clone(),
            serde_json::json!({
                "version": plugin.version,
            }),
        )).await;
        
        Ok(ActivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件激活成功".to_string(),
        })
    }
    
    /// 停用插件
    /// 
    /// 完整的停用流程：
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
        let plugin = self.repository.find_plugin(&request.plugin_id).await?
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
        self.service_registry.unregister_plugin_services(&request.plugin_id).await;
        
        // 步骤5：卸载 WASM 模块
        self.activation_manager.deactivate(&request.plugin_id).await
            .map_err(|e| PluginError::Deactivate(format!("卸载 WASM 模块失败: {}", e)))?;
        
        // 步骤6：更新状态
        self.repository.update_plugin_status(&request.plugin_id, "deactivated").await?;
        
        // 更新缓存
        self.cache.delete(&format!("plugin:{}", request.plugin_id)).await;
        
        // 步骤7：记录审计日志
        let audit_record = AuditRecord::success(
            request.plugin_id.clone(),
            OperationType::Deactivate,
        ).with_details(serde_json::json!({
            "version": plugin.version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginDeactivated,
            request.plugin_id.clone(),
            serde_json::json!({
                "version": plugin.version,
            }),
        )).await;
        
        Ok(DeactivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件停用成功".to_string(),
        })
    }
    
    /// 检查依赖是否已激活
    /// 
    /// 获取插件的依赖列表，检查每个依赖是否已激活。
    async fn check_dependencies_activated(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let plugin = self.repository.find_plugin(plugin_id).await?
            .ok_or_else(|| PluginError::plugin_not_found(plugin_id))?;
        
        let mut inactive_deps = Vec::new();
        
        // 从元数据中获取依赖信息
        if let Some(ref metadata) = plugin.metadata {
            if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                for dep in deps {
                    if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                        // 检查依赖是否已安装且激活
                        if let Some(dep_plugin) = self.repository.find_plugin(dep_id).await? {
                            if dep_plugin.status != "activated" {
                                inactive_deps.push(dep_id.to_string());
                            }
                        } else {
                            // 依赖未安装
                            inactive_deps.push(dep_id.to_string());
                        }
                    }
                }
            }
        }
        
        Ok(inactive_deps)
    }
    
    /// 检查已激活的依赖此插件的其他插件
    /// 
    /// 查询所有已激活的插件，检查它们的依赖列表中是否包含当前插件。
    async fn check_active_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self.repository.list_plugins(&crate::domain::plugin::PluginFilter::default()).await?;
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
    /// 
    /// 从插件元数据中获取服务列表并注册到服务注册表。
    async fn register_plugin_services(&self, plugin_id: &str, plugin: &crate::infrastructure::database::repository::PluginDbRecord) -> PluginResult<()> {
        // 从元数据中获取服务定义
        if let Some(ref metadata) = plugin.metadata {
            if let Some(services) = metadata.get("services").and_then(|s| s.as_array()) {
                for service in services {
                    let service_name = service.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    
                    let service_type = service.get("type")
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
                    
                    self.service_registry.register(definition).await
                        .map_err(|e| PluginError::Activate(format!("注册服务失败: {}", e)))?;
                    tracing::info!("已注册服务: {}.{}", plugin_id, service_name);
                }
            }
        }
        
        Ok(())
    }
    
    /// 检查插件是否已激活
    pub async fn is_activated(&self, plugin_id: &str) -> PluginResult<bool> {
        Ok(self.activation_manager.is_active(plugin_id).await)
    }
}

impl Default for ActivateService {
    fn default() -> Self {
        Self::new(
            Arc::new(PluginRepository::default()),
            Arc::new(LayeredCacheManager::default()),
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::default()),
            Arc::new(ActivationManager::new()),
            Arc::new(ServiceRegistry::new()),
        )
    }
}
