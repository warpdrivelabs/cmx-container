//! 属性校验：match 对象、列属性、columnModel 属性、维度属性判定。

use std::collections::HashSet;

use serde_json::Value;

use super::{
    Diag, COLUMN_AGGS, COLUMN_ALIGNS, COLUMN_TYPES, DISPLAY_MODES, EDIT_MODES, check_formula,
};

/// 校验 match 对象：键须在合法键集合内，操作符与属性条件不可混用。
pub(super) fn check_match_object(
    m: Option<&Value>,
    path: &str,
    valid_keys: &HashSet<String>,
    d: &mut Diag,
) {
    let Some(m) = m.filter(|v| !v.is_null()) else {
        return;
    };
    if !m.is_object() {
        d.error(path, "MATCH_OBJECT_REQUIRED", "anchor.match 必须是对象");
        return;
    }
    let m = m
        .as_object()
        .expect("invariant: m checked is_object above");
    for (dim, cond) in m {
        // 匹配键须在锚点列/维度/属性列中
        if !valid_keys.is_empty() && !valid_keys.contains(dim) {
            d.error(
                &format!("{path}.{dim}"),
                "MATCH_KEY_NOT_IN_ANCHOR",
                format!("匹配列 {dim} 不在锚点列、锚点维度或其属性列中"),
            );
        }
        // 对象条件：$ 操作符与属性条件不可混用
        if let Some(o) = cond.as_object() {
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

/// 校验 field.column 属性：type/agg/frozen/visible/display/edit 等的取值合法性。
pub(super) fn validate_column_props(column: Option<&Value>, path: &str, d: &mut Diag) {
    let Some(column) = column.filter(|v| !v.is_null()) else {
        return;
    };
    if !column.is_object() {
        d.error(path, "COLUMN_OBJECT_REQUIRED", "field.column 必须是对象");
        return;
    }
    // 列类型（警告级）
    if let Some(t) = column.get("type").and_then(|v| v.as_str())
        && !COLUMN_TYPES.contains(&t)
    {
        d.warn(
            &format!("{path}.type"),
            "COLUMN_TYPE_UNKNOWN",
            format!("未知列类型：{t}"),
        );
    }
    // 聚合类型（错误级）
    if let Some(a) = column.get("agg").and_then(|v| v.as_str())
        && !COLUMN_AGGS.contains(&a)
    {
        d.error(
            &format!("{path}.agg"),
            "COLUMN_AGG_INVALID",
            format!("agg 必须是 sum/count/avg/max/min，实际：{a}"),
        );
    }
    // frozen 须为布尔
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
    // visible 须为布尔
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
    // width 须为字符串/数字/对象
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
    // display 块校验
    if let Some(disp) = column.get("display").filter(|v| !v.is_null()) {
        if !disp.is_object() {
            d.error(
                &format!("{path}.display"),
                "COLUMN_DISPLAY_INVALID",
                "display 必须是对象",
            );
        } else {
            // align 须为 left/center/right
            if let Some(al) = disp.get("align").and_then(|v| v.as_str())
                && !COLUMN_ALIGNS.contains(&al)
            {
                d.error(
                    &format!("{path}.display.align"),
                    "COLUMN_DISPLAY_ALIGN_INVALID",
                    "display.align 必须是 left/center/right",
                );
            }
            // display.mode（警告级）
            if let Some(md) = disp.get("mode").and_then(|v| v.as_str())
                && !DISPLAY_MODES.contains(&md)
            {
                d.warn(
                    &format!("{path}.display.mode"),
                    "COLUMN_DISPLAY_MODE_UNKNOWN",
                    format!("未知 display.mode：{md}"),
                );
            }
            // cellStyle 须为数组，且每条 when 公式良构
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
    // edit 块校验
    if let Some(edit) = column.get("edit").filter(|v| !v.is_null()) {
        if !edit.is_object() {
            d.error(
                &format!("{path}.edit"),
                "COLUMN_EDIT_INVALID",
                "edit 必须是对象",
            );
        } else {
            // edit.mode（警告级）
            if let Some(md) = edit.get("mode").and_then(|v| v.as_str())
                && !EDIT_MODES.contains(&md)
            {
                d.warn(
                    &format!("{path}.edit.mode"),
                    "COLUMN_EDIT_MODE_UNKNOWN",
                    format!("未知 edit.mode：{md}"),
                );
            }
            // requiredWhen/readonlyWhen/validateWhen 公式良构
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

/// 校验 columnModel 属性：caption/datasetId/toTitleCols/iconCol 须为字符串。
pub(super) fn validate_column_model_props(cm: Option<&Value>, path: &str, d: &mut Diag) {
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

/// 判定维度是否声明了某属性（attributes 为数组或对象时分别判定）。
///
/// 维度缺省（None）时返回 true，表示不强制校验。
pub(super) fn has_dimension_attribute(dim: Option<&Value>, attr: &str) -> bool {
    let Some(dim) = dim else { return true };
    match dim.get("attributes") {
        // 数组形式：元素是否含该属性名
        Some(Value::Array(a)) => a.iter().any(|x| x.as_str() == Some(attr)),
        // 对象形式：键是否含该属性名
        Some(Value::Object(o)) => o.contains_key(attr),
        _ => false,
    }
}
