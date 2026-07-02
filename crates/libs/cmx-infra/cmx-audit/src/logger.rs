//! 审计日志记录器

use crate::store::database::DatabaseAuditStore;
use crate::store::{AuditFilter, AuditStore};
use crate::{AuditRecord, Result};
use async_trait::async_trait;
use cmx_database::DatabaseManager;
use std::sync::Arc;

/// 审计日志记录器 trait
///
/// 定义审计日志的记录和查询接口，由具体实现提供存储后端。
#[async_trait]
pub trait AuditLogger: Send + Sync {
    /// 记录审计日志
    async fn log(&self, record: AuditRecord) -> Result<()>;

    /// 查询审计日志
    async fn query(
        &self,
        filter: &AuditFilter,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<AuditRecord>>;
}

/// 默认审计日志记录器实现
pub struct DefaultAuditLogger {
    store: Arc<dyn AuditStore>,
}

impl DefaultAuditLogger {
    pub fn new(store: Arc<dyn AuditStore>) -> Self {
        Self { store }
    }

    /// 从数据库管理器快速构造 DatabaseAuditStore 并包装为 AuditLogger。
    /// 等价于 `DefaultAuditLogger::new(Arc::new(DatabaseAuditStore::new(mm, db_id, app_id)))`。
    ///
    /// `app_id` 通常来自 `ConfigManager::global().get_string("application.id")`，
    /// 缺省时回退 `"default"`（与表列 DEFAULT 保持一致）。
    pub fn with_db(
        mm: Arc<DatabaseManager>,
        db_id: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Self {
        Self::new(Arc::new(DatabaseAuditStore::new(mm, db_id, app_id)))
    }
}

#[async_trait]
impl AuditLogger for DefaultAuditLogger {
    async fn log(&self, record: AuditRecord) -> Result<()> {
        self.store.save(&record).await
    }

    async fn query(
        &self,
        filter: &AuditFilter,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<AuditRecord>> {
        self.store.query(filter, limit, offset).await
    }
}
