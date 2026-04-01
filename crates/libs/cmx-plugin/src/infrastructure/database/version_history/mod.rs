//! 版本历史数据库模块
//!
//! 提供插件版本历史的增删改查操作

pub mod model;
mod repository;

pub use model::{VersionCreateParams, VersionRecord, VersionUpdateParams};
pub use repository::VersionHistoryRepository;

#[deprecated(note = "请使用 VersionRecord 代替")]
pub type VersionHistoryRecord = VersionRecord;

#[deprecated(note = "请使用 VersionUpdateParams 代替")]
pub type VersionHistoryUpdateFields = VersionUpdateParams;
