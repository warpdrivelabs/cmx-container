//! 服务生命周期监听器
//!
//! 监听插件生命周期事件，同步服务缓存。

use std::sync::Arc;
use cmx_traits::{GlobalEventBus, EventHandler, plugin_events, PluginLifecyclePayload, ServiceQuery};
use crate::registry::ServiceRegistry;
use crate::repository::ServiceRepository;
use tracing::{info, error};

/// 服务生命周期监听器
///
/// 监听插件生命周期事件，自动同步服务定义缓存。
pub struct ServiceLifecycleListener {
    /// 服务查询（用于从缓存或数据库查询服务定义）
    service_query: Arc<dyn ServiceQuery>,
    /// 服务仓储（用于在升级/降级时强制从数据库加载）
    repository: Arc<ServiceRepository>,
    /// 服务注册表（内存缓存）
    service_registry: Arc<ServiceRegistry>,
    /// 应用ID（仅处理匹配的事件）
    app_id: String,
}

impl ServiceLifecycleListener {
    /// 创建监听器
    ///
    /// # 参数
    ///
    /// * `service_query` - 服务查询接口（缓存优先）
    /// * `repository` - 服务仓储（用于升级/降级时强制从数据库加载）
    /// * `service_registry` - 服务注册表（内存缓存）
    pub fn new(
        service_query: Arc<dyn ServiceQuery>,
        repository: Arc<ServiceRepository>,
        service_registry: Arc<ServiceRegistry>,
        app_id: String,
    ) -> Self {
        Self {
            service_query,
            repository,
            service_registry,
            app_id,
        }
    }

