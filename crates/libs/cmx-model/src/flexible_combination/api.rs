//! 弹性组合端点编排（复刻处理器 flexibleCombinationResolve/Rule/Preview/Validate 逻辑）。

use serde_json::{Map, Value, json};

use crate::flexible_combination::dict_meta::enrich_flexible_combination_dict_meta;
use crate::flexible_combination::engine::Engine;
use crate::flexible_combination::store::{FcRef, get_flexible_combination};
use crate::flexible_combination::validator::validate_flexible_combination;
use crate::error::PortalResult;

/// 由查询参数 + combination.anchorDimensions 构造 anchor（排除 DAM 键）。
fn resolve_anchor(raw: &Map<String, Value>, combination: &Value) -> Map<String, Value> {
    let dims: Vec<String> = match combination.get("anchorDimensions").and_then(|v| v.as_array()) {
        Some(a) if !a.is_empty() => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        _ => raw
            .keys()
            .filter(|k| !["domain", "app", "module", "scenario"].contains(&k.as_str()))
            .cloned()
            .collect(),
    };
    let mut anchor = Map::new();
    for d in dims {
        if let Some(v) = raw.get(&d).filter(|v| !v.is_null()) {
            anchor.insert(d, json!(value_to_string(v)));
        }
    }
    anchor
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// `/resolve`：读 combination → enrich → resolveMergedRule → fields + columnModel。
pub async fn resolve(r: &FcRef, query: &Map<String, Value>) -> PortalResult<Value> {
    let cfg = enrich_flexible_combination_dict_meta(&get_flexible_combination(r).await?).await?;
    let dims = cfg.get("dimensions").cloned().unwrap_or(json!({}));
    let rules = cfg.get("rules").cloned().unwrap_or(json!([]));
    let engine = Engine::new(&dims, &rules);

    // anchorDims：cfg.anchorDimensions 非空则用之，否则用 query 键
    let anchor_dims: Vec<String> = match cfg.get("anchorDimensions").and_then(|v| v.as_array()) {
        Some(a) if !a.is_empty() => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        _ => query.keys().cloned().collect(),
    };
    let mut anchor = Map::new();
    for d in &anchor_dims {
        if let Some(v) = query.get(d).filter(|v| !v.is_null()) {
            anchor.insert(d.clone(), json!(value_to_string(v)));
        }
    }
    let rule = engine.resolve_merged_rule(&anchor);
    let Some(rule) = rule else {
        return Ok(json!({ "ruleId": null, "anchor": anchor, "fields": [] }));
    };
    let fields = engine.build_columns(&rule);
    let column_model = engine.build_column_model_props(&rule, &cfg);
    Ok(json!({
        "ruleId": rule.get("id").cloned().unwrap_or(Value::Null),
        "anchor": anchor,
        "anchorDimensions": anchor_dims,
        "columnModel": column_model,
        "fields": fields,
    }))
}

/// `/rule`：解析规则 + 相关维度子集。
pub async fn rule(r: &FcRef, query: &Map<String, Value>) -> PortalResult<Value> {
    let cfg = enrich_flexible_combination_dict_meta(&get_flexible_combination(r).await?).await?;
    let dims = cfg.get("dimensions").cloned().unwrap_or(json!({}));
    let rules = cfg.get("rules").cloned().unwrap_or(json!([]));
    let engine = Engine::new(&dims, &rules);

    let anchor_dims: Vec<String> = match cfg.get("anchorDimensions").and_then(|v| v.as_array()) {
        Some(a) if !a.is_empty() => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        _ => query.keys().cloned().collect(),
    };
    let mut anchor = Map::new();
    for d in &anchor_dims {
        if let Some(v) = query.get(d).filter(|v| !v.is_null()) {
            anchor.insert(d.clone(), json!(value_to_string(v)));
        }
    }
    let rule = engine.resolve_merged_rule(&anchor);
    let Some(rule) = rule else {
        return Ok(json!({ "ruleId": null, "anchor": anchor, "rule": null, "dimensions": {} }));
    };
    // need 集合：锚点维度 + 规则字段引用的维度
    let mut need: std::collections::BTreeSet<String> = anchor_dims.iter().cloned().collect();
    if let Some(fields) = rule
        .get("detail")
        .and_then(|d| d.get("fields"))
        .and_then(|v| v.as_array())
    {
        for f in fields {
            let dim_type = f
                .get("dimType")
                .or_else(|| f.get("kind"))
                .and_then(|v| v.as_str());
            if dim_type == Some("dimension") {
                let dc = f
                    .get("refDict")
                    .or_else(|| f.get("dimension"))
                    .or_else(|| f.get("fieldName"))
                    .or_else(|| f.get("code"))
                    .and_then(|v| v.as_str());
                if let Some(dc) = dc {
                    need.insert(dc.to_string());
                }
            }
            if let Some(sd) = f
                .get("source")
                .and_then(|s| s.get("dimension"))
                .and_then(|v| v.as_str())
            {
                need.insert(sd.to_string());
            }
            if let Some(dd) = f
                .get("defaultFrom")
                .and_then(|s| s.get("dimension"))
                .and_then(|v| v.as_str())
            {
                need.insert(dd.to_string());
            }
        }
    }
    let mut out_dims = Map::new();
    for dc in &need {
        if let Some(dv) = dims.get(dc) {
            out_dims.insert(dc.clone(), dv.clone());
        }
    }
    Ok(json!({
        "ruleId": rule.get("id").cloned().unwrap_or(Value::Null),
        "anchor": anchor,
        "anchorDimensions": anchor_dims,
        "rule": rule,
        "dimensions": out_dims,
    }))
}

/// `/preview`：校验 + 解析预览（members/columns/columnModel）。
pub async fn preview(body: &Value, r: &FcRef) -> PortalResult<Value> {
    let combination = enrich_flexible_combination_dict_meta(&body_or_stored(body, r).await?).await?;
    let diagnostics = validate_flexible_combination(&combination);
    let anchor_raw = body
        .get("anchor")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let anchor = resolve_anchor(&anchor_raw, &combination);

    let dims = combination.get("dimensions").cloned().unwrap_or(json!({}));
    let rules = combination.get("rules").cloned().unwrap_or(json!([]));
    let engine = Engine::new(&dims, &rules);

    let mut rule_id = Value::Null;
    let mut members: Vec<Value> = Vec::new();
    let mut columns: Vec<Value> = Vec::new();
    let mut column_model = json!({});
    if diagnostics
        .get("valid")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && let Some(rule) = engine.resolve_merged_rule(&anchor)
    {
        rule_id = rule.get("id").cloned().unwrap_or(Value::Null);
        members = engine.build_members(&rule);
        columns = engine.build_columns(&rule);
        column_model = engine.build_column_model_props(&rule, &combination);
    }
    // 响应 anchorDimensions：与 Node 一致——combination.anchorDimensions 是数组就原样用（含空数组），
    // 仅当它根本不是数组时才回退 anchor 键。
    let anchor_dims: Vec<String> = match combination.get("anchorDimensions") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        _ => anchor.keys().cloned().collect(),
    };
    Ok(json!({
        "diagnostics": diagnostics,
        "anchor": anchor,
        "anchorDimensions": anchor_dims,
        "ruleId": rule_id,
        "columnModel": column_model,
        "columns": columns,
        "members": members,
    }))
}

/// `/validate`：仅校验。
pub async fn validate(body: &Value, r: &FcRef) -> PortalResult<Value> {
    let combination = body_or_stored(body, r).await?;
    Ok(validate_flexible_combination(&combination))
}

/// body.combination / body 本身(含 dimensions|rules) / 否则按 DAM 读存储。
async fn body_or_stored(body: &Value, r: &FcRef) -> PortalResult<Value> {
    if let Some(combination) = body.get("combination").filter(|v| v.is_object()) {
        return Ok(combination.clone());
    }
    if body.get("dimensions").is_some() || body.get("rules").is_some() {
        return Ok(body.clone());
    }
    get_flexible_combination(r).await
}
