//! 插件变更通知处理器模块
//!
//! 处理从 Redis Pub/Sub 接收到的插件变更通知，
//! 根据数据库最新状态执行本地运行时同步操作。
//!
//! # 设计原则
//!
//! - **单一写入原则**：本处理器只做运行时同步（下载文件 + 内存注册/卸载），
//!   不操作数据库，数据库操作由接收 API 请求的节点完成。
//! - 所有操作天然幂等（已注册则跳过、已卸载则忽略）
//! - 不需要分布式锁或请求去重

use std::path::PathBuf;
use std::sync::Arc;

use cmx_traits::plugin::{PluginLifecyclePayload, plugin_events};
use crate::cluster::notification::{PluginChangeAction, PluginChangeNotification};
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::event_publisher::EventPublisher;
use crate::service::runtime_ops::RuntimeOps;

/// 插件变更通知处理器
///
/// 接收 Redis Pub/Sub 的插件变更通知，
/// 从数据库查询最新状态后执行本地运行时同步操作。
///
/// # 与旧实现的区别
///
/// 旧实现通过 `DeployService` 间接操作数据库（违反单一写入原则），
/// 新实现通过 `RuntimeOps` 仅做运行时同步（下载文件 + 内存注册/卸载），
/// 不再操作数据库。
///
/// # app_id 隔离
///
/// 所有操作按 `app_id` 过滤，仅处理当前应用的插件通知，
/// 忽略其他应用的通知，确保多应用部署时的隔离性。
#[derive(Clone)]
pub struct PluginChangeHandler {
    /// 数据库仓库（只读查询）
    repository: Arc<PluginRepository>,
    /// 运行时操作层（内存注册/卸载、缓存更新、文件同步）
    runtime: Arc<RuntimeOps>,
    /// 统一事件发布器（仅发布进程内事件）
    event_publisher: EventPublisher,
    /// 插件根目录
    #[allow(dead_code)]
    plugin_root: PathBuf,
    /// 当前应用ID，用于过滤非本应用的通知
    app_id: String,
    /// 当前实例ID，用于跳过自己发出的通知
    instance_id: String,
}

impl PluginChangeHandler {
    /// 创建新的插件变更处理器
    pub fn new(
        repository: Arc<PluginRepository>,
        runtime: Arc<RuntimeOps>,
        event_publisher: EventPublisher,
        plugin_root: PathBuf,
        app_id: String,
        instance_id: String,
    ) -> Self {
        Self {
            repository,
            runtime,
            event_publisher,
            plugin_root,
            app_id,
            instance_id,
        }
    }

    /// 处理插件变更通知
    pub async fn handle(&self, notification: &PluginChangeNotification) {
        // 1. 跳过自身发出的通知
        if notification.instance_id == self.instance_id {
            tracing::info!(
                "跳过处理自身发出的redis通知: {} {:?} (instance_id={})",
                notification.plugin_id, notification.action, notification.instance_id
            );
            return;
        }
        tracing::debug!(
            "收到插件变更redis通知: {} {:?} (instance_id={})",
            notification.plugin_id, notification.action, notification.instance_id
        );

        // 2. 过滤非本应用的通知
        if notification.app_id != self.app_id {
            tracing::debug!(
                "Ignoring notification for different app_id: {}",
                notification.app_id
            );
            return;
        }

        // 3. 分发到具体处理器
        match &notification.action {
            PluginChangeAction::Installed
            | PluginChangeAction::Upgraded
            | PluginChangeAction::Downgraded => {
                self.handle_plugin_changed(notification).await;
            }
            PluginChangeAction::Reinstalled => {
                self.handle_plugin_reinstalled(notification).await;
            }
            PluginChangeAction::Removed => {
                self.handle_plugin_removed(notification).await;
            }
            // PluginChangeAction::RuntimeLoad => {
            //     self.handle_runtime_load(notification).await;
            // }
            // PluginChangeAction::RuntimeUnload => {
            //     self.handle_runtime_unload(notification).await;
            // }
        }
    }

