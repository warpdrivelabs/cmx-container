//! 字段校验：id 唯一性、维度引用、source/defaultFrom、公式良构、依赖存在性、循环检测。

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{Diag, FIELD_KINDS, check_formula, field_id};
use super::props::{has_dimension_attribute, validate_column_props};

/// 校验规则字段集合：id 唯一性、维度引用、source/defaultFrom、公式良构、依赖存在性、列属性。
pub(super) fn validate_fields(
    fields: &[Value],
    r_path: &str,
    dimensions: &Map<String, Value>,
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
        // 字段 id 唯一性
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
        // dimType 合法性（警告级）
        if let Some(dt) = field.get("dimType").and_then(|v| v.as_str())
            && !FIELD_KINDS.contains(&dt)
        {
            d.warn(
                &format!("{f_path}.dimType"),
                "FIELD_KIND_UNKNOWN",
                format!("未知字段 dimType：{dt}"),
            );
        }
        // dimension 字段：引用的维度须存在
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
        // source.dimension 存在性 + attribute 声明性
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
        // defaultFrom.dimension 存在性 + attribute 声明性
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
        // 公式字段：良构检查 + 收集用于循环检测
        if let Some(formula) = field.get("formula")
            && !formula.is_null()
        {
            computed.push(field.clone());
            if let Err(msg) = check_formula(formula) {
                d.error(&format!("{f_path}.formula"), "FORMULA_INVALID", msg);
            }
        }
        // dependsOn：依赖字段须存在
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
        // validations：每条须含 expr 且良构
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
        // 列属性校验
        validate_column_props(field.get("column"), &format!("{f_path}.column"), d);
    }
    // 公式循环依赖检测
    detect_formula_cycles(&computed, r_path, d);
}

/// 检测计算公式字段的循环依赖（DFS 三色标记法）。
fn detect_formula_cycles(computed: &[Value], r_path: &str, d: &mut Diag) {
    // 建 code → 字段 映射
    let by_code: std::collections::HashMap<String, Value> = computed
        .iter()
        .map(|f| (field_id(f), f.clone()))
        .filter(|(id, _)| !id.is_empty())
        .collect();
    let mut visiting: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();

    // DFS 访问单个字段，发现回边即报告循环
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
        // visiting 中命中 → 发现环
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
        // 递归访问依赖字段
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

    // 遍历所有计算字段，触发 DFS
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
