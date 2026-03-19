//! 事务管理模块 - 多数据库事务支持
//!
//! 提供多数据库架构下的事务管理功能，支持跨数据库事务、事务传播和回滚。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

/// 事务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// 活跃
    Active,
    /// 已提交
    Committed,
    /// 已回滚
    RolledBack,
    /// 失败
    Failed,
}

/// 事务项 - 记录每个数据库分支的事务信息
#[derive(Debug, Clone)]
pub struct TransactionBranch {
    /// 数据库 ID
    pub db_id: String,
    /// 事务 ID
    pub txn_id: String,
    /// 是否已完成
    pub completed: bool,
}

/// 事务上下文
#[derive(Debug, Clone)]
pub struct TransactionContext {
    /// 全局事务 ID
    pub global_txn_id: String,
    /// 分支事务列表
    pub branches: Vec<TransactionBranch>,
    /// 事务状态
    pub state: TransactionState,
    /// 创建时间
    pub create_time: chrono::DateTime<chrono::Utc>,
}

/// 事务管理器配置
#[derive(Debug, Clone)]
pub struct TransactionManagerConfig {
    /// 默认超时时间（秒）
    pub default_timeout_seconds: u64,
    /// 最大重试次数
    pub max_retry_count: u32,
    /// 是否启用自动回滚
    pub auto_rollback: bool,
}

impl Default for TransactionManagerConfig {
    fn default() -> Self {
        Self {
            default_timeout_seconds: 300,
            max_retry_count: 3,
            auto_rollback: true,
        }
    }
}

/// 事务管理器 - 负责多数据库事务管理
pub struct TransactionManager {
    config: TransactionManagerConfig,
    /// 活跃事务缓存
    active_transactions: Arc<RwLock<HashMap<String, TransactionContext>>>,
}

