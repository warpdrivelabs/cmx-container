//! Dataset 序列化/反序列化集成测试
//!
//! 测试 cmx-core 中 Dataset 结构的序列化和反序列化功能

use cmx_core::model::cell::{DataValue, Field, FieldType};
use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use std::sync::Arc;
use uuid::Uuid;

/// 创建带有多类型字段的 Schema
fn create_test_schema() -> Arc<Schema> {
    Arc::new(Schema::new_unchecked(
        "test_table",
        vec![
            Field {
                name: "id".into(),
                field_type: FieldType::Int,
                label: "ID".into(),
            },
            Field {
                name: "name".into(),
                field_type: FieldType::String,
                label: "名称".into(),
            },
            Field {
                name: "price".into(),
                field_type: FieldType::Decimal,
                label: "价格".into(),
            },
            Field {
                name: "created_at".into(),
                field_type: FieldType::DateTime,
                label: "创建时间".into(),
            },
            Field {
                name: "is_active".into(),
                field_type: FieldType::Bool,
                label: "是否激活".into(),
            },
            Field {
                name: "file_data".into(),
                field_type: FieldType::Binary,
                label: "文件数据".into(),
            },
            Field {
                name: "tags".into(),
                field_type: FieldType::Array,
                label: "标签".into(),
            },
            Field {
                name: "metadata".into(),
                field_type: FieldType::Json,
                label: "元数据".into(),
            },
            Field {
                name: "record_id".into(),
                field_type: FieldType::Uuid,
                label: "记录ID".into(),
            },
        ],
    ))
}

