/// 事务元数据模块，用于管理事务的元数据和状态

use std::sync::{Arc, OnceLock, RwLock};
use std::collections::HashMap;

/// 事务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Active,
    Committed,
    RolledBack,
}

/// 事务元数据
#[derive(Debug, Clone)]
pub struct TransactionMetadata {
    pub txn_id: String,
    pub db_id: String,
    pub created_at: std::time::Instant,
    pub status: TransactionStatus,
}

/// 全局事务注册表
pub static GLOBAL_TXN_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, TransactionMetadata>>>> = OnceLock::new();

/// 获取全局事务注册表
pub fn get_txn_registry() -> &'static Arc<RwLock<HashMap<String, TransactionMetadata>>> {
    GLOBAL_TXN_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 注册事务
pub fn register_txn(txn_id: String, db_id: String) {
    let metadata = TransactionMetadata {
        txn_id: txn_id.clone(),
        db_id,
        created_at: std::time::Instant::now(),
        status: TransactionStatus::Active,
    };
    get_txn_registry().write().unwrap().insert(txn_id, metadata);
}

/// 更新事务状态
pub fn update_txn_status(txn_id: &str, status: TransactionStatus) {
    if let Some(metadata) = get_txn_registry().write().unwrap().get_mut(txn_id) {
        metadata.status = status;
    }
}

/// 获取事务元数据
pub fn get_txn_metadata(txn_id: &str) -> Option<TransactionMetadata> {
    get_txn_registry().read().unwrap().get(txn_id).cloned()
}

/// 获取活跃事务列表
pub fn get_active_transactions() -> Vec<TransactionMetadata> {
    get_txn_registry()
        .read()
        .unwrap()
        .values()
        .filter(|meta| meta.status == TransactionStatus::Active)
        .cloned()
        .collect()
}

/// 清理已完成的事务
pub fn cleanup_completed_transactions() {
    let mut registry = get_txn_registry().write().unwrap();
    registry.retain(|_, meta| meta.status == TransactionStatus::Active);
}

/// 检查长时间运行的事务
pub fn check_long_running_transactions(timeout: std::time::Duration) -> Vec<TransactionMetadata> {
    get_txn_registry()
        .read()
        .unwrap()
        .values()
        .filter(|meta| {
            meta.status == TransactionStatus::Active && meta.created_at.elapsed() > timeout
        })
        .cloned()
        .collect()
}