impl TransactionManager {
    /// 创建新的事务管理器
    pub fn new(config: TransactionManagerConfig) -> Self {
        Self {
            config,
            active_transactions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 使用默认配置创建事务管理器
    pub fn with_default_config() -> Self {
        Self::new(TransactionManagerConfig::default())
    }

    /// 开始一个新事务
    pub async fn begin_transaction(&self, db_id: &str) -> Result<TransactionContext, crate::PluginError> {
        let global_txn_id = Uuid::new_v4().to_string();

        // TODO: 集成 cmx-database 实际创建事务
        // let txn_id = database_manager.begin_transaction(db_id, options).await?;

        let context = TransactionContext {
            global_txn_id: global_txn_id.clone(),
            branches: vec![TransactionBranch {
                db_id: db_id.to_string(),
                txn_id: format!("txn_{}", global_txn_id),
                completed: false,
            }],
            state: TransactionState::Active,
            create_time: chrono::Utc::now(),
        };

        // 缓存事务上下文
        let mut transactions = self.active_transactions.write().await;
        transactions.insert(global_txn_id, context.clone());

        log::info!("开始新事务: {} on db: {}", context.global_txn_id, db_id);

        Ok(context)
    }

    /// 为现有事务添加分支（跨数据库操作）
    pub async fn add_branch(&self, global_txn_id: &str, db_id: &str) -> Result<TransactionContext, crate::PluginError> {
        let mut transactions = self.active_transactions.write().await;

        let context = transactions
            .get_mut(global_txn_id)
            .ok_or_else(|| crate::PluginError::Transaction(format!("事务 {} 不存在", global_txn_id)))?;

        if context.state != TransactionState::Active {
            return Err(crate::PluginError::Transaction(format!(
                "事务 {} 状态不是活跃",
                global_txn_id
            )));
        }

        // TODO: 集成 cmx-database 实际创建分支事务

        context.branches.push(TransactionBranch {
            db_id: db_id.to_string(),
            txn_id: format!("txn_{}_{}", global_txn_id, db_id),
            completed: false,
        });

        log::info!("为事务 {} 添加分支: {} on db: {}", global_txn_id, context.global_txn_id, db_id);

        Ok(context.clone())
    }

    /// 提交事务
    pub async fn commit(&self, global_txn_id: &str) -> Result<(), crate::PluginError> {
        let mut transactions = self.active_transactions.write().await;

        let context = transactions
            .get_mut(global_txn_id)
            .ok_or_else(|| crate::PluginError::Transaction(format!("事务 {} 不存在", global_txn_id)))?;

        if context.state != TransactionState::Active {
            return Err(crate::PluginError::Transaction(format!(
                "事务 {} 状态不是活跃，无法提交",
                global_txn_id
            )));
        }

        // TODO: 集成 cmx-database 提交所有分支事务

        for branch in &context.branches {
            if !branch.completed {
                log::info!("提交分支事务: {} on db: {}", branch.txn_id, branch.db_id);
            }
        }

        context.state = TransactionState::Committed;

        log::info!("事务 {} 已提交", global_txn_id);

        Ok(())
    }

    /// 回滚事务
    pub async fn rollback(&self, global_txn_id: &str) -> Result<(), crate::PluginError> {
        let mut transactions = self.active_transactions.write().await;

        let context = transactions
            .get_mut(global_txn_id)
            .ok_or_else(|| crate::PluginError::Transaction(format!("事务 {} 不存在", global_txn_id)))?;

        if context.state != TransactionState::Active {
            log::warn!("事务 {} 状态不是活跃，当前状态: {:?}", global_txn_id, context.state);
            return Ok(());
        }

        // TODO: 集成 cmx-database 回滚所有分支事务

        for branch in &context.branches {
            if !branch.completed {
                log::info!("回滚分支事务: {} on db: {}", branch.txn_id, branch.db_id);
            }
        }

        context.state = TransactionState::RolledBack;

        log::info!("事务 {} 已回滚", global_txn_id);

        Ok(())
    }

    /// 获取事务状态
    pub async fn get_status(&self, global_txn_id: &str) -> Result<Option<TransactionState>, crate::PluginError> {
        let transactions = self.active_transactions.read().await;

        Ok(transactions.get(global_txn_id).map(|c| c.state))
    }

    /// 检查事务是否活跃
    pub async fn is_active(&self, global_txn_id: &str) -> bool {
        let transactions = self.active_transactions.read().await;

        transactions
            .get(global_txn_id)
            .map(|c| c.state == TransactionState::Active)
            .unwrap_or(false)
    }

    /// 清理已完成的事务
    pub async fn cleanup(&self) -> Result<usize, crate::PluginError> {
        let mut transactions = self.active_transactions.write().await;

        let initial_count = transactions.len();
        transactions.retain(|_, ctx| ctx.state == TransactionState::Active);

        let cleaned = initial_count - transactions.len();
        log::info!("清理了 {} 个已完成的事务", cleaned);

        Ok(cleaned)
    }

    /// 获取配置
    pub fn config(&self) -> &TransactionManagerConfig {
        &self.config
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::with_default_config()
    }
}

/// 事务守卫 - RAII 模式的自动回滚
pub struct TransactionGuard {
    manager: Arc<TransactionManager>,
    global_txn_id: String,
    committed: bool,
}

impl TransactionGuard {
    /// 创建新的事务守卫
    pub fn new(manager: Arc<TransactionManager>, global_txn_id: String) -> Self {
        Self {
            manager,
            global_txn_id,
            committed: false,
        }
    }

    /// 提交事务
    pub async fn commit(mut self) -> Result<(), crate::PluginError> {
        self.committed = true;
        self.manager.commit(&self.global_txn_id).await
    }

    /// 获取事务 ID
    pub fn transaction_id(&self) -> &str {
        &self.global_txn_id
    }
}

impl Drop for TransactionGuard {
    fn drop(&mut self) {
        if !self.committed {
            // 事务未提交，自动回滚
            let manager = self.manager.clone();
            let txn_id = self.global_txn_id.clone();

            // 在 async 上下文中无法直接 await，需要记录警告
            log::warn!("事务 {} 未提交，在 Drop 时自动回滚", txn_id);

            // 可以选择同步回滚或记录待回滚
            let _ = tokio::runtime::Handle::current().block_on(manager.rollback(&txn_id));
        }
    }
}
