//! 事务注册表模块，用于管理TxnHolder的全局注册表
//!
//! 该模块定义了全局TxnHolder注册表及相关操作函数

use std::sync::{Arc, OnceLock};
use std::collections::HashMap;
use tokio::sync::{Mutex, RwLock};
use crate::transaction::core::TxnHolder;

/// TxnHolder 的可锁容器类型
type TxnHolderMutex = Arc<Mutex<Option<TxnHolder>>>;
/// TxnHolder 注册表的 Map 类型
type TxnHolderMap = HashMap<String, TxnHolderMutex>;
/// TxnHolder 注册表类型
type TxnHolderRegistry = Arc<RwLock<TxnHolderMap>>;

/// 全局 TxnHolder 注册表
static GLOBAL_TXN_HOLDER_REGISTRY: OnceLock<TxnHolderRegistry> = OnceLock::new();

/// 获取全局 TxnHolder 注册表
pub fn get_txn_holder_registry() -> &'static TxnHolderRegistry {
    GLOBAL_TXN_HOLDER_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 通过事务ID获取 TxnHolder
pub async fn get_txn_holder_by_id(txn_id: &str) -> Option<TxnHolderMutex> {
    get_txn_holder_registry().read().await.get(txn_id).cloned()
}
