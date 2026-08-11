//! cmx_mdm_activation 激活映射配置读写。
//!
//! - [`find_by_doc_type`]：激活器主用（按 source_doc_type + cr_type 取映射）。
//! - [`list`] / [`upsert`]：映射配置器 UI 用。

use cmx_core::dv;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::DatabaseManager;
use cmx_mdm_model::activation::ActivationConfig;
use cmx_utils::snowflake_id_str;
use serde_json::Value;

use crate::error::{api_err, api_err_db, parse_jsonb_field};

/// 按来源单据类型 + cr_type 查激活映射（激活器主用）。
pub async fn find_by_doc_type(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: Option<&str>,
    source_doc_type: &str,
    cr_type: &str,
) -> Result<Option<ActivationConfig>, cmx_api_types::Error> {
    let sql = r#"SELECT activation_code, source_doc_type, cr_type, target_dict, target_table,
                        header_mapping, line_mappings, code_rule_code, subject_name_field, subject_code_field
                 FROM cmx_mdm_activation
                 WHERE source_doc_type = $1 AND cr_type = $2 AND is_active = TRUE
                 LIMIT 1"#;
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            txn_id,
            sql,
            dv![
                DataValue::String(source_doc_type.into()),
                DataValue::String(cr_type.into()),
            ],
            "mdm_act_find",
        )
        .await
        .map_err(|e| api_err(&format!("查激活映射失败: {e}")))?;
    let Some(row) = ds.rows.first() else {
        return Ok(None);
    };
    let mut v = row.to_json_value(ds.schema.as_ref());
    // header_mapping / line_mappings 是 JSONB，DB 里是 text，需 parse
    parse_jsonb_field(&mut v, "header_mapping");
    parse_jsonb_field(&mut v, "line_mappings");
    let cfg = serde_json::from_value::<ActivationConfig>(v)
        .map_err(|e| api_err(&format!("激活映射反序列化失败: {e}")))?;
    Ok(Some(cfg))
}

/// 列表（配置器 UI）。可选过滤 sourceDocType/crType。
pub async fn list(
    mm: &DatabaseManager,
    db_id: &str,
    source_doc_type: Option<&str>,
    cr_type: Option<&str>,
) -> Result<Vec<Value>, cmx_api_types::Error> {
    // 动态拼 WHERE（参数化，防注入）
    let mut where_clauses = vec!["is_active = TRUE".to_string()];
    let mut params: Vec<DataValue> = Vec::new();
    let mut idx = 1;
    if let Some(sdt) = source_doc_type {
        where_clauses.push(format!("source_doc_type = ${idx}"));
        params.push(DataValue::String(sdt.into()));
        idx += 1;
    }
    if let Some(ct) = cr_type {
        where_clauses.push(format!("cr_type = ${idx}"));
        params.push(DataValue::String(ct.into()));
    }
    let sql = format!(
        r#"SELECT id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                  header_mapping, line_mappings, code_rule_code, subject_name_field, subject_code_field,
                  header_groups, is_active
           FROM cmx_mdm_activation WHERE {} ORDER BY sort_order_of_none(), activation_code"#,
        where_clauses.join(" AND ")
    );
    // 上面 ORDER BY sort_order_of_none() 是占位——cmx_mdm_activation 无 sort_order 列，改用 activation_code
    let sql = sql.replace("ORDER BY sort_order_of_none(), ", "ORDER BY ");
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "mdm_act_list")
        .await
        .map_err(|e| api_err_db(&format!("列表激活映射失败: {e}")))?;
    let schema = ds.schema.as_ref();
    let mut out = Vec::with_capacity(ds.rows.len());
    for row in ds.rows.iter() {
        let mut v = row.to_json_value(schema);
        parse_jsonb_field(&mut v, "header_mapping");
        parse_jsonb_field(&mut v, "line_mappings");
        parse_jsonb_field(&mut v, "header_groups");
        out.push(v);
    }
    Ok(out)
}

/// 保存（upsert by activation_code）。id 用 snowflake_id_str()。返回 activation_code。
pub async fn upsert(
    mm: &DatabaseManager,
    db_id: &str,
    cfg: &ActivationConfig,
) -> Result<String, cmx_api_types::Error> {
    let id = snowflake_id_str();
    let header_json = serde_json::to_string(&cfg.header_mapping)
        .map_err(|e| api_err(&format!("header_mapping 序列化失败: {e}")))?;
    let line_json = serde_json::to_string(&cfg.line_mappings)
        .map_err(|e| api_err(&format!("line_mappings 序列化失败: {e}")))?;
    let groups_json = serde_json::to_string(&cfg.header_groups)
        .map_err(|e| api_err(&format!("header_groups 序列化失败: {e}")))?;
    let sql = r#"INSERT INTO cmx_mdm_activation
                   (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                    header_mapping, line_mappings, code_rule_code, subject_name_field, subject_code_field,
                    header_groups, is_active)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,TRUE)
                 ON CONFLICT (activation_code) DO UPDATE SET
                   source_doc_type     = EXCLUDED.source_doc_type,
                   cr_type             = EXCLUDED.cr_type,
                   target_dict         = EXCLUDED.target_dict,
                   target_table        = EXCLUDED.target_table,
                   header_mapping      = EXCLUDED.header_mapping,
                   line_mappings       = EXCLUDED.line_mappings,
                   code_rule_code      = EXCLUDED.code_rule_code,
                   subject_name_field  = EXCLUDED.subject_name_field,
                   subject_code_field  = EXCLUDED.subject_code_field,
                   header_groups       = EXCLUDED.header_groups,
                   is_active           = TRUE,
                   updated_at          = now()"#;
    let params = dv![
        DataValue::String(id),
        DataValue::String(cfg.activation_code.clone()),
        DataValue::String(cfg.source_doc_type.clone()),
        DataValue::String(cfg.cr_type.clone()),
        DataValue::String(cfg.target_dict.clone()),
        DataValue::String(cfg.target_table.clone()),
        DataValue::Json(header_json),
        DataValue::Json(line_json),
        cfg.code_rule_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        cfg.subject_name_field.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        cfg.subject_code_field.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        DataValue::Json(groups_json),
    ];
    mm.execute_sql_with_datavalues(db_id, None, sql, params)
        .await
        .map_err(|e| api_err_db(&format!("保存激活映射失败: {e}")))?;
    Ok(cfg.activation_code.clone())
}

/// 删除（按 activation_code，硬删除）。返回影响行数。
///
/// 注意：`list` 只返回 `is_active=TRUE` 的行，故「停用」（开关置 FALSE）也会让映射从列表消失；
/// 本函数是**彻底删除**（DELETE），与停用语义不同——配置器「删除」按钮用此。
pub async fn delete_by_code(
    mm: &DatabaseManager,
    db_id: &str,
    activation_code: &str,
) -> Result<u64, cmx_api_types::Error> {
    let sql = "DELETE FROM cmx_mdm_activation WHERE activation_code = $1";
    let n = mm
        .execute_sql_with_datavalues(
            db_id,
            None,
            sql,
            dv![DataValue::String(activation_code.into())],
        )
        .await
        .map_err(|e| api_err_db(&format!("删除激活映射失败: {e}")))?;
    Ok(n)
}
