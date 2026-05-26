//! 插件变更通知模块
//!
//! 提供基于 Redis Pub/Sub 的轻量级通知机制，
//! 用于多实例间的插件状态同步。
//!
//! # 核心设计
//!
//! - 数据库是唯一的真相来源（Single Source of Truth）
//! - 通知消息携带 plugin_id + action + version + app_id + instance_id，不携带业务数据
//! - 收到通知后从数据库读取最新状态，再决定本地操作
//! - 本地操作天然幂等（目录已存在则跳过）
//! - 每条通知携带 instance_id，接收方可以跳过自己发出的通知，避免重复处理
//!
//! # 通知分类
//!
//! 通知分为**持久化变更**和**运行时变更**两类：
//!
//! - **持久化变更**（文件/DB 发生变更）：其他实例需全量同步文件和 DB
//! - **运行时变更**（仅内存状态变更）：其他实例只需加载/卸载运行时

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件变更通知频道名称
pub const PLUGIN_CHANGE_CHANNEL: &str = "cmx:plugin:changed";

/// 插件变更动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginChangeAction {
    // === 持久化变更（文件/DB 发生变更，其他实例需全量同步） ===
    /// 插件首次安装
    Installed,
    /// 插件升级
    Upgraded,
    /// 插件降级
    Downgraded,
    /// 插件覆盖安装（先卸载再安装，原子操作）
    Reinstalled,
    /// 插件卸载
    Removed,

    // // === 运行时变更（仅内存状态变更，其他实例只需加载/卸载运行时） ===
    // /// 插件运行时加载
    // RuntimeLoad,
    // /// 插件运行时卸载
    // RuntimeUnload,
}

/// 插件变更通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginChangeNotification {
    /// 插件ID
    pub plugin_id: String,
    /// 变更动作
    pub action: PluginChangeAction,
    /// 通知时间
    pub timestamp: DateTime<Utc>,
    /// 插件版本
    pub version: String,
    /// 应用ID（用于通知过滤，仅匹配的实例处理）
    pub app_id: String,
    /// 发送方实例ID（用于接收方跳过自己发出的通知）
    pub instance_id: String,
}

/// 插件变更通知器
///
/// 通过 Redis Pub/Sub 发布插件变更通知。
/// 通知携带 plugin_id、action、version、app_id 和 instance_id，不携带业务数据。
/// instance_id 在创建时注入，所有发出的通知自动携带，接收方可据此跳过自己的通知。
pub struct PluginNotifier {
    /// Redis Pub/Sub 操作
    pubsub: Arc<cmx_buffer::PubSubOps>,
    /// 当前实例ID
    instance_id: String,
}

impl PluginNotifier {
    /// 创建新的插件变更通知器
    ///
    /// # 参数
    /// * `pubsub` - Redis Pub/Sub 操作实例
    /// * `instance_id` - 当前实例的唯一标识，用于接收方跳过自己的通知
    pub fn new(pubsub: Arc<cmx_buffer::PubSubOps>, instance_id: String) -> Self {
        Self { pubsub, instance_id }
    }

    /// 获取当前实例ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// 发布通知的内部方法
    async fn publish(&self, notification: PluginChangeNotification) {
        match self.pubsub.publish_json(PLUGIN_CHANGE_CHANNEL, &notification).await {
            Ok(subscribers) => {
                tracing::info!(
                    "已发布插件通知到redis: {} {:?} v{} (订阅者: {})",
                    notification.plugin_id, notification.action, notification.version, subscribers
                );
            }
            Err(e) => {
                tracing::error!(
                    "发布插件通知到redis失败: {} {:?} - {}",
                    notification.plugin_id, notification.action, e
                );
            }
        }
    }

    /// 构造通知的辅助方法
    fn build_notification(&self, plugin_id: &str, action: PluginChangeAction, version: &str, app_id: &str) -> PluginChangeNotification {
        PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action,
            timestamp: Utc::now(),
            version: version.to_string(),
            app_id: app_id.to_string(),
            instance_id: self.instance_id.clone(),
        }
    }

    // === 持久化变更通知 ===

    /// 发布插件安装通知
    pub async fn notify_installed(&self, plugin_id: &str, version: &str, app_id: &str) {
        self.publish(self.build_notification(plugin_id, PluginChangeAction::Installed, version, app_id)).await;
    }

    /// 发布插件升级通知
    pub async fn notify_upgraded(&self, plugin_id: &str, version: &str, app_id: &str) {
        self.publish(self.build_notification(plugin_id, PluginChangeAction::Upgraded, version, app_id)).await;
    }

    /// 发布插件降级通知
    pub async fn notify_downgraded(&self, plugin_id: &str, version: &str, app_id: &str) {
        self.publish(self.build_notification(plugin_id, PluginChangeAction::Downgraded, version, app_id)).await;
    }

    /// 发布插件覆盖安装通知
    pub async fn notify_reinstalled(&self, plugin_id: &str, version: &str, app_id: &str) {
        self.publish(self.build_notification(plugin_id, PluginChangeAction::Reinstalled, version, app_id)).await;
    }

    /// 发布插件卸载通知
    pub async fn notify_removed(&self, plugin_id: &str, version: &str, app_id: &str) {
        self.publish(self.build_notification(plugin_id, PluginChangeAction::Removed, version, app_id)).await;
    }

    // // === 运行时变更通知 ===
    //
    // /// 发布插件运行时加载通知
    // pub async fn notify_runtime_load(&self, plugin_id: &str, version: &str, app_id: &str) {
    //     self.publish(self.build_notification(plugin_id, PluginChangeAction::RuntimeLoad, version, app_id)).await;
    // }
    //
    // /// 发布插件运行时卸载通知
    // pub async fn notify_runtime_unload(&self, plugin_id: &str, version: &str, app_id: &str) {
    //     self.publish(self.build_notification(plugin_id, PluginChangeAction::RuntimeUnload, version, app_id)).await;
    // }
}
