pub mod rds;

use std::collections::HashMap;
use serde::{Deserialize, Serialize, Serializer};

/// Schema 定义了数据集的结构  
/// 核心优化：维护 name -> index 的映射，实现 O(1) 查找  
#[derive(Debug, Clone)]  
pub struct Schema {  
    /// Schema 唯一标识  
    pub id: String,  
    pub fields: Vec<Field>,  
    index_map: HashMap<String, usize>,  
}  

/// 用于 Schema 序列化/反序列化（不包含 index_map）  
#[derive(Debug, Serialize, Deserialize)]  
struct SchemaSerde {  
    id: String,  
    fields: Vec<Field>,  
}  
  
impl Schema {  
    /// 创建 Schema，字段名必须唯一，重复会 panic  
    pub fn new(id: impl Into<String>, fields: Vec<Field>) -> Self {  
        let mut index_map = HashMap::with_capacity(fields.len());  
        for (i, field) in fields.iter().enumerate() {  
            if index_map.insert(field.name.clone(), i).is_some() {  
                panic!("Schema 字段名重复: {}", field.name);  
            }  
        }  
        Schema { id: id.into(), fields, index_map }  
    }  
  
    pub fn get_index(&self, name: &str) -> Option<usize> {  
        self.index_map.get(name).copied()  
    }  

    #[inline]  
    pub fn field_count(&self) -> usize {  
        self.fields.len()  
    }  

    #[inline]  
    pub fn is_empty(&self) -> bool {  
        self.fields.is_empty()  
    }  
}  

impl Serialize for Schema {  
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>  
    where  
        S: Serializer,  
    {  
        SchemaSerde { id: self.id.clone(), fields: self.fields.clone() }.serialize(serializer)  
    }  
}  

impl<'de> Deserialize<'de> for Schema {  
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>  
    where  
        D: serde::Deserializer<'de>,  
    {  
        let s = SchemaSerde::deserialize(deserializer)?;  
        Ok(Schema::new(s.id, s.fields))  
    }  
}  

// ==========================================
// 3. 行式数据集 (Row / DataSet) 在 rds 子模块
// ==========================================


pub use rds::{DataSet, Row};

use crate::model::cell::Field;

/// 兼容旧命名：TableSchema 即当前 Schema
pub type TableSchema = Schema;

