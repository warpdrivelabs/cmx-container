/// 事务注册表模块，用于管理TxnHolder的全局注册表
///
/// 该模块定义了全局TxnHolder注册表及相关操作函数

use std::sync::{Arc, OnceLock, RwLock, Mutex};
use std::collections::HashMap;
use crate::transaction::core::TxnHolder;

/// 全局 TxnHolder 注册表
static GLOBAL_TXN_HOLDER_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>>> = OnceLock::new();

/// 获取全局 TxnHolder 注册表
pub fn get_txn_holder_registry() -> &'static Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>> {
    GLOBAL_TXN_HOLDER_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 通过事务ID获取 TxnHolder
pub fn get_txn_holder_by_id(txn_id: &str) -> Option<Arc<Mutex<Option<TxnHolder>>>> {
    get_txn_holder_registry().read().unwrap().get(txn_id).cloned()
}
