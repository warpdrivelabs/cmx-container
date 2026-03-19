//! 服务注册表模块
//! 
//! 管理插件提供的服务

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 服务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// 服务ID
    pub id: String,
    /// 服务名称
    pub name: String,
    /// 提供者插件ID
    pub provider_plugin_id: String,
    /// 服务类型
    pub service_type: String,
    /// 服务配置
    pub config: Option<serde_json::Value>,
}

/// 服务句柄
#[derive(Debug, Clone)]
pub struct ServiceHandle {
    /// 服务ID
    pub service_id: String,
    /// 提供者插件ID
    pub provider_plugin_id: String,
}

/// 服务注册表
pub struct ServiceRegistry {
    /// 服务映射
    services: Arc<RwLock<HashMap<String, ServiceDefinition>>>,
    /// 插件服务映射
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
    pub async fn register(&self, service: ServiceDefinition) -> Result<(), String> {
        let mut services = self.services.write().await;
        let mut plugin_services = self.plugin_services.write().await;
        
        if services.contains_key(&service.id) {
            return Err(format!("服务 {} 已存在", service.id));
        }
        
        let plugin_id = service.provider_plugin_id.clone();
        let service_id = service.id.clone();
        
        services.insert(service_id.clone(), service);
        
        plugin_services
            .entry(plugin_id)
            .or_insert_with(Vec::new)
            .push(service_id);
        
        Ok(())
    }
    
    /// 注销服务
    pub async fn unregister(&self, service_id: &str) -> Option<ServiceDefinition> {
        let mut services = self.services.write().await;
        let mut plugin_services = self.plugin_services.write().await;
        
        if let Some(service) = services.remove(service_id) {
            if let Some(service_ids) = plugin_services.get_mut(&service.provider_plugin_id) {
                service_ids.retain(|id| id != service_id);
            }
            Some(service)
        } else {
            None
        }
    }
    
    /// 获取服务
    pub async fn get(&self, service_id: &str) -> Option<ServiceDefinition> {
        let services = self.services.read().await;
        services.get(service_id).cloned()
    }
    
    /// 获取插件提供的所有服务
    pub async fn get_plugin_services(&self, plugin_id: &str) -> Vec<ServiceDefinition> {
        let services = self.services.read().await;
        let plugin_services = self.plugin_services.read().await;
        
        plugin_services
            .get(plugin_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| services.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// 注销插件的所有服务
    pub async fn unregister_plugin_services(&self, plugin_id: &str) {
        let mut services = self.services.write().await;
        let mut plugin_services = self.plugin_services.write().await;
        
        if let Some(service_ids) = plugin_services.remove(plugin_id) {
            for service_id in service_ids {
                services.remove(&service_id);
            }
        }
    }
    
    /// 获取所有服务
    pub async fn get_all(&self) -> Vec<ServiceDefinition> {
        let services = self.services.read().await;
        services.values().cloned().collect()
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