// ==========================================  
// 4. 使用演示 / 测试  
// ==========================================  
  
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use crate::model::cell::{DataValue, FieldType};

    use super::*;
  
    #[test]  
    fn test_nested_structure() {  
        // --- 1. 定义 Schema ---  
          
        // 订单头 Schema  
        let header_schema = Arc::new(Schema::new("order_header", vec![  
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },  
            Field { name: "doc_no".into(), field_type: FieldType::String, label: "单号".into() },  
        ]));  
  
        // 订单行 Schema  
        let line_schema = Arc::new(Schema::new("order_line", vec![  
            Field { name: "line_id".into(), field_type: FieldType::Int, label: "行ID".into() },  
            Field { name: "material".into(), field_type: FieldType::String, label: "物料".into() },  
            Field { name: "qty".into(), field_type: FieldType::Int, label: "数量".into() },  
        ]));  
  
        // --- 2. 构建数据 ---  
  
        // A. 构建子数据集 (订单行)  
        let mut lines_ds = DataSet::empty("order_lines", line_schema.clone());  
        lines_ds.add_row(Row::new(vec![10.into(), "Mat-A".into(), 5.into()]));  
        lines_ds.add_row(Row::new(vec![20.into(), "Mat-B".into(), 2.into()]));  
  
        // B. 构建主数据行 (订单头)  
        let mut header_row = Row::new(vec![  
            1001.into(),   
            "SO-20231027".into()  
        ]);  
  
        // C. 关键一步：将子数据集挂载到主行上  
        header_row.add_child("order_lines", lines_ds);  
  
        // D. 放入主数据集  
        let mut header_ds = DataSet::empty("orders", header_schema.clone());  
        header_ds.add_row(header_row);  
  
        // --- 3. 序列化验证 ---  
          
        let json = serde_json::to_string_pretty(&header_ds).unwrap();  
        println!("{}", json);  
  
        /*   
        期望输出:  
        [  
          {  
            "id": 1001,  
            "doc_no": "SO-20231027",  
            "order_lines": [  <-- 这是一个嵌套的 DataSet  
              {  
                "line_id": 10,  
                "material": "Mat-A",  
                "qty": 5  
              },  
              {  
                "line_id": 20,  
                "material": "Mat-B",  
                "qty": 2  
              }  
            ]  
          }  
        ]  
        */  
    }  

    #[test]  
    fn test_roundtrip_serialize_deserialize() {  
        let schema = Arc::new(Schema::new("t1", vec![  
            Field { name: "a".into(), field_type: FieldType::Int, label: "A".into() },  
            Field { name: "b".into(), field_type: FieldType::String, label: "B".into() },  
        ]));  
        let mut ds = DataSet::empty("d1", schema.clone());  
        ds.add_row(Row::new(vec![1.into(), "x".into()]));  
        let json = serde_json::to_string(&ds).unwrap();  
        let ds2: DataSet = serde_json::from_str(&json).unwrap();  
        assert_eq!(ds2.id, "d1");  
        assert_eq!(ds2.schema.id, "t1");  
        assert_eq!(ds2.row_count(), 1);  
        assert_eq!(ds2.iter().next().unwrap().get_by_name(ds2.schema.as_ref(), "a"), Some(&DataValue::Int(1)));  
    }

    // ========================================
    // P0 新增类型测试
    // ========================================

    /// 测试 Binary 类型在 DataSet 中的序列化
    #[test]
    fn test_dataset_binary_serialization() {
        let schema = Arc::new(Schema::new("attachments", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "file_data".into(), field_type: FieldType::Binary, label: "文件内容".into() },
        ]));
        
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Binary(vec![0x00, 0x01, 0x02, 0xFF]),
        ]);
        
        let mut ds = DataSet::empty("attachments", schema);
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // Binary 序列化为 base64 字符串
        assert!(json.contains("AAEC/w==")); // 0x00, 0x01, 0x02, 0xFF 的 base64
    }

    /// 测试 Binary 类型在 DataSet 中的反序列化
    #[test]
    fn test_dataset_binary_deserialization() {
        let schema = Arc::new(Schema::new("attachments", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "file_data".into(), field_type: FieldType::Binary, label: "文件内容".into() },
        ]));
        
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Binary(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        ]);
        
        let mut ds = DataSet::empty("attachments", schema.clone());
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // 反序列化
        let ds2: DataSet = serde_json::from_str(&json).unwrap();
        
        let row = &ds2.rows[0];
        // 检查第一个字段
        assert_eq!(row.values[0], DataValue::Int(1));
        
        // 检查第二个字段是 Binary
        match &row.values[1] {
            DataValue::Binary(data) => {
                assert_eq!(data, &vec![0xDE, 0xAD, 0xBE, 0xEF]);
            }
            other => panic!("Expected Binary type, got {:?}", other),
        }
    }

    /// 测试 Uuid 类型在 DataSet 中的序列化
    #[test]
    fn test_dataset_uuid_serialization() {
        use uuid::Uuid;
        
        let schema = Arc::new(Schema::new("orders", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "order_uuid".into(), field_type: FieldType::Uuid, label: "订单UUID".into() },
        ]));
        
        let order_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Uuid(order_uuid),
        ]);
        
        let mut ds = DataSet::empty("orders", schema);
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // Uuid 序列化为标准字符串
        assert!(json.contains("550e8400-e29b-41d4-a716-446655440000"));
    }

    /// 测试 Uuid 类型在 DataSet 中的反序列化
    #[test]
    fn test_dataset_uuid_deserialization() {
        use uuid::Uuid;
        
        let schema = Arc::new(Schema::new("orders", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "order_uuid".into(), field_type: FieldType::Uuid, label: "订单UUID".into() },
        ]));
        
        let order_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Uuid(order_uuid),
        ]);
        
        let mut ds = DataSet::empty("orders", schema.clone());
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // 反序列化
        let ds2: DataSet = serde_json::from_str(&json).unwrap();
        
        let row = &ds2.rows[0];
        if let DataValue::Uuid(uuid) = &row.values[1] {
            assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
        } else {
            panic!("Expected Uuid type");
        }
    }

    /// 测试 Array 类型在 DataSet 中的序列化
    #[test]
    fn test_dataset_array_serialization() {
        let schema = Arc::new(Schema::new("orders", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "tags".into(), field_type: FieldType::Array, label: "标签".into() },
        ]));
        
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Array(vec![
                DataValue::String("VIP".to_string()),
                DataValue::String("急单".to_string()),
            ]),
        ]);
        
        let mut ds = DataSet::empty("orders", schema);
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // Array 序列化为 JSON 数组
        assert!(json.contains("VIP"));
        assert!(json.contains("急单"));
    }

    /// 测试 Array 类型在 DataSet 中的反序列化
    #[test]
    fn test_dataset_array_deserialization() {
        let schema = Arc::new(Schema::new("orders", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "tags".into(), field_type: FieldType::Array, label: "标签".into() },
        ]));
        
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Array(vec![
                DataValue::String("VIP".to_string()),
                DataValue::String("急单".to_string()),
            ]),
        ]);
        
        let mut ds = DataSet::empty("orders", schema.clone());
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // 反序列化
        let ds2: DataSet = serde_json::from_str(&json).unwrap();
        
        // 只添加了一行，所以是 rows[0]
        let row = &ds2.rows[0];
        if let DataValue::Array(items) = &row.values[1] {
            assert_eq!(items.len(), 2);
            if let DataValue::String(s) = &items[0] {
                assert_eq!(s, "VIP");
            }
        } else {
            panic!("Expected Array type");
        }
    }

    /// 测试 Json 类型在 DataSet 中的序列化
    #[test]
    fn test_dataset_json_serialization() {
        let schema = Arc::new(Schema::new("customers", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "custom_fields".into(), field_type: FieldType::Json, label: "自定义字段".into() },
        ]));
        
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Json(r#"{"vip_level":"Gold","credit_limit":100000}"#.to_string()),
        ]);
        
        let mut ds = DataSet::empty("customers", schema);
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // Json 序列化为 JSON 对象
        assert!(json.contains("vip_level"));
        assert!(json.contains("Gold"));
    }

    /// 测试 Json 类型在 DataSet 中的反序列化
    #[test]
    fn test_dataset_json_deserialization() {
        let schema = Arc::new(Schema::new("customers", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "custom_fields".into(), field_type: FieldType::Json, label: "自定义字段".into() },
        ]));
        
        let row = Row::new(vec![
            DataValue::Int(1),
            DataValue::Json(r#"{"vip_level":"Gold","credit_limit":100000}"#.to_string()),
        ]);
        
        let mut ds = DataSet::empty("customers", schema.clone());
        ds.add_row(row);
        
        let json = serde_json::to_string(&ds).unwrap();
        
        // 反序列化
        let ds2: DataSet = serde_json::from_str(&json).unwrap();
        
        let row = &ds2.rows[0];
        if let DataValue::Json(s) = &row.values[1] {
            assert!(s.contains("vip_level"));
        } else {
            panic!("Expected Json type");
        }
    }

    /// 测试嵌套 DataSet 中的 P0 新类型
    #[test]
    fn test_dataset_nested_with_p0_types() {
        use uuid::Uuid;
        
        // 订单头 Schema
        let header_schema = Arc::new(Schema::new("order_header", vec![
            Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
            Field { name: "order_uuid".into(), field_type: FieldType::Uuid, label: "订单UUID".into() },
            Field { name: "tags".into(), field_type: FieldType::Array, label: "标签".into() },
        ]));

        // 附件 Schema
        let attachment_schema = Arc::new(Schema::new("attachments", vec![
            Field { name: "file_name".into(), field_type: FieldType::String, label: "文件名".into() },
            Field { name: "file_data".into(), field_type: FieldType::Binary, label: "文件内容".into() },
        ]));

        // 构建附件 DataSet
        let mut attachments_ds = DataSet::empty("attachments", attachment_schema);
        attachments_ds.add_row(Row::new(vec![
            DataValue::String("合同.pdf".to_string()),
            DataValue::Binary(vec![0x25, 0x50, 0x44, 0x46]), // "%PDF"
        ]));

        // 构建订单头行
        let order_uuid = Uuid::new_v4();
        let mut header_row = Row::new(vec![
            DataValue::Int(1001),
            DataValue::Uuid(order_uuid),
            DataValue::Array(vec![
                DataValue::String("VIP".to_string()),
                DataValue::String("出口".to_string()),
            ]),
        ]);
        header_row.add_child("attachments", attachments_ds);

        // 构建主 DataSet
        let mut header_ds = DataSet::empty("orders", header_schema);
        header_ds.add_row(header_row);

        // 序列化
        let json = serde_json::to_string(&header_ds).unwrap();
        
        // 反序列化
        let ds2: DataSet = serde_json::from_str(&json).unwrap();
        
        // 验证
        assert_eq!(ds2.row_count(), 1);
    }
}
