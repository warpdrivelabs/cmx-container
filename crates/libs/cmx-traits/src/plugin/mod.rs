//! 插件领域 trait 抽象。
//!
//! 包含插件状态查询、生命周期事件监听和数据导入接口。
//!
//! # 模块组织
//!
//! - [`query`] — 插件状态查询 trait（PluginQuery）。
//! - [`lifecycle`] — 插件生命周期事件监听 trait（PluginLifecycleListener）。
//! - [`data_importer`] — 插件数据导入 trait（PluginDataImporter）。

pub mod data_importer;
pub mod query;
pub mod lifecycle;

pub use data_importer::{
    PluginDataCategory, PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImportResult,
    PluginDataImporter,
};
pub use query::{PluginQuery, PluginSnapshot, PluginFilter};
pub use lifecycle::{PluginLifecycleListener, PluginLifecyclePayload, LifecycleEvent, plugin_events};
