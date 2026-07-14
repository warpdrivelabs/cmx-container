//! 事务元数据模块，用于管理事务的元数据和状态

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

/// 事务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    /// 活跃中（未提交也未回滚）。
    Active,
    /// 已提交。
    Committed,
    /// 已回滚。
    RolledBack,
}

/// 事务元数据
#[derive(Debug, Clone)]
pub struct TransactionMetadata {
    /// 事务唯一标识。
    pub txn_id: String,
    /// 所属数据库 ID。
    pub db_id: String,
    /// 创建时刻（用于超时检测）。
    pub create_time: std::time::Instant,
    /// 当前状态。
    pub status: TransactionStatus,
}

/// 全局事务注册表
pub static GLOBAL_TXN_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, TransactionMetadata>>>> =
    OnceLock::new();

/// 获取全局事务注册表
pub fn get_txn_registry() -> &'static Arc<RwLock<HashMap<String, TransactionMetadata>>> {
    GLOBAL_TXN_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 注册事务
pub async fn register_txn(txn_id: String, db_id: String) {
    let metadata = TransactionMetadata {
        txn_id: txn_id.clone(),
        db_id,
        create_time: std::time::Instant::now(),
        status: TransactionStatus::Active,
    };
    get_txn_registry().write().await.insert(txn_id, metadata);
}

/// 更新事务状态,已提交 已经回滚的事务移除掉
pub async fn update_txn_status(txn_id: &str, status: TransactionStatus) {
    if status == TransactionStatus::Committed || status == TransactionStatus::RolledBack {
        get_txn_registry().write().await.remove(txn_id);
    }
}

/// 获取事务元数据
pub async fn get_txn_metadata(txn_id: &str) -> Option<TransactionMetadata> {
    get_txn_registry().read().await.get(txn_id).cloned()
}

/// 获取活跃事务列表
pub async fn get_active_transactions() -> Vec<TransactionMetadata> {
    get_txn_registry()
        .read()
        .await
        .values()
        .filter(|meta| meta.status == TransactionStatus::Active)
        .cloned()
        .collect()
}

/// 清理已完成的事务
pub async fn cleanup_completed_transactions() {
    let mut registry = get_txn_registry().write().await;
    registry.retain(|_, meta| meta.status == TransactionStatus::Active);
}

/// 检查长时间运行的事务
pub async fn check_long_running_transactions(
    timeout: std::time::Duration,
) -> Vec<TransactionMetadata> {
    get_txn_registry()
        .read()
        .await
        .values()
        .filter(|meta| {
            meta.status == TransactionStatus::Active && meta.create_time.elapsed() > timeout
        })
        .cloned()
        .collect()
}
