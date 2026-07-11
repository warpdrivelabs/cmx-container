//! overlay —— 弹性组合（FLC）overlay 编译器（领域无关、纯函数），JS `flc-overlay.js` 的 Rust 对端。
//!
//! 把「引用 DOC 列 + 只写增量」形态展开成引擎吃的 inline `detail.fields`：
//!   use:"*"              采用关联表全部物理列，配 over:{colId:{…}} 打补丁
//!   pick:[{ref,as,over}] 只挑列出的列（顺序即声明顺序）
//!   fields:[…]           现状 inline（纯逻辑列）——恒等直通
//!
//! 合并语义（deep_merge，DOC列 ⊕ FLC增量，后者优先）：
//!   标量 → 覆盖；对象 → 递归；数组 → 默认替换；
//!   over 值为 null → 删除该键；键名后缀 '+' → 数组追加。
//! 另：DOC 非空列 nullable:false ⇒ 默认 edit.required:true（对齐 _docColumnToField / JS 对端）。
//!
//! DOC 列由调用方以闭包注入（保持模块自包含、可单测，不在此做 IO）：
//!   table_cols(table_name) -> Some(Vec<Value>) 该表物理列；表不存在 -> None

use serde_json::{Map, Value};

/// 深合并：over 覆盖 base。base/over 均为对象时递归；否则 over 决定。
/// - over 键值为 Null → 删除该键
/// - over 键名以 '+' 结尾 → 对应 base 数组追加
pub fn deep_merge(base: &Value, over: &Value) -> Value {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            let mut out = b.clone();
            for (k, v) in o {
                if let Some(target) = k.strip_suffix('+') {
                    // 数组追加语义
                    let mut arr = out
                        .get(target)
                        .and_then(|x| x.as_array())
                        .cloned()
                        .unwrap_or_default();
                    match v {
                        Value::Array(add) => arr.extend(add.iter().cloned()),
                        other => arr.push(other.clone()),
                    }
                    out.insert(target.to_string(), Value::Array(arr));
                    continue;
                }
                if v.is_null() {
                    out.remove(k); // 删除语义
                    continue;
                }
                match (out.get(k), v) {
                    (Some(bv @ Value::Object(_)), ov @ Value::Object(_)) => {
                        let merged = deep_merge(bv, ov);
                        out.insert(k.clone(), merged);
                    }
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            Value::Object(out)
        }
        _ => over.clone(),
    }
}

/// 拆 "table.column" / "column"（后者用 default_table）。
fn split_ref<'a>(r: &'a str, default_table: Option<&'a str>) -> (Option<&'a str>, &'a str) {
    match r.rfind('.') {
        Some(i) => (Some(&r[..i]), &r[i + 1..]),
        None => (default_table, r),
    }
}

fn field_id(f: &Value) -> Option<&str> {
    f.get("id").and_then(|v| v.as_str())
}

/// 解析一个 ref 字段 → inline 字段对象。base = col ⊕ (nullable⇒required) ⊕ over；id = as || col.id。
/// 找不到列返回 None（悬空引用，由上层诊断）。
pub fn expand_ref_field<F>(
    ref_str: &str,
    over: Option<&Value>,
    as_name: Option<&str>,
    default_table: Option<&str>,
    table_cols: &F,
) -> Option<Value>
where
    F: Fn(&str) -> Option<Vec<Value>>,
{
    let (table, column) = split_ref(ref_str, default_table);
    let table = table?;
    let cols = table_cols(table)?;
    let col = cols.iter().find(|c| field_id(c) == Some(column))?;

    let mut base = col.clone();
    // DOC 非空列 ⇒ 默认必填（over 之前，作者仍可用 over.edit.required:false 放松）
    if col.get("nullable") == Some(&Value::Bool(false)) {
        let obj = base.as_object_mut().unwrap();
        let mut edit = obj
            .get("edit")
            .and_then(|e| e.as_object())
            .cloned()
            .unwrap_or_default();
        edit.insert("required".to_string(), Value::Bool(true));
        obj.insert("edit".to_string(), Value::Object(edit));
    }

    let mut merged = match over {
        Some(o) => deep_merge(&base, o),
        None => base,
    };

    let obj = merged.as_object_mut().unwrap();
    match as_name {
        Some(name) => {
            obj.insert("id".to_string(), Value::String(name.to_string()));
            obj.insert("name".to_string(), Value::String(name.to_string()));
        }
        None => {
            if !obj.contains_key("name")
                && let Some(id) = obj.get("id").cloned() {
                    obj.insert("name".to_string(), id);
                }
        }
    }
    Some(merged)
}