    /// 处理插件变更（安装/升级/降级）。
    ///
    /// 从数据库查询最新版本，同步文件并注册到内存。
    /// 不操作数据库，仅做运行时同步。
    async fn handle_plugin_changed(&self, notification: &PluginChangeNotification) {
        let plugin_id = &notification.plugin_id;
        // 1. 查询数据库获取最新版本
        let db_plugin = match self.repository.find_plugin(plugin_id, &self.app_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                tracing::warn!(
                    "收到插件 {} 变更通知，但数据库中未找到记录 (app_id={})",
                    plugin_id,
                    self.app_id
                );
                return;
            }
            Err(e) => {
                tracing::error!("查询插件 {} 失败: {}", plugin_id, e);
                return;
            }
        };

        // 2. 同步文件并注册到内存（幂等操作）
        if let Err(e) = self.runtime.sync_and_register(plugin_id, &db_plugin.version).await {
            tracing::error!("插件 {} 运行时同步失败: {}", plugin_id, e);
            return;
        }

        tracing::info!(
            "插件 {} 运行时同步完成: version={}",
            plugin_id,
            db_plugin.version
        );

        // 3. 发布进程内事件
        let event_name = match notification.action {
            PluginChangeAction::Installed => plugin_events::INSTALLED,
            PluginChangeAction::Upgraded => plugin_events::UPGRADED,
            PluginChangeAction::Downgraded => plugin_events::DOWNGRADED,
            _ => return,
        };

        let payload = PluginLifecyclePayload::new(
            &self.app_id,
            plugin_id,
            &db_plugin.version,
        );

