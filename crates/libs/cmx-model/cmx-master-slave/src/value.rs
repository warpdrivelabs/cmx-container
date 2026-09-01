//! 数值语义 —— 与前端 `AGG_FUNCS` 的 `Number(v) || 0` 逐字对齐。
//!
//! 前端聚合把每个值经 `Number(v)` 转数，`NaN`/非数 → `0`（`|| 0`）。后端权威重算必须
//! 复现，否则跨端 golden 对拍不过。

use serde_json::Value;

/// 数值别名（对外 f64，对齐 JS `number`）。
pub type Number = f64;

/// 把一个 JSON 值按前端 `Number(v) || 0` 语义转 f64。
pub fn as_f64(v: &Value) -> Number {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.is_finite() { f } else { 0.0 }
        }
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return 0.0;
            }
            match t.parse::<f64>() {
                Ok(f) if f.is_finite() => f,
                _ => 0.0,
            }
        }
        _ => 0.0,
    }
}

/// 把 f64 写回 JSON 值。整数值写成整数（对齐 JS `JSON.stringify(84)` 而非 `84.0`）。
pub fn to_value(f: Number) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
        Value::Number((f as i64).into())
    } else {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Number(0.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn number_coercion_matches_js() {
        assert_eq!(as_f64(&json!(42)), 42.0);
        assert_eq!(as_f64(&json!(2.5)), 2.5);
        assert_eq!(as_f64(&json!("100")), 100.0);
        assert_eq!(as_f64(&json!("")), 0.0);
        assert_eq!(as_f64(&json!("nope")), 0.0);
        assert_eq!(as_f64(&json!(true)), 1.0);
        assert_eq!(as_f64(&json!(null)), 0.0);
        assert_eq!(as_f64(&json!([1, 2])), 0.0);
    }

    #[test]
    fn to_value_writes_integers_plainly() {
        assert_eq!(to_value(84.0), json!(84));
        assert_eq!(to_value(3.5), json!(3.5));
        assert_eq!(to_value(f64::NAN), json!(0));
    }
}
