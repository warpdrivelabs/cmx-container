//! 服务注册表模块 - 插件服务注册与管理
//!
//! 提供插件服务的注册、发现和调用功能。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 服务描述符 - 描述一个可调用的服务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    /// 服务 ID
    pub service_id: String,
    /// 服务名称
    pub name: String,
    /// 提供服务的插件 ID
    pub plugin_id: String,
    /// 服务版本
    pub version: String,
    /// 服务描述
    pub description: Option<String>,
    /// 输入参数类型定义
    pub input_schema: Option<serde_json::Value>,
    /// 输出类型定义
    pub output_schema: Option<serde_json::Value>,
    /// 是否需要认证
    pub requires_auth: bool,
    /// 是否已启用
    pub enabled: bool,
}

/// 服务实例 - 运行中的服务实例
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    /// 服务描述符
    pub descriptor: ServiceDescriptor,
    /// 服务句柄（用于调用）
    pub handle: ServiceHandle,
    /// 注册时间
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

/// 服务句柄 - 用于调用服务
#[derive(Debug, Clone)]
pub struct ServiceHandle {
    /// 插件 ID
    pub plugin_id: String,
    /// 服务 ID
    pub service_id: String,
    /// 函数名称
    pub function_name: String,
}

/// 服务调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCallRequest {
    /// 服务 ID
    pub service_id: String,
    /// 调用参数
    pub params: serde_json::Value,
    /// 调用者信息
    pub caller: Option<String>,
    /// 超时时间（毫秒）
    pub timeout_ms: Option<u64>,
}

/// 服务调用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCallResponse {
    /// 是否成功
    pub success: bool,
    /// 返回结果
    pub result: Option<serde_json::Value>,
    /// 错误信息
    pub error: Option<String>,
    /// 执行时间（毫秒）
    pub duration_ms: u64,
}

/// 服务注册表 - 管理插件提供的服务
pub struct ServiceRegistry {
    /// 已注册的服务
    services: Arc<RwLock<HashMap<String, ServiceInstance>>>,
    /// 插件到服务的映射
    plugin_services: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl ServiceRegistry {
    /// 创建新的服务注册表
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            plugin_services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册服务
    pub async fn register(&self, descriptor: ServiceDescriptor, handle: ServiceHandle) -> Result<(), ServiceError> {
        let service_id = descriptor.service_id.clone();
        let plugin_id = descriptor.plugin_id.clone();

        // 检查服务是否已存在
        {
            let services = self.services.read().await;
            if services.contains_key(&service_id) {
                return Err(ServiceError::AlreadyRegistered(service_id));
            }
        }

        // 创建服务实例
        let instance = ServiceInstance {
            descriptor,
            handle,
            registered_at: chrono::Utc::now(),
        };

        // 注册服务
        {
            let mut services = self.services.write().await;
            services.insert(service_id.clone(), instance);
        }

        // 更新插件到服务的映射
        {
            let mut plugin_services = self.plugin_services.write().await;
            plugin_services.entry(plugin_id.clone()).or_default().push(service_id.clone());
        }

        log::info!("服务注册成功: {} (插件: {})", service_id, plugin_id);

        Ok(())
    }

    /// 注销服务
    pub async fn unregister(&self, service_id: &str) -> Result<(), ServiceError> {
        // 获取服务信息
        let plugin_id = {
            let services = self.services.read().await;
            match services.get(service_id) {
                Some(instance) => instance.descriptor.plugin_id.clone(),
                None => return Err(ServiceError::NotFound(service_id.to_string())),
            }
        };

        // 移除服务
        {
            let mut services = self.services.write().await;
            services.remove(service_id);
        }

        // 更新插件到服务的映射
        {
            let mut plugin_services = self.plugin_services.write().await;
            if let Some(services) = plugin_services.get_mut(&plugin_id) {
                services.retain(|s| s != service_id);
                if services.is_empty() {
                    plugin_services.remove(&plugin_id);
                }
            }
        }

        log::info!("服务注销成功: {}", service_id);

        Ok(())
    }

    /// 注销插件的所有服务
    pub async fn unregister_plugin_services(&self, plugin_id: &str) -> Result<usize, ServiceError> {
        let service_ids = {
            let plugin_services = self.plugin_services.read().await;
            plugin_services.get(plugin_id).cloned().unwrap_or_default()
        };

        let count = service_ids.len();

        for service_id in &service_ids {
            self.unregister(service_id).await?;
        }

        Ok(count)
    }

    /// 获取服务描述符
    pub async fn get_service(&self, service_id: &str) -> Option<ServiceDescriptor> {
        let services = self.services.read().await;
        services.get(service_id).map(|i| i.descriptor.clone())
    }

    /// 获取服务的句柄
    pub async fn get_handle(&self, service_id: &str) -> Option<ServiceHandle> {
        let services = self.services.read().await;
        services.get(service_id).map(|i| i.handle.clone())
    }

    /// 列出所有服务
    pub async fn list_services(&self) -> Vec<ServiceDescriptor> {
        let services = self.services.read().await;
        services.values().map(|i| i.descriptor.clone()).collect()
    }

    /// 列出插件的所有服务
    pub async fn list_plugin_services(&self, plugin_id: &str) -> Vec<ServiceDescriptor> {
        let plugin_services = self.plugin_services.read().await;
        let services = self.services.read().await;

        match plugin_services.get(plugin_id) {
            Some(service_ids) => service_ids
                .iter()
                .filter_map(|id| services.get(id).map(|i| i.descriptor.clone()))
                .collect(),
            None => Vec::new(),
        }
    }

    /// 检查服务是否存在
    pub async fn has_service(&self, service_id: &str) -> bool {
        let services = self.services.read().await;
        services.contains_key(service_id)
    }

    /// 启用服务
    pub async fn enable_service(&self, service_id: &str) -> Result<(), ServiceError> {
        let mut services = self.services.write().await;
        match services.get_mut(service_id) {
            Some(instance) => {
                instance.descriptor.enabled = true;
                Ok(())
            }
            None => Err(ServiceError::NotFound(service_id.to_string())),
        }
    }

    /// 禁用服务
    pub async fn disable_service(&self, service_id: &str) -> Result<(), ServiceError> {
        let mut services = self.services.write().await;
        match services.get_mut(service_id) {
            Some(instance) => {
                instance.descriptor.enabled = false;
                Ok(())
            }
            None => Err(ServiceError::NotFound(service_id.to_string())),
        }
    }

    /// 获取服务数量
    pub async fn service_count(&self) -> usize {
        let services = self.services.read().await;
        services.len()
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 服务错误
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("服务已注册: {0}")]
    AlreadyRegistered(String),
    #[error("服务不存在: {0}")]
    NotFound(String),
    #[error("服务调用失败: {0}")]
    CallFailed(String),
    #[error("服务已禁用: {0}")]
    Disabled(String),
    #[error("权限不足: {0}")]
    PermissionDenied(String),
}
