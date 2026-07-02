//! 内存审计存储（测试用）

use super::{AuditFilter, AuditStore};
use crate::{AuditRecord, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 内存审计存储
pub struct MemoryAuditStore {
    records: Arc<RwLock<Vec<AuditRecord>>>,
}

impl MemoryAuditStore {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for MemoryAuditStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditStore for MemoryAuditStore {
    async fn save(&self, record: &AuditRecord) -> Result<()> {
        self.records.write().await.push(record.clone());
        Ok(())
    }

    async fn save_batch(&self, records: &[AuditRecord]) -> Result<()> {
        let mut store = self.records.write().await;
        store.extend(records.iter().cloned());
        Ok(())
    }

    async fn query(
        &self,
        filter: &AuditFilter,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<AuditRecord>> {
        let store = self.records.read().await;
        let filtered: Vec<AuditRecord> = store
            .iter()
            .filter(|r| {
                if let Some(ref domain) = filter.domain
                    && &r.domain != domain
                {
                    return false;
                }
                if let Some(ref actor_id) = filter.actor_id
                    && r.actor_id.as_ref() != Some(actor_id)
                {
                    return false;
                }
                if let Some(ref target_type) = filter.target_type
                    && r.target_type.as_ref() != Some(target_type)
                {
                    return false;
                }
                if let Some(ref target_id) = filter.target_id
                    && r.target_id.as_ref() != Some(target_id)
                {
                    return false;
                }
                if let Some(ref request_id) = filter.request_id
                    && r.request_id.as_ref() != Some(request_id)
                {
                    return false;
                }
                if let Some(ref result) = filter.result
                    && &r.result != result
                {
                    return false;
                }
                if let Some(from) = filter.from
                    && r.started_at < from
                {
                    return false;
                }
                if let Some(to) = filter.to
                    && r.started_at > to
                {
                    return false;
                }
                // 按 ID 精确列表过滤（行级匹配，与 DatabaseAuditStore 行为一致）
                if let Some(ref ids) = filter.ids
                    && !ids.is_empty()
                    && !ids.iter().any(|id| id == &r.id)
                {
                    return false;
                }
                // 注：app_id 不在内存存储过滤（进程内单租户，隐式归属同一 app_id）
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::AuditDomain;
    use crate::record::OperationResult;

    fn sample(domain: AuditDomain, op: &str, actor: &str) -> AuditRecord {
        AuditRecord::new(domain, op, OperationResult::Success).with_actor(actor, actor)
    }

    #[tokio::test]
    async fn memory_save_and_query() -> Result<()> {
        let store = MemoryAuditStore::new();
        store
            .save(&sample(AuditDomain::Auth, "login", "u1"))
            .await?;
        store
            .save(&sample(AuditDomain::Iam, "role_assign", "u2"))
            .await?;
        store
            .save(&sample(AuditDomain::Biz, "app_create", "u3"))
            .await?;

        let all = store.query(&AuditFilter::default(), 100, 0).await?;
        assert_eq!(all.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn memory_query_with_filter() -> Result<()> {
        let store = MemoryAuditStore::new();
        store
            .save(&sample(AuditDomain::Auth, "login", "u1"))
            .await?;
        store
            .save(&sample(AuditDomain::Iam, "role_assign", "u2"))
            .await?;
        store
            .save(&sample(AuditDomain::Auth, "logout", "u1"))
            .await?;

        // 按 domain 过滤
        let auth = store
            .query(
                &AuditFilter {
                    domain: Some(AuditDomain::Auth),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(auth.len(), 2);

        // 按 actor_id 过滤
        let u2 = store
            .query(
                &AuditFilter {
                    actor_id: Some("u2".to_string()),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(u2.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn memory_query_with_ids_filter() -> Result<()> {
        let store = MemoryAuditStore::new();
        let r1 = sample(AuditDomain::Auth, "login", "u1");
        let r2 = sample(AuditDomain::Auth, "login", "u2");
        let r3 = sample(AuditDomain::Auth, "login", "u3");
        store.save(&r1).await?;
        store.save(&r2).await?;
        store.save(&r3).await?;

        // ids 精确过滤
        let got = store
            .query(
                &AuditFilter {
                    ids: Some(vec![r1.id.clone(), r3.id.clone()]),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(got.len(), 2);

        // 空 ids 列表不应过滤（返回全部，与 DatabaseAuditStore 行为一致）
        let empty_ids = store
            .query(
                &AuditFilter {
                    ids: Some(vec![]),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(empty_ids.len(), 3);
        Ok(())
    }
}
