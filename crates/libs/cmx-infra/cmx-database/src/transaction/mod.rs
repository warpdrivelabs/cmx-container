/// 事务管理模块，负责数据库事务的创建、提交和回滚
///
/// 该模块提供了完整的事务管理功能，包括：
/// - 事务的创建、提交和回滚
/// - 事务传播机制
/// - 事务状态跟踪
/// - 事务元数据管理
/// - 通过事务ID操作事务

mod error;

// 导出错误类型和结果类型
pub use error::{Error, Result};

// 导入必要的依赖
use crate::connection::DbPool;
use sqlx::{Postgres, Transaction};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::Mutex;
use uuid;

/// 数据库访问对象，支持事务管理
///
/// Dbx 是与数据库交互的主要入口点，提供了事务管理功能
#[derive(Debug, Clone)]
pub struct Dbx {
    /// 数据库连接池，用于获取数据库连接
    db_pool: DbPool,
    /// 事务持有器，使用互斥锁保护，确保线程安全
    txn_holder: Arc<Mutex<Option<TxnHolder>>>,
    /// 是否启用事务，为false时不能使用事务功能
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
///
/// 支持多种数据库类型的事务，目前仅实现了PostgreSQL
#[derive(Debug)]
pub enum DbTransaction {
    /// PostgreSQL事务
    Postgres(Transaction<'static, Postgres>),
    // MySql(Transaction<'static, MySql>),  // 预留MySQL支持
    // Sqlite(Transaction<'static, Sqlite>),  // 预留SQLite支持
}

// impl DbTransaction {
//     /// 获取PostgreSQL事务的可变引用
//     ///
//     /// # 返回值
//     /// * `Option<&mut Transaction<'static, Postgres>>` - PostgreSQL事务的可变引用，如果不是PostgreSQL事务则返回None
//     pub fn as_postgres_mut(&mut self) -> Option<&mut Transaction<'static, Postgres>> {
//         match self {
//             DbTransaction::Postgres(txn) => Some(txn),
//             // DbTransaction::MySql(_) => None,
//             // DbTransaction::Sqlite(_) => None,
//         }
//     }
//
//     /// 获取PostgreSQL事务的引用
//     ///
//     /// # 返回值
//     /// * `Option<&Transaction<'static, Postgres>>` - PostgreSQL事务的引用，如果不是PostgreSQL事务则返回None
//     pub fn as_postgres(&self) -> Option<&Transaction<'static, Postgres>> {
//         match self {
//             DbTransaction::Postgres(txn) => Some(txn),
//             // DbTransaction::MySql(_) => None,
//             // DbTransaction::Sqlite(_) => None,
//         }
//     }
// }

/// 事务持有器，管理事务和引用计数
///
/// 用于跟踪事务的状态和引用计数，支持事务的嵌套使用
#[derive(Debug)]
struct TxnHolder {
    /// 数据库事务实例
    txn: DbTransaction,
    /// 引用计数器，用于跟踪事务的嵌套次数
    counter: i32,
    /// 事务ID，全局唯一
    txn_id: String,
    /// 创建时间，用于计算事务运行时间
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
            counter: 1,  // 初始引用计数为1
            txn_id: uuid::Uuid::new_v4().to_string(),  // 生成唯一事务ID
            created_at: std::time::Instant::now(),  // 记录创建时间
        }
    }

    /// 增加引用计数
    ///
    /// 用于支持事务的嵌套使用
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
    ///
    /// 执行实际的事务回滚操作
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    async fn rollback(self) -> Result<()> {
        match self.txn {
            DbTransaction::Postgres(dbtnx) => dbtnx.rollback().await?,
            // DbTransaction::MySql(dbtnx) => dbtnx.rollback().await?,
            // DbTransaction::Sqlite(dbtnx) => dbtnx.rollback().await?,
        }
        Ok(())
    }

    /// 提交事务
    ///
    /// 执行实际的事务提交操作
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    async fn commit(self) -> Result<()> {
        match self.txn {
            DbTransaction::Postgres(dbtnx) => dbtnx.commit().await?,
            // DbTransaction::MySql(dbtnx) => dbtnx.commit().await?,
            // DbTransaction::Sqlite(dbtnx) => dbtnx.commit().await?,
        }
        Ok(())
    }