        self.event_publisher.publish_local_event(event_name, payload).await;
    }

    /// 处理插件覆盖安装。
    ///
    /// 与 handle_plugin_changed 不同，此方法**不检查本地路径是否存在**，
    /// 强制重新同步，因为发送方已经删掉旧文件重新安装了。
    async fn handle_plugin_reinstalled(&self, notification: &PluginChangeNotification) {
        let plugin_id = &notification.plugin_id;
        // 1. 查询数据库获取最新版本
        let db_plugin = match self.repository.find_plugin(plugin_id, &self.app_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                tracing::warn!(
                    "收到插件 {} 覆盖安装通知，但数据库中未找到记录 (app_id={})",
                    plugin_id,
                    self.app_id
                );
                return;
            }
            Err(e) => {
                tracing::error!("查询插件 {} 失败: {}", plugin_id, e);
                return;
            }
        };

        tracing::info!(
            "收到插件 {} 覆盖安装通知，强制重新同步 (app_id={})",
            plugin_id,
            self.app_id
        );

        // 2. 强制重新同步并注册（不检查本地路径）
        if let Err(e) = self.runtime.force_resync_and_register(plugin_id, &db_plugin.version).await {
            tracing::error!("插件 {} 覆盖安装运行时同步失败: {}", plugin_id, e);
            return;
        }

        tracing::info!(
            "插件 {} 覆盖安装运行时同步完成: version={}",
            plugin_id,
            db_plugin.version
        );

        // 3. 发布进程内事件
        let payload = PluginLifecyclePayload::new(
            &self.app_id,
            plugin_id,
            &db_plugin.version,
        )
            .with_old_version(notification.version.as_str());

        self.event_publisher.publish_local_event(plugin_events::REINSTALLED, payload).await;
    }

    /// 处理插件移除。
    ///
    /// 从内存中注销插件并清理本地文件。不操作数据库。
    async fn handle_plugin_removed(&self, notification: &PluginChangeNotification) {
        let plugin_id = &notification.plugin_id;

        // 1. 从内存注销并清理本地文件
        if let Err(e) = self.runtime.unregister_and_cleanup(plugin_id).await {
            tracing::error!("插件 {} 运行时卸载失败: {}", plugin_id, e);
        }

        // 2. 发布进程内事件
        let payload = PluginLifecyclePayload::new(&self.app_id, plugin_id, &notification.version);
        self.event_publisher.publish_local_event(plugin_events::UNINSTALLED, payload).await;
    }

    // /// 处理运行时加载。
    // ///
    // /// 从数据库查询插件信息，下载文件（如需）并注册到内存。
    // async fn handle_runtime_load(&self, notification: &PluginChangeNotification) {
    //     let plugin_id = &notification.plugin_id;
    //     let version = &notification.version;
    //
    //     tracing::info!(
    //         "收到 RuntimeLoad 通知: plugin={}, version={}",
    //         plugin_id,
    //         version
    //     );
    //
    //     // 1. 从数据库查询并注册到内存（幂等操作）
    //     if let Err(e) = self.runtime.sync_and_register(plugin_id, version).await {
    //         tracing::error!("RuntimeLoad 失败: plugin={}, error={}", plugin_id, e);
    //         return;
    //     }
    //
    //     // 2. 发布进程内事件
    //     let payload = PluginLifecyclePayload::new(&self.app_id, plugin_id, version);
    //     self.event_publisher.publish_local_event(plugin_events::LOADED, payload).await;
    // }
    //
    // /// 处理运行时卸载。
    // ///
    // /// 从内存中注销插件。不操作数据库。
    // async fn handle_runtime_unload(&self, notification: &PluginChangeNotification) {
    //     let plugin_id = &notification.plugin_id;
    //     let version = &notification.version;
    //
    //     tracing::info!(
    //         "收到 RuntimeUnload 通知: plugin={}",
    //         plugin_id
    //     );
    //
    //     // 1. 从内存注销（幂等操作）
    //     if let Err(e) = self.runtime.unregister_plugin(plugin_id).await {
    //         tracing::error!("RuntimeUnload 失败: plugin={}, error={}", plugin_id, e);
    //         return;
    //     }
    //
    //     // 2. 发布进程内事件
    //     let payload = PluginLifecyclePayload::new(&self.app_id, plugin_id, version);
    //     self.event_publisher.publish_local_event(plugin_events::UNLOADED, payload).await;
    // }

    // /// 全量同步（启动时或收到全量同步请求时调用）
    // ///
    // /// 对比数据库中所有插件与本地文件系统状态，执行差异同步。
    // /// 仅做运行时同步（下载文件 + 内存注册/卸载），不操作数据库。
    // pub async fn full_sync(&self) -> PluginResult<SyncResult> {
    //     let mut result = SyncResult::default();
    //
    //     // 1. 查询数据库中所有期望的插件（按当前 app_id 过滤，避免处理其他应用的插件）
    //     let filter = crate::domain::plugin::PluginFilter {
    //         app_id: Some(self.app_id.clone()),
    //         ..Default::default()
    //     };
    //     let expected_plugins = self.repository.list_plugins(&filter).await?;
    //     let expected_map: HashMap<String, String> = expected_plugins
    //         .iter()
    //         .map(|p| (p.plugin_id.clone(), p.version.clone()))
    //         .collect();
    //
    //     // 2. 扫描本地文件系统
    //     let local_plugins = scan_local_plugins(&self.plugin_root, &self.app_id).await?;
    //
    //     // 3. 同步缺失或版本不一致的插件
    //     for (plugin_id, version) in &expected_map {
    //         let local_version = local_plugins.get(plugin_id);
    //
    //         match local_version {
    //             Some(ver) if ver == version => {
    //                 result.synced.push(plugin_id.clone());
    //             }
    //             Some(_) | None => {
    //                 // 本地版本不一致或不存在，使用 RuntimeOps 同步
    //                 match self.runtime.sync_and_register(plugin_id, version).await {
    //                     Ok(_) => {
    //                         result.synced.push(plugin_id.clone());
    //                     }
    //                     Err(e) => {
    //                         tracing::error!("插件 {} 全量同步失败: {}", plugin_id, e);
    //                         result.failed.push((plugin_id.clone(), e.to_string()));
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //
    //     // 4. 清理本地存在但数据库不存在的插件
    //     for plugin_id in local_plugins.keys() {
    //         if !expected_map.contains_key(plugin_id) {
    //             if let Err(e) = self.runtime.unregister_and_cleanup(plugin_id).await {
    //                 tracing::error!("清理插件 {} 运行时状态失败: {}", plugin_id, e);
    //             }
    //             result.cleaned.push(plugin_id.clone());
    //         }
    //     }
    //
    //     Ok(result)
    // }
}

// /// 同步结果
// #[derive(Debug, Clone, Default)]
// pub struct SyncResult {
//     /// 已同步的插件列表
//     pub synced: Vec<String>,
//     /// 清理的插件列表
//     pub cleaned: Vec<String>,
//     /// 失败的插件及错误信息
//     pub failed: Vec<(String, String)>,
// }
