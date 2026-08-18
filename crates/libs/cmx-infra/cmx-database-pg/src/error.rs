//! 数据库错误类型定义（tokio-postgres 版）。
//!
//! 与 cmx-database（sqlx 版）对齐，仅替换后端错误变体：
//! `Error::Sqlx(sqlx::Error)` → `Error::Postgres(tokio_postgres::Error)` + `Error::Pool(PoolError)`。
//!
//! tokio-postgres / deadpool 顶层错误的 `Display` 只输出固定分类串（`db error`、
//! `error communicating with the server` 等），真实明细藏在 `source()` 链里。本模块
//! 的 `Display` 与 serde 序列化统一拼接完整错误链（见 [`render_pg_error`] 与
//! [`format_error_chain`]），保证日志与 API 返回都能看到真实原因。
use std::error::Error as StdError;
use std::fmt::Write as _;

use serde::Serialize;
use serde::Serializer;
use serde_with::{SerializeAs, serde_as};
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
    /// tokio-postgres 后端错误（查询执行、协议、数据库返回的 SQLSTATE 等）。
    ///
    /// Display 与 serde 序列化均输出真实明细：DbError 取 message/detail/constraint
    /// （与 [`pg_detail`] 同源），其余错误沿 source 链拼接——不再只剩 `db error`
    /// 之类的固定分类串。
    #[error("{}", render_pg_error(.0))]
    Postgres(
        #[from]
        #[serde_as(as = "PgErrorDetail")]
        tokio_postgres::Error,
    ),
    /// deadpool 连接池错误（获取连接失败、池耗尽、创建连接失败等）。
    ///
    /// Display 与 serde 序列化输出「顶层说明 + source 链」；deadpool 的 Display 只
    /// 内嵌后端错误的固定分类串，明细需沿 source 链取。
    #[error("{}", format_error_chain(.0))]
    Pool(
        #[from]
        #[serde_as(as = "ErrorChain")]
        deadpool_postgres::PoolError,
    ),
}

/// serde 序列化 `tokio_postgres::Error` 为完整明细串（与 [`render_pg_error`] 同源）。
///
/// 替代原先的 `DisplayFromStr`：后者只拿 Display 的固定分类串（`db error` 等），
/// 真实明细全丢。
struct PgErrorDetail;

impl SerializeAs<tokio_postgres::Error> for PgErrorDetail {
    fn serialize_as<S>(
        value: &tokio_postgres::Error,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&render_pg_error(value))
    }
}

/// serde 序列化任意 `std::error::Error` 为「Display + source 链」完整串（与
/// [`format_error_chain`] 同源）。
struct ErrorChain;

impl<E> SerializeAs<E> for ErrorChain
where
    E: StdError + 'static,
{
    fn serialize_as<S>(value: &E, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format_error_chain(value))
    }
}

/// 渲染 `tokio_postgres::Error` 的真实明细。
///
/// - DbError（服务端返回的 SQLSTATE 错误）：`message + DETAIL + 约束名`，与
///   [`pg_detail`] 的 DbError 分支完全一致（`classify_db_error` / `brief_db_detail`
///   依赖该稳定格式做子串匹配，不可改动）。
/// - 其余（Io/Tls/ToSql/Encode 等）：顶层 Display 固定分类串 + 逐级 source 链
///   （如 `error communicating with the server: connection refused (os error 111)`）。
///
/// # Arguments
///
/// - `e`：tokio-postgres 后端原始错误。
///
/// # Returns
///
/// 拼接后的完整明细单行串。
pub(crate) fn render_pg_error(e: &tokio_postgres::Error) -> String {
    if let Some(db) = e.as_db_error() {
        let mut s = db.message().to_string();
        if let Some(d) = db.detail() {
            s.push(' ');
            s.push_str(d);
        }
        if let Some(c) = db.constraint() {
            let _ = write!(s, " constraint \"{c}\"");
        }
        return s;
    }
    format_error_chain(e)
}

