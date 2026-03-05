/// 事务管理模块，负责数据库事务的创建、提交和回滚

mod error;

pub use error::{Error, Result};

use crate::connection::DbPool;
use sqlx::{Postgres, Transaction};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::Mutex;
use uuid;

/// 数据库访问对象，支持事务管理
#[derive(Debug, Clone)]
pub struct Dbx {
    /// 数据库连接池
    db_pool: DbPool,
    /// 事务持有器，使用互斥锁保护
    txn_holder: Arc<Mutex<Option<TxnHolder>>>,
    /// 是否启用事务
    with_txn: bool,
}

impl Dbx {
    /// 创建新的数据库访问对象
    ///
    /// # 参数
    /// * `db_pool` - 数据库连接池
    /// * `with_txn` - 是否启用事务
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回 Dbx 实例，失败返回错误
    pub fn new(db_pool: DbPool, with_txn: bool) -> Result<Self> {
        Ok(Dbx {
            db_pool,
            // 初始化为空的事务持有者
            txn_holder: Arc::default(),
            with_txn,
        })
    }
}

/// 统一的数据库事务类型
#[derive(Debug)]
pub enum DbTransaction {
    Postgres(Transaction<'static, Postgres>),
    // MySql(Transaction<'static, MySql>),
    // Sqlite(Transaction<'static, Sqlite>),
}

/// 事务持有器，管理事务和引用计数
#[derive(Debug)]
struct TxnHolder {
    /// 数据库事务
    txn: DbTransaction,
    /// 引用计数器
    counter: i32,
    /// 事务ID
    txn_id: String,
    /// 创建时间
    created_at: std::time::Instant,
}

impl TxnHolder {
    /// 创建新的事务持有器
    ///
    /// # 参数
    /// * `txn` - 数据库事务
    ///
    /// # 返回值
    /// * `Self` - 事务持有器实例
    fn new(txn: DbTransaction) -> Self {
        TxnHolder {
            txn,
            counter: 1,
            txn_id: uuid::Uuid::new_v4().to_string(),
            created_at: std::time::Instant::now(),
        }
    }

    /// 增加引用计数
    fn inc(&mut self) {
        self.counter += 1;
    }

    /// 减少引用计数并返回当前值
    ///
    /// # 返回值
    /// * `i32` - 当前引用计数
    fn dec(&mut self) -> i32 {
        self.counter -= 1;
        self.counter
    }

    /// 回滚事务
    async fn rollback( self) -> Result<()> {
        match self.txn {
            DbTransaction::Postgres(dbtnx) => dbtnx.rollback().await?,
            // DbTransaction::MySql(dbtnx) => dbtnx.rollback().await?,
            // DbTransaction::Sqlite(dbtnx) => dbtnx.rollback().await?,
        }
        Ok(())
    }
    /// 提交事务
    async fn commit( self) -> Result<()> {
        match self.txn {
            DbTransaction::Postgres(dbtnx) => dbtnx.commit().await?,
            // DbTransaction::MySql(dbtnx) => dbtnx.commit().await?,
            // DbTransaction::Sqlite(dbtnx) => dbtnx.commit().await?,
        }
        Ok(())
    }

    /// 获取事务ID
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 获取事务运行时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
}

impl Deref for TxnHolder {
    type Target = DbTransaction;

    /// 解引用获取事务引用
    ///
    /// # 返回值
    /// * `&Self::Target` - 事务引用
    fn deref(&self) -> &Self::Target {
        &self.txn
    }
}

impl DerefMut for TxnHolder {
    /// 可变解引用获取事务可变引用
    ///
    /// # 返回值
    /// * `&mut Self::Target` - 事务可变引用
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.txn
    }
}

/// 事务隔离级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// 读取未提交
    ReadUncommitted,
    /// 读取已提交
    ReadCommitted,
    /// 可重复读
    RepeatableRead,
    /// 串行化
    Serializable,
}

