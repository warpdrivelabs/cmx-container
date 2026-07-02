//! 统一事件发布器模块。
//!
//! 封装 GlobalEventBus 和 Redis PluginNotifier 的发布逻辑，
//! 消除各服务中重复的事件发布代码。
//!
//! # 安全设计
//!
//! 所有 `serde_json::to_value()` 调用使用 `.unwrap_or_default()` 而非 `.unwrap()`，
//! 防止序列化失败时触发 panic，确保事件发布不会中断主流程。

use std::path::PathBuf;
use std::sync::Arc;

use cmx_traits::event_bus::GlobalEventBus;
use cmx_traits::plugin::{PluginLifecyclePayload, plugin_events};

use crate::cluster::notification::PluginNotifier;
use crate::service::persistence::PersistResult;

/// 统一的事件发布器。
///
/// 封装 GlobalEventBus（进程内事件）和 Redis PluginNotifier（跨实例通知）
/// 的发布逻辑，消除各服务中重复的 `if send_event { ... }` 模式。
#[derive(Clone)]
pub struct EventPublisher {
    /// Redis 跨实例通知器。为 `None` 时仅发布进程内事件（单实例部署场景）
    notifier: Option<Arc<PluginNotifier>>,
}

impl EventPublisher {
    /// 创建新的事件发布器。
    ///
    /// # Arguments
    ///
    /// * `notifier` - Redis 跨实例通知器，为 `None` 时仅发布进程内事件
    pub fn new(notifier: Option<Arc<PluginNotifier>>) -> Self {
        Self { notifier }
    }

    /// 发布安装完成事件（进程内 + 跨实例）。
    pub async fn publish_installed(&self, result: &PersistResult) {
        // 1. 构建生命周期负载
        let payload =
            PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
                .with_install_path(result.install_path.clone())
                .with_wasm_path(PathBuf::from(&result.wasm_path));

        // 2. 发布进程内事件
        GlobalEventBus::get()
            .publish(
                plugin_events::INSTALLED,
                serde_json::to_value(&payload).unwrap_or_default(),
            )
            .await;

        // 3. 发布跨实例通知
        if let Some(notifier) = &self.notifier {
            notifier
                .notify_installed(&result.plugin_id, &result.version, &result.app_id)
                .await;
        }
    }

    /// 发布升级完成事件（进程内 + 跨实例）。
    pub async fn publish_upgraded(&self, result: &PersistResult) {
        // 1. 构建生命周期负载
        let payload =
            PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
                .with_old_version(result.old_version.as_deref().unwrap_or("unknown"))
                .with_install_path(result.install_path.clone())
                .with_wasm_path(PathBuf::from(&result.wasm_path));

        // 2. 发布进程内事件
        GlobalEventBus::get()
            .publish(
                plugin_events::UPGRADED,
                serde_json::to_value(&payload).unwrap_or_default(),
            )
            .await;

        // 3. 发布跨实例通知
        if let Some(notifier) = &self.notifier {
            notifier
                .notify_upgraded(&result.plugin_id, &result.version, &result.app_id)
                .await;
        }
    }

    /// 发布降级完成事件（进程内 + 跨实例）。
    pub async fn publish_downgraded(&self, result: &PersistResult) {
        // 1. 构建生命周期负载
        let payload =
            PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
                .with_old_version(result.old_version.as_deref().unwrap_or("unknown"))
                .with_install_path(result.install_path.clone())
                .with_wasm_path(PathBuf::from(&result.wasm_path));

        // 2. 发布进程内事件
        GlobalEventBus::get()
            .publish(
                plugin_events::DOWNGRADED,
                serde_json::to_value(&payload).unwrap_or_default(),
            )
            .await;

        // 3. 发布跨实例通知
        if let Some(notifier) = &self.notifier {
            notifier
                .notify_downgraded(&result.plugin_id, &result.version, &result.app_id)
                .await;
        }
    }

    /// 发布卸载完成事件（进程内 + 跨实例）。
    pub async fn publish_uninstalled(&self, result: &PersistResult) {
        // 1. 构建生命周期负载
        let payload =
            PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
                .with_install_path(result.install_path.clone())
                .with_wasm_path(PathBuf::from(&result.wasm_path));

        // 2. 发布进程内事件
        GlobalEventBus::get()
            .publish(
                plugin_events::UNINSTALLED,
                serde_json::to_value(&payload).unwrap_or_default(),
            )
            .await;

        // 3. 发布跨实例通知
        if let Some(notifier) = &self.notifier {
            notifier
                .notify_removed(&result.plugin_id, &result.version, &result.app_id)
                .await;
        }
    }

    /// 发布覆盖安装完成事件（进程内 + 跨实例）。
    pub async fn publish_reinstalled(&self, result: &PersistResult) {
        // 1. 构建生命周期负载
        let payload =
            PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
                .with_old_version(result.old_version.as_deref().unwrap_or("unknown"))
                .with_install_path(result.install_path.clone());

        // 2. 发布进程内事件
        GlobalEventBus::get()
            .publish(
                plugin_events::REINSTALLED,
                serde_json::to_value(&payload).unwrap_or_default(),
            )
            .await;

        // 3. 发布跨实例通知
        if let Some(notifier) = &self.notifier {
            notifier
                .notify_reinstalled(&result.plugin_id, &result.version, &result.app_id)
                .await;
        }
    }

    /// 仅发布进程内事件（不发送 Redis 通知）。
    ///
    /// 用于其他节点收到 Redis 通知后，在本地发布 GlobalEventBus 事件。
    pub async fn publish_local_event(&self, event: &str, payload: PluginLifecyclePayload) {
        // 发布进程内事件（不发送 Redis 通知，用于其他节点收到 Redis 通知后的本地事件传播）
        GlobalEventBus::get()
            .publish(event, serde_json::to_value(&payload).unwrap_or_default())
            .await;
    }

    // /// 仅发布 Redis 运行时加载通知（管控模式使用）。
    // pub async fn notify_runtime_load(&self, plugin_id: &str, version: &str, app_id: &str) {
    //     // 管控模式使用：仅通知其他节点加载插件到运行时，不发布完整生命周期事件
    //     if let Some(notifier) = &self.notifier {
    //         notifier.notify_runtime_load(plugin_id, version, app_id).await;
    //     }
    // }
    //
    // /// 仅发布 Redis 运行时卸载通知（管控模式使用）。
    // pub async fn notify_runtime_unload(&self, plugin_id: &str, version: &str, app_id: &str) {
    //     // 管控模式使用：仅通知其他节点从运行时卸载插件，不发布完整生命周期事件
    //     if let Some(notifier) = &self.notifier {
    //         notifier.notify_runtime_unload(plugin_id, version, app_id).await;
    //     }
    // }
}
