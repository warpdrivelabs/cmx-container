//! 审计日志存储后端

pub mod memory;

use async_trait::async_trait;
use crate::{AuditRecord, Result};
use crate::record::AuditDomain;

/// 审计日志查询过滤器
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// 按审计域过滤
    pub domain: Option<AuditDomain>,
    /// 按操作者 ID 过滤
    pub actor_id: Option<String>,
    /// 按目标类型过滤
    pub target_type: Option<String>,
    /// 按目标 ID 过滤
    pub target_id: Option<String>,
    /// 按操作结果过滤
    pub result: Option<crate::record::OperationResult>,
    /// 按时间范围过滤（开始）
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    /// 按时间范围过滤（结束）
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

/// 审计日志存储 trait
#[async_trait]
pub trait AuditStore: Send + Sync {
    /// 保存审计记录
    async fn save(&self, record: &AuditRecord) -> Result<()>;

    /// 批量保存审计记录
    async fn save_batch(&self, records: &[AuditRecord]) -> Result<()> {
        for record in records {
            self.save(record).await?;
        }
        Ok(())
    }

    /// 查询审计记录
    async fn query(&self, filter: &AuditFilter, limit: u64, offset: u64) -> Result<Vec<AuditRecord>>;
}
