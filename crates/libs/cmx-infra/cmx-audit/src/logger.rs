//! 审计日志记录器

use async_trait::async_trait;
use std::sync::Arc;
use crate::{AuditRecord, Result};
use crate::store::{AuditStore, AuditFilter};

/// 审计日志记录器 trait
///
/// 定义审计日志的记录和查询接口，由具体实现提供存储后端。
#[async_trait]
pub trait AuditLogger: Send + Sync {
    /// 记录审计日志
    async fn log(&self, record: AuditRecord) -> Result<()>;

    /// 查询审计日志
    async fn query(&self, filter: &AuditFilter, limit: u64, offset: u64) -> Result<Vec<AuditRecord>>;
}

/// 默认审计日志记录器实现
pub struct DefaultAuditLogger {
    store: Arc<dyn AuditStore>,
}

impl DefaultAuditLogger {
    pub fn new(store: Arc<dyn AuditStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl AuditLogger for DefaultAuditLogger {
    async fn log(&self, record: AuditRecord) -> Result<()> {
        self.store.save(&record).await
    }

    async fn query(&self, filter: &AuditFilter, limit: u64, offset: u64) -> Result<Vec<AuditRecord>> {
        self.store.query(filter, limit, offset).await
    }
}
