//! `cmx_code_seq` 发号序列表读写（集群安全发号源）。
//!
//! 对应方案 H1/H2：`use_sequence=true` 时，serial/dateSerial 段的发号走本表，
//! 用 `SELECT ... FOR UPDATE` 行级锁取连续号段，集群安全（AGENTS.md §五红线）。
//! 首启（current_val=0）时从业务表探测真实 max 作基线，避免覆盖存量号。

use cmx_code_model::error::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::get_default_pg_db_manager;
use cmx_utils::next_pk_id;

use super::rule_store::db_err;

/// 从发号序列表取 N 个连续流水号（集群安全）。
///
/// 必须在调用方的事务内执行（`txn_id` 非 None），使 `FOR UPDATE` 行锁与落库形成原子段。
/// 若 `txn_id=None`，内部开短事务保证 INSERT + SELECT FOR UPDATE + UPDATE 原子。
///
/// # 参数
/// - `rule_code`：规则码（关联 cmx_code_rule.rule_code）
/// - `prefix`：发号分组键（含 reset_key，如 FV20260804）
/// - `count`：本次要取的连续号个数（≥1）
/// - `start` / `step`：流水段起始值/步长（来自 RuleSpec）
/// - `probed_max`：调用方已反查的业务表 max（首启基线探测用，避免重复查）
/// - `width`：流水宽度（记录首次发号时的宽度，供断号补零用）
/// - `db_id` / `txn_id`：事务句柄
///
/// # 返回
/// `count` 个连续流水值（i64），第 i 个 = `base + i * step`。
#[allow(clippy::too_many_arguments)]
pub async fn alloc_serial_segment(
    rule_code: &str,
    prefix: &str,
    count: usize,
    start: i64,
    step: i64,
    probed_max: i64,
    width: usize,
    db_id: &str,
    txn_id: Option<&str>,
) -> Result<Vec<i64>> {
    debug_assert!(count >= 1, "alloc_serial_segment: count 必须 ≥1");

    let mm = get_default_pg_db_manager();

    // ① 确保行存在（current_val=0）。ON CONFLICT 保证幂等——并发首次同 prefix 时只有一行。
    let id = next_pk_id() as i64;
    let insert_sql = r#"INSERT INTO cmx_code_seq (id, rule_code, prefix, current_val, width)
        VALUES ($1, $2, $3, 0, $4)
        ON CONFLICT (rule_code, prefix) DO NOTHING"#;
    mm.execute_sql_with_datavalues(
        db_id,
        txn_id,
        insert_sql,
        vec![
            DataValue::Int(id),
            DataValue::String(rule_code.into()),
            DataValue::String(prefix.into()),
            DataValue::Int(width as i64),
        ],
    )
    .await
    .map_err(|e| db_err("发号序列表初始化", e))?;

    // ② 行级锁 SELECT 当前值。txn_id 非 None 时 FOR UPDATE 锁随事务提交释放；
    //    txn_id=None 时锁随本语句所在隐式事务（每条 autocommit）立即释放——极端并发下仍有
    //    窗口，故调用方应尽量传 txn_id（DCT save / DOC saver 都已传主事务）。
    let select_sql = r#"SELECT current_val FROM cmx_code_seq
        WHERE rule_code = $1 AND prefix = $2 FOR UPDATE"#;
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            txn_id,
            select_sql,
            vec![
                DataValue::String(rule_code.into()),
                DataValue::String(prefix.into()),
            ],
            "code_seq",
        )
        .await
        .map_err(|e| db_err("发号序列表加锁读取", e))?;

    let stored_val = ds
        .rows
        .first()
        .and_then(|r| r.get(0))
        .and_then(|dv| match dv {
            DataValue::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(0);

    // ③ 首启基线探测：current_val=0 表示首次发号，取业务表真实 max（probed_max）作起点，
    //    避免序列表从 0 开始覆盖存量号。后续发号 stored_val 已 >0，直接推进。
    let effective_base_val = if stored_val == 0 {
        probed_max.max(0)
    } else {
        stored_val
    };

    let base = cmx_code_model::rule_algo::next_after(effective_base_val, start, step);
    let last = base + (count as i64 - 1) * step;

    // ④ 推进 current_val 到本次取走的最大号。
    let update_sql = r#"UPDATE cmx_code_seq SET current_val = $3, update_time = NOW()
        WHERE rule_code = $1 AND prefix = $2"#;
    mm.execute_sql_with_datavalues(
        db_id,
        txn_id,
        update_sql,
        vec![
            DataValue::String(rule_code.into()),
            DataValue::String(prefix.into()),
            DataValue::Int(last),
        ],
    )
    .await
    .map_err(|e| db_err("发号序列表推进", e))?;

    // ⑤ 生成本次号段（base, base+step, ..., last）
    let codes: Vec<i64> = (0..count as i64).map(|i| base + i * step).collect();

    tracing::debug!(
        target: "cmx_code::seq",
        rule_code = rule_code, prefix = prefix,
        count = count, base = base, last = last,
        stored_val = stored_val, probed_max = probed_max,
        "alloc_serial_segment"
    );

    Ok(codes)
}
