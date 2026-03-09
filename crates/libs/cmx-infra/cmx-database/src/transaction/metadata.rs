/// 事务元数据模块，用于管理事务的元数据和状态
///
/// 该模块定义了事务的元数据结构和状态管理功能，包括：
/// - TransactionStatus：事务状态枚举
/// - TransactionMetadata：事务元数据结构
/// - 全局事务注册表及相关操作函数

use std::sync::{Arc, OnceLock, RwLock};
use std::collections::HashMap;

/// 事务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    /// 活跃状态
    Active,
    /// 已提交状态
    Committed,
    /// 已回滚状态
    RolledBack,
}

/// 事务元数据
///
/// 用于存储事务的元信息，包括事务ID、数据库ID、创建时间和状态
#[derive(Debug, Clone)]
pub struct TransactionMetadata {
    /// 事务ID
    pub txn_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 创建时间
    pub created_at: std::time::Instant,
    /// 状态
    pub status: TransactionStatus,
}

/// 全局事务注册表
///
/// 用于存储所有事务的元数据，便于监控和管理
pub static GLOBAL_TXN_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, TransactionMetadata>>>> = OnceLock::new();

/// 获取全局事务注册表
///
/// # 返回值
/// * `&'static Arc<RwLock<HashMap<String, TransactionMetadata>>>` - 全局事务注册表
pub fn get_txn_registry() -> &'static Arc<RwLock<HashMap<String, TransactionMetadata>>> {
    GLOBAL_TXN_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 注册事务
///
/// 将事务元数据注册到全局注册表
///
/// # 参数
/// * `txn_id` - 事务ID
/// * `db_id` - 数据库ID
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
///
/// 更新事务的状态（活跃、已提交、已回滚）
///
/// # 参数
/// * `txn_id` - 事务ID
/// * `status` - 新的事务状态
pub fn update_txn_status(txn_id: &str, status: TransactionStatus) {
    if let Some(metadata) = get_txn_registry().write().unwrap().get_mut(txn_id) {
        metadata.status = status;
    }
}

/// 获取事务元数据
///
/// 通过事务ID获取事务的元数据
///
/// # 参数
/// * `txn_id` - 事务ID
///
/// # 返回值
/// * `Option<TransactionMetadata>` - 事务元数据，如果事务不存在则返回 None
pub fn get_txn_metadata(txn_id: &str) -> Option<TransactionMetadata> {
    get_txn_registry().read().unwrap().get(txn_id).cloned()
}

/// 获取活跃事务列表
///
/// 获取所有状态为活跃的事务
///
/// # 返回值
/// * `Vec<TransactionMetadata>` - 活跃事务列表
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
///
/// 从注册表中移除已提交或已回滚的事务，减少内存使用
pub fn cleanup_completed_transactions() {
    let mut registry = get_txn_registry().write().unwrap();
    registry.retain(|_, meta| meta.status == TransactionStatus::Active);
}

/// 检查长时间运行的事务
///
/// 检查运行时间超过指定超时时间的活跃事务
///
/// # 参数
/// * `timeout` - 超时时间
///
/// # 返回值
/// * `Vec<TransactionMetadata>` - 超时事务列表
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
