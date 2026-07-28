//! cmx-dct-store-pg 错误助手——HTTP 语义稳定、PG 真实明细抽取、统一日志结构。
//!
//! 三个函数的分工：
//! - [`api_err`] / [`api_err_db`]：对外暴露（通过 lib.rs `pub use` 重导出），构造业务错误。
//! - [`map_db_err`]：`pub(crate)`，save 路径（upsert/delete/save_apply）统一包装 DB 错误，
//!   供 resolve/query/write 模块复用。
//! - [`pg_detail`]：私有，从 `cmx_database_pg::Error` 抽 PG 真实明细（SQLSTATE + DETAIL + 约束名）。

use cmx_api_types::Error;
use cmx_database_pg::Error as DbError;

use cmx_dct_model::DictView;

/// 普通业务错误 → cmx_api_types::Error（BusinessError，code!=0/HTTP 200）。
pub fn api_err(msg: &str) -> Error {
    cmx_biz::BizError::business(msg.to_string()).into()
}

/// DB 原始错误 → 已翻译的优雅错误（稳定错误码 + 中文），不再暴露 PG 英文原文。
pub fn api_err_db(raw: &str) -> Error {
    cmx_biz::BizError::from_db_error(raw).into()
}

/// 从 `cmx_database_pg::Error` 抽出 **PostgreSQL 真实错误明细**（SQLSTATE 文案 + DETAIL + 约束名）。
///
/// 背景：tokio-postgres 的 `Error` 顶层 `Display` 恒为无信息的 `db error`——真正的
/// message/detail/constraint 藏在 `as_db_error()` 里。若直接 `format!("{e}")` 会把
/// 「唯一键冲突」这类可翻译错误塌缩成 `db error`，前端无从判断。
///
/// 把三段拼成一个完整串，交给 [`cmx_biz::BizError::from_db_error`] 归类成
/// `CmxErrCode` + 优雅中文。拼接保证含 `unique constraint "..."` / `foreign key` 等稳定
/// 子串，令 `classify_db_error` 命中；`brief_db_detail` 再从中抽约束名脱敏展示。
/// 非 PG 错误（连接/池/事务）回退顶层 Display。
///
/// 与 `cmx-rpt-store-pg::pg_detail` 实现一致，保持 DCT/RPT 落库错误翻译口径统一。
pub(crate) fn pg_detail(e: &DbError) -> String {
    if let DbError::Postgres(pg) = e
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

/// 统一包装 save 路径（upsert/delete/save_apply）的 DB 执行错误：翻译为优雅错误（稳定码 +
/// 中文）+ 结构化日志。
///
/// - `error` 级：只留阶段（phase）+ dict_code + table + row_index + 真实 PG 明细（pg_detail
///   抽出的 message/detail/constraint），便于日志侧反查；不打印 SQL 全文（避免日志膨胀）。
/// - `debug` 级：记录失败 SQL 全文，排查时按需开启。
///
/// 仅用于**原本就走 api_err_db** 的 save 路径（语义不变，只是统一日志结构 + 抽真实 PG 错误）。
/// search/count/search_zmc 路径仍用 api_err（business 错误），不在本函数范围。
///
/// # Arguments
///
/// - `e`：cmx_database_pg 错误（必传具体类型，以便 `as_db_error()` 取 PG 明细）
/// - `phase`：阶段标签（`upsert` / `insert` / `update` / `delete` / `select_parent_id` /
///   `recompute_hierarchy` / `export` / `import_*`）
/// - `view`：字典表视图
/// - `row_index`：行索引（删除/插入单行用 `Some(i)`，批/不限用 `None`）
/// - `sql`：执行的 SQL（仅 debug 级别打印）
pub(crate) fn map_db_err(
    e: DbError,
    phase: &str,
    view: &DictView,
    row_index: Option<usize>,
    sql: &str,
) -> Error {
    let detail = pg_detail(&e);
    tracing::error!(
        target: "cmx_dct::db",
        phase = phase,
        dict_code = %view.dict_code,
        table = %view.table_name,
        row_index = ?row_index,
        error = %e,
        pg_detail = %detail,
        sql = sql,
        "db exec failed"
    );
    api_err_db(&detail)
}
