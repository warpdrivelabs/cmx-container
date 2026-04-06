//! 插件生命周期监听 trait 定义
//!
//! 定义插件生命周期事件的监听接口，cmx-service 等模块实现此 trait，
/// 在插件激活/停用/卸载时收到通知，而无需直接依赖 cmx-plugin。

use std::path::PathBuf;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// 生命周期事件载荷
///
/// 在插件生命周期变更时携带的事件数据。
#[derive(Debug, Clone)]
pub struct LifecycleEvent {
    /// 插件ID
    pub plugin_id: String,

    /// 插件版本
    pub version: String,

    /// WASM 文件绝对路径
    pub wasm_path: Option<PathBuf>,

    /// 事件发生时间
    pub timestamp: DateTime<Utc>,
}

impl LifecycleEvent {
    /// 创建新的生命周期事件
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件ID
    /// * `version` - 插件版本
    pub fn new(plugin_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            version: version.into(),
            wasm_path: None,
            timestamp: Utc::now(),
        }
    }

    /// 设置 WASM 文件路径
    pub fn with_wasm_path(mut self, path: PathBuf) -> Self {
        self.wasm_path = Some(path);
        self
    }
}

/// 插件生命周期监听 trait
///
/// cmx-plugin 在插件状态变更时调用此 trait 的方法通知监听者。
/// cmx-service 实现此 trait，在收到通知后加载/卸载 WASM 模块。
///
/// # 注意
///
/// trait 方法不返回 Result，监听者内部自行处理错误并记录日志，
/// 不应阻塞插件生命周期流程。
#[async_trait]
pub trait PluginLifecycleListener: Send + Sync {
    /// 插件已激活 — 通知监听者加载 WASM 模块
    ///
    /// # 参数
    ///
    /// * `event` - 生命周期事件载荷
    async fn on_plugin_activated(&self, event: LifecycleEvent);

    /// 插件已停用 — 通知监听者卸载 WASM 模块
    ///
    /// # 参数
    ///
    /// * `event` - 生命周期事件载荷
    async fn on_plugin_deactivated(&self, event: LifecycleEvent);

    /// 插件已卸载 — 通知监听者清理资源
    ///
    /// # 参数
    ///
    /// * `event` - 生命周期事件载荷
    async fn on_plugin_uninstalled(&self, event: LifecycleEvent);
}
