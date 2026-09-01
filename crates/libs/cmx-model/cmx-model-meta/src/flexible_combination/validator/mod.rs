//! FlexibleCombination 校验器（复刻 `flexible-combination-validator.js`）。
//!
//! domain-neutral schema 校验，产出 `{ valid, errors:[{path,code,message}], warnings:[...] }`。
//! 公式（formula/expr/when）只做基础良构检查（非空 + 括号配平），等价 compileFormula 的拒绝面。
//!
//! 模块拆分：本 mod 承载常量 / [`Diag`] 收集器 / 入口 [`validate_flexible_combination`] 与共享工具；
//! 字段校验见 [`fields`]、分组校验见 [`groups`]、属性校验见 [`props`]。

mod fields;
mod groups;
mod props;

use std::collections::HashSet;

use serde_json::{Value, json};

use fields::validate_fields;
use groups::validate_groups;
use props::{check_match_object, validate_column_model_props};

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
    "cmx-dict-select",
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

/// 校验诊断收集器：累积 errors / warnings，最终汇总为 `{ valid, errors, warnings }`。
pub(super) struct Diag {
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
    /// 记录一条错误（path + code + message）。
    pub(super) fn error(&mut self, path: &str, code: &str, message: impl Into<String>) {
        self.errors
            .push(json!({ "path": path, "code": code, "message": message.into() }));
    }
    /// 记录一条警告（path + code + message）。
    pub(super) fn warn(&mut self, path: &str, code: &str, message: impl Into<String>) {
        self.warnings
            .push(json!({ "path": path, "code": code, "message": message.into() }));
    }
    /// 汇总为最终结果：valid 取决于 errors 是否为空。
    fn finish(self) -> Value {
        json!({ "valid": self.errors.is_empty(), "errors": self.errors, "warnings": self.warnings })
    }
}

/// 取字段 id（兼容字符串与数字 id，统一归一为字符串）。
pub(super) fn field_id(field: &Value) -> String {
    field
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        // 兼容数字 id：转字符串
        .or_else(|| {
            field.get("id").map(|v| match v {
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            })
        })
        .unwrap_or_default()
}

/// 公式良构检查：非空字符串 + 括号配平。失败返回 Err(message)。
pub(super) fn check_formula(expr: &Value) -> Result<(), String> {
    let s = match expr {
        Value::String(s) => s.clone(),
        _ => return Err("公式必须是字符串".to_string()),
    };
    if s.trim().is_empty() {
        return Err("公式不能为空".to_string());
    }
    // 逐字符统计括号深度，中途为负说明右括号多余
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
    // 最终非零说明左括号多余
    if depth != 0 {
        return Err(format!("公式括号不配平：{s}"));
    }
    Ok(())
}

