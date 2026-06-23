//! 内存审计存储（测试用）

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{AuditRecord, Result};
use super::{AuditStore, AuditFilter};

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

    async fn query(&self, filter: &AuditFilter, limit: u64, offset: u64) -> Result<Vec<AuditRecord>> {
        let store = self.records.read().await;
        let filtered: Vec<AuditRecord> = store
            .iter()
            .filter(|r| {
                if let Some(ref domain) = filter.domain
                    && &r.domain != domain { return false; }
                if let Some(ref actor_id) = filter.actor_id
                    && r.actor_id.as_ref() != Some(actor_id) { return false; }
                if let Some(ref target_type) = filter.target_type
                    && r.target_type.as_ref() != Some(target_type) { return false; }
                if let Some(ref target_id) = filter.target_id
                    && r.target_id.as_ref() != Some(target_id) { return false; }
                if let Some(ref result) = filter.result
                    && &r.result != result { return false; }
                if let Some(from) = filter.from
                    && r.started_at < from { return false; }
                if let Some(to) = filter.to
                    && r.started_at > to { return false; }
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }
}
