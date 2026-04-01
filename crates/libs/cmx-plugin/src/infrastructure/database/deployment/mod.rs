//! 部署记录数据库模块
//!
//! 提供插件节点部署记录的增删改查操作

pub mod model;
mod repository;

pub use model::{DeploymentCreateParams, DeploymentRecord, DeploymentUpdateParams};
pub use repository::DeploymentRepository;

// #[deprecated(note = "请使用 DeploymentUpdateParams 代替")]
// pub type DeploymentUpdateFields = DeploymentUpdateParams;
