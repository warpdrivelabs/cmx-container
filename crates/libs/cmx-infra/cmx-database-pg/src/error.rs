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

/// 从 [`Error`] 抽出 **PostgreSQL 真实错误明细**（message + DETAIL + 约束名）。
///
/// 背景：tokio-postgres 的 `Error` 顶层 `Display` 恒为无信息的 `db error`——真正的
/// message/detail/constraint 藏在 `as_db_error()` 里。若直接 `format!("{e}")` 会把
/// 「唯一键冲突」这类可翻译错误塌缩成 `db error`，无从判断。
///
/// 把三段拼成一个完整串，含 `unique constraint "..."` / `foreign key` 等稳定子串，
/// 供上层（cmx-biz 的 `classify_db_error` / `BizError::from_db_error`，或各 store-pg
/// 的冲突判定）归类。非 PG 错误（连接/池/事务）回退顶层 Display。
///
/// 历史：本函数原位于 `cmx_biz::error`，因其入参即本 crate 的 [`Error`]，归属地更
/// 自然，已下沉至此；cmx-biz / dct / doc / mdm / rpt / code 等调用方统一改用本路径。
pub fn pg_detail(e: &Error) -> String {
    if let Error::Postgres(pg) = e
        && let Some(db) = pg.as_db_error()
    {
        let mut s = db.message().to_string();
        if let Some(d) = db.detail() {
            s.push(' ');
            s.push_str(d);
        }
        if let Some(c) = db.constraint() {
            s.push_str(&format!(" constraint \"{c}\""));
        }
        return s;
    }
    e.to_string()
}
