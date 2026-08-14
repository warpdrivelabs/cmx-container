//! 激活器主流程：单事务编排七步（V3.1 单事务原子 + 乐观锁并发控制）。
//!
//! 读 CR → 读映射 → 头处理（create/update）→ 明细处理 → 记审计 → 发事件 → CR 归档。
//! 全程在一个 DB 事务内，任一步失败 guard drop 自动回滚，无中间态。

use cmx_database_pg::DatabaseManager;
use cmx_dct_store_pg::{DctQuery, Txn, UpsertOutcome, dict_upsert};
use cmx_mdm_model::activation::{plan_create, plan_lines, plan_update};
use cmx_mdm_model::codegen::CodeGenerator;
use serde_json::{json, Value};

use crate::{
    activation_store, dct_accessor, doc_accessor, error::api_err, md_accessor,
};

/// 激活一份 CR（审批通过后调用）。
///
/// 两条触发路径统一入口：① 审批型 CR（M2 的 ServiceTask/JavaDelegate）；
/// ② 手动 / 内部 CR（API 端点直接调）。
///
/// # Arguments
///
/// * `mm` - 数据库管理器（取全局单例）。
/// * `db_id` - 数据源 id（业务库）。
/// * `cr_id` - 待激活的 CR id。
/// * `operated_by` - 操作人 id（从 handler 的 SVRContext.user_id 解析）。
/// * `codegen` - 编码生成器（M1 传 RandomCodeGenerator stub）。
///
/// # Returns
///
/// 新建 / 变更的主数据记录 id。
///
/// # Errors
///
/// CR 状态不可激活、无激活映射、乐观锁冲突、审计 / 事件写入失败等任一步出错时返回错误
/// （事务已回滚，`cm_*` 无中间态）。
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

/// 激活器七步编排（事务内执行，由 [`activate`] 开事务后调用）。
///
/// 七步：① 读 CR 头行 → ② 读激活映射 → ③ 头处理（create/update 分支）→
/// ④ 明细处理 → ⑤ 记审计 → ⑥ 发事件 → ⑦ CR 归档。
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
            // 占位 code（编码引擎未配置/失败时的兜底，保证 NOT NULL）
            let fallback_code = codegen.generate(&cfg.target_dict, cfg.code_rule_code.as_deref());
            let mut plan = plan_create(&cfg, &cr_head, &fallback_code);
            // 按激活映射配的 code_rule_code 走编码引擎铸号（与 dct/doc 保存同源）。
            // 铸号用独立连接（txn_id=None——CodeEngine 跨 async trait 边界，主事务 holder 不可用）。
            if let Some(real_code) = mint_activation_code(
                cfg.code_rule_code.as_deref(),
                &cfg.target_table,
                &plan.header_row,
                db_id,
            )
            .await
            {
                plan.header_row.insert("code".into(), Value::String(real_code));
            }
            // 激活器铸 id 塞进 header_row——dct upsert 对 Number 类型不重铸（is_temp_id=false），
            // 直接用此 id 落库。这样激活器持 id 供后续（明细 upper_id / 审计 record_id）。
            let id = cmx_utils::next_pk_id();
            plan.header_row.insert("id".into(), Value::Number(id.into()));
            // dict_upsert 一步到位（内部 resolve_dict 拿 DictView 做列校验 + backfill），
            // 纳入激活器主事务（Txn::External）。
            match dict_upsert(
                &DctQuery::by_code(&cfg.target_dict),
                Value::Object(plan.header_row),
                db_id,
                Txn::External(txn_id.to_string()),
            )
            .await?
            {
                UpsertOutcome::Ok { .. } => {}
                UpsertOutcome::Invalid(violations) => {
                    return Err(api_err(&format!(
                        "激活落库列校验未通过：{}",
                        violations
                            .iter()
                            .map(|v| format!("{}({})", v.column.as_deref().unwrap_or("?"), v.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    )));
                }
            }
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

    // 4. 明细处理（diff 方案：按明细类型比对 cm_* 现有 vs CR，算出 软删/update/insert）
    let lines = plan_lines(&cfg, &cr_lines, record_id);
    use std::collections::HashSet;
    for lm in &cfg.line_mappings {
        // cm_* 现有 published 明细 id（= 该主数据下、此明细类型的全部现存明细）
        let existing: HashSet<i64> = dct_accessor::select_line_keys(
            mm, db_id, txn_id, &lm.target_table, &lm.parent_field, record_id, &[],
        )
        .await?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
        // CR 要 update 的明细（有 line_target_id 的行）
        let cr_upd: Vec<_> = lines
            .updates
            .iter()
            .filter(|(t, _, _)| t == &lm.target_table)
            .map(|(_, id, row)| (*id, row))
            .collect();
        let cr_upd_ids: HashSet<i64> = cr_upd.iter().map(|(id, _)| *id).collect();
        // ① cm_* 有但 CR 没要的 → 软删（用户删除的明细）
        let to_delete: Vec<i64> = existing.difference(&cr_upd_ids).copied().collect();
        if !to_delete.is_empty() {
            dct_accessor::set_lifecycle_by_ids(
                mm, db_id, txn_id, &lm.target_table, &to_delete, "published", "archived",
            )
            .await?;
            tracing::info!(
                target: "cmx_mdm::activation",
                table = %lm.target_table, parent = record_id, archived = to_delete.len(),
                "明细 diff：软删 CR 未保留的旧明细"
            );
        }
        // ② CR 有 id 的 → update（id 稳定，只改业务字段）
        for (id, row) in cr_upd {
            dct_accessor::update_line(mm, db_id, txn_id, &lm.target_table, id, row).await?;
        }
    }
    // ③ CR 无 id 的 → insert（新增明细）
    for (target_table, _parent_field, row) in &lines.inserts {
        dct_accessor::insert_header(mm, db_id, txn_id, target_table, row, operated_by).await?;
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
        None, // field
        None, // old_value
        None, // new_value
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

/// 按激活映射配的 code_rule_code 走编码引擎铸号（create 分支）。
///
/// 与 dct write.rs `mint_codes_for_inserts` / doc saver `mint_codes_for_changeset` 同源：
/// 构造挂载点声明 `{mode:"auto", field:"code", ruleCode}` + target，调 `GlobalCodeMinter::mint`。
/// 铸号用独立连接（`txn_id=None`——CodeEngine 跨 async trait 边界，主事务 holder 不可用，
/// 与 dct/doc 铸号一致）。引擎未注入、未配 ruleCode 或铸号失败时返回 None，由调用方回退占位 code。
async fn mint_activation_code(
    rule_code: Option<&str>,
    target_table: &str,
    header_row: &serde_json::Map<String, Value>,
    db_id: &str,
) -> Option<String> {
    // 编码引擎未注入 → 跳过（现状零影响）
    let minter = cmx_traits::code::GlobalCodeMinter::get()?;
    // 激活映射未配 ruleCode → 跳过（回退 RandomCodeGenerator 占位）
    let rule_code = rule_code?;
    let code_rule = json!({ "mode": "auto", "field": "code", "ruleCode": rule_code });
    let target = json!({ "kind": "dct", "code": target_table, "field": "code" });
    // 行字段作 attrs（供 ref 段取字段值 + condition 求值）
    let attrs = Value::Object(header_row.clone());
    match minter.mint(&code_rule, &target, &attrs, db_id, None).await {
        Ok(code) => Some(code),
        Err(e) => {
            tracing::warn!(
                target: "cmx_mdm::activation",
                rule_code = rule_code, table = target_table, error = %e,
                "编码引擎铸号失败，回退占位 code（不阻断激活）"
            );
            None
        }
    }
}
