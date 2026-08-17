//! cmx-doc-model/codec —— JSON ↔ DataValue 类型化转换(集中收口)。
//!
//! 消除三份重复实现:
//! - 替代 `saver.rs:1408 json_to_dv`(通用兜底) → [`json_to_dv_loose`]
//! - 替代 `saver.rs:1428 dv_for_col`(按 FieldType) → [`json_to_dv_typed`]
//! - 替代 `revision.rs:215 datavalue_to_json` → [`dv_to_json`]
//!
//! ## 设计要点
//!
//! - **宽松策略**:类型不符时优先保留原始值(而非报错),匹配 saver 现有行为,
//!   避免前端 changeset 中"数字字符串 → 整数列"等常见场景被误拒。
//! - **严格路径仍在 query.rs**:[`query::json_to_datavalue`] 保持 `Result` 返回与
//!   `ColumnView.data_type` 字符串分发(用于 SQL 绑定前的强校验),不走本模块。
//! - **日期时间统一走 [`datetime_util`]**,避免再写一份 RFC3339 解析。

use cmx_core::model::cell::{DataValue, FieldType, SqlTypeMarker};
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

use crate::datetime_util::{parse_datetime_utc, parse_naive_date};

/// 类型列的 NULL → 带类型的 [`DataValue::NullTyped`]。
///
/// 绑定层把普通 [`DataValue::Null`] 绑成 `Option::<String>::None`（text 型 NULL）；
/// saver 对非文本列占位符加了 `$p::bigint` 等显式强转后，text 型 NULL 会在客户端
/// to_sql 校验被拒。故已知目标列类型时 NULL 必须带类型标记；String/Text 等文本列
/// 保持普通 Null（text 本就是默认推断，无需标记）。
fn null_typed_for(ft: &FieldType) -> DataValue {
    match ft {
        FieldType::Int => DataValue::NullTyped(SqlTypeMarker::Int),
        FieldType::Float => DataValue::NullTyped(SqlTypeMarker::Float),
        FieldType::Decimal => DataValue::NullTyped(SqlTypeMarker::Decimal),
        FieldType::Date => DataValue::NullTyped(SqlTypeMarker::Date),
        FieldType::DateTime => DataValue::NullTyped(SqlTypeMarker::Timestamp),
        FieldType::Bool => DataValue::NullTyped(SqlTypeMarker::Bool),
        FieldType::Json => DataValue::NullTyped(SqlTypeMarker::Json),
        _ => DataValue::Null,
    }
}

/// 按 [`FieldType`] 把 JSON [`Value`] 转 [`DataValue`](宽松策略)。
///
/// 替代 saver.rs:1428 `dv_for_col`。用于回存参数绑定:前端 changeset 中 id/数值
/// 常以 JSON 字符串形式出现(如 `"1000000001"`),而目标列可能是 BIGINT/DECIMAL,
/// 这里按目标列类型做强转,避免 PG `bigint = text` 类型不匹配错误。
///
/// 类型不符时保留原值(`String`)或转 `Null`(空白字符串),不报错。
///
/// # Arguments
/// * `ft` - 目标列的 FieldType(由 layer.schema 推断)。
/// * `v`  - JSON 值(通常来自前端 changeset)。
pub fn json_to_dv_typed(ft: &FieldType, v: &Value) -> DataValue {
    match (v, ft) {
        // 类型列的 JSON null → NullTyped（配合 saver 的 $p::<type> 强转占位符；
        // 普通 Null 绑 text 型 NULL，遇到 bigint 等强转占位符客户端校验不过）
        (Value::Null, _) => null_typed_for(ft),
        // 目标是整数列:数字字符串/数字 → Int,空白 → Null,其余保留原值
        (Value::String(s), FieldType::Int) => match s.trim().parse::<i64>() {
            Ok(n) => DataValue::Int(n),
            Err(_) if s.trim().is_empty() => null_typed_for(ft),
            Err(_) => DataValue::String(s.clone()),
        },
        // 目标是浮点列:同上策略
        (Value::String(s), FieldType::Float) => match s.trim().parse::<f64>() {
            Ok(n) => DataValue::Float(n),
            Err(_) if s.trim().is_empty() => null_typed_for(ft),
            Err(_) => DataValue::String(s.clone()),
        },
        // Decimal/Date/DateTime/Json 列的空白字符串 → NULL（避免绑定层报错）
        // Date/DateTime 的 "invalid input syntax"；Jsonb 空串非合法 JSON → `$p::jsonb` cast 失败，
        // 被 brief_db_detail 兜底成「数据保存失败：请检查数据后重试」。统一转 NULL（PG jsonb 接受 NULL）。
        (Value::String(s),
            FieldType::Decimal | FieldType::Date | FieldType::DateTime | FieldType::Json)
            if s.trim().is_empty() =>
        {
            null_typed_for(ft)
        }
        // 目标是 Decimal 列的非空数字字符串 → Decimal(避免 text 绑 numeric 报错)
        (Value::String(s), FieldType::Decimal) => match s.trim().parse::<Decimal>() {
            Ok(d) => DataValue::Decimal(d),
            Err(_) => DataValue::String(s.clone()),
        },
        // 目标是 Decimal 列的 JSON 数字 → Decimal(保留精度,不走 f64)
        (Value::Number(n), FieldType::Decimal) => match Decimal::from_str(&n.to_string()) {
            Ok(d) => DataValue::Decimal(d),
            Err(_) => json_to_dv_loose(v),
        },
        // 目标是 DateTime 列(TIMESTAMP/TIMESTAMPTZ)的非空字符串 → DateTime<Utc>
        // 兼容 RFC3339 与无时区格式(按 UTC 解释)
        (Value::String(s), FieldType::DateTime) => match parse_datetime_utc(s) {
            Some(dt) => DataValue::DateTime(dt),
            None => DataValue::String(s.clone()),
        },
        // 目标是 DATE 列的非空字符串 → NaiveDate
        (Value::String(s), FieldType::Date) => match parse_naive_date(s) {
            Some(d) => DataValue::Date(d),
            None => DataValue::String(s.clone()),
        },
        // 其余场景走通用兜底
        _ => json_to_dv_loose(v),
    }
}