/// 展开一条规则的 detail（use/pick/fields → inline fields）。返回展开后的 fields 数组。
/// table_cols 为 None（未注入 DOC）时，use/pick 无法展开，仅回退 inline fields。
pub fn expand_detail_fields<F>(detail: &Value, table_cols: Option<&F>) -> Vec<Value>
where
    F: Fn(&str) -> Option<Vec<Value>>,
{
    let table = detail.get("table").and_then(|v| v.as_str());
    let mut out: Vec<Value> = Vec::new();

    if let Some(tc) = table_cols {
        // use:"*" —— 全部物理列 + over 补丁
        let use_all = matches!(detail.get("use").and_then(|v| v.as_str()), Some("*") | Some("all"));
        if use_all {
            let over_map = detail.get("over").and_then(|v| v.as_object());
            if let (Some(t), Some(cols)) = (table, table.and_then(tc)) {
                for col in &cols {
                    let id = match field_id(col) {
                        Some(x) => x.to_string(),
                        None => continue,
                    };
                    let over = over_map.and_then(|m| m.get(&id));
                    let r = format!("{t}.{id}");
                    if let Some(f) = expand_ref_field(&r, over, None, Some(t), tc) {
                        out.push(f);
                    }
                }
            }
        }
        // pick:[{ref,as,over}]
        if let Some(pick) = detail.get("pick").and_then(|v| v.as_array()) {
            for p in pick {
                let ref_str = match p.get("ref").and_then(|v| v.as_str()) {
                    Some(x) => x,
                    None => continue,
                };
                let over = p.get("over");
                let as_name = p.get("as").and_then(|v| v.as_str());
                if let Some(f) = expand_ref_field(ref_str, over, as_name, table, tc) {
                    out.push(f);
                }
            }
        }
    }

    // fields:[…] 恒等直通
    if let Some(fields) = detail.get("fields").and_then(|v| v.as_array()) {
        out.extend(fields.iter().cloned());
    }
    out
}

/// 规则是否使用 overlay 入口（use/pick）。
pub fn rule_uses_overlay(rule: &Value) -> bool {
    let detail = match rule.get("detail") {
        Some(d) => d,
        None => return false,
    };
    detail.get("use").map(|v| !v.is_null()).unwrap_or(false)
        || detail.get("pick").map(|v| v.is_array()).unwrap_or(false)
}

/// 把单条规则的 overlay detail 展开为 inline（就地返回新规则）；无 overlay 时原样克隆。
/// 展开后清除 use/pick/over，写入 fields；groups 原样保留。
pub fn expand_rule<F>(rule: &Value, table_cols: Option<&F>) -> Value
where
    F: Fn(&str) -> Option<Vec<Value>>,
{
    if !rule_uses_overlay(rule) || table_cols.is_none() {
        return rule.clone();
    }
    let detail = rule.get("detail").cloned().unwrap_or(Value::Object(Map::new()));
    let fields = expand_detail_fields(&detail, table_cols);

    let mut next_detail = detail.as_object().cloned().unwrap_or_default();
    next_detail.remove("use");
    next_detail.remove("pick");
    next_detail.remove("over");
    next_detail.insert("fields".to_string(), Value::Array(fields));

    let mut next_rule = rule.as_object().cloned().unwrap_or_default();
    next_rule.insert("detail".to_string(), Value::Object(next_detail));
    Value::Object(next_rule)
}

