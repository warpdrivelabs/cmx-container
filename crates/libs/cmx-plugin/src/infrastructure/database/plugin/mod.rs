//! 插件数据库模块
//!
//! 提供插件数据的增删改查操作

pub mod model;
mod repository;

pub use model::{PluginCreateParams, PluginRecord, PluginUpdateParams};
pub use repository::PluginRepository;

// #[deprecated(note = "请使用 PluginRecord 代替")]
// pub type PluginDbRecord = PluginRecord;

// #[deprecated(note = "请使用 PluginUpdateParams 代替")]
// pub type PluginUpdateFields = PluginUpdateParams;