/// 无类型信息的兜底转换(JSON 原生类型直接映射)。
///
/// 替代 saver.rs:1408 `json_to_dv`。用于无目标列类型信息时的默认转换:
/// - `Null` → `DataValue::Null`
/// - `Bool` → `DataValue::Bool`
/// - `Number` → 优先 `Int`(i64),否则 `Float`(f64)
/// - `String` → `DataValue::String`
/// - 其它(数组/对象)→ `DataValue::Json`(序列化为 JSON 字符串)
pub fn json_to_dv_loose(v: &Value) -> DataValue {
    match v {
        Value::Null => DataValue::Null,
        Value::Bool(b) => DataValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DataValue::Int(i)
            } else {
                DataValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => DataValue::String(s.clone()),
        other => DataValue::Json(other.to_string()),
    }
}

/// [`DataValue`] → JSON [`Value`]。
///
/// 替代 revision.rs:215 `datavalue_to_json`。依赖 `DataValue: Serialize`
/// (cmx-core 已派生),失败时退化为 `Value::Null`(与原实现一致)。
pub fn dv_to_json(dv: &DataValue) -> Value {
    serde_json::to_value(dv).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_int_from_string() {
        assert_eq!(
            json_to_dv_typed(&FieldType::Int, &json!("42")),
            DataValue::Int(42)
        );
        assert_eq!(
            json_to_dv_typed(&FieldType::Int, &json!("  -7 ")),
            DataValue::Int(-7)
        );
        // 空白字符串 → NullTyped（bigint 等强转占位符需要带类型的 NULL）
        assert_eq!(
            json_to_dv_typed(&FieldType::Int, &json!("  ")),
            DataValue::NullTyped(SqlTypeMarker::Int)
        );
        // JSON null → NullTyped（line_target_id 等可空外键列的常态值）
        assert_eq!(
            json_to_dv_typed(&FieldType::Int, &json!(null)),
            DataValue::NullTyped(SqlTypeMarker::Int)
        );
        assert_eq!(
            json_to_dv_typed(&FieldType::Json, &json!(null)),
            DataValue::NullTyped(SqlTypeMarker::Json)
        );
        // 文本列 null 保持普通 Null（text 本就是默认推断）
        assert_eq!(json_to_dv_typed(&FieldType::String, &json!(null)), DataValue::Null);
        // 非数字 → 保留原值
        assert_eq!(
            json_to_dv_typed(&FieldType::Int, &json!("abc")),
            DataValue::String("abc".into())
        );
    }

    #[test]
    fn typed_decimal_from_number_and_string() {
        let d = json_to_dv_typed(&FieldType::Decimal, &json!("3.14"));
        assert!(matches!(d, DataValue::Decimal(_)));
        let d = json_to_dv_typed(&FieldType::Decimal, &json!(3.14));
        assert!(matches!(d, DataValue::Decimal(_)));
        // 空白 → NullTyped
        assert_eq!(
            json_to_dv_typed(&FieldType::Decimal, &json!("")),
            DataValue::NullTyped(SqlTypeMarker::Decimal)
        );
    }

    #[test]
    fn typed_datetime_rfc3339_and_naive() {
        assert!(matches!(
            json_to_dv_typed(&FieldType::DateTime, &json!("2026-07-07T09:00:00Z")),
            DataValue::DateTime(_)
        ));
        assert!(matches!(
            json_to_dv_typed(&FieldType::DateTime, &json!("2026-07-07 09:00:00")),
            DataValue::DateTime(_)
        ));
        // 无法解析 → 保留原值
        assert_eq!(
            json_to_dv_typed(&FieldType::DateTime, &json!("not-a-date")),
            DataValue::String("not-a-date".into())
        );
    }

    #[test]
    fn typed_date_short() {
        assert!(matches!(
            json_to_dv_typed(&FieldType::Date, &json!("2026-07-07")),
            DataValue::Date(_)
        ));
    }

    #[test]
    fn loose_null_bool_number_string() {
        assert_eq!(json_to_dv_loose(&json!(null)), DataValue::Null);
        assert_eq!(json_to_dv_loose(&json!(true)), DataValue::Bool(true));
        assert_eq!(json_to_dv_loose(&json!(42)), DataValue::Int(42));
        assert!(matches!(json_to_dv_loose(&json!(3.14)), DataValue::Float(_)));
        assert_eq!(
            json_to_dv_loose(&json!("hi")),
            DataValue::String("hi".into())
        );
        assert!(matches!(
            json_to_dv_loose(&json!({"a": 1})),
            DataValue::Json(_)
        ));
    }

    #[test]
    fn dv_to_json_roundtrip() {
        assert_eq!(dv_to_json(&DataValue::Int(42)), json!(42));
        assert_eq!(dv_to_json(&DataValue::Null), json!(null));
        assert_eq!(dv_to_json(&DataValue::Bool(true)), json!(true));
    }
}
