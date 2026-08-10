//! 合并主流程：master + victims 单事务编排（审查修订版十步流程）。
//!
//! ① lock_record(master, FOR UPDATE) 串行化交叉 merge
//! ② 读 master/victims（须 published）
//! ③ survive 逐字段（多 victim 顺序累积到 master）
//! ④ victim set_lifecycle(published→merged, CAS+version+1)，n=0 冲突报错
//! ⑤ reparent_lines（各明细表 victim→master，先快照行 id 供 unmerge）
//! ⑥ update_header(master, 存活值+version+1, CAS)
//! ⑦ deactivate_xref(victim)
//! ⑧ write_audit(merge) ⑨ write_event(merged, payload 带追溯)
//! ⑩ update_match_group(status, survivorship_log{fields+reparented}, master_id)

use std::collections::HashMap;

use cmx_database_pg::DatabaseManager;
use cmx_mdm_model::survivorship::{survive, SurvivorRule};
use serde_json::{json, Value};

use crate::{
    dct_accessor, error::api_err, match_store, md_accessor,
};

use super::{lifecycle_of, master_record};

/// 合并：master + victims → 单事务（审查修订版流程）。
///
/// 详见模块级文档的十步说明。任一步失败事务回滚，`cm_*` 无中间态。
///
/// # Arguments
///
/// * `mm` / `db_id` - 数据库管理器与数据源 id。
/// * `dict_code` - 字典代码（治理表关联键）。
/// * `head_table` - 目标头物理表（由调用方从查重规则带入）。
/// * `master_id` - 存活记录 id。
/// * `victim_ids` - 被合并记录 id 列表。
/// * `survive_fields` - 参与存活的字段清单。
/// * `rules` - 字段级存活策略（键 ⊆ survive_fields）。
/// * `overrides` - 人工裁决显式真值（键 ⊆ survive_fields）。
/// * `line_tables` - 明细表 reparent 映射 `(表名, 外键列名)`。
/// * `operated_by` - 操作人 id。
/// * `match_group_id` - 合并请求 group id（CAS pending→reviewed）。
///
/// # Returns
///
/// master 记录 id（即 `master_id`）。
///
/// # Errors
///
/// overrides 键越界、master/victim 非 published、CAS 版本冲突、group 已被驳回等任一步出错时返回错误。
#[allow(clippy::too_many_arguments)]
pub async fn merge(
    mm: &DatabaseManager,
    db_id: &str,
    dict_code: &str,
    head_table: &str,
    master_id: i64,
    victim_ids: &[i64],
    survive_fields: &[String],
    rules: &HashMap<String, SurvivorRule>,
    overrides: &serde_json::Map<String, Value>,
    line_tables: &[(String, String)],
    operated_by: i64,
    match_group_id: i64,
) -> Result<i64, cmx_api_types::Error> {
    let txn_ctx = mm.get_transaction_context();
    let guard = txn_ctx
        .begin_with_guard(db_id)
        .await
        .map_err(|e| api_err(&format!("开事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();

    let result = merge_inner(
        mm, db_id, &txn_id, dict_code, head_table, master_id, victim_ids,
        survive_fields, rules, overrides, line_tables, operated_by, match_group_id,
    )
    .await;

    match result {
        Ok(id) => {
            guard
                .commit()
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(id)
        }
        Err(e) => {
            tracing::error!(target: "cmx_mdm::merge", master_id, error = %e, "合并失败,事务已回滚");
            Err(e)
        }
    }
}

/// 合并十步编排（事务内执行，由 [`merge`] 开事务后调用）。
#[allow(clippy::too_many_arguments)]
async fn merge_inner(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    dict_code: &str,
    head_table: &str,
    master_id: i64,
    victim_ids: &[i64],
    survive_fields: &[String],
    rules: &HashMap<String, SurvivorRule>,
    overrides: &serde_json::Map<String, Value>,
    line_tables: &[(String, String)],
    operated_by: i64,
    match_group_id: i64,
) -> Result<i64, cmx_api_types::Error> {
    // overrides 键必须 ⊆ survive_fields（审查 A2，超范围静默丢弃→改报错）
    for k in overrides.keys() {
        if !survive_fields.contains(k) {
            return Err(api_err(&format!("overrides 字段 {k} 不在存活字段清单")));
        }
    }

    // ① 锁 master 行（FOR UPDATE）
    if !dct_accessor::lock_record(mm, db_id, txn_id, head_table, master_id).await? {
        return Err(api_err(&format!("master {master_id} 不存在")));
    }

    // 装载列 = 存活字段 + 状态/版本/时间
    let mut cols: Vec<&str> = vec!["id", "lifecycle_status", "published_version", "update_time"];
    cols.extend(survive_fields.iter().map(|s| s.as_str()));

    // ② 读 master + victims
    let mut all = match_store::load_by_ids(
        mm, db_id, Some(txn_id), head_table, &cols,
        &[vec![master_id], victim_ids.to_vec()].concat(),
    )
    .await?;
    let master = all
        .iter()
        .find(|r| r.id == master_id)
        .ok_or_else(|| api_err(&format!("master {master_id} 读取失败")))?
        .clone();
    if lifecycle_of(&master) != "published" {
        return Err(api_err(&format!("master {master_id} 非 published")));
    }
    let victims: Vec<_> = all
        .drain(..)
        .filter(|r| victim_ids.contains(&r.id))
        .collect();
    if victims.len() != victim_ids.len() {
        return Err(api_err("部分 victim 读取失败"));
    }
    for v in &victims {
        if lifecycle_of(v) != "published" {
            return Err(api_err(&format!("victim {} 非 published（可能已被合并）", v.id)));
        }
    }

    // ③ survive 逐 victim 累积
    let mut master_row = master.fields.clone();
    let mut all_log = Vec::new();
    let mut reparented = json!({});
    let current_v = master
        .fields
        .get("published_version")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);

    for v in &victims {
        let (row, log) = survive(&master_record(&master_row), v, survive_fields, rules);
        for (k, val) in row {
            master_row.insert(k, val);
        }
        all_log.extend(log);

        // ④ victim → merged（CAS+version+1）
        let n = dct_accessor::set_lifecycle(mm, db_id, txn_id, head_table, v.id, "published", "merged")
            .await?;
        if n == 0 {
            return Err(api_err(&format!("victim {} 状态冲突（双 merge 拦截）", v.id)));
        }

        // ⑤ re-parent 明细（快照行 id 供 unmerge）
        for (table, parent_field) in line_tables {
            let ids = dct_accessor::select_line_ids(mm, db_id, txn_id, table, parent_field, v.id)
                .await?;
            dct_accessor::reparent_lines(mm, db_id, txn_id, table, parent_field, v.id, master_id)
                .await?;
            reparented[table.clone()] = json!(ids);
        }

        // ⑦ xref inactive
        match_store::deactivate_xref(mm, db_id, Some(txn_id), dict_code, v.id).await?;
    }

    // ③' 人工裁决 overrides 覆盖（M4 审查 A3）：survive 之后应用，改写 log from=override
    for (k, val) in overrides {
        master_row.insert(k.clone(), val.clone());
        if let Some(entry) = all_log.iter_mut().find(|e| &e.field == k) {
            entry.from = "override".to_string();
            entry.value = val.clone();
        } else {
            all_log.push(cmx_mdm_model::survivorship::SurvivorLogEntry {
                field: k.clone(),
                from: "override".to_string(),
                value: val.clone(),
            });
        }
    }

    // ⑥ 存活值写回 master（version+1, CAS）
    let mut upd = serde_json::Map::new();
    for f in survive_fields {
        if let Some(val) = master_row.get(f) {
            upd.insert(f.clone(), val.clone());
        }
    }
    upd.insert("published_version".into(), json!(current_v + 1));
    let n = dct_accessor::update_header(mm, db_id, txn_id, head_table, master_id, &upd, current_v)
        .await?;
    if n == 0 {
        return Err(api_err(&format!("master {master_id} 版本冲突")));
    }

    // ⑧ 审计
    md_accessor::write_audit(
        mm, db_id, txn_id, dict_code, master_id, current_v + 1, "merge",
        None, None, None, None, operated_by,
    )
    .await?;

    // ⑨ 事件（payload 带追溯，审查重要-1/建议-9）
    let payload = json!({
        "match_group_id": match_group_id,
        "master_id": master_id,
        "victim_ids": victim_ids,
        "survivorship": all_log.iter().map(|l| json!({"field": l.field, "from": l.from})).collect::<Vec<_>>(),
    });
    md_accessor::write_event(mm, db_id, txn_id, dict_code, master_id, "merged", payload).await?;

    // ⑩ match_group 归档（审查 C3/C6：CAS pending→reviewed 占位，防与 reject 并发裂态）
    let t = match_store::transition_match_group(mm, db_id, Some(txn_id), match_group_id, "pending", "reviewed")
        .await?;
    if t == 0 {
        // 非 pending：若已 rejected 报错；若已 reviewed（M3 手工新插）继续落 slog
        let st = match_store::get_match_group(mm, db_id, match_group_id)
            .await?
            .and_then(|g| g.get("status").and_then(|s| s.as_str().map(|x| x.to_string())))
            .unwrap_or_default();
        if st == "rejected" {
            return Err(api_err(&format!("group {match_group_id} 已被驳回，不可合并")));
        }
    }
    let slog = json!({ "fields": all_log, "reparented": reparented });
    match_store::update_match_group(
        mm, db_id, Some(txn_id), match_group_id, "reviewed", Some(&slog), Some(master_id),
    )
    .await?;

    Ok(master_id)
}
