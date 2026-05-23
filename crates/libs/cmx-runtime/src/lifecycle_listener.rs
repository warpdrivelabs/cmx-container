/*
 * @Author: yqs
 * @Date: 2026-04-11 07:59:57
 * @Describe: 
 * @LastEditors: yqs
 * @LastEditTime: 2026-04-11 09:21:41
 */
//! 运行时生命周期监听器
//!
//! 监听插件升级、卸载、降级事件，清除 WASM 实例缓存。

use std::sync::Arc;
use cmx_traits::{GlobalEventBus, EventHandler, plugin_events, PluginLifecyclePayload, RuntimeInvoker};
use tracing::{info, warn};

/// 运行时生命周期监听器
///
/// 监听插件生命周期事件，在插件升级/卸载/降级时清除 WASM 实例缓存。
pub struct RuntimeLifecycleListener {
    /// 运行时调用器
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    /// 应用ID（仅处理匹配的事件）
    app_id: String,
}

impl RuntimeLifecycleListener {
    /// 创建监听器。
    ///
    /// # Arguments
    ///
    /// * `runtime_invoker` - 运行时调用器接口
    /// * `app_id` - 当前应用ID，用于过滤非本应用的事件
    pub fn new(runtime_invoker: Arc<dyn RuntimeInvoker>, app_id: String) -> Self {
        Self { runtime_invoker, app_id }
    }

    /// 注册到全局事件总线。
    ///
    /// 订阅插件升级、卸载、降级事件。
    pub async fn register(&self) {
        // 订阅升级事件
        let invoker = self.runtime_invoker.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
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
                    Self::handle_upgraded(invoker, event).await;
                } else {
                    warn!("解析插件升级事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UPGRADED, handler).await;

        // 订阅卸载事件
        let invoker = self.runtime_invoker.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
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
                    Self::handle_uninstalled(invoker, event).await;
                } else {
                    warn!("解析插件卸载事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNINSTALLED, handler).await;

        // 订阅降级事件
        let invoker = self.runtime_invoker.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
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
                    Self::handle_downgraded(invoker, event).await;
                } else {
                    warn!("解析插件降级事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::DOWNGRADED, handler).await;

        // 订阅覆盖安装事件
        let invoker = self.runtime_invoker.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
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
                    Self::handle_reinstalled(invoker, event).await;
                } else {
                    warn!("解析插件覆盖安装事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::REINSTALLED, handler).await;

        // 订阅运行时卸载事件
        let invoker = self.runtime_invoker.clone();
        let app_id = self.app_id.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
            let app_id = app_id.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    if event.app_id != app_id {
                        tracing::debug!(
                            "忽略不同应用的运行时卸载事件: app_id={} (当前应用: {})",
                            event.app_id, app_id
                        );
                        return;
                    }
                    Self::handle_unloaded(invoker, event).await;
                } else {
                    warn!("解析插件运行时卸载事件载荷失败");
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNLOADED, handler).await;

        info!("运行时生命周期监听器已注册 (app_id={}, 订阅: 升级/卸载/降级/覆盖安装/运行时卸载)", self.app_id);
    }

    /// 处理升级事件：清除 WASM 实例缓存。
    async fn handle_upgraded(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!(
            "处理插件升级事件，清除 WASM 缓存: {} {} -> {} (app_id={})",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version,
            event.app_id
        );

        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存 (app_id={})", event.plugin_id, event.app_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {} (app_id={})", event.plugin_id, e, event.app_id),
        }
    }

    /// 处理卸载事件：清除 WASM 实例缓存。
    async fn handle_uninstalled(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!(
            "处理插件卸载事件，清除 WASM 缓存: {} v{} (app_id={})",
            event.plugin_id, event.version, event.app_id
        );

        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存 (app_id={})", event.plugin_id, event.app_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {} (app_id={})", event.plugin_id, e, event.app_id),
        }
    }

    /// 处理降级事件：清除 WASM 实例缓存。
    async fn handle_downgraded(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!(
            "处理插件降级事件，清除 WASM 缓存: {} {} -> {} (app_id={})",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version,
            event.app_id
        );

        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存 (app_id={})", event.plugin_id, event.app_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {} (app_id={})", event.plugin_id, e, event.app_id),
        }
    }

    /// 处理覆盖安装事件：清除 WASM 实例缓存。
    async fn handle_reinstalled(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!(
            "处理插件覆盖安装事件，清除 WASM 缓存: {} {} -> {} (app_id={})",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version,
            event.app_id
        );

        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存 (app_id={})", event.plugin_id, event.app_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {} (app_id={})", event.plugin_id, e, event.app_id),
        }
    }

    /// 处理运行时卸载事件：清除 WASM 实例缓存。
    async fn handle_unloaded(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!(
            "处理插件运行时卸载事件，清除 WASM 缓存: {} v{} (app_id={})",
            event.plugin_id, event.version, event.app_id
        );

        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存 (app_id={})", event.plugin_id, event.app_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {} (app_id={})", event.plugin_id, e, event.app_id),
        }
    }
}
