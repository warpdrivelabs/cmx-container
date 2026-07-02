//! ContextProfile 校验器（复刻 `context-profile-validator.js`）。
//!
//! domain-neutral schema 校验，产出 `{ valid, errors:[{path,code,message}], warnings:[...] }`。
//! 公式（formula/expr/when）只做基础良构检查（非空 + 括号配平），等价 compileFormula 的拒绝面。

use std::collections::HashSet;

use serde_json::{Value, json};

const FIELD_KINDS: &[&str] = &["dimension", "attribute", "measure", "relation", ""];
const COLUMN_TYPES: &[&str] = &[
    "text",
    "number",
    "date",
    "boolean",
    "select",
    "ref",
    "combo",
    "dict-select",
];
const COLUMN_ALIGNS: &[&str] = &["left", "center", "right"];
const COLUMN_AGGS: &[&str] = &["sum", "count", "avg", "max", "min"];
const DISPLAY_MODES: &[&str] = &["text", "badge", "link", "icon"];
const EDIT_MODES: &[&str] = &[
    "cmx-text-input",
    "cmx-textarea-input",
    "cmx-richtext-input",
    "cmx-number-input",
    "cmx-date-input",
    "cmx-datetime-input",
    "select",
    "ref",
    "combo",
    "ignite-combo",
    "cmx-dict-selct",
    "checkbox",
    "image",
    "video",
    "readonly",
    "none",
    "computed",
    "tree-ref",
];
const GROUP_AGG_KEYS: &[&str] = &["sum", "avg", "max", "min", "count"];
const GROUP_AGG_POSITIONS: &[&str] = &["before", "after"];

struct Diag {
    errors: Vec<Value>,
    warnings: Vec<Value>,
}

impl Diag {
    fn new() -> Diag {
        Diag {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
    fn error(&mut self, path: &str, code: &str, message: impl Into<String>) {
        self.errors
            .push(json!({ "path": path, "code": code, "message": message.into() }));
    }
    fn warn(&mut self, path: &str, code: &str, message: impl Into<String>) {
        self.warnings
            .push(json!({ "path": path, "code": code, "message": message.into() }));
    }
    fn finish(self) -> Value {
        json!({ "valid": self.errors.is_empty(), "errors": self.errors, "warnings": self.warnings })
    }
}

fn field_id(field: &Value) -> String {
    field
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            field.get("id").map(|v| match v {
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            })
        })
        .unwrap_or_default()
}

/// 公式良构检查：非空字符串 + 括号配平。失败返回 Err(message)。
fn check_formula(expr: &Value) -> Result<(), String> {
    let s = match expr {
        Value::String(s) => s.clone(),
        _ => return Err("公式必须是字符串".to_string()),
    };
    if s.trim().is_empty() {
        return Err("公式不能为空".to_string());
    }
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("公式括号不配平：{s}"));
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("公式括号不配平：{s}"));
    }
    Ok(())
}