    /// 获取事务ID
    ///
    /// # 返回值
    /// * `&str` - 事务ID
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 获取事务运行时间
    ///
    /// # 返回值
    /// * `std::time::Duration` - 事务运行时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
}

/// 实现Deref trait，允许直接访问内部事务
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

/// 实现DerefMut trait，允许直接修改内部事务
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
    /// 读取未提交：允许读取未提交的数据
    ReadUncommitted,
    /// 读取已提交：只能读取已提交的数据
    ReadCommitted,
    /// 可重复读：保证在同一个事务中多次读取同一数据时结果一致
    RepeatableRead,
    /// 串行化：最高隔离级别，完全锁定数据
    Serializable,
}

/// 事务传播行为
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Propagation {
    /// REQUIRED (默认)：如果当前存在事务，则加入该事务；如果当前不存在事务，则创建一个新事务
    Required,
    /// REQUIRES_NEW：创建一个新事务，如果当前存在事务，则将当前事务挂起
    RequiresNew,
    // /// NESTED：如果当前存在事务，则创建一个嵌套事务；如果当前不存在事务，则创建一个新事务
    // Nested,
    /// SUPPORTS：如果当前存在事务，则加入该事务；如果当前不存在事务，则以非事务方式执行
    Supports,
    /// NOT_SUPPORTED：以非事务方式执行，如果当前存在事务，则将当前事务挂起
    NotSupported,
    /// MANDATORY：必须在事务中执行，如果当前不存在事务，则抛出异常
    Mandatory,
    /// NEVER：必须以非事务方式执行，如果当前存在事务，则抛出异常
    Never,
}

impl Dbx {
    /// 开始事务
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    /// * `propagation` - 事务传播行为
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    pub async fn begin_txn(&self, db_id: &str, propagation: Propagation) -> Result<String> {
        // 检查是否启用了事务
        if !self.with_txn {
            return Err(Error::CannotBeginTxnWithTxnFalse);
        }

        // 根据传播行为执行不同的逻辑
        match propagation {
            Propagation::Required => {
                // 如果存在事务则加入，否则创建新事务
                let mut txh_g = self.txn_holder.lock().await;
                if txh_g.is_some() {
                    if let Some(txh) = txh_g.as_mut() {
                        // 增加引用计数
                        txh.inc();
                        Ok(txh.txn_id().to_string())
                    } else {
                        Err(Error::NoTxn)
                    }
                } else {
                    // 释放锁
                    drop(txh_g);
                    // 创建新事务
                    self.create_new_txn(db_id).await
                }
            },
            Propagation::RequiresNew => {
                // 创建新事务，挂起当前事务
                // 注意：这里简化实现，实际应该保存当前事务状态
                // 由于当前实现不支持事务挂起，我们先创建新事务
                // self.create_new_txn(db_id).await
                //不使用当前事务，创建新事务
              Dbx::with_transaction(&self)?.create_new_txn(db_id).await
            },
            // Propagation::Nested => {
            //     // 如果存在事务则创建嵌套事务，否则创建新事务
            //     // 注意：这里简化实现，实际应该使用保存点
            //     let mut txh_g = self.txn_holder.lock().await;
            //     if txh_g.is_some() {
            //         if let Some(txh) = txh_g.as_mut() {
            //             // 增加引用计数
            //             txh.inc();
            //             Ok(txh.txn_id().to_string())
            //         } else {
            //             Err(Error::NoTxn)
            //         }
            //     } else {
            //         // 释放锁
            //         drop(txh_g);
            //         // 创建新事务
            //         self.create_new_txn(db_id).await
            //     }
            // },
            Propagation::Supports => {
                // 如果存在事务则加入，否则以非事务方式执行
                let mut txh_g = self.txn_holder.lock().await;
                if txh_g.is_some() {
                    if let Some(txh) = txh_g.as_mut() {
                        // 增加引用计数
                        txh.inc();
                        Ok(txh.txn_id().to_string())
                    } else {
                        Err(Error::NoTxn)
                    }
                } else {
                    // 以非事务方式执行，返空字符串
                    Ok("non-transactional".to_string())
                }
            },
            Propagation::NotSupported => {
                // 以非事务方式执行，挂起当前事务
                // 注意：这里简化实现，实际应该保存当前事务状态
                Ok("non-transactional".to_string())
            },
            Propagation::Mandatory => {
                // 必须在事务中执行，否则抛出异常
                let mut txh_g = self.txn_holder.lock().await;
                if txh_g.is_some() {
                    if let Some(txh) = txh_g.as_mut() {
                        // 增加引用计数
                        txh.inc();
                        Ok(txh.txn_id().to_string())
                    } else {
                        Err(Error::NoTxn)
                    }
                } else {
                    Err(Error::TransactionRequired)
                }
            },
            Propagation::Never => {
                // 必须以非事务方式执行，否则抛出异常
                let txh_g = self.txn_holder.lock().await;
                if txh_g.is_some() {
                    Err(Error::TransactionNotAllowed)
                } else {
                    Ok("non-transactional".to_string())
                }
            },
        }
    }

