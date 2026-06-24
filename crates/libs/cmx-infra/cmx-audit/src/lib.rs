//! cmx-audit 模块
//!
//! 通用审计日志基础设施，提供领域无关的审计记录和查询能力。

pub mod error;
pub mod record;
pub mod logger;
pub mod store;

pub use error::{AuditError, Result};
pub use record::{AuditRecord, AuditDomain, OperationResult};
pub use logger::AuditLogger;
pub use store::database::DatabaseAuditStore;