/// 把整份 rules 数组里所有 overlay 规则展开为 inline（读时展开）。
/// table_cols 为 None（未能加载 DOC）时原样返回。返回展开后的 `Value::Array`。
pub fn expand_rules_value<F>(rules: &Value, table_cols: Option<&F>) -> Value
where
    F: Fn(&str) -> Option<Vec<Value>>,
{
    let arr = match rules.as_array() {
        Some(a) => a,
        None => return rules.clone(),
    };
    Value::Array(arr.iter().map(|r| expand_rule(r, table_cols)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cols() -> Vec<Value> {
        vec![
            json!({"id":"amount","dataType":"DECIMAL","decimalDigits":2,"dimType":"measure","agg":"sum","nullable":false,"caption":{"zh_CN":"明细金额"}}),
            json!({"id":"cost_center_id","dataType":"BIGINT","dimType":"dimension","refDict":"cost_center","nullable":true,"caption":{"zh_CN":"成本中心"}}),
            json!({"id":"remark","dataType":"VARCHAR","fieldLength":512,"dimType":"attribute","nullable":true}),
        ]
    }
    fn tc(t: &str) -> Option<Vec<Value>> {
        if t == "voucher_detail" { Some(cols()) } else { None }
    }

    #[test]
    fn deep_merge_scalar_object_null_append() {
        let base = json!({"a":1,"edit":{"mode":"input","required":false},"agg":"sum"});
        let over = json!({"a":2,"edit":{"required":true},"agg":null,"tags+":["x"]});
        let r = deep_merge(&base, &over);
        assert_eq!(r["a"], json!(2));
        assert_eq!(r["edit"], json!({"mode":"input","required":true}));
        assert!(r.get("agg").is_none());
        assert_eq!(r["tags"], json!(["x"]));
    }

    #[test]
    fn ref_field_inherits_and_overrides() {
        let f = expand_ref_field("voucher_detail.cashflow_item_id", None, None, None, &tc);
        assert!(f.is_none()); // not in cols
        let f = expand_ref_field("voucher_detail.cost_center_id", Some(&json!({"edit":{"required":true}})), None, None, &tc).unwrap();
        assert_eq!(f["dataType"], json!("BIGINT"));
        assert_eq!(f["edit"]["required"], json!(true));
    }

    #[test]
    fn nullable_false_implies_required() {
        let f = expand_ref_field("voucher_detail.amount", None, None, None, &tc).unwrap();
        assert_eq!(f["edit"]["required"], json!(true));
    }

    #[test]
    fn as_renames_keeps_physical_type() {
        let f = expand_ref_field("voucher_detail.cost_center_id", Some(&json!({"caption":{"zh_CN":"部门"}})), Some("department"), None, &tc).unwrap();
        assert_eq!(f["id"], json!("department"));
        assert_eq!(f["name"], json!("department"));
        assert_eq!(f["dataType"], json!("BIGINT"));
        assert_eq!(f["refDict"], json!("cost_center"));
    }

    #[test]
    fn use_star_expands_all_with_over() {
        let detail = json!({"table":"voucher_detail","use":"*","over":{"cost_center_id":{"edit":{"required":true}}}});
        let fields = expand_detail_fields(&detail, Some(&tc));
        let ids: Vec<_> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(ids, vec!["amount", "cost_center_id", "remark"]);
        let cc = fields.iter().find(|f| f["id"] == json!("cost_center_id")).unwrap();
        assert_eq!(cc["edit"]["required"], json!(true));
    }

    #[test]
    fn pick_with_as_and_order() {
        let detail = json!({"table":"voucher_detail","pick":[
            {"ref":"voucher_detail.cost_center_id","as":"department"},
            {"ref":"amount"}
        ]});
        let fields = expand_detail_fields(&detail, Some(&tc));
        let ids: Vec<_> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(ids, vec!["department", "amount"]);
    }

    #[test]
    fn expand_rule_clears_overlay_keys() {
        let rule = json!({"id":"r","detail":{"table":"voucher_detail","use":"*"}});
        let out = expand_rule(&rule, Some(&tc));
        assert!(out["detail"].get("use").is_none());
        assert!(out["detail"]["fields"].is_array());
        assert_eq!(out["detail"]["fields"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn no_doc_tables_keeps_inline_only() {
        let detail = json!({"table":"voucher_detail","use":"*","fields":[{"id":"x"}]});
        let none: Option<&fn(&str) -> Option<Vec<Value>>> = None;
        let fields = expand_detail_fields(&detail, none);
        let ids: Vec<_> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(ids, vec!["x"]);
    }
}