/// 入口：校验弹性组合。
///
/// 校验 dimensions / rules / columnModel 三大块，以及规则内的锚点/字段/分组/公式。
///
/// # Arguments
///
/// * `combination` - 弹性组合定义 JSON。
///
/// # Returns
///
/// 返回 `{ valid, errors, warnings }`；errors 非空时 valid 为 false。
pub fn validate_flexible_combination(combination: &Value) -> Value {
    let mut d = Diag::new();
    // 顶层必须是对象
    if !combination.is_object() {
        d.error(
            "",
            "COMBINATION_OBJECT_REQUIRED",
            "FlexibleCombination 必须是对象",
        );
        return d.finish();
    }
    let dimensions = combination.get("dimensions");
    let rules = combination.get("rules");
    // dimensions 须为对象，rules 须为数组（硬性要求）
    if !dimensions.map(|v| v.is_object()).unwrap_or(false) {
        d.error("dimensions", "DIMENSIONS_REQUIRED", "缺少 dimensions 对象");
    }
    if !rules.map(|v| v.is_array()).unwrap_or(false) {
        d.error("rules", "RULES_REQUIRED", "缺少 rules 数组");
    }
    // 结构性错误时提前返回，不做后续细查
    if !d.errors.is_empty() {
        return d.finish();
    }
    // 上面已校验 dimensions 是对象、rules 是数组，并提前返回；此处安全解构。
    let Some(dimensions) = dimensions.and_then(|v| v.as_object()) else {
        return d.finish();
    };
    let dim_codes: HashSet<String> = dimensions.keys().cloned().collect();
    if dim_codes.is_empty() {
        d.warn("dimensions", "NO_DIMENSIONS", "未定义任何上下文维度");
    }

    // 校验各维度定义
    for (dim_code, dim) in dimensions {
        let base = format!("dimensions.{dim_code}");
        if !dim.is_object() {
            d.error(&base, "DIMENSION_OBJECT_REQUIRED", "维度定义必须是对象");
            continue;
        }
        // 维度须有 name 或 caption
        if dim.get("name").is_none() && dim.get("caption").is_none() {
            d.warn(
                &base,
                "DIMENSION_NAME_MISSING",
                format!("维度 {dim_code} 未设置 name/caption"),
            );
        }
        // attributes 须为数组或对象
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
        // values 须为数组
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

    // 校验顶层 anchorDimensions 引用的维度是否存在
    let combination_anchor_dims: Vec<String> = combination
        .get("anchorDimensions")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    for (i, dd) in combination_anchor_dims.iter().enumerate() {
        if !dim_codes.contains(dd) {
            d.error(
                &format!("anchorDimensions.{i}"),
                "ANCHOR_DIMENSION_UNKNOWN",
                format!("锚点维度 {dd} 未在 dimensions 中定义"),
            );
        }
    }

    // 校验顶层 columnModel
    validate_column_model_props(combination.get("columnModel"), "columnModel", &mut d);

    // 逐条校验规则
    let mut rule_ids: HashSet<String> = HashSet::new();
    let Some(rules_arr) = rules.and_then(|v| v.as_array()) else {
        return d.finish();
    };
    for (rule_index, rule) in rules_arr.iter().enumerate() {
        let r_path = format!("rules.{rule_index}");
        if !rule.is_object() {
            d.error(&r_path, "RULE_OBJECT_REQUIRED", "规则必须是对象");
            continue;
        }
        // 规则 id 唯一性
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

        // 规则锚点维度存在性（缺省继承顶层 anchorDimensions）
        let anchor_dims: Vec<String> = rule
            .get("anchor")
            .and_then(|a| a.get("dimensions"))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| combination_anchor_dims.clone());
        for (i, dd) in anchor_dims.iter().enumerate() {
            if !dim_codes.contains(dd) {
                d.error(
                    &format!("{r_path}.anchor.dimensions.{i}"),
                    "ANCHOR_DIMENSION_UNKNOWN",
                    format!("锚点维度 {dd} 未在 dimensions 中定义"),
                );
            }
        }
        // 规则锚点列（须为非空字符串）
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
        // 无锚点维度且无锚点列：仅作兜底规则
        if anchor_dims.is_empty() && anchor_cols.is_empty() {
            d.warn(
                &format!("{r_path}.anchor"),
                "ANCHOR_EMPTY",
                "规则未声明锚点维度/列，解析时只能作为兜底规则",
            );
        }
        // 计算 match 合法键集合：锚点列 + 锚点维度 + 维度属性路径
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

        // 校验规则级 columnModel
        validate_column_model_props(
            rule.get("columnModel"),
            &format!("{r_path}.columnModel"),
            &mut d,
        );

        // overlay 形态（use:"*" / pick[]）在编译期展开为 fields，结构上不要求 detail.fields；
        // 三入口至少有其一即可。纯 inline 规则仍要求 fields 为数组。
        let detail = rule.get("detail");
        let has_overlay = detail
            .and_then(|de| de.get("use"))
            .map(|v| !v.is_null())
            .unwrap_or(false)
            || detail
                .and_then(|de| de.get("pick"))
                .map(|v| v.is_array())
                .unwrap_or(false);
        let fields = detail.and_then(|de| de.get("fields"));
        // 非 overlay 规则必须有 fields 数组
        if !has_overlay && !fields.map(|v| v.is_array()).unwrap_or(false) {
            d.error(
                &format!("{r_path}.detail.fields"),
                "FIELDS_REQUIRED",
                "规则缺少 detail.fields 数组（或改用 overlay 的 use/pick）",
            );
        }
        let fields_arr = fields
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        validate_fields(&fields_arr, &r_path, dimensions, &dim_codes, &mut d);
        // 分组成员存在性依赖展开后字段：overlay 规则跳过（成员由 ref 展开后产生），纯 inline 照常。
        if !has_overlay {
            validate_groups(
                rule.get("detail").and_then(|de| de.get("groups")),
                &fields_arr,
                &r_path,
                &mut d,
            );
        }
    }

    d.finish()
}