    /// 开始事务（默认传播行为：Required）
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    pub async fn begin_txn_default(&self, db_id: &str) -> Result<String> {
        // 使用默认的传播行为：Required
        self.begin_txn(db_id, Propagation::Required).await
    }

    /// 创建新事务
    ///
    /// 内部方法，用于创建新的事务实例
    ///
    /// # 参数
    /// * `db_id` - 数据库标识符
    ///
    /// # 返回值
    /// * `Result<String>` - 成功返回事务ID，失败返回错误
    async fn create_new_txn(&self, db_id: &str) -> Result<String> {
        // 根据数据库类型创建事务
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

        // 创建事务持有器
        let txh = TxnHolder::new(txn);
        let txn_id = txh.txn_id().to_string();

        // 获取事务持有器的锁并插入新事务
        let mut txh_g = self.txn_holder.lock().await;
        let _ = txh_g.insert(txh);

        // 注册事务到元数据注册表
        register_txn(txn_id.clone(), db_id.to_string());

        // 注册TxnHolder到全局注册表，以便通过事务ID操作事务
        get_txn_holder_registry().write().unwrap().insert(txn_id.clone(), self.txn_holder.clone());

        // 返回事务ID
        Ok(txn_id)
    }

    /// 回滚事务
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn rollback_txn(&self) -> Result<()> {
        let mut txn_id: Option<String> = None;
        let mut should_rollback = false;
        let mut txn_to_rollback = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = self.txn_holder.lock().await;

            // 检查是否存在事务
            if let Some(mut txn_holder) = txh_g.take() {
                txn_id = Some(txn_holder.txn_id().to_string());

                // 检查引用计数
                if txn_holder.counter > 1 {
                    // 如果不是最后一个引用，减少计数并放回
                    txn_holder.counter -= 1;
                    let _ = txh_g.replace(txn_holder);
                } else {
                    // 保存事务以便后续回滚
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                    // 不需要替换，因为我们希望将其留为 None
                }
                Ok(())
            } else {
                // 没有活跃事务，返回错误
                Err(Error::NoTxn)
            }
        };
        result?;

        // 如果需要回滚，执行回滚操作并更新事务状态
        if should_rollback && txn_to_rollback.is_some() && txn_id.is_some() {
            let txn = txn_to_rollback.unwrap();
            let txn_id = txn_id.unwrap();

            // 执行回滚操作
            txn.rollback().await?;

            // 更新事务状态为已回滚
            crate::transaction::update_txn_status(&txn_id, crate::transaction::TransactionStatus::RolledBack);
            // 从全局TxnHolder注册表中移除
            get_txn_holder_registry().write().unwrap().remove(&txn_id);
        }