/// 入口：校验上下文档案。
pub fn validate_context_profile(profile: &Value) -> Value {
    let mut d = Diag::new();
    if !profile.is_object() {
        d.error("", "PROFILE_OBJECT_REQUIRED", "ContextProfile 必须是对象");
        return d.finish();
    }
    let dimensions = profile.get("dimensions");
    let rules = profile.get("rules");
    if !dimensions.map(|v| v.is_object()).unwrap_or(false) {
        d.error("dimensions", "DIMENSIONS_REQUIRED", "缺少 dimensions 对象");
    }
    if !rules.map(|v| v.is_array()).unwrap_or(false) {
        d.error("rules", "RULES_REQUIRED", "缺少 rules 数组");
    }
    if !d.errors.is_empty() {
        return d.finish();
    }
    let dimensions = dimensions.unwrap();
    let dim_codes: HashSet<String> = dimensions.as_object().unwrap().keys().cloned().collect();
    if dim_codes.is_empty() {
        d.warn("dimensions", "NO_DIMENSIONS", "未定义任何上下文维度");
    }

    for (dim_code, dim) in dimensions.as_object().unwrap() {
        let base = format!("dimensions.{dim_code}");
        if !dim.is_object() {
            d.error(&base, "DIMENSION_OBJECT_REQUIRED", "维度定义必须是对象");
            continue;
        }
        if dim.get("name").is_none() && dim.get("caption").is_none() {
            d.warn(
                &base,
                "DIMENSION_NAME_MISSING",
                format!("维度 {dim_code} 未设置 name/caption"),
            );
        }
        if let Some(attrs) = dim.get("attributes")
            && !attrs.is_null()
            && !attrs.is_array()
            && !attrs.is_object()
        {
            d.error(
                &format!("{base}.attributes"),
                "DIMENSION_ATTRIBUTES_INVALID",
                "attributes 必须是数组或对象",
            );
        }
        if let Some(values) = dim.get("values")
            && !values.is_null()
            && !values.is_array()
        {
            d.error(
                &format!("{base}.values"),
                "DIMENSION_VALUES_INVALID",
                "values 必须是数组",
            );
        }
    }

    let profile_anchor_dims: Vec<String> = profile
        .get("anchorDimensions")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    for (i, dd) in profile_anchor_dims.iter().enumerate() {
        if !dim_codes.contains(dd) {
            d.error(
                &format!("anchorDimensions.{i}"),
                "ANCHOR_DIMENSION_UNKNOWN",
                format!("锚点维度 {dd} 未在 dimensions 中定义"),
            );
        }
    }

    validate_column_model_props(profile.get("columnModel"), "columnModel", &mut d);

    let mut rule_ids: HashSet<String> = HashSet::new();
    let rules_arr = rules.unwrap().as_array().unwrap();
    for (rule_index, rule) in rules_arr.iter().enumerate() {
        let r_path = format!("rules.{rule_index}");
        if !rule.is_object() {
            d.error(&r_path, "RULE_OBJECT_REQUIRED", "规则必须是对象");
            continue;
        }
        match rule.get("id").and_then(|v| v.as_str()) {
            None => d.warn(&format!("{r_path}.id"), "RULE_ID_MISSING", "规则未设置 id"),
            Some(id) => {
                if rule_ids.contains(id) {
                    d.error(
                        &format!("{r_path}.id"),
                        "RULE_ID_DUPLICATE",
                        format!("规则 id 重复：{id}"),
                    );
                } else {
                    rule_ids.insert(id.to_string());
                }
            }
        }

        let anchor_dims: Vec<String> = rule
            .get("anchor")
            .and_then(|a| a.get("dimensions"))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| profile_anchor_dims.clone());
        for (i, dd) in anchor_dims.iter().enumerate() {
            if !dim_codes.contains(dd) {
                d.error(
                    &format!("{r_path}.anchor.dimensions.{i}"),
                    "ANCHOR_DIMENSION_UNKNOWN",
                    format!("锚点维度 {dd} 未在 dimensions 中定义"),
                );
            }
        }
        let anchor_cols: Vec<String> = rule
            .get("anchor")
            .and_then(|a| a.get("columns"))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .enumerate()
                    .filter_map(|(i, c)| match c.as_str() {
                        Some(s) if !s.trim().is_empty() => Some(s.to_string()),
                        _ => {
                            d.error(
                                &format!("{r_path}.anchor.columns.{i}"),
                                "ANCHOR_COLUMN_INVALID",
                                "锚点列必须是非空字符串",
                            );
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        if anchor_dims.is_empty() && anchor_cols.is_empty() {
            d.warn(
                &format!("{r_path}.anchor"),
                "ANCHOR_EMPTY",
                "规则未声明锚点维度/列，解析时只能作为兜底规则",
            );
        }
        // valid match keys
        let mut valid_match_keys: HashSet<String> = anchor_cols
            .iter()
            .cloned()
            .chain(anchor_dims.iter().cloned())
            .collect();
        for dd in &anchor_dims {
            if let Some(dim) = dimensions.get(dd) {
                let attrs: Vec<String> = match dim.get("attributes") {
                    Some(Value::Array(a)) => a
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                    Some(Value::Object(o)) => o.keys().cloned().collect(),
                    _ => vec![],
                };
                for a in attrs {
                    valid_match_keys.insert(format!("{dd}.{a}"));
                }
            }
        }
        check_match_object(
            rule.get("anchor").and_then(|a| a.get("match")),
            &format!("{r_path}.anchor.match"),
            &valid_match_keys,
            &mut d,
        );

        validate_column_model_props(
            rule.get("columnModel"),
            &format!("{r_path}.columnModel"),
            &mut d,
        );

        let fields = rule.get("detail").and_then(|de| de.get("fields"));
        if !fields.map(|v| v.is_array()).unwrap_or(false) {
            d.error(
                &format!("{r_path}.detail.fields"),
                "FIELDS_REQUIRED",
                "规则缺少 detail.fields 数组",
            );
        }
        let fields_arr = fields
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        validate_fields(&fields_arr, &r_path, dimensions, &dim_codes, &mut d);
        validate_groups(
            rule.get("detail").and_then(|de| de.get("groups")),
            &fields_arr,
            &r_path,
            &mut d,
        );
    }

    d.finish()
}

fn validate_fields(
    fields: &[Value],
    r_path: &str,
    dimensions: &Value,
    dim_codes: &HashSet<String>,
    d: &mut Diag,
) {
    let mut field_codes: HashSet<String> = HashSet::new();
    let mut computed: Vec<Value> = Vec::new();
    for (fi, field) in fields.iter().enumerate() {
        let f_path = format!("{r_path}.detail.fields.{fi}");
        if !field.is_object() {
            d.error(&f_path, "FIELD_OBJECT_REQUIRED", "字段必须是对象");
            continue;
        }
        let id = field_id(field);
        if id.is_empty() {
            d.error(
                &format!("{f_path}.id"),
                "FIELD_CODE_REQUIRED",
                "字段缺少 id",
            );
        } else if field_codes.contains(&id) {
            d.error(
                &format!("{f_path}.id"),
                "FIELD_CODE_DUPLICATE",
                format!("字段 id 重复：{id}"),
            );
        } else {
            field_codes.insert(id.clone());
        }
        if let Some(dt) = field.get("dimType").and_then(|v| v.as_str())
            && !FIELD_KINDS.contains(&dt)
        {
            d.warn(
                &format!("{f_path}.dimType"),
                "FIELD_KIND_UNKNOWN",
                format!("未知字段 dimType：{dt}"),
            );
        }
        if field.get("dimType").and_then(|v| v.as_str()) == Some("dimension") {
            let dim_code = field
                .get("refDict")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            if !dim_codes.contains(&dim_code) {
                d.error(
                    &format!("{f_path}.refDict"),
                    "FIELD_DIMENSION_UNKNOWN",
                    format!("字段引用的维度 {dim_code} 未定义"),
                );
            }
        }
        if let Some(src_dim) = field
            .get("source")
            .and_then(|s| s.get("dimension"))
            .and_then(|v| v.as_str())
        {
            if !dim_codes.contains(src_dim) {
                d.error(
                    &format!("{f_path}.source.dimension"),
                    "SOURCE_DIMENSION_UNKNOWN",
                    format!("source.dimension {src_dim} 未定义"),
                );
            } else if let Some(attr) = field
                .get("source")
                .and_then(|s| s.get("attribute"))
                .and_then(|v| v.as_str())
                && !has_dimension_attribute(dimensions.get(src_dim), attr)
            {
                d.warn(
                    &format!("{f_path}.source.attribute"),
                    "SOURCE_ATTRIBUTE_NOT_DECLARED",
                    format!("维度 {src_dim} 未声明属性 {attr}"),
                );
            }
        }
        if let Some(df_dim) = field
            .get("defaultFrom")
            .and_then(|s| s.get("dimension"))
            .and_then(|v| v.as_str())
        {
            if !dim_codes.contains(df_dim) {
                d.error(
                    &format!("{f_path}.defaultFrom.dimension"),
                    "DEFAULT_DIMENSION_UNKNOWN",
                    format!("defaultFrom.dimension {df_dim} 未定义"),
                );
            } else if let Some(attr) = field
                .get("defaultFrom")
                .and_then(|s| s.get("attribute"))
                .and_then(|v| v.as_str())
                && !has_dimension_attribute(dimensions.get(df_dim), attr)
            {
                d.warn(
                    &format!("{f_path}.defaultFrom.attribute"),
                    "DEFAULT_ATTRIBUTE_NOT_DECLARED",
                    format!("维度 {df_dim} 未声明属性 {attr}"),
                );
            }
        }
        if let Some(formula) = field.get("formula")
            && !formula.is_null()
        {
            computed.push(field.clone());
            if let Err(msg) = check_formula(formula) {
                d.error(&format!("{f_path}.formula"), "FORMULA_INVALID", msg);
            }
        }
        if let Some(deps) = field.get("dependsOn").and_then(|v| v.as_array()) {
            for (di, dep) in deps.iter().enumerate() {
                let dep_s = dep.as_str().unwrap_or("");
                let exists =
                    field_codes.contains(dep_s) || fields.iter().any(|f| field_id(f) == dep_s);
                if !exists {
                    d.error(
                        &format!("{f_path}.dependsOn.{di}"),
                        "DEPENDENCY_UNKNOWN",
                        format!("依赖字段 {dep_s} 不存在"),
                    );
                }
            }
        }
        if let Some(vals) = field.get("validations").and_then(|v| v.as_array()) {
            for (vi, rule) in vals.iter().enumerate() {
                match rule.get("expr") {
                    None | Some(Value::Null) => d.error(
                        &format!("{f_path}.validations.{vi}.expr"),
                        "VALIDATION_EXPR_REQUIRED",
                        "校验规则缺少 expr",
                    ),
                    Some(expr) => {
                        if let Err(msg) = check_formula(expr) {
                            d.error(
                                &format!("{f_path}.validations.{vi}.expr"),
                                "VALIDATION_EXPR_INVALID",
                                msg,
                            );
                        }
                    }
                }
            }
        }
        validate_column_props(field.get("column"), &format!("{f_path}.column"), d);
    }
    detect_formula_cycles(&computed, r_path, d);
}

fn validate_groups(groups: Option<&Value>, fields: &[Value], r_path: &str, d: &mut Diag) {
    let Some(groups) = groups.filter(|v| !v.is_null()) else {
        return;
    };
    if !groups.is_array() {
        d.error(
            &format!("{r_path}.detail.groups"),
            "GROUPS_ARRAY_REQUIRED",
            "groups 必须是数组",
        );
        return;
    }
    let field_codes: HashSet<String> = fields
        .iter()
        .map(field_id)
        .filter(|s| !s.is_empty())
        .collect();
    let mut seen: HashSet<String> = HashSet::new();

    fn walk(
        node: &Value,
        path: &str,
        field_codes: &HashSet<String>,
        seen: &mut HashSet<String>,
        d: &mut Diag,
    ) {
        if !node.is_object() {
            d.error(path, "GROUP_OBJECT_REQUIRED", "分组节点必须是对象");
            return;
        }
        let members = node.get("members");
        if !members.map(|v| v.is_array()).unwrap_or(false) {
            d.error(
                &format!("{path}.members"),
                "GROUP_MEMBERS_REQUIRED",
                "分组缺少 members 数组",
            );
            return;
        }
        validate_group_props(node, path, d);
        for (index, member) in members.unwrap().as_array().unwrap().iter().enumerate() {
            let m_path = format!("{path}.members.{index}");
            match member {
                Value::String(s) => {
                    if !field_codes.contains(s) {
                        d.error(
                            &m_path,
                            "GROUP_FIELD_UNKNOWN",
                            format!("分组引用了不存在的字段 {s}"),
                        );
                    } else if seen.contains(s) {
                        d.warn(
                            &m_path,
                            "GROUP_FIELD_DUPLICATE",
                            format!("字段 {s} 被多个分组引用，将只使用首次出现"),
                        );
                    } else {
                        seen.insert(s.clone());
                    }
                }
                _ => walk(member, &m_path, field_codes, seen, d),
            }
        }
    }
    for (i, g) in groups.as_array().unwrap().iter().enumerate() {
        walk(
            g,
            &format!("{r_path}.detail.groups.{i}"),
            &field_codes,
            &mut seen,
            d,
        );
    }
}

fn check_match_object(m: Option<&Value>, path: &str, valid_keys: &HashSet<String>, d: &mut Diag) {
    let Some(m) = m.filter(|v| !v.is_null()) else {
        return;
    };
    if !m.is_object() {
        d.error(path, "MATCH_OBJECT_REQUIRED", "anchor.match 必须是对象");
        return;
    }
    for (dim, cond) in m.as_object().unwrap() {
        if !valid_keys.is_empty() && !valid_keys.contains(dim) {
            d.error(
                &format!("{path}.{dim}"),
                "MATCH_KEY_NOT_IN_ANCHOR",
                format!("匹配列 {dim} 不在锚点列、锚点维度或其属性列中"),
            );
        }
        if cond.is_object() && !cond.is_array() {
            let o = cond.as_object().unwrap();
            let ops = o.keys().filter(|k| k.starts_with('$')).count();
            let attrs = o.keys().filter(|k| !k.starts_with('$')).count();
            if ops > 0 && attrs > 0 {
                d.error(
                    &format!("{path}.{dim}"),
                    "MATCH_OPERATOR_MIXED",
                    "匹配条件不能同时混用操作符和属性条件",
                );
            }
        }
    }
}

fn validate_column_props(column: Option<&Value>, path: &str, d: &mut Diag) {
    let Some(column) = column.filter(|v| !v.is_null()) else {
        return;
    };
    if !column.is_object() {
        d.error(path, "COLUMN_OBJECT_REQUIRED", "field.column 必须是对象");
        return;
    }
    if let Some(t) = column.get("type").and_then(|v| v.as_str())
        && !COLUMN_TYPES.contains(&t)
    {
        d.warn(
            &format!("{path}.type"),
            "COLUMN_TYPE_UNKNOWN",
            format!("未知列类型：{t}"),
        );
    }
    if let Some(a) = column.get("agg").and_then(|v| v.as_str())
        && !COLUMN_AGGS.contains(&a)
    {
        d.error(
            &format!("{path}.agg"),
            "COLUMN_AGG_INVALID",
            format!("agg 必须是 sum/count/avg/max/min，实际：{a}"),
        );
    }
    if let Some(f) = column.get("frozen")
        && !f.is_null()
        && !f.is_boolean()
    {
        d.error(
            &format!("{path}.frozen"),
            "COLUMN_FROZEN_INVALID",
            "frozen 必须是布尔值",
        );
    }
    if let Some(v) = column.get("visible")
        && !v.is_null()
        && !v.is_boolean()
    {
        d.error(
            &format!("{path}.visible"),
            "COLUMN_VISIBLE_INVALID",
            "visible 必须是布尔值",
        );
    }
    if let Some(w) = column.get("width")
        && !w.is_null()
        && !w.is_string()
        && !w.is_number()
        && !w.is_object()
    {
        d.error(
            &format!("{path}.width"),
            "COLUMN_WIDTH_INVALID",
            "width 必须是字符串/数字/对象",
        );
    }
    if let Some(disp) = column.get("display").filter(|v| !v.is_null()) {
        if !disp.is_object() {
            d.error(
                &format!("{path}.display"),
                "COLUMN_DISPLAY_INVALID",
                "display 必须是对象",
            );
        } else {
            if let Some(al) = disp.get("align").and_then(|v| v.as_str())
                && !COLUMN_ALIGNS.contains(&al)
            {
                d.error(
                    &format!("{path}.display.align"),
                    "COLUMN_DISPLAY_ALIGN_INVALID",
                    "display.align 必须是 left/center/right",
                );
            }
            if let Some(md) = disp.get("mode").and_then(|v| v.as_str())
                && !DISPLAY_MODES.contains(&md)
            {
                d.warn(
                    &format!("{path}.display.mode"),
                    "COLUMN_DISPLAY_MODE_UNKNOWN",
                    format!("未知 display.mode：{md}"),
                );
            }
            match disp.get("cellStyle") {
                Some(cs) if !cs.is_null() && !cs.is_array() => {
                    d.error(
                        &format!("{path}.display.cellStyle"),
                        "COLUMN_CELLSTYLE_INVALID",
                        "display.cellStyle 必须是数组",
                    );
                }
                Some(Value::Array(arr)) => {
                    for (i, rule) in arr.iter().enumerate() {
                        if let Some(when) = rule.get("when").filter(|v| !v.is_null())
                            && let Err(msg) = check_formula(when)
                        {
                            d.error(
                                &format!("{path}.display.cellStyle.{i}.when"),
                                "COLUMN_CELLSTYLE_WHEN_INVALID",
                                msg,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(edit) = column.get("edit").filter(|v| !v.is_null()) {
        if !edit.is_object() {
            d.error(
                &format!("{path}.edit"),
                "COLUMN_EDIT_INVALID",
                "edit 必须是对象",
            );
        } else {
            if let Some(md) = edit.get("mode").and_then(|v| v.as_str())
                && !EDIT_MODES.contains(&md)
            {
                d.warn(
                    &format!("{path}.edit.mode"),
                    "COLUMN_EDIT_MODE_UNKNOWN",
                    format!("未知 edit.mode：{md}"),
                );
            }
            for key in ["requiredWhen", "readonlyWhen", "validateWhen"] {
                if let Some(expr) = edit.get(key).filter(|v| !v.is_null())
                    && let Err(msg) = check_formula(expr)
                {
                    d.error(
                        &format!("{path}.edit.{key}"),
                        "COLUMN_EDIT_EXPR_INVALID",
                        msg,
                    );
                }
            }
        }
    }
}

fn validate_group_props(node: &Value, path: &str, d: &mut Diag) {
    if let Some(agg) = node.get("aggregate").filter(|v| !v.is_null()) {
        if !agg.is_object() {
            d.error(
                &format!("{path}.aggregate"),
                "GROUP_AGGREGATE_INVALID",
                "aggregate 必须是对象，如 { sum:true }",
            );
        } else {
            for (k, v) in agg.as_object().unwrap() {
                if !GROUP_AGG_KEYS.contains(&k.as_str()) {
                    d.warn(
                        &format!("{path}.aggregate.{k}"),
                        "GROUP_AGGREGATE_KEY_UNKNOWN",
                        format!("未知聚合类型：{k}（支持 sum/avg/max/min/count）"),
                    );
                } else if !v.is_boolean() {
                    d.error(
                        &format!("{path}.aggregate.{k}"),
                        "GROUP_AGGREGATE_VALUE_INVALID",
                        format!("aggregate.{k} 必须是布尔值"),
                    );
                }
            }
        }
    }
    if let Some(pos) = node.get("aggregatePosition").and_then(|v| v.as_str())
        && !GROUP_AGG_POSITIONS.contains(&pos)
    {
        d.error(
            &format!("{path}.aggregatePosition"),
            "GROUP_AGGREGATE_POSITION_INVALID",
            format!("aggregatePosition 必须是 before/after，实际：{pos}"),
        );
    }
}

fn validate_column_model_props(cm: Option<&Value>, path: &str, d: &mut Diag) {
    let Some(cm) = cm.filter(|v| !v.is_null()) else {
        return;
    };
    if !cm.is_object() {
        d.error(
            path,
            "COLUMN_MODEL_OBJECT_REQUIRED",
            "columnModel 必须是对象",
        );
        return;
    }
    for key in ["caption", "datasetId", "toTitleCols", "iconCol"] {
        if let Some(v) = cm.get(key).filter(|v| !v.is_null())
            && !v.is_string()
        {
            d.error(
                &format!("{path}.{key}"),
                "COLUMN_MODEL_FIELD_INVALID",
                format!("columnModel.{key} 必须是字符串"),
            );
        }
    }
}

fn has_dimension_attribute(dim: Option<&Value>, attr: &str) -> bool {
    let Some(dim) = dim else { return true };
    match dim.get("attributes") {
        Some(Value::Array(a)) => a.iter().any(|x| x.as_str() == Some(attr)),
        Some(Value::Object(o)) => o.contains_key(attr),
        _ => false,
    }
}

fn detect_formula_cycles(computed: &[Value], r_path: &str, d: &mut Diag) {
    let by_code: std::collections::HashMap<String, Value> = computed
        .iter()
        .map(|f| (field_id(f), f.clone()))
        .filter(|(id, _)| !id.is_empty())
        .collect();
    let mut visiting: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();

    fn visit(
        field: &Value,
        by_code: &std::collections::HashMap<String, Value>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
        r_path: &str,
        d: &mut Diag,
    ) {
        let id = field_id(field);
        if id.is_empty() || visited.contains(&id) {
            return;
        }
        if visiting.contains(&id) {
            let start = stack.iter().position(|x| *x == id).unwrap_or(0);
            let mut cycle: Vec<String> = stack[start..].to_vec();
            cycle.push(id.clone());
            d.error(
                &format!("{r_path}.detail.fields.{id}.dependsOn"),
                "FORMULA_DEPENDENCY_CYCLE",
                format!("计算公式存在循环依赖：{}", cycle.join(" -> ")),
            );
            return;
        }
        visiting.insert(id.clone());
        stack.push(id.clone());
        if let Some(deps) = field.get("dependsOn").and_then(|v| v.as_array()) {
            for dep in deps {
                if let Some(dep_s) = dep.as_str()
                    && let Some(df) = by_code.get(dep_s)
                {
                    visit(df, by_code, visiting, visited, stack, r_path, d);
                }
            }
        }
        stack.pop();
        visiting.remove(&id);
        visited.insert(id);
    }

    for f in computed {
        visit(
            f,
            &by_code,
            &mut visiting,
            &mut visited,
            &mut stack,
            r_path,
            d,
        );
    }
}
