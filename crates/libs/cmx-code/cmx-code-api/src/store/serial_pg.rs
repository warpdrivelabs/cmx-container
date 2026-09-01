//! 反查 max SQL 实现（impl Advance trait）。
//!
//! 对应方案 §4.1：`SELECT MAX(CAST(SUBSTRING(code FROM ...))) FROM table WHERE code LIKE 'prefix%'`。
//! minted_buffer 非空时 union 进候选集（同事务多行铸号推进 max）。

use async_trait::async_trait;
use cmx_code_model::advance::Advance;
use cmx_code_model::error::Result;
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
            .map_err(|e| super::rule_store::db_err("反查 max", e))?;

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
        // 透传 PgAdvance 的 txn_id，使 FOR UPDATE SKIP LOCKED 行锁与落库形成原子段
        crate::store::gap_store::take_gap(prefix, width, &self.db_id, self.txn_id.as_deref())
            .await
    }

    async fn try_insert(&self, _target: &Target, _code: &str) -> Result<()> {
        // 铸号阶段不做真实 INSERT —— 设计决策：铸号函数只算号，不落库。
        //
        // 唯一性保证分两层：
        // 1. use_sequence=true：发号序列表（cmx_code_seq）的 FOR UPDATE 行锁保证取号原子，
        //    同 prefix 不会分发出重号（集群安全）。
        // 2. use_sequence=false（默认反查 max）：minted_buffer 推进保证同事务多行不重，
        //    跨事务并发靠业务表 UNIQUE 约束兜底。
        //
        // saver 层（DCT write.rs apply_inserts/upsert、DOC saver.rs exec）在落库时捕获
        // UNIQUE 冲突，清空 code 重新调 mint_codes_for_inserts 取下一个号重试（上限 3 次）。
        //
        // 因此 evaluate_segments 的 MAX_RETRY 重试循环在铸号阶段恒不触发（本函数恒 Ok），
        // 重试责任由 saver 层承担（已实现，见 is_unique_violation 判定 + 重铸重试）。
        //
        // 如未来需在铸号阶段预检（SELECT EXISTS 查重），在此返回 Err 触发重试循环。
        Ok(())
    }
}
