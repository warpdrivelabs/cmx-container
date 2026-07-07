//! 数据库迁移模块
//!
//! 提供数据库表自动初始化与迁移功能，支持：
//! - 从文件系统加载版本化迁移 SQL 文件
//! - 自动创建迁移记录表
//! - 校验和验证，防止迁移文件被篡改
//! - 分布式锁，防止多节点并发执行迁移
//! - 迁移回滚支持

pub mod error;
pub mod loader;
pub mod record;
pub mod runner;

pub use error::{MigrationError, MigrationResult};
pub use loader::MigrationLoader;
pub use record::{
    ChecksumMismatch, FailedMigration, MigrationRecord, MigrationStatus, MigrationSummary,
    PendingMigration, ValidationResult,
};
pub use runner::MigrationRunner;
