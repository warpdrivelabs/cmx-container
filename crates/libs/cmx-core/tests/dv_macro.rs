//! dv! 宏单元测试

use cmx_core::model::cell::{DataValue, SqlTypeMarker};

#[test]
fn dv_empty() {
    let v: Vec<DataValue> = cmx_core::dv!();
    assert!(v.is_empty());
}

#[test]
fn dv_string_and_option() {
    let name: Option<String> = Some("alice".into());
    let desc: Option<String> = None;
    let v = cmx_core::dv![
        "id123".to_string(),
        name,
        desc,
    ];
    assert_eq!(v.len(), 3);
    assert_eq!(v[0], DataValue::String("id123".into()));
    assert_eq!(v[1], DataValue::String("alice".into()));
    assert_eq!(v[2], DataValue::Null);
}

#[test]
fn dv_option_int_some() {
    let n: Option<i64> = Some(42);
    let v = cmx_core::dv![n];
    assert_eq!(v[0], DataValue::Int(42));
}

#[test]
fn dv_option_int_null_typed() {
    let n: Option<i64> = None;
    let v = cmx_core::dv![n];
    assert_eq!(v[0], DataValue::NullTyped(SqlTypeMarker::Int));
}

#[test]
fn dv_option_bool_null_typed() {
    let b: Option<bool> = None;
    let v = cmx_core::dv![b];
    assert_eq!(v[0], DataValue::NullTyped(SqlTypeMarker::Bool));
}

#[test]
fn dv_mixed_types() {
    let v: Vec<DataValue> = cmx_core::dv![
        "str".to_string(),
        42_i64,
        true,
        3.14_f64,
    ];
    assert_eq!(v.len(), 4);
    assert_eq!(v[0], DataValue::String("str".into()));
    assert_eq!(v[1], DataValue::Int(42));
    assert_eq!(v[2], DataValue::Bool(true));
    assert_eq!(v[3], DataValue::Float(3.14));
}

#[test]
fn dv_null_marker() {
    let v: DataValue = cmx_core::dv!(null Uuid);
    assert_eq!(v, DataValue::NullTyped(SqlTypeMarker::Uuid));
}

#[test]
fn dv_null_marker_int() {
    let v: DataValue = cmx_core::dv!(null Int);
    assert_eq!(v, DataValue::NullTyped(SqlTypeMarker::Int));
}

#[test]
fn dv_null_marker_inside_vec() {
    let v: Vec<DataValue> = cmx_core::dv![
        "id".to_string(),
        cmx_core::dv!(null Uuid),
        100_i64,
    ];
    assert_eq!(v.len(), 3);
    assert_eq!(v[0], DataValue::String("id".into()));
    assert_eq!(v[1], DataValue::NullTyped(SqlTypeMarker::Uuid));
    assert_eq!(v[2], DataValue::Int(100));
}

#[test]
fn dv_trailing_comma() {
    let v: Vec<DataValue> = cmx_core::dv![
        "a".to_string(),
        "b".to_string(),
    ];
    assert_eq!(v.len(), 2);
}
