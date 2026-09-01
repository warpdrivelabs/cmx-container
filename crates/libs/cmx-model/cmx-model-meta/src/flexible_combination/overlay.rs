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

use serde_json::Value;

/// 深合并：over 覆盖 base。base/over 均为对象时递归；否则 over 决定。
///
/// - over 键值为 Null → 删除该键
/// - over 键名以 '+' 结尾 → 对应 base 数组追加
///
/// # Arguments
///
/// * `base` - 被覆盖的基础值。
/// * `over` - 覆盖增量。
///
/// # Returns
///
/// 返回合并后的新值（不修改输入）。
pub fn deep_merge(base: &Value, over: &Value) -> Value {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            let mut out = b.clone();
            for (k, v) in o {
                // 键名以 '+' 结尾 → 数组追加语义
                if let Some(target) = k.strip_suffix('+') {
                    let mut arr = out
                        .get(target)
                        .and_then(|x| x.as_array())
                        .cloned()
                        .unwrap_or_default();
                    match v {
                        // 数组追加数组
                        Value::Array(add) => arr.extend(add.iter().cloned()),
                        // 标量追加为单元素
                        other => arr.push(other.clone()),
                    }
                    out.insert(target.to_string(), Value::Array(arr));
                    continue;
                }
                // null → 删除该键
                if v.is_null() {
                    out.remove(k); // 删除语义
                    continue;
                }
                // 双方均为对象 → 递归合并
                match (out.get(k), v) {
                    (Some(bv @ Value::Object(_)), ov @ Value::Object(_)) => {
                        let merged = deep_merge(bv, ov);
                        out.insert(k.clone(), merged);
                    }
                    // 否则 over 直接覆盖
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            Value::Object(out)
        }
        // 非双对象：over 决定
        _ => over.clone(),
    }
}

/// 拆 "table.column" / "column"（后者用 default_table）。
///
/// # Arguments
///
/// * `r` - 引用字符串，形如 "table.column" 或 "column"。
/// * `default_table` - 无显式 table 段时的兜底表名。
///
/// # Returns
///
/// 返回 `(Option<table>, column)`。
fn split_ref<'a>(r: &'a str, default_table: Option<&'a str>) -> (Option<&'a str>, &'a str) {
    match r.rfind('.') {
        Some(i) => (Some(&r[..i]), &r[i + 1..]),
        None => (default_table, r),
    }
}

/// 取列定义的 id（字符串形式）。
fn field_id(f: &Value) -> Option<&str> {
    f.get("id").and_then(|v| v.as_str())
}

/// 解析一个 ref 字段 → inline 字段对象。base = col ⊕ (nullable⇒required) ⊕ over；id = as || col.id。
/// 找不到列返回 None（悬空引用，由上层诊断）。
///
/// # Arguments
///
/// * `ref_str` - 引用字符串（`table.column` 或 `column`）。
/// * `over` - 作者声明的覆盖增量（合并到物理列之上）。
/// * `as_name` - 重命名后的字段 id（`as` 声明）；为 None 时保留物理列 id。
/// * `default_table` - 兜底表名（ref 无显式 table 段时使用）。
/// * `table_cols` - 表物理列查询闭包（tableName → 该表列定义数组）。
///
/// # Returns
///
/// 返回展开后的 inline 字段对象；表/列不存在（悬空引用）时返回 `None`。
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
    // 拆出 table 与 column，定位目标表与列
    let (table, column) = split_ref(ref_str, default_table);
    let table = table?;
    let cols = table_cols(table)?;
    let col = cols.iter().find(|c| field_id(c) == Some(column))?;

    let mut base = col.clone();
    // DOC 非空列 ⇒ 默认必填（over 之前，作者仍可用 over.edit.required:false 放松）
    if col.get("nullable") == Some(&Value::Bool(false)) {
        let obj = base
            .as_object_mut()
            .expect("invariant: col 能经 field_id 取到 id,必为对象");
        let mut edit = obj
            .get("edit")
            .and_then(|e| e.as_object())
            .cloned()
            .unwrap_or_default();
        edit.insert("required".to_string(), Value::Bool(true));
        obj.insert("edit".to_string(), Value::Object(edit));
    }

    // 合并 over 增量（作者覆盖优先）
    let mut merged = match over {
        Some(o) => deep_merge(&base, o),
        None => base,
    };

    let obj = merged
        .as_object_mut()
        .expect("invariant: merged 基底为 DOC 列对象(必为对象),deep_merge 双对象保持对象");
    match as_name {
        // as 重命名：id 与 name 都改为新名
        Some(name) => {
            obj.insert("id".to_string(), Value::String(name.to_string()));
            obj.insert("name".to_string(), Value::String(name.to_string()));
        }
        // 无 as：缺 name 时用 id 补齐
        None => {
            if !obj.contains_key("name")
                && let Some(id) = obj.get("id").cloned()
            {
                obj.insert("name".to_string(), id);
            }
        }
    }
    Some(merged)
}