        Ok(())
    }

    /// 提交事务
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn commit_txn(&self) -> Result<()> {
        // 检查是否启用了事务
        if !self.with_txn {
            return Err(Error::CannotCommitTxnWithTxnFalse);
        }

        let mut txn_id: Option<String> = None;
        let mut should_commit = false;
        let mut txn_to_commit = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = self.txn_holder.lock().await;

            // 检查是否存在事务
            if let Some(txh) = txh_g.as_mut() {
                txn_id = Some(txh.txn_id().to_string());
                // 减少引用计数
                let counter = txh.dec();

                // 如果计数器为 0，则应该提交事务
                if counter == 0 {
                    // 从 Option 中取出事务
                    txn_to_commit = txh_g.take();
                    should_commit = true;
                }

                Ok(())
            }
            // 否则，返回错误
            else {
                Err(Error::TxnCantCommitNoOpenTxn)
            }
        };
        result?;

        // 如果需要提交，执行提交操作并更新事务状态
        if should_commit && txn_to_commit.is_some() && txn_id.is_some() {
            let txn = txn_to_commit.unwrap();
            let txn_id = txn_id.unwrap();

            // 执行提交操作
            txn.commit().await?;

            // 更新事务状态为已提交
            crate::transaction::update_txn_status(&txn_id, crate::transaction::TransactionStatus::Committed);
            // 从全局TxnHolder注册表中移除
            get_txn_holder_registry().write().unwrap().remove(&txn_id);
        }

        Ok(())
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

    /// 创建一个支持事务的 Dbx 实例
    ///
    /// 从当前 Dbx 实例克隆连接池，并创建一个新的支持事务的 Dbx 实例
    /// 该实例不需要注册到注册表中，适用于临时的事务操作
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回支持事务的 Dbx 实例，失败返回错误
    pub fn with_transaction(&self) -> Result<Self> {
        Dbx::new(self.db_pool.clone(), true)
    }
    
}

