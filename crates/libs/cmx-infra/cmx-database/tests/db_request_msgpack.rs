//! DbRequest 的 MsgPack 序列化往返测试
//!
//! 验证 wasm 边界的 data_values 字段能正确通过 MsgPack 序列化/反序列化,
//! 确保带类型 NULL (NullTyped) 跨 wasm 边界传递时不丢失类型信息。

use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_core::wasm_types::DbRequest;

#[test]
fn db_request_data_values_msgpack_roundtrip() {
    let original = DbRequest {
        sql: "INSERT INTO t(id, parent_id, sort_order) VALUES ($1, $2, $3)".to_string(),
        data_values: Some(vec![
            DataValue::String("id123".into()),
            DataValue::NullTyped(SqlTypeMarker::Uuid),
            DataValue::NullTyped(SqlTypeMarker::Int),
        ]),
        ..Default::default()
    };

    let bytes = rmp_serde::to_vec(&original).expect("序列化失败");
    assert!(!bytes.is_empty());

    let decoded: DbRequest = rmp_serde::from_slice(&bytes).expect("反序列化失败");
    assert_eq!(decoded.sql, original.sql);

    let data_values = decoded.data_values.expect("data_values 应存在");
    assert_eq!(data_values.len(), 3);
    assert_eq!(data_values[0], DataValue::String("id123".into()));
    assert_eq!(data_values[1], DataValue::NullTyped(SqlTypeMarker::Uuid));
    assert_eq!(data_values[2], DataValue::NullTyped(SqlTypeMarker::Int));
}

#[test]
fn db_request_params_only_backward_compatible() {
    // 旧 plugin 只发 params JSON,无 data_values
    let original = DbRequest {
        sql: "SELECT * FROM t WHERE id = $1".to_string(),
        params: Some(serde_json::json!(["id123"])),
        data_values: None,
        ..Default::default()
    };

    let bytes = rmp_serde::to_vec(&original).expect("序列化失败");
    let decoded: DbRequest = rmp_serde::from_slice(&bytes).expect("反序列化失败");

    assert_eq!(decoded.sql, original.sql);
    assert!(decoded.params.is_some());
    assert!(decoded.data_values.is_none());
}

#[test]
fn db_request_no_params() {
    let original = DbRequest {
        sql: "SELECT 1".to_string(),
        ..Default::default()
    };

    let bytes = rmp_serde::to_vec(&original).expect("序列化失败");
    let decoded: DbRequest = rmp_serde::from_slice(&bytes).expect("反序列化失败");

    assert_eq!(decoded.sql, "SELECT 1");
    assert!(decoded.params.is_none());
    assert!(decoded.data_values.is_none());
}

#[test]
fn db_request_data_values_with_mixed_types() {
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    let dt = DateTime::parse_from_rfc3339("2024-06-15T10:30:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let original = DbRequest {
        sql: "UPDATE t SET name=$1, age=$2, created=$3, parent=$4 WHERE id=$5".to_string(),
        data_values: Some(vec![
            DataValue::String("alice".into()),
            DataValue::NullTyped(SqlTypeMarker::Int),
            DataValue::DateTime(dt),
            DataValue::NullTyped(SqlTypeMarker::Uuid),
            DataValue::Uuid(Uuid::new_v4()),
        ]),
        txn_id: Some("txn-123".into()),
        ..Default::default()
    };

    let bytes = rmp_serde::to_vec(&original).expect("序列化失败");
    let decoded: DbRequest = rmp_serde::from_slice(&bytes).expect("反序列化失败");

    assert_eq!(decoded.sql, original.sql);
    assert_eq!(decoded.txn_id, Some("txn-123".to_string()));

    let data_values = decoded.data_values.expect("data_values 应存在");
    assert_eq!(data_values.len(), 5);
    assert_eq!(data_values[0], DataValue::String("alice".into()));
    assert_eq!(data_values[1], DataValue::NullTyped(SqlTypeMarker::Int));
    assert_eq!(data_values[2], DataValue::DateTime(dt));
    assert_eq!(data_values[3], DataValue::NullTyped(SqlTypeMarker::Uuid));
    match &data_values[4] {
        DataValue::Uuid(_) => {}
        other => panic!("Expected Uuid, got {:?}", other),
    }
}

#[test]
fn db_request_default() {
    let req = DbRequest::default();
    assert!(req.sql.is_empty());
    assert!(req.params.is_none());
    assert!(req.data_values.is_none());
    assert!(req.dataset_id.is_none());
    assert!(req.db_id.is_none());
    assert!(req.txn_id.is_none());
}