impl Dbx {
    /// 开始事务
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    pub async fn begin_txn(&self, db_id: &str) -> Result<String> {
        if !self.with_txn {
            return Err(Error::CannotBeginTxnWithTxnFalse);
        }

        let mut txh_g = self.txn_holder.lock().await;
        // 如果已经有事务持有器，则增加引用计数
        if let Some(txh) = txh_g.as_mut() {
            txh.inc();
            Ok(txh.txn_id().to_string())
        }
        // 否则，创建一个新的事务持有器
        else {
            let txn = match &self.db_pool {
                DbPool::Postgres(pool) => {
                    let transaction = pool.begin().await?;
                    DbTransaction::Postgres(transaction)
                },
                // DbPool::MySql(pool) => {
                //     let transaction = pool.begin().await?;
                //     DbTransaction::MySql(transaction)
                // },
                // DbPool::Sqlite(pool) => {
                //     let transaction = pool.begin().await?;
                //     DbTransaction::Sqlite(transaction)
                // },
            };
            let txh = TxnHolder::new(txn);
            let txn_id = txh.txn_id().to_string();
            let _ = txh_g.insert(txh);
            
            // 注册事务
            register_txn(txn_id.clone(), db_id.to_string());
            
            Ok(txn_id)
        }
    }

    /// 回滚事务
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn rollback_txn(&self) -> Result<()> {
        let mut txh_g = self.txn_holder.lock().await;
        if let Some(mut txn_holder) = txh_g.take() {
            let txn_id = txn_holder.txn_id().to_string();
            // 从 Option 中取出 TxnHolder
            if txn_holder.counter > 1 {
                txn_holder.counter -= 1;
                // 替换 Option 中的值，返回旧值
                let _ = txh_g.replace(txn_holder); // 如果不是最后一个引用，将其放回
            } else {
                // 执行实际的回滚操作
                txn_holder.rollback().await?;
                // 更新事务状态
                crate::transaction::update_txn_status(&txn_id, crate::transaction::TransactionStatus::RolledBack);
                // 不需要替换，因为我们希望将其留为 None
            }
            Ok(())
        } else {
            Err(Error::NoTxn)
        }
    }

    /// 提交事务
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn commit_txn(&self) -> Result<()> {
        if !self.with_txn {
            return Err(Error::CannotCommitTxnWithTxnFalse);
        }

        let mut txh_g = self.txn_holder.lock().await;
        if let Some(txh) = txh_g.as_mut() {
            let txn_id = txh.txn_id().to_string();
            let counter = txh.dec();
            // 如果计数器为 0，则应该提交事务
            if counter == 0 {
                // 从 Option 中取出 txh 从 Option 中取出值，将原位置设为 None
                if let Some(txn) = txh_g.take() {
                    txn.commit().await?;
                    // 更新事务状态
                    crate::transaction::update_txn_status(&txn_id, crate::transaction::TransactionStatus::Committed);
                }
            }

            Ok(())
        }
        // 否则，返回错误
        else {
            Err(Error::TxnCantCommitNoOpenTxn)
        }
    }

    /// 获取数据库连接池
    ///
    /// # 返回值
    /// * `&DbPool` - 数据库连接池引用
    pub fn db(&self) -> &DbPool {
        &self.db_pool
    }

    /// 获取当前事务ID
    ///
    /// # 返回值
    /// * `Option<String>` - 事务ID，如果没有活跃事务则返回 None
    pub async fn get_txn_id(&self) -> Option<String> {
        let txh_g = self.txn_holder.lock().await;
        txh_g.as_ref().map(|txh| txh.txn_id().to_string())
    }

    /// 检查事务是否超时
    ///
    /// # 参数
    /// * `timeout` - 超时时间
    ///
    /// # 返回值
    /// * `bool` - 如果事务超时则返回 true
    pub async fn is_txn_timeout(&self, timeout: std::time::Duration) -> bool {
        let txh_g = self.txn_holder.lock().await;
        txh_g.as_ref().map(|txh| txh.elapsed() > timeout).unwrap_or(false)
    }
}

/// 声明式事务管理宏
#[macro_export]
macro_rules! transaction {
    ($db_id:expr, $dbx:expr, $body:expr) => {
        async {
            let txn_id = $dbx.begin_txn($db_id).await?;
            let result = $body.await;
            match result {
                Ok(value) => {
                    $dbx.commit_txn().await?;
                    Ok(value)
                },
                Err(err) => {
                    $dbx.rollback_txn().await.ok();
                    Err(err)
                }
            }
        }
    };
}

/// 事务元数据
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

/// 事务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Active,
    Committed,
    RolledBack,
}

// 全局事务注册表
pub static GLOBAL_TXN_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, TransactionMetadata>>>> = OnceLock::new();

/// 获取全局事务注册表
fn get_txn_registry() -> &'static Arc<RwLock<HashMap<String, TransactionMetadata>>> {
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