    /// 注册到全局事件总线
    ///
    /// 订阅插件安装、升级、卸载事件。
    pub async fn register(&self) {
        // 订阅安装事件
        let query = self.service_query.clone();
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let query = query.clone();
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的安装事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_installed(query, registry, event).await;
                } else {
                    error!("解析插件安装事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::INSTALLED, handler).await;

        // 订阅升级事件
        let repository = self.repository.clone();
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let repository = repository.clone();
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的升级事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_upgraded(repository, registry, event).await;
                } else {
                    error!("解析插件升级事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UPGRADED, handler).await;

        // 订阅卸载事件
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的卸载事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_uninstalled(registry, event).await;
                } else {
                    error!("解析插件卸载事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNINSTALLED, handler).await;

        // 订阅降级事件
        let repository = self.repository.clone();
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let repository = repository.clone();
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的降级事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_downgraded(repository, registry, event).await;
                } else {
                    error!("解析插件降级事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::DOWNGRADED, handler).await;

        // 订阅覆盖安装事件
        let repository = self.repository.clone();
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let repository = repository.clone();
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的覆盖安装事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_reinstalled(repository, registry, event).await;
                } else {
                    error!("解析插件覆盖安装事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::REINSTALLED, handler).await;

        // 订阅运行时加载事件
        let query = self.service_query.clone();
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let query = query.clone();
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的加载事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_loaded(query, registry, event).await;
                } else {
                    error!("解析插件加载事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::LOADED, handler).await;

        // 订阅运行时卸载事件
        let registry = self.service_registry.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let registry = registry.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的卸载事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_unloaded(registry, event).await;
                } else {
                    error!("解析插件卸载事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNLOADED, handler).await;

        info!("服务生命周期监听器已注册 (app_id={}, 订阅: 安装/升级/卸载/降级/覆盖安装/加载/卸载)", self.app_id);
    }

    /// 处理安装事件：从数据库加载服务定义到缓存
    ///
    /// 安装时缓存本来就不存在，所以可以直接使用 service_query 的缓存优先逻辑。
    async fn handle_installed(
        query: Arc<dyn ServiceQuery>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!(
            "处理插件安装事件: {} v{} (app_id={})",
            event.plugin_id, event.version, event.app_id
        );

        match query.get_services_by_plugin(&event.plugin_id).await {
            Ok(services) => {
                let mut orchestrations = std::collections::HashMap::new();
                for service in &services {
                    if let Some(ref config) = service.config
                        && !config.is_empty()
                        && let Ok(orch) = serde_json::from_str::<serde_json::Value>(config) {
                            orchestrations.insert(service.service_key.clone(), orch);
                        }
                }

                let service_count = services.len();
                registry.sync_plugin_services(&event.plugin_id, services, orchestrations).await;
                info!(
                    "插件 {} 服务定义已加载到缓存，共 {} 个服务 (app_id={})",
                    event.plugin_id, service_count, event.app_id
                );
            }
            Err(e) => {
                error!(
                    "加载插件 {} 服务定义失败: {} (app_id={})",
                    event.plugin_id, e, event.app_id
                );
            }
        }
    }

    /// 处理升级事件：强制从数据库加载最新服务定义到缓存
    ///
    /// 升级时必须强制从数据库加载，因为缓存中可能是旧版本的数据。
    async fn handle_upgraded(
        repository: Arc<ServiceRepository>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!(
            "处理插件升级事件: {} {} -> {} (app_id={})",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version,
            event.app_id
        );

        // 先清空该插件在缓存中的数据，确保使用数据库最新数据
        let existing_services = registry.get_by_plugin(&event.plugin_id).await;
        let removed_count = existing_services.len();
        for service in &existing_services {
            registry.unregister(&service.service_key, &event.plugin_id).await;
        }
        if removed_count > 0 {
            info!(
                "插件 {} 升级前已清理 {} 个旧服务缓存 (app_id={})",
                event.plugin_id, removed_count, event.app_id
            );
        }

        // 强制从数据库加载最新服务定义（使用 repository 绕过缓存）
        match repository.get_services_by_plugin(&event.plugin_id, &event.app_id).await {
            Ok(service_defs) => {
                let mut orchestrations = std::collections::HashMap::new();
                let service_defs: Vec<_> = service_defs
                    .into_iter()
                    .map(|def| {
                        if let Some(ref config) = def.config
                            && !config.is_empty()
                                && let Ok(orch) = serde_json::from_str::<serde_json::Value>(config) {
                                    orchestrations.insert(def.service_key.clone(), orch);
                                }
                        def
                    })
                    .collect();

                let service_count = service_defs.len();
                registry.sync_plugin_services(&event.plugin_id, service_defs, orchestrations).await;
                info!(
                    "插件 {} 升级后服务定义已更新到缓存，共 {} 个服务 (app_id={})",
                    event.plugin_id, service_count, event.app_id
                );
            }
            Err(e) => {
                error!(
                    "升级后加载插件 {} 服务定义失败: {} (app_id={})",
                    event.plugin_id, e, event.app_id
                );
            }
        }
    }

    /// 处理卸载事件：清理服务定义缓存
    ///
    /// 注意：数据库层面的服务清理已在 uninstall.rs 中完成，
    /// 这里只负责清理内存缓存。如果缓存为空，说明服务从未被加载到缓存，
    /// 这是正常的，跳过清理即可。
    async fn handle_uninstalled(registry: Arc<ServiceRegistry>, event: PluginLifecyclePayload) {
        info!(
            "处理插件卸载事件: {} v{} (app_id={})",
            event.plugin_id, event.version, event.app_id
        );

        let services = registry.get_by_plugin(&event.plugin_id).await;

        if services.is_empty() {
            info!(
                "插件 {} 的服务缓存为空，跳过缓存清理 (app_id={})",
                event.plugin_id, event.app_id
            );
        } else {
            let count = services.len();
            for service in &services {
                registry.unregister(&service.service_key, &event.plugin_id).await;
            }
            info!(
                "插件 {} 服务定义已从缓存清理，共清理 {} 个服务 (app_id={})",
                event.plugin_id, count, event.app_id
            );
        }
    }

    /// 处理降级事件：强制从数据库加载最新服务定义到缓存
    ///
    /// 降级时必须强制从数据库加载，因为缓存中可能是新版本的数据。
    /// 同时，降级逻辑（downgrade.rs）已经处理了数据库中多余服务的删除。
    async fn handle_downgraded(
        repository: Arc<ServiceRepository>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!(
            "处理插件降级事件: {} {} -> {} (app_id={})",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version,
            event.app_id
        );

        // 先清空该插件在缓存中的数据，确保使用数据库最新数据
        let existing_services = registry.get_by_plugin(&event.plugin_id).await;
        let removed_count = existing_services.len();
        for service in &existing_services {
            registry.unregister(&service.service_key, &event.plugin_id).await;
        }
        if removed_count > 0 {
            info!(
                "插件 {} 降级前已清理 {} 个旧服务缓存 (app_id={})",
                event.plugin_id, removed_count, event.app_id
            );
        }

        // 强制从数据库加载最新服务定义（使用 repository 绕过缓存）
        match repository.get_services_by_plugin(&event.plugin_id, &event.app_id).await {
            Ok(service_defs) => {
                let mut orchestrations = std::collections::HashMap::new();
                let service_defs: Vec<_> = service_defs
                    .into_iter()
                    .map(|def| {
                        if let Some(ref config) = def.config
                            && !config.is_empty()
                                && let Ok(orch) = serde_json::from_str::<serde_json::Value>(config) {
                                    orchestrations.insert(def.service_key.clone(), orch);
                                }
                        def
                    })
                    .collect();

                let service_count = service_defs.len();
                registry.sync_plugin_services(&event.plugin_id, service_defs, orchestrations).await;
                info!(
                    "插件 {} 降级后服务定义已更新到缓存，共 {} 个服务 (app_id={})",
                    event.plugin_id, service_count, event.app_id
                );
            }
            Err(e) => {
                error!(
                    "降级后加载插件 {} 服务定义失败: {} (app_id={})",
                    event.plugin_id, e, event.app_id
                );
            }
        }
    }

    /// 处理覆盖安装事件：清除旧缓存，强制从数据库加载最新服务定义。
    ///
    /// 逻辑与 UPGRADED/DOWNGRADED 相同。
    async fn handle_reinstalled(
        repository: Arc<ServiceRepository>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!(
            "处理插件覆盖安装事件: {} {} -> {} (app_id={})",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version,
            event.app_id
        );

        // 先清空该插件在缓存中的数据
        let existing_services = registry.get_by_plugin(&event.plugin_id).await;
        let removed_count = existing_services.len();
        for service in &existing_services {
            registry.unregister(&service.service_key, &event.plugin_id).await;
        }
        if removed_count > 0 {
            info!(
                "插件 {} 覆盖安装前已清理 {} 个旧服务缓存 (app_id={})",
                event.plugin_id, removed_count, event.app_id
            );
        }

        // 强制从数据库加载最新服务定义
        match repository.get_services_by_plugin(&event.plugin_id, &event.app_id).await {
            Ok(service_defs) => {
                let mut orchestrations = std::collections::HashMap::new();
                let service_defs: Vec<_> = service_defs
                    .into_iter()
                    .map(|def| {
                        if let Some(ref config) = def.config
                            && !config.is_empty()
                                && let Ok(orch) = serde_json::from_str::<serde_json::Value>(config) {
                                    orchestrations.insert(def.service_key.clone(), orch);
                                }
                        def
                    })
                    .collect();

                let service_count = service_defs.len();
                registry.sync_plugin_services(&event.plugin_id, service_defs, orchestrations).await;
                info!(
                    "插件 {} 覆盖安装后服务定义已更新到缓存，共 {} 个服务 (app_id={})",
                    event.plugin_id, service_count, event.app_id
                );
            }
            Err(e) => {
                error!(
                    "覆盖安装后加载插件 {} 服务定义失败: {} (app_id={})",
                    event.plugin_id, e, event.app_id
                );
            }
        }
    }

    /// 处理运行时加载事件：从数据库加载服务定义到缓存。
    ///
    /// 与 INSTALLED 类似，但语义上是"运行时加载"而非"安装"。
    async fn handle_loaded(
        query: Arc<dyn ServiceQuery>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!(
            "处理插件加载事件: {} v{} (app_id={})",
            event.plugin_id, event.version, event.app_id
        );

        match query.get_services_by_plugin(&event.plugin_id).await {
            Ok(services) => {
                let mut orchestrations = std::collections::HashMap::new();
                for service in &services {
                    if let Some(ref config) = service.config
                        && !config.is_empty()
                        && let Ok(orch) = serde_json::from_str::<serde_json::Value>(config) {
                            orchestrations.insert(service.service_key.clone(), orch);
                        }
                }

                let service_count = services.len();
                registry.sync_plugin_services(&event.plugin_id, services, orchestrations).await;
                info!(
                    "插件 {} 服务定义已加载到缓存，共 {} 个服务 (app_id={})",
                    event.plugin_id, service_count, event.app_id
                );
            }
            Err(e) => {
                error!(
                    "加载插件 {} 服务定义失败: {} (app_id={})",
                    event.plugin_id, e, event.app_id
                );
            }
        }
    }

    /// 处理运行时卸载事件：清理服务定义缓存。
    ///
    /// 与 UNINSTALLED 类似，但语义上是"运行时卸载"而非"卸载"。
    async fn handle_unloaded(registry: Arc<ServiceRegistry>, event: PluginLifecyclePayload) {
        info!(
            "处理插件卸载事件: {} v{} (app_id={})",
            event.plugin_id, event.version, event.app_id
        );

        let services = registry.get_by_plugin(&event.plugin_id).await;

        if services.is_empty() {
            info!(
                "插件 {} 的服务缓存为空，跳过缓存清理 (app_id={})",
                event.plugin_id, event.app_id
            );
        } else {
            let count = services.len();
            for service in &services {
                registry.unregister(&service.service_key, &event.plugin_id).await;
            }
            info!(
                "插件 {} 服务定义已从缓存清理，共清理 {} 个服务 (app_id={})",
                event.plugin_id, count, event.app_id
            );
        }
    }
}
