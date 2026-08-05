//! 反查 max SQL 实现（impl Advance trait）。
//!
//! 对应方案 §4.1：`SELECT MAX(CAST(SUBSTRING(code FROM ...))) FROM table WHERE code LIKE 'prefix%'`。
//! minted_buffer 非空时 union 进候选集（同事务多行铸号推进 max）。

use async_trait::async_trait;
use cmx_code_model::advance::Advance;
use cmx_code_model::error::{CodeError, Result};
use cmx_code_model::spec::Target;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::get_default_pg_db_manager;

/// PG Advance 实现：反查 max + UNIQUE 重试。
///
/// `db_id` / `txn_id` 通过 `PgAdvance { db_id, txn_id }` 构造时传入。
pub struct PgAdvance {
    pub db_id: String,
    pub txn_id: Option<String>,
}

impl PgAdvance {
    pub fn new(db_id: &str, txn_id: Option<&str>) -> Self {
        Self {
            db_id: db_id.to_string(),
            txn_id: txn_id.map(|s| s.to_string()),
        }
    }
}

#[async_trait]
impl Advance for PgAdvance {
    async fn query_max_serial(
        &self,
        target: &Target,
        prefix: &str,
        width: usize,
        minted_buffer: &[String],
    ) -> Result<i64> {
        let mm = get_default_pg_db_manager();
        let table = &target.code;
        let field = &target.field;
        let prefix_len = prefix.len();

        // 反查 max SQL（方案 §4.1）
        // WHERE field LIKE 'prefix%' AND LENGTH(field) = prefix_len + width AND 尾部纯数字
        let sql = format!(
            r#"SELECT COALESCE(
                MAX(CAST(SUBSTRING("{field}" FROM {prefix_len} + 1 FOR {width}) AS BIGINT)),
                0
            ) AS max_serial
            FROM "{table}"
            WHERE "{field}" LIKE $1
              AND LENGTH("{field}") = {prefix_len} + {width}
              AND SUBSTRING("{field}" FROM {prefix_len} + 1) ~ '^[0-9]+$'"#,
            field = field,
            table = table,
            prefix_len = prefix_len,
            width = width,
        );

        let ds = mm
            .query_sql_with_datavalues(
                &self.db_id,
                self.txn_id.as_deref(),
                &sql,
                vec![DataValue::String(format!("{prefix}%"))],
                "code_max",
            )
            .await
            .map_err(|e| CodeError::Database(format!("反查 max 失败：{e}")))?;

        let db_max = ds
            .rows
            .first()
            .and_then(|r| r.get(0))
            .and_then(|dv| match dv {
                DataValue::Int(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(0);

        // union minted_buffer（同事务已铸号）
        let buffer_max = minted_buffer
            .iter()
            .filter_map(|s| {
                s.get(prefix_len..)
                    .and_then(|tail| tail.parse::<i64>().ok())
            })
            .max()
            .unwrap_or(0);

        Ok(db_max.max(buffer_max))
    }

    async fn take_gap(&self, prefix: &str, width: usize) -> Result<Option<i64>> {
        // C6：从 stub 升级为真实断号表查询（gap_store::take_gap）
        crate::store::gap_store::take_gap(prefix, width, &self.db_id).await
    }

    async fn try_insert(&self, _target: &Target, _code: &str) -> Result<()> {
        // 铸号阶段不做真实 INSERT —— 这是设计决策，非占位：
        //
        // DCT/DOC saver 的铸号发生在 apply_merge 之前（钩子算出 code 写回 changeset），
        // 真正的 INSERT 由 saver 的 apply_merge / write 完成，业务表的 UNIQUE 约束在那里兜底。
        // 若 saver 落库时 UNIQUE 冲突，由 saver 捕获后重新调 mint 取下一个号（C3 钩子接入）。
        //
        // 因此 evaluate_segments 的重试循环在铸号阶段恒不触发（try_insert 恒 Ok），
        // 重试责任上移到 saver 层。这是有意为之 —— 铸号函数只算号，不落库。
        //
        // 如果未来需要在铸号阶段预检（如 SELECT EXISTS 查重），在此实现，
        // 返回 Err(CodeError::UniqueViolation) 触发 evaluate_segments 的重试循环。
        Ok(())
    }
}