/// 渲染「顶层 Display + 逐级 source 链」为单行串。
///
/// tokio-postgres / deadpool 的顶层 Display 只有固定分类串，真实原因（io 错误、
/// SQLSTATE 明细等）在 source 链上，逐级拼接才能完整呈现。每段 source 的首尾空白
/// 会被裁掉，避免 deadpool `writeln!` 之类实现带入的多余换行。
///
/// # Arguments
///
/// - `err`：任意标准错误对象。
///
/// # Returns
///
/// 形如 `顶层说明: 一级原因: 二级原因` 的完整串（无 source 时即顶层 Display）。
pub(crate) fn format_error_chain<E>(err: &E) -> String
where
    E: StdError + ?Sized + 'static,
{
    let mut s = err.to_string();
    let mut cur = err.source();
    while let Some(src) = cur {
        let _ = write!(s, ": {}", src.to_string().trim());
        cur = src.source();
    }
    s
}

/// 从 [`Error`] 抽出 **PostgreSQL 真实错误明细**（message + DETAIL + 约束名）。
///
/// 背景：tokio-postgres 的 `Error` 顶层 `Display` 恒为无信息的 `db error`——真正的
/// message/detail/constraint 藏在 `as_db_error()` 里。若直接 `format!("{e}")` 会把
/// 「唯一键冲突」这类可翻译错误塌缩成 `db error`，无从判断。
///
/// 把三段拼成一个完整串，含 `unique constraint "..."` / `foreign key` 等稳定子串，
/// 供上层（cmx-biz 的 `classify_db_error` / `BizError::from_db_error`，或各 store-pg
/// 的冲突判定）归类。非 PG 错误（连接/池/事务）回退顶层 Display——Postgres / Pool
/// 变体的 Display 本身已拼接完整错误链，非 db 错误同样能透出底层原因。
///
/// 历史：本函数原位于 `cmx_biz::error`，因其入参即本 crate 的 [`Error`]，归属地更
/// 自然，已下沉至此；cmx-biz / dct / doc / mdm / rpt / code 等调用方统一改用本路径。
pub fn pg_detail(e: &Error) -> String {
    match e {
        Error::Postgres(pg) => render_pg_error(pg),
        _ => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最底层测试错误（source 链末端）。
    #[derive(Debug, Error)]
    #[error("dns resolution failed")]
    struct RootCause;

    /// 带一层 source 的外层测试错误。
    #[derive(Debug, Error)]
    #[error("connect failed")]
    struct MiddleError(#[source] RootCause);

    #[test]
    fn format_error_chain_joins_all_sources() {
        assert_eq!(
            format_error_chain(&MiddleError(RootCause)),
            "connect failed: dns resolution failed"
        );
        assert_eq!(format_error_chain(&RootCause), "dns resolution failed");
    }

    #[test]
    fn pool_variant_display_and_serde_carry_detail() {
        // HookError::Message 无需真实 PG 连接即可构造带 Display 的 PoolError，
        // 用于验证 Pool 变体的 Display / serde 序列化确实透出明细。
        let pool_err = deadpool_postgres::PoolError::PostCreateHook(
            deadpool_postgres::HookError::Message("boom-hook-msg".into()),
        );
        let e = Error::from(pool_err);

        let display = e.to_string();
        assert!(
            display.contains("boom-hook-msg"),
            "Display 应含 hook 明细: {display}"
        );

        let json = serde_json::to_string(&e).expect("Error 序列化应成功");
        assert!(
            json.contains("boom-hook-msg"),
            "serde 序列化应含 hook 明细: {json}"
        );
    }

    #[test]
    fn pg_detail_fallback_returns_display_for_plain_variants() {
        assert_eq!(
            pg_detail(&Error::DbNotFound("db_x".into())),
            "数据库未找到: db_x"
        );
    }
}
