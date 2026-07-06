//! 插件领域 trait 抽象。
//!
//! 包含插件状态查询和生命周期事件监听。
//!
//! # 模块组织
//!
//! - [`query`] — 插件状态查询 trait（PluginQuery）。
//! - [`lifecycle`] — 插件生命周期事件监听 trait（PluginLifecycleListener）。

pub mod lifecycle;
pub mod query;

pub use lifecycle::{
    LifecycleEvent, PluginLifecycleListener, PluginLifecyclePayload, plugin_events,
};
pub use query::{PluginFilter, PluginQuery, PluginSnapshot};
