//! 激活器主流程：单事务编排七步（V3.1 单事务原子 + 乐观锁并发控制）。
//!
//! 读 CR → 读映射 → 头处理(create/update) → 明细处理 → 记审计 → 发事件 → CR 归档。
//! 全程在一个 DB 事务内，任一步失败 guard drop 自动回滚，无中间态。

use std::collections::HashMap;

use cmx_database_pg::DatabaseManager;
use cmx_mdm_model::activation::{plan_create, plan_lines, plan_update};
use cmx_mdm_model::codegen::CodeGenerator;
use cmx_mdm_model::survivorship::{survive, SurvivorRule};
use serde_json::{json, Value};

use crate::{
    activation_store, dct_accessor, doc_accessor, error::api_err, match_store, md_accessor,
};

/// 激活一份 CR（审批通过后调用）。
///
/// 两条触发路径统一入口：① 审批型 CR（M2 的 ServiceTask/JavaDelegate）；
/// ② 手动/内部 CR（API 端点直接调）。
///
/// - `operated_by`：操作人 id（从 handler 的 SVRContext.user_id 解析）
/// - `codegen`：编码生成器（M1 传 RandomCodeGenerator stub）
///
/// 返回新建/变更的主数据记录 id。
pub async fn activate(
    mm: &DatabaseManager,
    db_id: &str,
    cr_id: i64,
    operated_by: i64,
    codegen: &dyn CodeGenerator,
) -> Result<i64, cmx_api_types::Error> {
    // 开事务（RAII guard，drop 自动回滚）
    let txn_ctx = mm.get_transaction_context();
    let guard = txn_ctx
        .begin_with_guard(db_id)
        .await
        .map_err(|e| api_err(&format!("开事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();

    let result = activate_inner(mm, db_id, &txn_id, cr_id, operated_by, codegen).await;

    match result {
        Ok(record_id) => {
            guard
                .commit()
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(record_id)
        }
        Err(e) => {
            // guard drop 自动回滚（commit 未调用）
            tracing::error!(
                target: "cmx_mdm::activation",
                cr_id, error = %e, "激活失败,事务已回滚"
            );
            Err(e)
        }
    }
}

async fn activate_inner(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    cr_id: i64,
    operated_by: i64,
    codegen: &dyn CodeGenerator,
) -> Result<i64, cmx_api_types::Error> {
    // 1. 读 CR 头 + 行
    let cr_head = doc_accessor::load_cr_head(mm, db_id, Some(txn_id), cr_id).await?;
    let cr_lines = doc_accessor::load_cr_lines(mm, db_id, Some(txn_id), cr_id).await?;

    // 幂等：doc_status 必须 = approved/activating/approving，否则拒
    // M2-0:加 approving(M2 approve 端点直接对 approving 的 CR 调激活器,单事务 approving→activated)
    let status = cr_head
        .get("doc_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if status != "approved" && status != "activating" && status != "approving" {
        return Err(api_err(&format!(
            "CR {cr_id} 状态「{status}」不可激活（须 approved/approving）"
        )));
    }

    let doc_type = cr_head
        .get("doc_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cr_type = cr_head
        .get("cr_type")
        .and_then(|v| v.as_str())
        .unwrap_or("create");

    // 2. 读激活映射
    let cfg = activation_store::find_by_doc_type(mm, db_id, Some(txn_id), doc_type, cr_type)
        .await?
        .ok_or_else(|| {
            api_err(&format!(
                "无激活映射: doc_type={doc_type} cr_type={cr_type}"
            ))
        })?;

    // 3. 头处理（create / update）
    let (record_id, new_version) = match cr_type {
        "create" => {
            let code = codegen.generate(&cfg.target_dict, cfg.code_rule_code.as_deref());
            let plan = plan_create(&cfg, &cr_head, &code);
            let id = dct_accessor::insert_header(
                mm,
                db_id,
                txn_id,
                &cfg.target_table,
                &plan.header_row,
                operated_by,
            )
            .await?;
            (id, 1_i64)
        }
        "update" => {
            let target_id = cr_head
                .get("target_record_id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| api_err(&format!("变更 CR {cr_id} 缺 target_record_id")))?;
            // 乐观锁：读当前 version 作 CAS 期望值
            let current_v =
                dct_accessor::get_version(mm, db_id, Some(txn_id), &cfg.target_table, target_id)
                    .await?
                    .ok_or_else(|| api_err(&format!("目标记录 {target_id} 不存在")))?;
            let field_deltas = cr_head
                .get("field_deltas")
                .cloned()
                .unwrap_or(Value::Null);
            let plan = plan_update(&cfg, &cr_head, &field_deltas, current_v);
            // CAS：WHERE id=? AND published_version=current_v；0 行=版本冲突
            let n = dct_accessor::update_header(
                mm,
                db_id,
                txn_id,
                &cfg.target_table,
                target_id,
                &plan.header_row,
                current_v,
            )
            .await?;
            if n == 0 {
                return Err(api_err(&format!(
                    "乐观锁冲突:记录 {target_id} 版本已变(期望 v{current_v}),CR {cr_id} 需重审"
                )));
            }
            (target_id, current_v + 1)
        }
        other => {
            return Err(api_err(&format!(
                "cr_type={other} 暂不支持(M1 仅 create/update)"
            )))
        }
    };

    // 4. 明细处理（plan_lines 已过滤 delete 行，这里只 insert）
    let line_rows = plan_lines(&cfg, &cr_lines, record_id);
    for (target_table, _parent_field, row) in line_rows {
        dct_accessor::insert_header(mm, db_id, txn_id, &target_table, &row, operated_by).await?;
    }

    // 5. 记审计
    md_accessor::write_audit(
        mm,
        db_id,
        txn_id,
        &cfg.target_dict,
        record_id,
        new_version,
        cr_type,
        Some(cr_id),
        None,      // field
        None,      // old_value
        None,      // new_value
        operated_by,
    )
    .await?;

    // 6. 发事件
    let payload = serde_json::json!({
        "cr_id": cr_id,
        "record_id": record_id,
        "version": new_version,
        "cr_type": cr_type
    });
    md_accessor::write_event(
        mm,
        db_id,
        txn_id,
        &cfg.target_dict,
        record_id,
        if cr_type == "create" {
            "created"
        } else {
            "updated"
        },
        payload,
    )
    .await?;

    // 7. CR 归档
    md_accessor::set_cr_status(mm, db_id, Some(txn_id), cr_id, "activated").await?;

    Ok(record_id)
}

// ═══════════════════════════════════════════════════════════════════════════
// M3 · 合并 / 还原（merge 分支，统一经 dct_accessor 闸口，单事务原子）
// ═══════════════════════════════════════════════════════════════════════════

/// 合并：master + victims → 单事务（审查修订版流程）。
///
/// ① lock_record(master, FOR UPDATE) 串行化交叉 merge
/// ② 读 master/victims（须 published）
/// ③ survive 逐字段（多 victim 顺序累积到 master）
/// ④ victim set_lifecycle(published→merged, CAS+version+1)，n=0 冲突报错
/// ⑤ reparent_lines（各明细表 victim→master，先快照行 id 供 unmerge）
/// ⑥ update_header(master, 存活值+version+1, CAS)
/// ⑦ deactivate_xref(victim)
/// ⑧ write_audit(merge) ⑨ write_event(merged, payload 带追溯)
/// ⑩ update_match_group(status, survivorship_log{fields+reparented}, master_id)
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
    line_tables: &[(String, String)], // (明细表, parent_field)
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
            guard.commit().await.map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(id)
        }
        Err(e) => {
            tracing::error!(target: "cmx_mdm::merge", master_id, error = %e, "合并失败,事务已回滚");
            Err(e)
        }
    }
}

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

/// unmerge：反向还原（victim merged→published、明细指回、xref active、group=unmerged）。
///
/// master 存活值不回退（仅留痕），避免数据抖动。双 unmerge 第二笔 CAS n=0 报错。
#[allow(clippy::too_many_arguments)]
pub async fn unmerge(
    mm: &DatabaseManager,
    db_id: &str,
    dict_code: &str,
    head_table: &str,
    master_id: i64,
    victim_id: i64,
    line_tables: &[(String, String)],
    operated_by: i64,
    match_group_id: i64,
) -> Result<(), cmx_api_types::Error> {
    let txn_ctx = mm.get_transaction_context();
    let guard = txn_ctx
        .begin_with_guard(db_id)
        .await
        .map_err(|e| api_err(&format!("开事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();

    let result = unmerge_inner(
        mm, db_id, &txn_id, dict_code, head_table, master_id, victim_id,
        line_tables, operated_by, match_group_id,
    )
    .await;

    match result {
        Ok(()) => {
            guard.commit().await.map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(())
        }
        Err(e) => {
            tracing::error!(target: "cmx_mdm::merge", master_id, victim_id, error = %e, "还原失败,事务已回滚");
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn unmerge_inner(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    dict_code: &str,
    head_table: &str,
    master_id: i64,
    victim_id: i64,
    line_tables: &[(String, String)],
    operated_by: i64,
    match_group_id: i64,
) -> Result<(), cmx_api_types::Error> {
    // victim merged→published（CAS，双 unmerge 拦截）
    let n = dct_accessor::set_lifecycle(mm, db_id, txn_id, head_table, victim_id, "merged", "published")
        .await?;
    if n == 0 {
        return Err(api_err(&format!("victim {victim_id} 非 merged（双 unmerge 拦截）")));
    }

    // 读 group 的 survivorship_log.reparented，按 id 指回 victim
    // （JSONB 列 to_json_value 为转义字符串，需 parse）
    let group = match_store::get_match_group(mm, db_id, match_group_id).await?;
    let slog_raw = group.as_ref().and_then(|g| g.get("survivorship_log")).cloned();
    let slog = match slog_raw {
        Some(Value::String(s)) => serde_json::from_str::<Value>(&s).unwrap_or(Value::Null),
        Some(v) => v,
        None => Value::Null,
    };
    let reparented = slog
        .get("reparented")
        .cloned()
        .unwrap_or(json!({}));
    for (table, _parent_field) in line_tables {
        let ids: Vec<i64> = reparented
            .get(table)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if !ids.is_empty() {
            // parent_field 从 line_tables 取
            let pf = line_tables.iter().find(|(t, _)| t == table).map(|(_, p)| p.as_str()).unwrap_or("");
            dct_accessor::reparent_lines_by_ids(mm, db_id, txn_id, table, pf, &ids, victim_id)
                .await?;
        }
    }

    // xref active
    match_store::activate_xref(mm, db_id, Some(txn_id), dict_code, victim_id).await?;

    // 审计 + group=unmerged
    md_accessor::write_audit(
        mm, db_id, txn_id, dict_code, master_id, 0, "unmerge",
        Some(victim_id), None, None, None, operated_by,
    )
    .await?;
    match_store::update_match_group(mm, db_id, Some(txn_id), match_group_id, "unmerged", None, None)
        .await?;

    Ok(())
}

/// 取记录 lifecycle_status 字符串。
fn lifecycle_of(r: &cmx_mdm_model::match_algo::MatchRecord) -> &str {
    r.fields.get("lifecycle_status").and_then(|v| v.as_str()).unwrap_or("")
}

/// 用累积 row 构造临时 MatchRecord（供下一轮 survive 作 master）。
fn master_record(row: &serde_json::Map<String, Value>) -> cmx_mdm_model::match_algo::MatchRecord {
    let id = row.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    cmx_mdm_model::match_algo::MatchRecord { id, fields: row.clone() }
}
