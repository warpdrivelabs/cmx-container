/*
 * @Author: yqs
 * @Date: 2026-03-05 19:28:02
 * @Describe: 数据库错误类型定义
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-09 15:00:00
 */
use serde::Serialize;
use serde_with::{DisplayFromStr, serde_as};
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;

#[serde_as]
#[derive(Error, Debug, Serialize)]
pub enum Error {
    #[error("无法提交事务，因为没有打开的事务")]
    TxnCantCommitNoOpenTxn,
    #[error("无法开始事务，因为 with_txn 为 false")]
    CannotBeginTxnWithTxnFalse,
    #[error("无法提交事务，因为 with_txn 为 false")]
    CannotCommitTxnWithTxnFalse,
    #[error("没有活跃事务")]
    NoTxn,
    #[error("数据库未找到: {0}")]
    DbNotFound(String),
    #[error("无法创建模型管理器提供者: {0}")]
    CantCreateModelManagerProvider(String),
    #[error("连接超时")]
    ConnectionTimeout,
    #[error("连接池耗尽")]
    PoolExhausted,
    #[error("需要事务")]
    TransactionRequired,
    #[error("不允许事务")]
    TransactionNotAllowed,
    #[error("不支持的数据库类型")]
    UnsupportedDbType,
    #[error("数据库不存在")]
    NoDb,
    #[error("无效的参数: {0}")]
    InvalidParams(String),
    #[error("默认数据源不能删除: {0}")]
    DefaultDbSourceCantDelete(String),
    #[error(transparent)]
    Sqlx(
        #[from]
        #[serde_as(as = "DisplayFromStr")]
        sqlx::Error,
    ),
}
