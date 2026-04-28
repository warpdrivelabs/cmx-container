/// 事务上下文模块
///
/// 提供事务栈管理支持
use std::sync::Arc;
use crate::error::Result;
use crate::transaction::core::{TxnHolder, Propagation};

/// 事务帧
///
/// 用于保存事务状态，支持挂起和恢复
pub struct TransactionFrame {
    pub handle: Arc<TxnHolder>,
    pub propagation: Propagation,
    pub db_id: String,
}

/// 事务上下文
///
/// 管理事务栈，支持嵌套事务
pub struct TransactionContextStack {
    frames: Vec<TransactionFrame>,
}

impl TransactionContextStack {
    /// 创建新的事务上下文
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
        }
    }

    /// 推入事务帧
    pub fn push(&mut self, frame: TransactionFrame) {
        self.frames.push(frame);
    }

    /// 弹出事务帧
    pub fn pop(&mut self) -> Option<TransactionFrame> {
        self.frames.pop()
    }

    /// 获取顶部事务帧
    pub fn top(&self) -> Option<&TransactionFrame> {
        self.frames.last()
    }

    /// 获取顶部事务帧的可变引用
    pub fn top_mut(&mut self) -> Option<&mut TransactionFrame> {
        self.frames.last_mut()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// 获取帧的数量
    pub fn len(&self) -> usize {
        self.frames.len()
    }
}

impl Default for TransactionContextStack {
    fn default() -> Self {
        Self::new()
    }
}

/// 挂起的事务状态
#[derive(Debug, Clone)]
pub struct SuspendedTransaction {
    pub handle: Arc<TxnHolder>,
    pub propagation: Propagation,
    pub db_id: String,
}

/// 事务管理器 trait
pub trait TransactionManager: Send + Sync {
    /// 开始事务
    fn begin_transaction(&self, db_id: &str, propagation: Propagation) -> impl std::future::Future<Output = Result<String>> + Send;

    /// 提交事务
    fn commit_transaction(&self, txn_id: &str) -> impl std::future::Future<Output = Result<()>> + Send;

    /// 回滚事务
    fn rollback_transaction(&self, txn_id: &str) -> impl std::future::Future<Output = Result<()>> + Send;
}
