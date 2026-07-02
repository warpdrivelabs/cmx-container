//! 审计日志存储后端

pub mod database;
pub mod memory;

use crate::record::AuditDomain;
use crate::{AuditRecord, Result};
use async_trait::async_trait;

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
    /// 按请求 ID 过滤（链路追踪）
    pub request_id: Option<String>,
    /// 按操作结果过滤
    pub result: Option<crate::record::OperationResult>,
    /// 按时间范围过滤（开始）
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    /// 按时间范围过滤（结束）
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    /// 按 ID 精确列表过滤（用于 delete_hard 安全调用 / 精确查询）
    pub ids: Option<Vec<String>>,
    /// 按 app_id 过滤（默认 None 表示使用 DatabaseAuditStore 构造时的 app_id）
    ///
    /// 内存存储是进程内单租户场景，隐式归属同一 app_id，
    /// 因此 `MemoryAuditStore` 不对此字段做实际过滤（有意的行为差异）。
    pub app_id: Option<String>,
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
    async fn query(
        &self,
        filter: &AuditFilter,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<AuditRecord>>;
}
