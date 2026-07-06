//! IAM 跨 crate 抽象层。
//!
//! 定义权限校验和数据权限相关的 trait，供 cmx-iam 实现和其他 crate 消费。

pub mod data_scope;
pub mod permission_checker;

pub use data_scope::DataScope;
pub use permission_checker::PermissionChecker;
