//! 插件变更通知模块
//!
//! 提供基于 Redis Pub/Sub 的轻量级通知机制，
//! 用于多实例间的插件状态同步。
//!
//! # 核心设计
//!
//! - 数据库是唯一的真相来源（Single Source of Truth）
//! - 通知消息只携带 plugin_id + action，不携带业务数据
//! - 收到通知后从数据库读取最新状态，再决定本地操作
//! - 本地操作天然幂等（目录已存在则跳过）

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件变更通知频道名称
pub const PLUGIN_CHANGE_CHANNEL: &str = "cmx:plugin:changed";

/// 插件变更动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginChangeAction {
    /// 插件已安装或版本已变更（安装/升级/降级统一使用此动作）
    Changed,
    /// 插件已卸载
    Removed,
    /// 插件运行时加载
    RuntimeLoad,
    /// 插件运行时卸载
    RuntimeUnload,
}

/// 插件变更通知（极简设计，不携带业务数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginChangeNotification {
    /// 插件ID
    pub plugin_id: String,
    /// 变更动作
    pub action: PluginChangeAction,
    /// 通知时间
    pub timestamp: DateTime<Utc>,
    /// 插件版本（用于运行时加载/卸载通知）
    pub version: Option<String>,
    /// 应用ID（用于通知过滤，仅匹配的实例处理）
    pub app_id: String,
}

/// 插件变更通知器
///
/// 通过 Redis Pub/Sub 发布插件变更通知。
/// 通知只携带 plugin_id 和 action，不携带业务数据。
pub struct PluginNotifier {
    /// Redis Pub/Sub 操作
    pubsub: Arc<cmx_buffer::PubSubOps>,
}

impl PluginNotifier {
    /// 创建新的插件变更通知器
    ///
    /// # 参数
    /// * `pubsub` - Redis Pub/Sub 操作实例
    pub fn new(pubsub: Arc<cmx_buffer::PubSubOps>) -> Self {
        Self { pubsub }
    }

    /// 发布插件变更通知（安装/升级/降级）
    ///
    /// # 参数
    /// * `plugin_id` - 变更的插件ID
    pub async fn notify_changed(&self, plugin_id: &str, version: &str, app_id: &str) {
        let notification = PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action: PluginChangeAction::Changed,
            timestamp: Utc::now(),
            version: Some(version.to_string()),
            app_id: app_id.to_string(),
        };

        match self.pubsub.publish_json(PLUGIN_CHANGE_CHANNEL, &notification).await {
            Ok(subscribers) => {
                tracing::info!(
                    "已发布插件变更通知到redis: {} (订阅者: {})",
                    plugin_id, subscribers
                );
            }
            Err(e) => {
                tracing::error!("发布插件变更通知到redis失败: {} - {}", plugin_id, e);
            }
        }
    }

    /// 发布插件移除通知
    ///
    /// # 参数
    /// * `plugin_id` - 被移除的插件ID
    /// * `app_id` - 应用ID
    pub async fn notify_removed(&self, plugin_id: &str, app_id: &str) {
        let notification = PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action: PluginChangeAction::Removed,
            timestamp: Utc::now(),
            version: None,
            app_id: app_id.to_string(),
        };

        match self.pubsub.publish_json(PLUGIN_CHANGE_CHANNEL, &notification).await {
            Ok(subscribers) => {
                tracing::info!(
                    "已发布插件移除通知: {} (订阅者: {})",
                    plugin_id, subscribers
                );
            }
            Err(e) => {
                tracing::error!("发布插件移除通知失败: {} - {}", plugin_id, e);
            }
        }
    }

    /// 发布插件运行时加载通知
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 加载的插件ID
    /// * `version` - 插件版本
    /// * `app_id` - 目标应用ID
    pub async fn notify_runtime_load(&self, plugin_id: &str, version: &str, app_id: &str) {
        let notification = PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action: PluginChangeAction::RuntimeLoad,
            timestamp: Utc::now(),
            version: Some(version.to_string()),
            app_id: app_id.to_string(),
        };

        match self.pubsub.publish_json(PLUGIN_CHANGE_CHANNEL, &notification).await {
            Ok(subscribers) => {
                tracing::info!(
                    "已发布插件运行时加载通知: {} v{} (订阅者: {})",
                    plugin_id, version, subscribers
                );
            }
            Err(e) => {
                tracing::error!("发布插件运行时加载通知失败: {} - {}", plugin_id, e);
            }
        }
    }

    /// 发布插件运行时卸载通知
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 卸载的插件ID
    /// * `version` - 插件版本
    /// * `app_id` - 目标应用ID
    pub async fn notify_runtime_unload(&self, plugin_id: &str, version: &str, app_id: &str) {
        let notification = PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action: PluginChangeAction::RuntimeUnload,
            timestamp: Utc::now(),
            version: Some(version.to_string()),
            app_id: app_id.to_string(),
        };

        match self.pubsub.publish_json(PLUGIN_CHANGE_CHANNEL, &notification).await {
            Ok(subscribers) => {
                tracing::info!(
                    "已发布插件运行时卸载通知: {} v{} (订阅者: {})",
                    plugin_id, version, subscribers
                );
            }
            Err(e) => {
                tracing::error!("发布插件运行时卸载通知失败: {} - {}", plugin_id, e);
            }
        }
    }
}
