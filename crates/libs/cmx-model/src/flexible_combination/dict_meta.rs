//! 维度 dict 元数据补全（复刻处理器侧 `enrichFlexibleCombinationDictMeta` / `enrichDimensionDictMeta`）。
//!
//! resolve/rule/preview 前对 combination 的每个维度 `dim.dict` 补全 id/code/label 列、columns 默认等，
//! 并标 `dim.valueType = 'dict-select'`。dict schema 取自字典注册表（registry.json）。

use serde_json::{Value, json};

use crate::dict::schema::try_get_schema;
use crate::error::PortalResult;

/// 对整份 combination 的 dimensions 做 dict 元数据补全（返回新对象，不改原值）。
pub async fn enrich_flexible_combination_dict_meta(cfg: &Value) -> PortalResult<Value> {
    if !cfg.is_object() {
        return Ok(cfg.clone());
    }
    let mut out = cfg.clone();
    let dims = match out.get_mut("dimensions").and_then(|v| v.as_object_mut()) {
        Some(d) => d,
        None => return Ok(out),
    };
    // 收集需要补全的维度（先收集 key 避免借用冲突）
    let keys: Vec<String> = dims.keys().cloned().collect();
    for k in keys {
        let dim = dims.get(&k).cloned().unwrap_or(Value::Null);
        if !dim.is_object() {
            continue;
        }
        let dict = dim.get("dict");
        if dict.is_none() || dict == Some(&Value::Null) {
            continue;
        }
        let enriched_dict = enrich_dimension_dict_meta(dict.unwrap(), &dim).await?;
        if let Some(obj) = dims.get_mut(&k).and_then(|v| v.as_object_mut()) {
            obj.insert("dict".to_string(), enriched_dict);
            obj.insert("valueType".to_string(), json!("dict-select"));
        }
    }
    Ok(out)
}

/// 默认字典帮助列（与处理器 defaultDictHelpColumns 一致）。
fn default_dict_help_columns(id_col: &str, code_col: &str, label_col: &str) -> Value {
    let mut cols: Vec<Value> = Vec::new();
    if !code_col.is_empty() {
        cols.push(json!({ "id": code_col, "caption": "编码", "type": "text", "width": "130px", "editMode": "readonly" }));
    }
    if !label_col.is_empty() && label_col != code_col {
        cols.push(json!({ "id": label_col, "caption": "名称", "type": "text", "width": "180px", "editMode": "readonly" }));
    }
    if !id_col.is_empty() && id_col != code_col && id_col != label_col {
        cols.push(json!({ "id": id_col, "caption": "ID", "type": "text", "width": "110px", "editMode": "readonly" }));
    }
    cols.push(json!({ "id": "status", "caption": "状态", "type": "number", "width": "70px", "editMode": "readonly" }));
    Value::Array(cols)
}

/// 由字典 schema 推断 code 字段（fullTextFields 含 code → code，否则 fallback）。
fn derive_dict_code_field(
    schema: Option<&crate::dict::schema::DictSchema>,
    fallback: &str,
) -> String {
    if let Some(s) = schema
        && let Some(fields) = &s.full_text_fields
        && fields.iter().any(|f| f == "code")
    {
        return "code".to_string();
    }
    if fallback.is_empty() {
        "id".to_string()
    } else {
        fallback.to_string()
    }
}

/// 补全单个维度的 dict 元数据。
async fn enrich_dimension_dict_meta(dict_def: &Value, dim: &Value) -> PortalResult<Value> {
    // base：dict 为字符串视为 { dictId }，对象则克隆
    let base = match dict_def {
        Value::String(s) => json!({ "dictId": s }),
        Value::Object(_) => dict_def.clone(),
        _ => json!({}),
    };
    let bget = |k: &str| base.get(k).cloned();
    let dict_id = bget("dictId")
        .or_else(|| bget("dictCode"))
        .or_else(|| bget("code"))
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let Some(dict_id) = dict_id else {
        return Ok(base);
    };
    let schema = try_get_schema(&dict_id).await?;

    let id_col = bget("idCol")
        .or_else(|| bget("idField"))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| schema.as_ref().and_then(|s| s.id_field.clone()))
        .unwrap_or_else(|| "id".to_string());
    let label_col = bget("labelCol")
        .or_else(|| bget("labelField"))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| schema.as_ref().and_then(|s| s.label_field.clone()))
        .unwrap_or_else(|| "name".to_string());
    let code_col = bget("codeCol")
        .or_else(|| bget("codeField"))
        .or_else(|| bget("valueField"))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| derive_dict_code_field(schema.as_ref(), &id_col));
    let parent_col = bget("parentCol")
        .or_else(|| bget("parentField"))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| schema.as_ref().and_then(|s| s.parent_field.clone()));
    let hierarchical = bget("hierarchical")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| schema.as_ref().map(|s| s.hierarchical).unwrap_or(false));
    let dim_name = dim.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let schema_label = schema
        .as_ref()
        .and_then(|s| s.label.as_deref())
        .unwrap_or("");
    let value_field = bget("valueField")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| code_col.clone());
    let columns = bget("columns")
        .filter(|v| v.is_array())
        .unwrap_or_else(|| default_dict_help_columns(&id_col, &code_col, &label_col));
    let dict_title = bget("dictTitle")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            format!(
                "选择{}",
                if !dim_name.is_empty() {
                    dim_name
                } else if !schema_label.is_empty() {
                    schema_label
                } else {
                    &dict_id
                }
            )
        });

    Ok(json!({
        "dictId": dict_id,
        "dictCode": dict_id,
        "idCol": id_col,
        "codeCol": code_col,
        "labelCol": label_col,
        "parentCol": parent_col,
        "hierarchical": hierarchical,
        "helpLayout": bget("helpLayout").and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "grid".to_string()),
        "valueField": value_field,
        "displayMode": bget("displayMode").and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "code-label".to_string()),
        "dictTitle": dict_title,
        "filters": bget("filters").or_else(|| bget("dictFilters")),
        "pageSize": bget("pageSize").and_then(|v| v.as_i64()).unwrap_or(50),
        "writeBack": bget("writeBack"),
        "columns": columns,
    }))
}