#[test]
fn test_dataset_basic_serialization() {
    let schema = create_test_schema();

    let row = Row::new(vec![
        DataValue::Int(1),
        DataValue::String("Test Product".into()),
        DataValue::Decimal("99.99".parse().unwrap()),
        DataValue::DateTime(
            chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
        DataValue::Bool(true),
        DataValue::Binary(vec![0x00, 0x01, 0x02, 0xFF]),
        DataValue::Array(vec![
            DataValue::String("VIP".into()),
            DataValue::String("热卖".into()),
        ]),
        DataValue::Json(r#"{"color":"red","size":"L"}"#.into()),
        DataValue::Uuid(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()),
    ]);

    let mut ds = DataSet::empty("products", schema);
    ds.add_row(row);

    let json = serde_json::to_string_pretty(&ds).unwrap();
    println!("[test_dataset_basic_serialization] JSON:\n{}", json);

    assert!(json.contains("\"id\": 1"));
    assert!(json.contains("\"name\": \"Test Product\""));
    assert!(json.contains("\"is_active\": true"));
}

#[test]
fn test_dataset_roundtrip_all_types() {
    let schema = create_test_schema();

    let original_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let row = Row::new(vec![
        DataValue::Int(42),
        DataValue::String("Complete Test".into()),
        DataValue::Decimal("123.45".parse().unwrap()),
        DataValue::DateTime(
            chrono::DateTime::parse_from_rfc3339("2024-06-20T15:45:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
        DataValue::Bool(false),
        DataValue::Binary(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        DataValue::Array(vec![
            DataValue::String("tag1".into()),
            DataValue::String("tag2".into()),
            DataValue::String("tag3".into()),
        ]),
        DataValue::Json(r#"{"key":"value","num":100}"#.into()),
        DataValue::Uuid(original_uuid),
    ]);

    let mut original_ds = DataSet::empty("roundtrip_test", schema);
    original_ds.add_row(row);

    let json = serde_json::to_string(&original_ds).unwrap();
    println!(
        "[test_dataset_roundtrip_all_types] Serialized JSON:\n{}",
        json
    );

    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized_ds.id, "roundtrip_test");
    assert_eq!(deserialized_ds.row_count(), 1);

    let deserialized_row = deserialized_ds.iter().next().unwrap();
    let schema = deserialized_ds.schema.as_ref();

    assert_eq!(
        deserialized_row.get_by_name(schema, "id"),
        Some(&DataValue::Int(42))
    );
    assert_eq!(
        deserialized_row.get_by_name(schema, "name"),
        Some(&DataValue::String("Complete Test".into()))
    );
    assert_eq!(
        deserialized_row.get_by_name(schema, "is_active"),
        Some(&DataValue::Bool(false))
    );

    if let Some(DataValue::Binary(bytes)) = deserialized_row.get_by_name(schema, "file_data") {
        assert_eq!(bytes, &vec![0xDE, 0xAD, 0xBE, 0xEF]);
    } else {
        panic!("Expected Binary type at file_data");
    }

    if let Some(DataValue::Array(items)) = deserialized_row.get_by_name(schema, "tags") {
        assert_eq!(items.len(), 3);
    } else {
        panic!("Expected Array type at tags");
    }

    if let Some(DataValue::Json(json_str)) = deserialized_row.get_by_name(schema, "metadata") {
        assert!(json_str.contains("key"));
    } else {
        panic!("Expected Json type at metadata");
    }

    if let Some(DataValue::Uuid(uuid)) = deserialized_row.get_by_name(schema, "record_id") {
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    } else {
        panic!("Expected Uuid type at record_id");
    }
}

#[test]
fn test_dataset_nested_serialization() {
    let header_schema = Arc::new(Schema::new_unchecked(
        "order_header",
        vec![
            Field {
                name: "order_id".into(),
                field_type: FieldType::Int,
                label: "订单ID".into(),
            },
            Field {
                name: "order_no".into(),
                field_type: FieldType::String,
                label: "订单号".into(),
            },
            Field {
                name: "total_amount".into(),
                field_type: FieldType::Decimal,
                label: "总金额".into(),
            },
        ],
    ));

    let line_schema = Arc::new(Schema::new_unchecked(
        "order_line",
        vec![
            Field {
                name: "line_id".into(),
                field_type: FieldType::Int,
                label: "行ID".into(),
            },
            Field {
                name: "material_code".into(),
                field_type: FieldType::String,
                label: "物料编码".into(),
            },
            Field {
                name: "quantity".into(),
                field_type: FieldType::Int,
                label: "数量".into(),
            },
            Field {
                name: "unit_price".into(),
                field_type: FieldType::Decimal,
                label: "单价".into(),
            },
        ],
    ));

    let attachment_schema = Arc::new(Schema::new_unchecked(
        "attachments",
        vec![
            Field {
                name: "file_name".into(),
                field_type: FieldType::String,
                label: "文件名".into(),
            },
            Field {
                name: "file_content".into(),
                field_type: FieldType::Binary,
                label: "文件内容".into(),
            },
        ],
    ));

    let mut attachments_ds = DataSet::empty("attachments", attachment_schema);
    attachments_ds.add_row(Row::new(vec![
        DataValue::String("合同.pdf".into()),
        DataValue::Binary(vec![0x25, 0x50, 0x44, 0x46, 0x00, 0x00]),
    ]));

    let mut order_lines_ds = DataSet::empty("order_lines", line_schema.clone());
    order_lines_ds.add_row(Row::new(vec![
        DataValue::Int(1),
        DataValue::String("MAT-001".into()),
        DataValue::Int(10),
        DataValue::Decimal("100.00".parse().unwrap()),
    ]));
    order_lines_ds.add_row(Row::new(vec![
        DataValue::Int(2),
        DataValue::String("MAT-002".into()),
        DataValue::Int(5),
        DataValue::Decimal("50.00".parse().unwrap()),
    ]));

    let mut header_row = Row::new(vec![
        DataValue::Int(1001),
        DataValue::String("SO-2024-001".into()),
        DataValue::Decimal("1250.00".parse().unwrap()),
    ]);
    header_row.add_child("order_lines", order_lines_ds);
    header_row.add_child("attachments", attachments_ds);

    let mut header_ds = DataSet::empty("orders", header_schema);
    header_ds.add_row(header_row);

    let json = serde_json::to_string_pretty(&header_ds).unwrap();
    println!("[test_dataset_nested_serialization] Nested JSON:\n{}", json);

    assert!(json.contains("\"order_no\": \"SO-2024-001\""));
    assert!(json.contains("\"order_lines\""));
    assert!(json.contains("\"attachments\""));
    assert!(json.contains("\"MAT-001\""));
    assert!(json.contains("\"合同.pdf\""));

    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized_ds.row_count(), 1);

    let deserialized_row = deserialized_ds.iter().next().unwrap();

    let order_lines = deserialized_row.get_child("order_lines");
    assert!(order_lines.is_some());
    let order_lines_ds = order_lines.unwrap();
    assert_eq!(order_lines_ds.row_count(), 2);

    let attachments = deserialized_row.get_child("attachments");
    assert!(attachments.is_some());
    let attachments_ds = attachments.unwrap();
    assert_eq!(attachments_ds.row_count(), 1);
}

#[test]
fn test_dataset_empty_and_null_handling() {
    let schema = Arc::new(Schema::new_unchecked(
        "nullable_test",
        vec![
            Field {
                name: "id".into(),
                field_type: FieldType::Int,
                label: "ID".into(),
            },
            Field {
                name: "value".into(),
                field_type: FieldType::String,
                label: "值".into(),
            },
            Field {
                name: "optional_data".into(),
                field_type: FieldType::Binary,
                label: "可选数据".into(),
            },
        ],
    ));

    let mut ds = DataSet::empty("nullable", schema.clone());
    ds.add_row(Row::new(vec![
        DataValue::Int(1),
        DataValue::Null,
        DataValue::Null,
    ]));

    let json = serde_json::to_string(&ds).unwrap();
    println!(
        "[test_dataset_empty_and_null_handling] JSON with Null: {}",
        json
    );

    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized_ds.row_count(), 1);
    let row = deserialized_ds.iter().next().unwrap();
    let schema = deserialized_ds.schema.as_ref();

    assert_eq!(row.get_by_name(schema, "id"), Some(&DataValue::Int(1)));
    assert_eq!(row.get_by_name(schema, "value"), Some(&DataValue::Null));
    assert_eq!(
        row.get_by_name(schema, "optional_data"),
        Some(&DataValue::Null)
    );
}

#[test]
fn test_dataset_multiple_rows() {
    let schema = Arc::new(Schema::new_unchecked(
        "multi_row",
        vec![
            Field {
                name: "id".into(),
                field_type: FieldType::Int,
                label: "ID".into(),
            },
            Field {
                name: "name".into(),
                field_type: FieldType::String,
                label: "名称".into(),
            },
        ],
    ));

    let mut ds = DataSet::empty("multiple_rows", schema);

    for i in 1..=5 {
        let row = Row::new(vec![
            DataValue::Int(i as i64),
            DataValue::String(format!("Item {}", i)),
        ]);
        ds.add_row(row);
    }

    assert_eq!(ds.row_count(), 5);

    let json = serde_json::to_string(&ds).unwrap();
    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized_ds.row_count(), 5);

    let schema = deserialized_ds.schema.as_ref();
    for (i, row) in deserialized_ds.iter().enumerate() {
        let expected_id = (i + 1) as i64;
        let expected_name = format!("Item {}", i + 1);
        assert_eq!(
            row.get_by_name(schema, "id"),
            Some(&DataValue::Int(expected_id))
        );
        assert_eq!(
            row.get_by_name(schema, "name"),
            Some(&DataValue::String(expected_name))
        );
    }
}

#[test]
fn test_dataset_binary_with_special_characters() {
    let schema = Arc::new(Schema::new_unchecked(
        "binary_test",
        vec![
            Field {
                name: "id".into(),
                field_type: FieldType::Int,
                label: "ID".into(),
            },
            Field {
                name: "data".into(),
                field_type: FieldType::Binary,
                label: "二进制数据".into(),
            },
        ],
    ));

    let test_bytes: Vec<u8> = (0..=255).collect();

    let row = Row::new(vec![
        DataValue::Int(1),
        DataValue::Binary(test_bytes.clone()),
    ]);

    let mut ds = DataSet::empty("binary_full_range", schema);
    ds.add_row(row);

    let json = serde_json::to_string(&ds).unwrap();
    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized_ds.row_count(), 1);

    let row = deserialized_ds.iter().next().unwrap();
    let schema = deserialized_ds.schema.as_ref();

    if let Some(DataValue::Binary(decoded_bytes)) = row.get_by_name(schema, "data") {
        assert_eq!(decoded_bytes.len(), 256);
        assert_eq!(decoded_bytes[0], 0);
        assert_eq!(decoded_bytes[128], 128);
        assert_eq!(decoded_bytes[255], 255);
    } else {
        panic!("Expected Binary type");
    }
}

#[test]
fn test_dataset_with_unicode_content() {
    let schema = Arc::new(Schema::new_unchecked(
        "unicode_test",
        vec![
            Field {
                name: "id".into(),
                field_type: FieldType::Int,
                label: "ID".into(),
            },
            Field {
                name: "chinese_text".into(),
                field_type: FieldType::String,
                label: "中文文本".into(),
            },
            Field {
                name: "tags".into(),
                field_type: FieldType::Array,
                label: "标签".into(),
            },
        ],
    ));

    let row = Row::new(vec![
        DataValue::Int(1),
        DataValue::String("简体中文、繁體中文、日本語、한국어".into()),
        DataValue::Array(vec![
            DataValue::String("高端产品".into()),
            DataValue::String("热销🔥".into()),
            DataValue::String("NEW📦".into()),
        ]),
    ]);

    let mut ds = DataSet::empty("unicode", schema);
    ds.add_row(row);

    let json = serde_json::to_string_pretty(&ds).unwrap();
    println!(
        "[test_dataset_with_unicode_content] Unicode JSON:\n{}",
        json
    );

    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    let row = deserialized_ds.iter().next().unwrap();
    let schema = deserialized_ds.schema.as_ref();

    if let Some(DataValue::String(text)) = row.get_by_name(schema, "chinese_text") {
        assert!(text.contains("简体中文"));
        assert!(text.contains("繁體中文"));
    } else {
        panic!("Expected String type");
    }

    if let Some(DataValue::Array(tags)) = row.get_by_name(schema, "tags") {
        assert_eq!(tags.len(), 3);
    } else {
        panic!("Expected Array type");
    }
}

#[test]
fn test_dataset_schema_id_preserved() {
    let schema = Arc::new(Schema::new_unchecked(
        "schema_identity_test",
        vec![Field {
            name: "field1".into(),
            field_type: FieldType::Int,
            label: "字段1".into(),
        }],
    ));

    let row = Row::new(vec![DataValue::Int(100)]);

    let mut ds = DataSet::empty("dataset_with_schema", schema.clone());
    ds.add_row(row);

    let json = serde_json::to_string(&ds).unwrap();
    let deserialized_ds: DataSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized_ds.schema.id, "schema_identity_test");
    assert_eq!(deserialized_ds.schema.field_count(), 1);
    assert_eq!(deserialized_ds.schema.fields[0].name, "field1");
}
