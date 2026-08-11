//! cmx-dct-store-pg 错误助手——save 路径统一包装 + UNIQUE 冲突判定。
//!
//! 公共错误构造（`api_err`/`api_err_db`）来自 `cmx_biz::error`，经 lib.rs
//! `pub use cmx_biz::{api_err, api_err_db}` 重导出，保持本 crate 对外接口不变；
//! `pg_detail` 因入参即 `cmx_database_pg::Error`，已下沉至 `cmx_database_pg`。
//! 本模块仅保留依赖 `DictView` 的 save 路径专属助手（`map_db_err` / `is_unique_violation`）。

use cmx_api_types::Error;
use cmx_database_pg::Error as DbError;

use cmx_dct_model::DictView;

// 公共错误助手重导出（向后兼容：本 crate 内 `api_err`/`api_err_db` 调用点零改动）。
pub use cmx_biz::{api_err, api_err_db};

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
    let detail = cmx_database_pg::pg_detail(&e);
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
    cmx_biz::api_err_db(&detail)
}

/// 判断 DB 错误是否为 UNIQUE（唯一约束）冲突。
///
/// 用于 saver 层编码兜底重试：落库 UNIQUE 冲突时，清空该行 code 重新铸号后重试 INSERT
/// （防御发号序列表与业务表不一致的极端并发情况）。判定口径与 `classify_db_error` 一致。
pub(crate) fn is_unique_violation(e: &DbError) -> bool {
    let detail = cmx_database_pg::pg_detail(e).to_ascii_lowercase();
    detail.contains("duplicate key") || detail.contains("unique constraint")
}
