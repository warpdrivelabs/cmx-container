//! 插件生命周期事件定义
//!
//! 定义插件生命周期事件的主题常量和载荷结构。

use std::path::PathBuf;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ==================== 事件主题常量 ====================

/// 插件生命周期事件主题
pub mod plugin_events {
    /// 插件已安装
    pub const INSTALLED: &str = "plugin.installed";
    /// 插件已升级
    pub const UPGRADED: &str = "plugin.upgraded";
    /// 插件已卸载
    pub const UNINSTALLED: &str = "plugin.uninstalled";
    
    // 暂时注释，暂无此功能
    // /// 插件已激活
    // pub const ACTIVATED: &str = "plugin.activated";
    // /// 插件已停用
    // pub const DEACTIVATED: &str = "plugin.deactivated";
}

// ==================== 事件载荷 ====================

/// 插件生命周期事件载荷
///
/// 在插件生命周期变更时携带的事件数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLifecyclePayload {
    /// 插件ID
    pub plugin_id: String,
    /// 当前版本
    pub version: String,
    /// 旧版本（仅升级事件）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// WASM 文件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_path: Option<String>,
    /// 安装路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    /// 事件时间戳
    pub timestamp: DateTime<Utc>,
}

impl PluginLifecyclePayload {
    /// 创建新的生命周期事件载荷
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件ID
    /// * `version` - 插件版本
    pub fn new(plugin_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            version: version.into(),
            old_version: None,
            wasm_path: None,
            install_path: None,
            timestamp: Utc::now(),
        }
    }

    /// 设置旧版本（用于升级事件）
    pub fn with_old_version(mut self, old_version: impl Into<String>) -> Self {
        self.old_version = Some(old_version.into());
        self
    }

    /// 设置 WASM 文件路径
    pub fn with_wasm_path(mut self, path: PathBuf) -> Self {
        self.wasm_path = Some(path.to_string_lossy().to_string());
        self
    }

    /// 设置安装路径
    pub fn with_install_path(mut self, path: PathBuf) -> Self {
        self.install_path = Some(path.to_string_lossy().to_string());
        self
    }
}

// ==================== 生命周期监听器 Trait ====================

/// 插件生命周期监听器 trait
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
    /// 插件已安装 — 通知监听者加载服务定义
    async fn on_plugin_installed(&self, event: PluginLifecyclePayload);

    /// 插件已升级 — 通知监听者更新服务定义
    async fn on_plugin_upgraded(&self, event: PluginLifecyclePayload);

    /// 插件已卸载 — 通知监听者清理资源
    async fn on_plugin_uninstalled(&self, event: PluginLifecyclePayload);

    // 暂时注释，暂无此功能
    // /// 插件已激活 — 通知监听者加载 WASM 模块
    // async fn on_plugin_activated(&self, event: PluginLifecyclePayload);
    // 
    // /// 插件已停用 — 通知监听者卸载 WASM 模块
    // async fn on_plugin_deactivated(&self, event: PluginLifecyclePayload);
}

// ==================== 向后兼容 ====================

/// 生命周期事件载荷（向后兼容别名）
pub type LifecycleEvent = PluginLifecyclePayload;
