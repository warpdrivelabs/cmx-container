//! CR 变更请求:状态校验 + 列表/详情/克隆/新建/作废。
//!
//! 状态转移强校验(封禁非法跳转)。approve 由 api 层直接调激活器(M2-0 方案 A:
//! 激活器接受 approving,单事务 approving→activated)。
//!
//! 状态机:
//!   draft ──submit──→ approving ──approve(激活器)──→ activated(归档)
//!                          └──reject──→ rejected(归档)
//!   rejected ──clone-revise──→ draft(新 CR, source_cr_id 指向旧)
//!   draft ──abort──→ aborted(作废)

use cmx_core::dv;
use cmx_core::model::cell::{DataValue, SqlTypeMarker};
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

/// CR 列表。可选过滤 docStatus。返回精简字段。
pub async fn list_cr(
    mm: &DatabaseManager,
    db_id: &str,
    doc_status: Option<&str>,
) -> Result<Vec<Value>, cmx_api_types::Error> {
    let (sql, params): (String, Vec<DataValue>) = if let Some(st) = doc_status {
        (
            "SELECT id, doc_no, name, cr_type, doc_status, create_time \
             FROM cv_mdm_apply WHERE doc_status = $1 AND delete_flag = 0 \
             ORDER BY create_time DESC"
                .to_string(),
            vec![DataValue::String(st.into())],
        )
    } else {
        (
            "SELECT id, doc_no, name, cr_type, doc_status, create_time \
             FROM cv_mdm_apply WHERE delete_flag = 0 \
             ORDER BY create_time DESC"
                .to_string(),
            vec![],
        )
    };
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_cr_list")
        .await
        .map_err(|e| api_err(&format!("查 CR 列表失败: {e}")))?;
    let schema = ds.schema.as_ref();
    Ok(ds.rows.iter().map(|r| r.to_json_value(schema)).collect())
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

/// 新建 draft CR(录入台用)。头+行在一个事务内(原子)。返回新 CR id。
pub async fn create_cr(
    mm: &DatabaseManager,
    db_id: &str,
    head: &Value,
    lines: &[Value],
    operated_by: i64,
) -> Result<i64, cmx_api_types::Error> {
    let cr_id = cmx_utils::next_pk_id();
    let h = head.as_object().ok_or_else(|| api_err("CR head 非对象"))?;

    // 开事务(N2:头+行原子)
    let txn_ctx = mm.get_transaction_context();
    let guard = txn_ctx
        .begin_with_guard(db_id)
        .await
        .map_err(|e| api_err(&format!("开事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();

    let result = create_cr_inner(mm, db_id, &txn_id, cr_id, h, lines, operated_by).await;
    match result {
        Ok(()) => {
            guard
                .commit()
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(cr_id)
        }
        Err(e) => {
            tracing::error!(target: "cmx_mdm::cr", cr_id, error = %e, "新建 CR 失败,事务已回滚");
            Err(e)
        }
    }
}

async fn create_cr_inner(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    cr_id: i64,
    h: &serde_json::Map<String, Value>,
    lines: &[Value],
    operated_by: i64,
) -> Result<(), cmx_api_types::Error> {
    // 头:28 列(非 source_* 子集),doc_status 强制 draft
    let sql = r#"INSERT INTO cv_mdm_apply
        (id, upper_id, line_no, doc_no, doc_type_id, doc_type, target_dict_code, target_record_id,
         source_cr_id, cr_type, effective_date, name, tax_no, credit_code, short_name, ext_attrs,
         field_deltas, doc_status, business_status, entity_id, doc_date, attach_count, remark,
         create_by, create_time, update_by, update_time, delete_flag)
      VALUES ($1, NULL, 1, $2, 1, $3, $4, $5, NULL, $6, NULL, $7, $8, $9, $10, NULL, NULL,
         'draft', 'normal', 1, CURRENT_DATE, 0, NULL, $11, now(), $11, now(), 0)"#;
    mm.execute_sql_with_datavalues(
        db_id,
        Some(txn_id),
        sql,
        dv![
            DataValue::Int(cr_id),
            DataValue::String(format!("CR-{cr_id}")),
            DataValue::String(str_val(h, "doc_type", "mdm_supplier_apply").into()),
            DataValue::String(str_val(h, "target_dict_code", "supplier").into()),
            // 空 target_record_id 必须 NullTyped(Int)：裸 Null 绑成 VARCHAR NULL，BIGINT 列拒收（executor/mod.rs:280）
            h.get("target_record_id")
                .and_then(|v| v.as_i64())
                .map(DataValue::Int)
                .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Int)),
            DataValue::String(str_val(h, "cr_type", "create").into()),
            DataValue::String(str_val(h, "name", "").into()),
            DataValue::String(str_val(h, "tax_no", "").into()),
            DataValue::String(str_val(h, "credit_code", "").into()),
            DataValue::String(str_val(h, "short_name", "").into()),
            DataValue::Int(operated_by),
        ],
    )
    .await
    .map_err(|e| api_err_db(&format!("新建 CR 头失败: {e}")))?;

    // 行:逐行铸号
    for (i, line) in lines.iter().enumerate() {
        let line_id = cmx_utils::next_pk_id();
        let lo = line.as_object().ok_or_else(|| api_err("CR line 非对象"))?;
        let sql_l = r#"INSERT INTO cv_mdm_apply_line
            (id, upper_id, line_no, line_type, line_action, line_payload,
             create_by, create_time, update_by, update_time, delete_flag)
          VALUES ($1, $2, $3, $4, $5, $6, $7, now(), $7, now(), 0)"#;
        let payload = lo.get("line_payload").map(|v| v.to_string()).unwrap_or_else(|| "{}".into());
        mm.execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            sql_l,
            dv![
                DataValue::Int(line_id),
                DataValue::Int(cr_id),
                DataValue::Int((i as i64) + 1),
                DataValue::String(str_val(lo, "line_type", "bank_account").into()),
                DataValue::String(str_val(lo, "line_action", "insert").into()),
                DataValue::Json(payload),
                DataValue::Int(operated_by),
            ],
        )
        .await
        .map_err(|e| api_err_db(&format!("新建 CR 行失败: {e}")))?;
    }
    Ok(())
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

async fn clone_revise_inner(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    new_id: i64,
    src_cr_id: i64,
    operated_by: i64,
) -> Result<(), cmx_api_types::Error> {
    // 头:INSERT...SELECT 复制(28 列严格对齐,含 doc_type_id)
    let sql = r#"INSERT INTO cv_mdm_apply
        (id, upper_id, line_no, doc_no, doc_type_id, doc_type, target_dict_code, target_record_id,
         source_cr_id, cr_type, effective_date, name, tax_no, credit_code, short_name, ext_attrs,
         field_deltas, doc_status, business_status, entity_id, doc_date, attach_count, remark,
         create_by, create_time, update_by, update_time, delete_flag)
      SELECT $1, upper_id, line_no, $2, doc_type_id, doc_type, target_dict_code, target_record_id,
         $3, cr_type, effective_date, name, tax_no, credit_code, short_name, ext_attrs,
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
            DataValue::String(format!("CR-CLONE-{new_id}")),
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
            (id, upper_id, line_no, line_type, line_action, line_payload,
             create_by, create_time, update_by, update_time, delete_flag)
          VALUES ($1, $2, $3, $4, $5, $6, $7, now(), $7, now(), 0)"#;
        let payload = lo.get("line_payload").map(|v| v.to_string()).unwrap_or_else(|| "{}".into());
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

/// 从 JSON Map 取字符串字段(带默认值)。
fn str_val<'a>(m: &'a serde_json::Map<String, Value>, key: &str, default: &'a str) -> &'a str {
    m.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}
