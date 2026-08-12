//! CR 变更请求:状态校验 + 列表/详情/克隆/新建/作废。
//!
//! 状态转移强校验(封禁非法跳转)。approve 由 api 层直接调激活器(M2-0 方案 A:
//! 激活器接受 approving,单事务 approving→activated)。
//!
//! 状态机:
//!   draft ──submit──→ approving ──approve(激活器)──→ activated(归档)
//!                          └──reject──→ rejected(归档)
//!   rejected ──submit──→ approving（驳回后可直接编辑重新提交，无需 clone 新 CR）
//!   rejected ──clone-revise──→ draft(新 CR, source_cr_id 指向旧；可选，一般直接 resubmit)
//!   draft ──abort──→ aborted(作废)

use cmx_core::dv;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::DatabaseManager;
use serde_json::{json, Value};

use crate::error::{api_err, api_err_db};
use crate::md_accessor::set_cr_status;

/// 校验 CR 当前状态,返回头 Map。状态不符报错。
pub async fn check_status(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    cr_id: i64,
    expect: &str,
) -> Result<serde_json::Map<String, Value>, cmx_api_types::Error> {
    let head = crate::doc_accessor::load_cr_head(mm, db_id, txn_id, cr_id).await?;
    let cur = head.get("doc_status").and_then(|v| v.as_str()).unwrap_or("");
    if cur != expect {
        return Err(api_err(&format!(
            "CR {cr_id} 状态「{cur}」不符(须 {expect})"
        )));
    }
    Ok(head)
}

/// 校验 CR 当前状态在允许集合内，返回头 Map。状态不符报错。
/// 用于跨状态操作（如 submit 允许 draft / rejected）。
pub async fn check_status_in(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    cr_id: i64,
    expect: &[&str],
) -> Result<serde_json::Map<String, Value>, cmx_api_types::Error> {
    let head = crate::doc_accessor::load_cr_head(mm, db_id, txn_id, cr_id).await?;
    let cur = head.get("doc_status").and_then(|v| v.as_str()).unwrap_or("");
    if !expect.contains(&cur) {
        return Err(api_err(&format!(
            "CR {cr_id} 状态「{cur}」不符(须 {})",
            expect.join(" 或 ")
        )));
    }
    Ok(head)
}

