//! 服务注册中心
//!
//! 提供服务信息的内存缓存管理。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use cmx_core::model::service::ServiceInfo;

/// 服务注册中心
///
/// 提供服务信息的内存缓存管理，包括：
/// - 服务定义缓存（service_key -> ServiceInfo）
/// - 插件服务映射（plugin_id -> Vec<service_key>）
/// - 编排定义缓存（service_key -> JSON）
#[derive(Clone)]
pub struct ServiceRegistry {
    /// 服务定义缓存（service_key -> ServiceInfo）
    services: Arc<RwLock<HashMap<String, ServiceInfo>>>,
    /// 插件服务映射（plugin_id -> Vec<service_key>）
    plugin_services: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// 编排定义缓存（service_key -> JSON）
    orchestration_cache: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl ServiceRegistry {
    /// 创建服务注册中心
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            plugin_services: Arc::new(RwLock::new(HashMap::new())),
            orchestration_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册服务到内存
    ///
    /// # 参数
    /// * `service` - 服务信息
    /// * `orchestration` - 编排定义（可选）
    pub async fn register(&self, service: ServiceInfo, orchestration: Option<serde_json::Value>) {
        let service_key = service.service_key.clone();
        let plugin_id = service.plugin_id.clone();

        self.services.write().await.insert(service_key.clone(), service);

        if let Some(orch) = orchestration {
            self.orchestration_cache.write().await.insert(service_key.clone(), orch);
        }

        self.plugin_services.write().await
            .entry(plugin_id)
            .or_insert_with(Vec::new)
            .push(service_key);
    }

    /// 从内存移除服务
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `plugin_id` - 所属插件ID
    pub async fn unregister(&self, service_key: &str, plugin_id: &str) {
        self.services.write().await.remove(service_key);
        self.orchestration_cache.write().await.remove(service_key);

        if let Ok(mut map) = self.plugin_services.try_write()
            && let Some(keys) = map.get_mut(plugin_id) {
                keys.retain(|k| k != service_key);
            }
    }

    /// 获取服务信息
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回服务信息的克隆，如果不存在则返回 None
    pub async fn get(&self, service_key: &str) -> Option<ServiceInfo> {
        self.services.read().await.get(service_key).cloned()
    }

    /// 根据插件ID获取所有服务
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    ///
    /// # 返回值
    /// 返回该插件下所有服务信息的列表
    pub async fn get_by_plugin(&self, plugin_id: &str) -> Vec<ServiceInfo> {
        let plugin_services = self.plugin_services.read().await;
        let service_keys = plugin_services.get(plugin_id);

        match service_keys {
            Some(keys) => {
                let services = self.services.read().await;
                keys.iter()
                    .filter_map(|k| services.get(k).cloned())
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// 获取编排定义
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    ///
    /// # 返回值
    /// 返回编排定义的 JSON 值，如果不存在则返回 None
    pub async fn get_orchestration(&self, service_key: &str) -> Option<serde_json::Value> {
        self.orchestration_cache.read().await.get(service_key).cloned()
    }

    /// 获取所有服务键
    ///
    /// # 返回值
    /// 返回所有已注册服务的 service_key 列表
    pub async fn get_all_keys(&self) -> Vec<String> {
        self.services.read().await.keys().cloned().collect()
    }

    /// 从数据库加载所有服务到内存
    ///
    /// # 参数
    /// * `services` - 服务信息列表
    /// * `orchestrations` - 编排定义映射（service_key -> JSON）
    pub async fn load_all(&self, services: Vec<ServiceInfo>, orchestrations: HashMap<String, serde_json::Value>) {
        let mut services_map = self.services.write().await;
        let mut plugin_map = self.plugin_services.write().await;
        let mut orch_map = self.orchestration_cache.write().await;

        services_map.clear();
        plugin_map.clear();
        orch_map.clear();

        for service in services {
            let plugin_id = service.plugin_id.clone();
            let service_key = service.service_key.clone();

            services_map.insert(service_key.clone(), service);

            plugin_map
                .entry(plugin_id)
                .or_insert_with(Vec::new)
                .push(service_key);
        }

        for (key, orch) in orchestrations {
            orch_map.insert(key, orch);
        }
    }

    /// 同步插件关联的服务
    ///
    /// 先移除该插件的旧服务，再添加新服务
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `services` - 新的服务信息列表
    /// * `orchestrations` - 编排定义映射（service_key -> JSON）
    pub async fn sync_plugin_services(&self, plugin_id: &str, services: Vec<ServiceInfo>, orchestrations: HashMap<String, serde_json::Value>) {
        let existing_keys = self.plugin_services.read().await
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();

        for key in existing_keys {
            self.services.write().await.remove(&key);
            self.orchestration_cache.write().await.remove(&key);
        }

        let mut keys = Vec::new();
        for service in services {
            let service_key = service.service_key.clone();
            self.services.write().await.insert(service_key.clone(), service);
            keys.push(service_key.clone());

            if let Some(orch) = orchestrations.get(&service_key) {
                self.orchestration_cache.write().await.insert(service_key, orch.clone());
            }
        }

        self.plugin_services.write().await.insert(plugin_id.to_string(), keys);
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
