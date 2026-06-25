//! SqlParam 枚举单元测试

use cmx_core::model::cell::{DataValue, SqlParam, SqlTypeMarker};
use rust_decimal::Decimal;
use chrono::{DateTime, Utc, NaiveDate};
use uuid::Uuid;

#[test]
fn sql_param_from_data_value_null() {
    let dv = DataValue::Null;
    let p: SqlParam = dv.into();
    assert_eq!(p, SqlParam::Null(SqlTypeMarker::Text));
}

#[test]
fn sql_param_from_data_value_null_typed() {
    let dv = DataValue::NullTyped(SqlTypeMarker::Int);
    let p: SqlParam = dv.into();
    assert_eq!(p, SqlParam::Null(SqlTypeMarker::Int));
}

#[test]
fn sql_param_from_data_value_string() {
    let dv = DataValue::String("hello".into());
    let p: SqlParam = dv.into();
    assert_eq!(p, SqlParam::Text("hello".into()));
}

#[test]
fn sql_param_from_data_value_short_str() {
    let dv = DataValue::ShortStr("abc".into());
    let p: SqlParam = dv.into();
    assert_eq!(p, SqlParam::Text("abc".into()));
}

#[test]
fn sql_param_from_data_value_array() {
    let dv = DataValue::Array(vec![DataValue::Int(1), DataValue::Int(2)]);
    let p: SqlParam = dv.into();
    match p {
        SqlParam::Array(els) => assert_eq!(els.len(), 2),
        _ => panic!("Expected Array variant"),
    }
}

#[test]
fn sql_param_to_data_value_null() {
    let p = SqlParam::Null(SqlTypeMarker::Uuid);
    let dv: DataValue = p.into();
    assert_eq!(dv, DataValue::NullTyped(SqlTypeMarker::Uuid));
}

#[test]
fn sql_param_to_data_value_int() {
    let p = SqlParam::Int(42);
    let dv: DataValue = p.into();
    assert_eq!(dv, DataValue::Int(42));
}

#[test]
fn sql_param_roundtrip_json() {
    let original = SqlParam::Int(123);
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_null_typed() {
    let original = SqlParam::Null(SqlTypeMarker::Timestamp);
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_complex() {
    let original = SqlParam::Text("测试中文".into());
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_decimal() {
    let original = SqlParam::Decimal(Decimal::new(12345, 2));
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_uuid() {
    let original = SqlParam::Uuid(Uuid::new_v4());
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_timestamp() {
    let dt = DateTime::parse_from_rfc3339("2024-06-15T10:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let original = SqlParam::Timestamp(dt);
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_date() {
    let original = SqlParam::Date(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap());
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn sql_param_roundtrip_array() {
    let original = SqlParam::Array(vec![
        SqlParam::Int(1),
        SqlParam::Int(2),
        SqlParam::Int(3),
    ]);
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SqlParam = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}
