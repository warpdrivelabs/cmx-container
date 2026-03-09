/// 事务注册表模块，用于管理TxnHolder的全局注册表
///
/// 该模块定义了全局TxnHolder注册表及相关操作函数，包括：
/// - 全局TxnHolder注册表
/// - 获取和管理TxnHolder的函数
/// - 通过事务ID操作事务的函数

use std::sync::{Arc, OnceLock, RwLock, Mutex};
use std::collections::HashMap;
use crate::transaction::core::TxnHolder;

/// 全局TxnHolder注册表
///
/// 用于通过事务ID获取TxnHolder，支持通过事务ID操作事务
static GLOBAL_TXN_HOLDER_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>>> = OnceLock::new();

/// 获取全局TxnHolder注册表
///
/// # 返回值
/// * `&'static Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>` - 全局TxnHolder注册表
pub fn get_txn_holder_registry() -> &'static Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>> {
    GLOBAL_TXN_HOLDER_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 通过事务ID获取TxnHolder的可变引用
///
/// 允许通过事务ID获取到TxnHolder的可变引用，以便操作事务
///
/// # 参数
/// * `txn_id` - 事务ID
///
/// # 返回值
/// * `Option<Arc<Mutex<Option<TxnHolder>>>>` - TxnHolder的互斥锁，如果事务不存在则返回None
pub fn get_txn_holder_by_id(txn_id: &str) -> Option<Arc<Mutex<Option<TxnHolder>>>> {
    get_txn_holder_registry().read().unwrap().get(txn_id).cloned()
}