/// 声明式事务管理宏
///
/// 提供了一种简洁的方式来管理事务，自动处理事务的开始、提交和回滚
#[macro_export]
macro_rules! transaction {
    // 基本用法，使用默认传播行为
    ($db_id:expr, $dbx:expr, $body:expr) => {
        async {
            // 开始事务，使用默认传播行为
            let txn_id = $dbx.begin_txn_default($db_id).await?;
            // 执行事务体
            let result = $body.await;
            // 根据执行结果决定提交或回滚
            match result {
                Ok(value) => {
                    // 提交事务
                    $dbx.commit_txn().await?;
                    Ok(value)
                },
                Err(err) => {
                    // 回滚事务
                    $dbx.rollback_txn().await.ok();
                    Err(err)
                }
            }
        }
    };

    // 带传播行为的事务宏
    ($db_id:expr, $dbx:expr, $propagation:expr, $body:expr) => {
        async {
            // 开始事务，使用指定的传播行为
            let txn_id = $dbx.begin_txn($db_id, $propagation).await?;
            // 执行事务体
            let result = $body.await;
            // 根据执行结果决定提交或回滚
            match result {
                Ok(value) => {
                    // 提交事务
                    $dbx.commit_txn().await?;
                    Ok(value)
                },
                Err(err) => {
                    // 回滚事务
                    $dbx.rollback_txn().await.ok();
                    Err(err)
                }
            }
        }
    };
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

/// 全局事务注册表
///
/// 用于存储所有事务的元数据，便于监控和管理
pub static GLOBAL_TXN_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, TransactionMetadata>>>> = OnceLock::new();

/// 全局TxnHolder注册表
///
/// 用于通过事务ID获取TxnHolder，支持通过事务ID操作事务
static GLOBAL_TXN_HOLDER_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>>> = OnceLock::new();

/// 获取全局事务注册表
///
/// # 返回值
/// * `&'static Arc<RwLock<HashMap<String, TransactionMetadata>>>` - 全局事务注册表
fn get_txn_registry() -> &'static Arc<RwLock<HashMap<String, TransactionMetadata>>> {
    GLOBAL_TXN_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// 获取全局TxnHolder注册表
///
/// # 返回值
/// * `&'static Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>` - 全局TxnHolder注册表
fn get_txn_holder_registry() -> &'static Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>> {
    GLOBAL_TXN_HOLDER_REGISTRY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
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

/// 通过事务ID提交事务
///
/// 允许通过事务ID远程提交事务
///
/// # 参数
/// * `txn_id` - 事务ID
///
/// # 返回值
/// * `Result<()>` - 成功返回 Ok(())，失败返回错误
pub async fn commit_txn_by_id(txn_id: &str) -> Result<()> {
    // 从全局TxnHolder注册表中获取事务
    let txn_holder_mutex = get_txn_holder_registry().read().unwrap().get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        let mut should_commit = false;
        let mut txn_to_commit = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = txn_holder_mutex.lock().await;

            // 检查是否存在事务
            if let Some(txh) = txh_g.as_mut() {
                // 减少引用计数
                let counter = txh.dec();

                // 如果计数器为 0，则提交事务
                if counter == 0 {
                    // 从 Option 中取出事务
                    txn_to_commit = txh_g.take();
                    should_commit = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        // 如果需要提交，执行提交操作并更新事务状态
        if should_commit && txn_to_commit.is_some() {
            let txn = txn_to_commit.unwrap();

            // 执行提交操作
            txn.commit().await?;

            // 更新事务状态
            update_txn_status(txn_id, TransactionStatus::Committed);
            // 从注册表中移除
            get_txn_holder_registry().write().unwrap().remove(txn_id);
        }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}

/// 通过事务ID回滚事务
///
/// 允许通过事务ID远程回滚事务
///
/// # 参数
/// * `txn_id` - 事务ID
///
/// # 返回值
/// * `Result<()>` - 成功返回 Ok(())，失败返回错误
pub async fn rollback_txn_by_id(txn_id: &str) -> Result<()> {
    // 从全局TxnHolder注册表中获取事务
    let txn_holder_mutex = get_txn_holder_registry().read().unwrap().get(txn_id).cloned();

    if let Some(txn_holder_mutex) = txn_holder_mutex {
        let mut should_rollback = false;
        let mut txn_to_rollback = None;

        // 获取事务持有器的锁
        let result = {
            let mut txh_g = txn_holder_mutex.lock().await;

            // 检查是否存在事务
            if let Some(mut txn_holder) = txh_g.take() {
                // 检查引用计数
                if txn_holder.counter > 1 {
                    // 如果不是最后一个引用，减少计数并放回
                    txn_holder.counter -= 1;
                    let _ = txh_g.replace(txn_holder);
                } else {
                    // 保存事务以便后续回滚
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        // 如果需要回滚，执行回滚操作并更新事务状态
        if should_rollback && txn_to_rollback.is_some() {
            let txn = txn_to_rollback.unwrap();

            // 执行回滚操作
            txn.rollback().await?;

            // 更新事务状态
            update_txn_status(txn_id, TransactionStatus::RolledBack);
            // 从注册表中移除
            get_txn_holder_registry().write().unwrap().remove(txn_id);
        }
    } else {
        return Err(Error::NoTxn);
    }

    Ok(())
}
//
// /// 通过事务ID获取PostgreSQL事务的可变引用
// ///
// /// 允许通过事务ID获取到sqlx的Transaction可变引用，以便直接操作底层事务
// ///
// /// # 参数
// /// * `txn_id` - 事务ID
// /// * `f` - 闭包，用于操作事务
// ///
// /// # 返回值
// /// * `Result<T>` - 成功返回闭包的返回值，失败返回错误
// pub async fn with_transaction_by_id<T, F>(txn_id: &str, f: F) -> Result<T>
// where
//     F: FnOnce(&mut Transaction<'static, Postgres>) -> Result<T>,
// {
//     // 从全局TxnHolder注册表中获取事务
//     let txn_holder_mutex = get_txn_holder_registry().read().unwrap().get(txn_id).cloned();
//
//     if let Some(txn_holder_mutex) = txn_holder_mutex {
//         // 获取事务持有器的锁
//         let mut txh_g = txn_holder_mutex.lock().await;
//
//         // 检查是否存在事务
//         if let Some(txh) = txh_g.as_mut() {
//             // 获取PostgreSQL事务的可变引用
//             if let Some(postgres_txn) = txh.txn.as_postgres_mut() {
//                 // 执行闭包
//                 let result = f(postgres_txn);
//                 result
//             } else {
//                 Err(Error::NoTxn)
//             }
//         } else {
//             Err(Error::NoTxn)
//         }
//     } else {
//         Err(Error::NoTxn)
//     }
// }
