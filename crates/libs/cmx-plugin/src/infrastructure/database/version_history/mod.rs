//! 版本历史数据库模块
//!
//! 提供插件版本历史的增删改查操作

pub mod model;
mod repository;

pub use model::{VersionCreateParams, VersionRecord, VersionUpdateParams};
pub use repository::VersionHistoryRepository;

