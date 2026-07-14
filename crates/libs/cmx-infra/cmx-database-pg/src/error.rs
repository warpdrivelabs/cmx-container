//! 数据库错误类型定义（tokio-postgres 版）。
//!
//! 与 cmx-database（sqlx 版）对齐，仅替换后端错误变体：
//! `Error::Sqlx(sqlx::Error)` → `Error::Postgres(tokio_postgres::Error)` + `Error::Pool(PoolError)`。
use serde::Serialize;
use serde_with::{DisplayFromStr, serde_as};
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;

#[serde_as]
#[derive(Error, Debug, Serialize)]
pub enum Error {
    /// 无法提交事务，因为没有打开的事务。
    #[error("无法提交事务，因为没有打开的事务")]
    TxnCantCommitNoOpenTxn,
    /// 无法开始事务，因为 with_txn 为 false。
    #[error("无法开始事务，因为 with_txn 为 false")]
    CannotBeginTxnWithTxnFalse,
    /// 无法提交事务，因为 with_txn 为 false。
    #[error("无法提交事务，因为 with_txn 为 false")]
    CannotCommitTxnWithTxnFalse,
    /// 没有活跃事务。
    #[error("没有活跃事务")]
    NoTxn,
    /// 指定 db_id 的数据库未找到。
    #[error("数据库未找到: {0}")]
    DbNotFound(String),
    /// 无法创建模型管理器提供者。
    #[error("无法创建模型管理器提供者: {0}")]
    CantCreateModelManagerProvider(String),
    /// 连接超时。
    #[error("连接超时")]
    ConnectionTimeout,
    /// 连接池耗尽（无可用连接）。
    #[error("连接池耗尽")]
    PoolExhausted,
    /// 当前操作需要在事务中执行。
    #[error("需要事务")]
    TransactionRequired,
    /// 当前操作不允许在事务中执行。
    #[error("不允许事务")]
    TransactionNotAllowed,
    /// 不支持的数据库类型。
    #[error("不支持的数据库类型")]
    UnsupportedDbType,
    /// 数据库不存在。
    #[error("数据库不存在")]
    NoDb,
    /// 无效的参数。
    #[error("无效的参数: {0}")]
    InvalidParams(String),
    /// 默认数据源不能删除。
    #[error("默认数据源不能删除: {0}")]
    DefaultDbSourceCantDelete(String),
    /// tokio-postgres 后端错误（查询执行、协议、数据库返回的 SQLSTATE 等）
    #[error(transparent)]
    Postgres(
        #[from]
        #[serde_as(as = "DisplayFromStr")]
        tokio_postgres::Error,
    ),
    /// deadpool 连接池错误（获取连接失败、池耗尽、创建连接失败等）
    #[error(transparent)]
    Pool(
        #[from]
        #[serde_as(as = "DisplayFromStr")]
        deadpool_postgres::PoolError,
    ),
}