/// CR 列表（分页，返回 total）。可选过滤 docStatus。
/// 返回 (list, total)。page 从 1 起；page_size<=0 时默认 20。
pub async fn list_cr(
    mm: &DatabaseManager,
    db_id: &str,
    doc_status: Option<&str>,
    page: i64,
    page_size: i64,
    with_payload: bool,
) -> Result<(Vec<Value>, i64), cmx_api_types::Error> {
    let (where_sql, mut params): (String, Vec<DataValue>) = if let Some(st) = doc_status {
        ("doc_status = $1 AND delete_flag = 0".to_string(), vec![DataValue::String(st.into())])
    } else {
        ("delete_flag = 0".to_string(), vec![])
    };
    // 总数
    let cnt_sql = format!("SELECT COUNT(*) AS c FROM cv_mdm_apply WHERE {where_sql}");
    let cds = mm
        .query_sql_with_datavalues(db_id, None, &cnt_sql, params.clone(), "mdm_cr_count")
        .await
        .map_err(|e| api_err(&format!("查 CR 总数失败: {e}")))?;
    let total = cds.rows.first()
        .and_then(|r| r.get_by_name_as::<i64>(cds.schema.as_ref(), "c"))
        .unwrap_or(0);
    // 分页
    let ps = if page_size > 0 { page_size } else { 20 };
    let pg = if page > 0 { page } else { 1 };
    let off = (pg - 1) * ps;
    let n = params.len() as i64;
    params.push(DataValue::Int(ps));
    params.push(DataValue::Int(off));
    let payload_col = if with_payload { ", payload" } else { "" };
    let sql = format!(
        "SELECT id, doc_no, subject_name, cr_type, doc_status, create_time{payload_col} \
         FROM cv_mdm_apply WHERE {where_sql} ORDER BY create_time DESC \
         LIMIT ${} OFFSET ${}", n + 1, n + 2);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_cr_list")
        .await
        .map_err(|e| api_err(&format!("查 CR 列表失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok((ds.rows.iter().map(|r| r.to_json_value(schema)).collect(), total))
}

/// CR 详情(头+行)。
pub async fn get_cr_detail(
    mm: &DatabaseManager,
    db_id: &str,
    cr_id: i64,
) -> Result<Value, cmx_api_types::Error> {
    let head = crate::doc_accessor::load_cr_head(mm, db_id, None, cr_id).await?;
    let lines = crate::doc_accessor::load_cr_lines(mm, db_id, None, cr_id).await?;
    Ok(json!({ "head": head, "lines": lines }))
}

/// 克隆 CR(驳回复活):基于 rejected CR 复制新 draft。行逐行铸号。返回新 CR id。
pub async fn clone_revise(
    mm: &DatabaseManager,
    db_id: &str,
    src_cr_id: i64,
    operated_by: i64,
) -> Result<i64, cmx_api_types::Error> {
    // 校验旧 CR 须 rejected(事务外只读)
    check_status(mm, db_id, None, src_cr_id, "rejected").await?;
    let new_id = cmx_utils::next_pk_id();

    // 开事务(N2:头+行原子)
    let txn_ctx = mm.get_transaction_context();
    let guard = txn_ctx
        .begin_with_guard(db_id)
        .await
        .map_err(|e| api_err(&format!("开事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();

    let result = clone_revise_inner(mm, db_id, &txn_id, new_id, src_cr_id, operated_by).await;
    match result {
        Ok(()) => {
            guard
                .commit()
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(new_id)
        }
        Err(e) => {
            tracing::error!(target: "cmx_mdm::cr", new_id, src_cr_id, error = %e, "克隆 CR 失败,事务已回滚");
            Err(e)
        }
    }
}

/// 克隆复活编排（事务内执行，由 [`clone_revise`] 开事务后调用）。
///
/// 三步：① 铸新 doc_no（走 cmx-code MDM_GYS 规则）→ ② INSERT...SELECT 复制头表
/// （26 列，`source_cr_id` 指向旧 CR，`doc_status='draft'`）→ ③ 读旧行逐行铸号插入
/// （`upper_id=new_id`）。
async fn clone_revise_inner(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    new_id: i64,
    src_cr_id: i64,
    operated_by: i64,
) -> Result<(), cmx_api_types::Error> {
    // 铸新 doc_no：走 cmx-code（MDM_GYS 规则），与新建 CR 走标准 /doc/save 的铸号口径一致。
    // minter 未注入 → 显式报错（main.rs 已无条件注入，None 仅在注入失败时触发，不应静默兜底）。
    // 铸号在事务内做（传 txn_id），反查 max 的 SELECT 与本事务可见性一致。
    let doc_no = mint_cr_doc_no(db_id, Some(txn_id)).await?;

    // 头:INSERT...SELECT 复制(26 列,payload 化后:删 5 旧业务列,加 subject_name/subject_code/payload 3 列)
    let sql = r#"INSERT INTO cv_mdm_apply
        (id, upper_id, line_no, doc_no, doc_type_id, doc_type, target_dict_code, target_record_id,
         source_cr_id, cr_type, effective_date, subject_name, subject_code, payload,
         field_deltas, doc_status, business_status, entity_id, doc_date, attach_count, remark,
         create_by, create_time, update_by, update_time, delete_flag)
      SELECT $1, upper_id, line_no, $2, doc_type_id, doc_type, target_dict_code, target_record_id,
         $3, cr_type, effective_date, subject_name, subject_code, payload,
         field_deltas, 'draft', business_status, entity_id, CURRENT_DATE,
         COALESCE(attach_count, 0), remark,
         $4, now(), $4, now(), delete_flag
      FROM cv_mdm_apply WHERE id = $5"#;
    mm.execute_sql_with_datavalues(
        db_id,
        Some(txn_id),
        sql,
        dv![
            DataValue::Int(new_id),
            DataValue::String(doc_no),
            DataValue::Int(src_cr_id), // source_cr_id 指向旧
            DataValue::Int(operated_by),
            DataValue::Int(src_cr_id),
        ],
    )
    .await
    .map_err(|e| api_err_db(&format!("克隆 CR 头失败: {e}")))?;

    // 行:读旧行 → 逐行铸号插入(upper_id=new_id)
    let old_lines = crate::doc_accessor::load_cr_lines(mm, db_id, Some(txn_id), src_cr_id).await?;
    for (i, line) in old_lines.iter().enumerate() {
        let line_id = cmx_utils::next_pk_id();
        let lo = line.as_object().ok_or_else(|| api_err("旧 CR line 非对象"))?;
        let sql_l = r#"INSERT INTO cv_mdm_apply_line
            (id, upper_id, line_no, line_type, line_action, line_payload, line_target_id, line_deltas,
             create_by, create_time, update_by, update_time, delete_flag)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), $9, now(), 0)"#;
        let payload = lo.get("line_payload").map(|v| v.to_string()).unwrap_or_else(|| "{}".into());
        let line_target_id = lo
            .get("line_target_id")
            .and_then(|v| v.as_i64())
            .map(DataValue::Int)
            .unwrap_or(DataValue::Null);
        let line_deltas = lo.get("line_deltas").map(|v| v.to_string()).unwrap_or_else(|| "null".into());
        mm.execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            sql_l,
            dv![
                DataValue::Int(line_id),
                DataValue::Int(new_id),
                DataValue::Int((i as i64) + 1),
                DataValue::String(
                    lo.get("line_type").and_then(|v| v.as_str()).unwrap_or("").into()
                ),
                DataValue::String(
                    lo.get("line_action").and_then(|v| v.as_str()).unwrap_or("insert").into()
                ),
                DataValue::Json(payload),
                line_target_id,
                DataValue::Json(line_deltas),
                DataValue::Int(operated_by),
            ],
        )
        .await
        .map_err(|e| api_err_db(&format!("克隆 CR 行失败: {e}")))?;
    }
    Ok(())
}

/// 作废 draft CR。draft → aborted。
pub async fn abort_cr(
    mm: &DatabaseManager,
    db_id: &str,
    cr_id: i64,
) -> Result<u64, cmx_api_types::Error> {
    check_status(mm, db_id, None, cr_id, "draft").await?;
    set_cr_status(mm, db_id, None, cr_id, "aborted").await
}

/// 走 cmx-code 为 CR 铸 doc_no（MDM_GYS 规则）。
///
/// clone_revise 用：与新建 CR 走标准 `/doc/save` 的铸号口径保持一致（同一规则 MDM_GYS）。
/// minter 未注入（`GlobalCodeMinter::get()` 返回 None）时**显式报错**——main.rs 已无条件注入
/// CodeEngine，None 仅在注入失败时触发，静默回退到旧模板（`CR-CLONE-{id}`）会让 doc_no 格式
/// 与正常 CR 不一致，属隐藏 bug，应显式暴露。
async fn mint_cr_doc_no(db_id: &str, txn_id: Option<&str>) -> Result<String, cmx_api_types::Error> {
    let minter = cmx_traits::code::GlobalCodeMinter::get()
        .ok_or_else(|| api_err("编码引擎未注入，无法铸 CR 单据号"))?;
    let code_rule = serde_json::json!({ "ruleCode": "MDM_GYS" });
    let target = serde_json::json!({ "kind": "doc", "code": "cv_mdm_apply", "field": "doc_no" });
    let attrs = serde_json::json!({});
    minter
        .mint(&code_rule, &target, &attrs, db_id, txn_id)
        .await
        .map_err(|e| api_err(&format!("铸 CR 单据号失败: {e}")))
}
