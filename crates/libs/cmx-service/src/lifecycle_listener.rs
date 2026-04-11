//! 服务生命周期监听器
//!
//! 监听插件生命周期事件，同步服务缓存。

use std::sync::Arc;
use cmx_traits::{GlobalEventBus, EventHandler, plugin_events, PluginLifecyclePayload, ServiceQuery};
use crate::registry::ServiceRegistry;
use tracing::{info, error};

/// 服务生命周期监听器
///
/// 监听插件生命周期事件，自动同步服务定义缓存。
pub struct ServiceLifecycleListener {
    /// 服务查询（用于从数据库加载服务定义）
    service_query: Arc<dyn ServiceQuery>,
    /// 服务注册表（内存缓存）
    service_registry: Arc<ServiceRegistry>,
}

impl ServiceLifecycleListener {
    /// 创建监听器
    ///
    /// # 参数
    ///
    /// * `service_query` - 服务查询接口
    /// * `service_registry` - 服务注册表（内存缓存）
    pub fn new(
        service_query: Arc<dyn ServiceQuery>,
        service_registry: Arc<ServiceRegistry>,
    ) -> Self {
        Self {
            service_query,
            service_registry,
        }
    }

    /// 注册到全局事件总线
    ///
    /// 订阅插件安装、升级、卸载事件。
    pub async fn register(&self) {
        // 订阅安装事件
        let query = self.service_query.clone();
        let registry = self.service_registry.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let query = query.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_installed(query, registry, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::INSTALLED, handler).await;

        // 订阅升级事件
        let query = self.service_query.clone();
        let registry = self.service_registry.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let query = query.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_upgraded(query, registry, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UPGRADED, handler).await;

        // 订阅卸载事件
        let registry = self.service_registry.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let registry = registry.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_uninstalled(registry, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNINSTALLED, handler).await;

        info!("服务生命周期监听器已注册");
    }

    /// 处理安装事件：从数据库加载服务定义到缓存
    async fn handle_installed(
        query: Arc<dyn ServiceQuery>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!("处理插件安装事件: {} v{}", event.plugin_id, event.version);
        
        match query.get_services_by_plugin(&event.plugin_id).await {
            Ok(services) => {
                let mut orchestrations = std::collections::HashMap::new();
                for service in &services {
                    if !service.config.is_empty() {
                        if let Ok(orch) = serde_json::from_str::<serde_json::Value>(&service.config) {
                            orchestrations.insert(service.service_key.clone(), orch);
                        }
                    }
                }
                
                registry.sync_plugin_services(&event.plugin_id, services, orchestrations).await;
                info!("插件 {} 服务定义已加载到缓存", event.plugin_id);
            }
            Err(e) => {
                error!("加载插件 {} 服务定义失败: {}", event.plugin_id, e);
            }
        }
    }

    /// 处理升级事件：更新服务定义缓存
    async fn handle_upgraded(
        query: Arc<dyn ServiceQuery>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!("处理插件升级事件: {} {} -> {}", event.plugin_id, 
            event.old_version.as_deref().unwrap_or("?"), event.version);
        
        // 升级时重新加载服务定义（逻辑与安装相同）
        Self::handle_installed(query, registry, event).await;
    }

    /// 处理卸载事件：清理服务定义缓存
    async fn handle_uninstalled(registry: Arc<ServiceRegistry>, event: PluginLifecyclePayload) {
        info!("处理插件卸载事件: {} v{}", event.plugin_id, event.version);
        
        // 获取该插件的所有服务键
        let services = registry.get_by_plugin(&event.plugin_id).await;
        
        // 从缓存中移除
        for service in services {
            registry.unregister(&service.service_key, &event.plugin_id).await;
        }
        
        info!("插件 {} 服务定义已从缓存清理", event.plugin_id);
    }
}
