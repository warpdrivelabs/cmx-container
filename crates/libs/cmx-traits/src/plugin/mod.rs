//! 插件领域 trait 抽象
//!
//! 包含插件状态查询和生命周期事件监听接口。
//!
//! # 模块组织
//!
//! - [`query`] — 插件状态查询 trait（PluginQuery）
//! - [`lifecycle`] — 插件生命周期事件监听 trait（PluginLifecycleListener）

pub mod query;
pub mod lifecycle;

pub use query::{PluginQuery, PluginSnapshot, PluginFilter};
pub use lifecycle::{PluginLifecycleListener, PluginLifecyclePayload, LifecycleEvent, plugin_events};