/// 展开一条规则的 detail（use/pick/fields → inline fields）。返回展开后的 fields 数组。
///
/// table_cols 为 None（未注入 DOC）时，use/pick 无法展开，仅回退 inline fields。
///
/// # Arguments
///
/// * `detail` - 规则的 detail 节点（含 use/pick/over/fields/table）。
/// * `table_cols` - 表物理列查询闭包；None 表示未注入 DOC 列。
///
/// # Returns
///
/// 返回展开后的 inline fields 数组（use/pick 展开结果 + 原始 fields 直通）。
pub fn expand_detail_fields<F>(detail: &Value, table_cols: Option<&F>) -> Vec<Value>
where
    F: Fn(&str) -> Option<Vec<Value>>,
{
    let table = detail.get("table").and_then(|v| v.as_str());
    let mut out: Vec<Value> = Vec::new();

    // 仅在注入了 DOC 列闭包时才展开 use/pick
    if let Some(tc) = table_cols {
        // use:"*" —— 全部物理列 + over 补丁
        let use_all = matches!(
            detail.get("use").and_then(|v| v.as_str()),
            Some("*") | Some("all")
        );
        if use_all {
            let over_map = detail.get("over").and_then(|v| v.as_object());
            if let (Some(t), Some(cols)) = (table, table.and_then(tc)) {
                // 逐列展开，按列 id 查 over 补丁
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
            // 逐个 pick 项展开（按声明顺序）
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

    // fields:[…] 恒等直通（无论是否注入 DOC 列）
    if let Some(fields) = detail.get("fields").and_then(|v| v.as_array()) {
        out.extend(fields.iter().cloned());
    }
    out
}

/// 判断一个 detail 形节点（rule.detail 或 fieldTabs[] 项）是否使用 overlay 入口（use/pick）。
fn node_uses_overlay(node: &Value) -> bool {
    node.get("use").map(|v| !v.is_null()).unwrap_or(false)
        || node.get("pick").map(|v| v.is_array()).unwrap_or(false)
}

/// 规则是否使用 overlay 入口（detail 或任一 fieldTabs 字段集）。
///
/// # Arguments
///
/// * `rule` - 规则对象。
///
/// # Returns
///
/// detail.use 非空 / detail.pick 为数组 / 任一 fieldTabs[].use|pick 非空时返回 true。
pub fn rule_uses_overlay(rule: &Value) -> bool {
    let Some(detail) = rule.get("detail") else {
        return false;
    };
    if node_uses_overlay(detail) {
        return true;
    }
    detail
        .get("fieldTabs")
        .and_then(|v| v.as_array())
        .map(|tabs| tabs.iter().any(node_uses_overlay))
        .unwrap_or(false)
}

/// 把单条规则的 overlay（detail 及 fieldTabs[] 各字段集）展开为 inline；无 overlay 时原样克隆。
///
/// 展开后清除 use/pick/over，写入 fields；groups / table / name / columnModel 原样保留。
/// fieldTabs 各项与 detail 同构（各自带 table），按各自 table 独立展开。
///
/// # Arguments
///
/// * `rule` - 待展开的规则对象。
/// * `table_cols` - 表物理列查询闭包；None 或规则无 overlay 时原样克隆。
///
/// # Returns
///
/// 返回展开后的新规则（无 overlay 时为原规则克隆）。
pub fn expand_rule<F>(rule: &Value, table_cols: Option<&F>) -> Value
where
    F: Fn(&str) -> Option<Vec<Value>>,
{
    // 无 overlay 或未注入 DOC 列：原样克隆
    if !rule_uses_overlay(rule) || table_cols.is_none() {
        return rule.clone();
    }
    let Some(detail) = rule.get("detail") else {
        return rule.clone();
    };
    let mut next_detail = detail.as_object().cloned().unwrap_or_default();
    if node_uses_overlay(detail) {
        let fields = expand_detail_fields(detail, table_cols);
        next_detail.remove("use");
        next_detail.remove("pick");
        next_detail.remove("over");
        next_detail.insert("fields".to_string(), Value::Array(fields));
    }
    // fieldTabs 逐项展开（无 overlay 的项原样保留）
    if let Some(tabs) = detail.get("fieldTabs").and_then(|v| v.as_array()) {
        let expanded: Vec<Value> = tabs
            .iter()
            .map(|t| {
                if !node_uses_overlay(t) {
                    return t.clone();
                }
                let fields = expand_detail_fields(t, table_cols);
                let mut nt = t.as_object().cloned().unwrap_or_default();
                nt.remove("use");
                nt.remove("pick");
                nt.remove("over");
                nt.insert("fields".to_string(), Value::Array(fields));
                Value::Object(nt)
            })
            .collect();
        next_detail.insert("fieldTabs".to_string(), Value::Array(expanded));
    }
    let mut next_rule = rule.as_object().cloned().unwrap_or_default();
    next_rule.insert("detail".to_string(), Value::Object(next_detail));
    Value::Object(next_rule)
}

/// 把整份 rules 数组里所有 overlay 规则展开为 inline（读时展开）。
///
/// table_cols 为 None（未能加载 DOC）时原样返回。返回展开后的 `Value::Array`。
///
/// # Arguments
///
/// * `rules` - rules 数组（`Value::Array`）。
/// * `table_cols` - 表物理列查询闭包；None 时所有规则原样克隆。
///
/// # Returns
///
/// 返回展开后的数组；输入非数组时原样返回。
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

    // 测试用列查询闭包的类型别名，简化「未注入 DOC 列」场景的 None 标注。
    // 使用 fn 指针（ Sized），可直接传入泛型 F: Fn(...) 的函数。
    type TableColsFn = fn(&str) -> Option<Vec<Value>>;

    fn cols() -> Vec<Value> {
        vec![
            json!({"id":"amount","dataType":"DECIMAL","decimalDigits":2,"dimType":"measure","agg":"sum","nullable":false,"caption":{"zh_CN":"明细金额"}}),
            json!({"id":"cost_center_id","dataType":"BIGINT","dimType":"dimension","refDict":"cost_center","nullable":true,"caption":{"zh_CN":"成本中心"}}),
            json!({"id":"remark","dataType":"VARCHAR","fieldLength":512,"dimType":"attribute","nullable":true}),
        ]
    }
    fn tc(t: &str) -> Option<Vec<Value>> {
        if t == "voucher_detail" {
            Some(cols())
        } else {
            None
        }
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
        let f = expand_ref_field(
            "voucher_detail.cost_center_id",
            Some(&json!({"edit":{"required":true}})),
            None,
            None,
            &tc,
        )
        .unwrap();
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
        let f = expand_ref_field(
            "voucher_detail.cost_center_id",
            Some(&json!({"caption":{"zh_CN":"部门"}})),
            Some("department"),
            None,
            &tc,
        )
        .unwrap();
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
        let cc = fields
            .iter()
            .find(|f| f["id"] == json!("cost_center_id"))
            .unwrap();
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
    fn rule_uses_overlay_detects_field_tabs() {
        let rule = json!({"detail": {"fields": [], "fieldTabs": [ {"table": "t1", "use": "*"} ]}});
        assert!(rule_uses_overlay(&rule));
        let rule = json!({"detail": {"fields": [], "fieldTabs": [ {"table": "t1", "pick": [{"ref": "t1.c1"}]} ]}});
        assert!(rule_uses_overlay(&rule));
        let rule = json!({"detail": {"fields": [], "fieldTabs": [ {"table": "t1", "fields": [{"id": "c1"}]} ]}});
        assert!(!rule_uses_overlay(&rule));
    }

    #[test]
    fn expand_rule_expands_field_tabs_by_own_table() {
        // detail 本身无 overlay、仅 fieldTabs 有 use:"*" —— tabs 也要按各自 table 展开
        fn tc2(t: &str) -> Option<Vec<Value>> {
            match t {
                "t_line" => Some(vec![json!({"id": "c1"}), json!({"id": "c2"})]),
                _ => None,
            }
        }
        let rule = json!({
            "id": "r1",
            "detail": {
                "table": "t_head",
                "fields": [{"id": "a"}],
                "fieldTabs": [ {"id": "ft1", "table": "t_line", "use": "*"} ]
            }
        });
        let out = expand_rule(&rule, Some(&tc2));
        let detail = &out["detail"];
        // detail.fields 原样直通
        let head_ids: Vec<_> = detail["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["id"].as_str())
            .collect();
        assert_eq!(head_ids, vec!["a"]);
        // fieldTabs 展开为 inline fields，overlay 键清除
        let tabs = detail["fieldTabs"].as_array().unwrap();
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].get("use").is_none());
        let tab_ids: Vec<_> = tabs[0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["id"].as_str())
            .collect();
        assert_eq!(tab_ids, vec!["c1", "c2"]);
    }

    #[test]
    fn no_doc_tables_keeps_inline_only() {
        let detail = json!({"table":"voucher_detail","use":"*","fields":[{"id":"x"}]});
        let none: Option<&TableColsFn> = None;
        let fields = expand_detail_fields(&detail, none);
        let ids: Vec<_> = fields.iter().filter_map(|f| f["id"].as_str()).collect();
        assert_eq!(ids, vec!["x"]);
    }
}
