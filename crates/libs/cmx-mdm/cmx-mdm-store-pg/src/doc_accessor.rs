//! 读 CR 单据（cv_mdm_apply 头 + cv_mdm_apply_line 行）。
//!
//! 自己拼 SQL（不复用 DocLoader：激活器只需按 id 读头 + 按 upper_id 读行，无需整树装载）。
//! 转换走 `Row::to_json_value(schema)`（iam 范例 A）；JSONB 列（field_deltas/ext_attrs/line_payload）
//! 在 DB 里是 text，需按需 parse 成对象。

use cmx_core::dv;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::DatabaseManager;
use serde_json::{Map, Value};

use crate::error::{api_err, parse_jsonb_field};

/// 读 CR 头（cv_mdm_apply 一行，按 id）。返回字段名→值。
///
/// JSONB 列（field_deltas/ext_attrs）若为合法 JSON 文本，parse 成对象；否则保持原样。
pub async fn load_cr_head(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    cr_id: i64,
) -> Result<Map<String, Value>, cmx_api_types::Error> {
    let sql = r#"SELECT id, doc_no, doc_type, target_dict_code, target_record_id, source_cr_id,
                        cr_type, effective_date, name, tax_no, credit_code, short_name,
                        ext_attrs, field_deltas, doc_status, create_by, create_time
                 FROM cv_mdm_apply WHERE id = $1"#;
    let ds = mm
        .query_sql_with_datavalues(db_id, txn_id, sql, dv![DataValue::Int(cr_id)], "mdm_cr_head")
        .await
        .map_err(|e| api_err(&format!("读 CR 头失败: {e}")))?;
    let Some(row) = ds.rows.first() else {
        return Err(api_err(&format!("CR {cr_id} 不存在")));
    };
    let mut map = row.to_json_value(ds.schema.as_ref());
    parse_jsonb_field(&mut map, "field_deltas");
    parse_jsonb_field(&mut map, "ext_attrs");
    map.as_object_mut()
        .map(std::mem::take)
        .ok_or_else(|| api_err(&format!("CR {cr_id} 头非对象")))
}

/// 读 CR 明细行（cv_mdm_apply_line，按 upper_id）。
pub async fn load_cr_lines(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    cr_id: i64,
) -> Result<Vec<Value>, cmx_api_types::Error> {
    let sql = r#"SELECT line_type, line_action, line_payload
                 FROM cv_mdm_apply_line WHERE upper_id = $1 ORDER BY line_no"#;
    let ds = mm
        .query_sql_with_datavalues(db_id, txn_id, sql, dv![DataValue::Int(cr_id)], "mdm_cr_lines")
        .await
        .map_err(|e| api_err(&format!("读 CR 行失败: {e}")))?;
    let schema = ds.schema.as_ref();
    let mut out = Vec::with_capacity(ds.rows.len());
    for row in ds.rows.iter() {
        let mut v = row.to_json_value(schema);
        parse_jsonb_field(&mut v, "line_payload");
        out.push(v);
    }
    Ok(out)
}

