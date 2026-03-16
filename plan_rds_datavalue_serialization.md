# P0 数据类型序列化/反序列化修改方案

## 背景

P0 阶段已在 `cell.rs` 中新增了 4 种数据类型：

* `Binary(Vec<u8>)` - 二进制数据

* `Array(Vec<DataValue>)` - 数组类型

* `Json(String)` - JSON 字符串

* `Uuid(Uuid)` - 全局唯一标识

现在需要修改 `rds.rs` 中的 DataValue 序列化和反序列化逻辑以支持这些新类型。

***

## 问题分析

### 1. 序列化问题

当前 `rds.rs` 中的序列化逻辑 ([rds.rs:L334-357](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-core/src/model/data/dataset/rds.rs#L334-L357)) 使用：

```rust
map.serialize_entry(&field.name, value)?;
```

由于 `DataValue` 使用了 `#[serde(untagged)]` 属性，新增的类型可以正常序列化，但可能存在以下问题：

| 类型     | 当前行为                   | 潜在问题                   |
| ------ | ---------------------- | ---------------------- |
| Binary | 序列化为 JSON 数组 `[1,2,3]` | 数据库通常使用 BLOB，需要 base64 |
| Array  | 递归序列化每个元素              | 正常，无问题                 |
| Json   | 输出为转义字符串               | 可能被误解析为普通 String       |
| Uuid   | 输出为字符串                 | 正常，但可能需要标准化格式          |

### 2. 反序列化问题

当前 `rds.rs` 中的反序列化逻辑 ([rds.rs:L412-434](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-core/src/model/data/dataset/rds.rs#L412-L434)) 使用：

```rust
let dv = obj
    .get(&field.name)
    .map(|x: &serde_json::Value| serde_json::from_value::<DataValue>(x.clone()))
    .transpose()
    .map_err(|e| e.to_string())?
    .unwrap_or(DataValue::Null);
```

由于 `DataValue` 使用了 `#[serde(untagged)]`，反序列化时会尝试匹配所有变体，可能导致类型推断错误。

***

## 修改方案

### 方案 A：增强序列化策略（推荐）

为不同数据类型提供定制化序列化逻辑：

1. **Binary** → base64 编码字符串（兼容数据库 BLOB）
2. **Uuid** → 标准化字符串格式（如小写带连字符）
3. **Json** → 保持 JSON 对象格式（不被转义）
4. **Array** → 保持数组格式

### 方案 B：添加自定义序列化器

为 `DataValue` 实现 `Serialize` 和 `Deserialize` trait，手动处理每种类型的序列化/反序列化。

***

## 实施步骤

### 步骤 1：修改 cell.rs - 为 DataValue 添加自定义序列化

在 `cell.rs` 中为 `DataValue` 添加手动实现的 `Serialize` 和 `Deserialize`，替换当前的 `#[serde(untagged)]` 派生。

```rust
// 伪代码示例
impl Serialize for DataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            DataValue::Binary(v) => {
                // 序列化为带类型标记的对象或 base64 字符串
            }
            DataValue::Uuid(v) => serializer.serialize_str(&v.to_string()),
            // ... 其他类型
        }
    }
}
```

### 步骤 2：修改 rds.rs - 添加类型辅助函数

在 `rds.rs` 中添加辅助函数处理新类型的序列化和反序列化：

```rust
// 序列化辅助函数
fn serialize_datavalue(value: &DataValue) -> serde_json::Value

// 反序列化辅助函数  
fn deserialize_datavalue(value: &serde_json::Value) -> Result<DataValue, String>
```

### 步骤 3：更新 row\_from\_value 函数

修改 `row_from_value` 函数 ([rds.rs:L412](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-core/src/model/data/dataset/rds.rs#L412)) 以正确处理新类型。

### 步骤 4：添加单元测试

为新类型的序列化和反序列化添加测试用例：

```rust
#[test]
fn test_binary_serialization_in_dataset() {
    // 测试 Binary 在 DataSet 中的序列化
}

#[test]
fn test_uuid_deserialization_in_dataset() {
    // 测试 Uuid 在 DataSet 中的反序列化
}
```

***

## 详细代码修改

### 1. cell.rs 修改

#### 1.1 移除 untagged 属性，添加自定义序列化

```rust
// 修改前
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DataValue { ... }

// 修改后
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataValue { ... }

// 然后手动实现 Serialize 和 Deserialize
```

#### 1.2 实现自定义 Serialize

```rust
impl Serialize for DataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        
        match self {
            DataValue::Binary(v) => {
                // 使用 base64 编码
                let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, v);
                serializer.serialize_str(&encoded)
            }
            DataValue::Uuid(v) => {
                serializer.serialize_str(&v.to_string())
            }
            DataValue::Json(v) => {
                // 解析 JSON 字符串为 Value 后再序列化，保持对象格式
                let json_value: serde_json::Value = serde_json::from_str(v)
                    .unwrap_or(serde_json::Value::String(v.clone()));
                json_value.serialize(serializer)
            }
            DataValue::Array(v) => {
                // 递归序列化数组元素
                let mut seq = serializer.serialize_seq(Some(v.len()))?;
                for item in v {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            // 其他类型使用默认序列化
            _ => {
                // 使用 serde derive 的默认实现
                // 需要为每个变体单独实现
                unreachable!()
            }
        }
    }
}
```

#### 1.3 实现自定义 Deserialize

```rust
impl<'de> Deserialize<'de> for DataValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // 使用 serde_json::Value 作为中间类型
        let value = serde_json::Value::deserialize(deserializer)?;
        
        match value {
            serde_json::Value::Array(arr) => {
                // 尝试解析为 Binary (数字数组) 或 Array
                if arr.iter().all(|v| v.is_u64() || v.is_i64()) {
                    //认为是 Binary
                    let bytes: Vec<u8> = arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u8))
                        .collect();
                    Ok(DataValue::Binary(bytes))
                } else {
                    // 解析为 Array
                    let items: Result<Vec<DataValue>, _> = arr.iter()
                        .map(|v| DataValue::deserialize(v.clone()))
                        .collect();
                    Ok(DataValue::Array(items?))
                }
            }
            serde_json::Value::String(s) => {
                // 尝试解析为 Uuid、Json 或普通 String
                if let Ok(uuid) = Uuid::parse_str(&s) {
                    Ok(DataValue::Uuid(uuid))
                } else if s.starts_with('{') || s.starts_with('[') {
                    // 尝试解析为 JSON
                    if serde_json::from_str::<serde_json::Value>(&s).is_ok() {
                        Ok(DataValue::Json(s))
                    } else {
                        Ok(DataValue::String(s))
                    }
                } else {
                    Ok(DataValue::String(s))
                }
            }
            // 其他类型使用默认反序列化
            _ => {
                // 使用 serde derive 的默认实现
                Err(serde::de::Error::custom("unsupported type"))
            }
        }
    }
}
```

### 2. rds.rs 修改

#### 2.1 添加辅助函数

```rust
/// 将 DataValue 序列化为 serde_json::Value（用于 DataSet 内部）
pub fn serialize_datavalue(value: &DataValue) -> serde_json::Value {
    match value {
        DataValue::Binary(v) => {
            // 序列化为 base64 字符串，添加类型标记以便反序列化时识别
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD, 
                v
            );
            serde_json::json!({ "__binary": encoded })
        }
        DataValue::Uuid(v) => {
            serde_json::json!({ "__uuid": v.to_string() })
        }
        DataValue::Json(v) => {
            // 尝试解析为 JSON Value
            serde_json::from_str(v).unwrap_or(serde_json::Value::String(v.clone()))
        }
        DataValue::Array(items) => {
            serde_json::Value::Array(
                items.iter().map(serialize_datavalue).collect()
            )
        }
        DataValue::Null => serde_json::Value::Null,
        DataValue::Bool(b) => serde_json::Value::Bool(*b),
        DataValue::Int(i) => serde_json::Value::Number((*i).into()),
        DataValue::Float(f) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        DataValue::String(s) => serde_json::Value::String(s.clone()),
        DataValue::Decimal(d) => {
            serde_json::Value::String(d.to_string())
        }
        DataValue::DateTime(dt) => {
            serde_json::Value::String(dt.to_rfc3339())
        }
        DataValue::Date(d) => {
            serde_json::Value::String(d.to_string())
        }
        DataValue::ShortStr(s) => serde_json::Value::String(s.to_string()),
        DataValue::LongStr(s) => serde_json::Value::String(s.to_string()),
    }
}

/// 从 serde_json::Value 反序列化为 DataValue
pub fn deserialize_datavalue(value: &serde_json::Value) -> Result<DataValue, String> {
    match value {
        serde_json::Value::Object(obj) => {
            // 检查类型标记
            if let Some(encoded) = obj.get("__binary").and_then(|v| v.as_str()) {
                let bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    encoded
                ).map_err(|e| e.to_string())?;
                return Ok(DataValue::Binary(bytes));
            }
            if let Some(s) = obj.get("__uuid").and_then(|v| v.as_str()) {
                let uuid = Uuid::parse_str(s).map_err(|e| e.to_string())?;
                return Ok(DataValue::Uuid(uuid));
            }
            // 普通对象，递归处理
            Err("unsupported object type".to_string())
        }
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<DataValue>, _> = arr.iter()
                .map(deserialize_datavalue)
                .collect();
            Ok(DataValue::Array(items?))
        }
        serde_json::Value::String(s) => {
            if let Ok(uuid) = Uuid::parse_str(s) {
                return Ok(DataValue::Uuid(uuid));
            }
            Ok(DataValue::String(s.clone()))
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(DataValue::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(DataValue::Float(f))
            } else {
                Err("invalid number".to_string())
            }
        }
        serde_json::Value::Bool(b) => Ok(DataValue::Bool(*b)),
        serde_json::Value::Null => Ok(DataValue::Null),
    }
}
```

#### 2.2 修改 row\_from\_value 函数

```rust
fn row_from_value(v: serde_json::Value, schema: &Schema) -> Result<Row, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "row 应为对象".to_string())?;
    
    let mut values = Vec::with_capacity(schema.field_count());
    
    for field in &schema.fields {
        let dv = obj
            .get(&field.name)
            .map(|x| {
                // 使用辅助函数进行反序列化
                deserialize_datavalue(x)
            })
            .transpose()
            .map_err(|e: String| e)?
            .unwrap_or(DataValue::Null);
        values.push(dv);
    }
    
    let mut children = HashMap::new();
    for (k, val) in obj.iter() {
        if schema.get_index(k).is_none() {
            let ds = serde_json::from_value::<DataSet>(val.clone())
                .map_err(|e| e.to_string())?;
            children.insert(k.clone(), ds);
        }
    }
    
    Ok(Row { values, children })
}
```

***

## 测试用例

### 测试 1：Binary 在 DataSet 中的序列化

```rust
#[test]
fn test_dataset_binary_serialization() {
    // 创建包含 Binary 字段的 Schema
    let schema = Arc::new(Schema::new("test", vec![
        Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
        Field { name: "attachment".into(), field_type: FieldType::Binary, label: "附件".into() },
    ]));
    
    // 创建包含 Binary 数据的行
    let mut row = Row::new(vec![
        DataValue::Int(1),
        DataValue::Binary(vec![0x00, 0x01, 0x02]),
    ]);
    
    // 创建 DataSet
    let mut ds = DataSet::empty("test", schema);
    ds.add_row(row);
    
    // 序列化
    let json = serde_json::to_string(&ds).unwrap();
    
    // 验证 Binary 被正确序列化为 base64
    assert!(json.contains("AAEC"));
}
```

### 测试 2：Uuid 在 DataSet 中的反序列化

```rust
#[test]
fn test_dataset_uuid_deserialization() {
    let json = r#"{
        "id": "test",
        "schema": {...},
        "rows": [{"id": 1, "order_id": "550e8400-e29b-41d4-a716-446655440000"}]
    }"#;
    
    let ds: DataSet = serde_json::from_str(json).unwrap();
    
    // 验证 Uuid 被正确解析
    let row = &ds.rows[0];
    if let DataValue::Uuid(uuid) = &row.values[1] {
        assert_eq!(uuid.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    } else {
        panic!("Expected Uuid");
    }
}
```

### 测试 3：Array 在 DataSet 中的序列化

```rust
#[test]
fn test_dataset_array_serialization() {
    let schema = Arc::new(Schema::new("test", vec![
        Field { name: "tags".into(), field_type: FieldType::Array, label: "标签".into() },
    ]));
    
    let row = Row::new(vec![
        DataValue::Array(vec![
            DataValue::String("VIP".to_string()),
            DataValue::String("急单".to_string()),
        ]),
    ]);
    
    let mut ds = DataSet::empty("test", schema);
    ds.add_row(row);
    
    let json = serde_json::to_string(&ds).unwrap();
    
    // 验证 Array 被正确序列化
    assert!(json.contains("VIP"));
    assert!(json.contains("急单"));
}
```

***

## 风险评估

| 风险          | 影响              | 缓解措施          |
| ----------- | --------------- | ------------- |
| 序列化格式变化     | 现有数据可能不兼容       | 添加版本字段或提供迁移脚本 |
| base64 编码性能 | 大文件处理可能慢        | 使用流式处理或异步编码   |
| UUID 格式识别   | 错误格式可能导致解析失败    | 添加严格的格式验证     |
| Json 解析失败   | 格式错误的 JSON 无法处理 | 添加错误日志和默认值    |

***

## 时间估算

| 任务                               | 预估时间     |
| -------------------------------- | -------- |
| 修改 cell.rs Serialize/Deserialize | 2 小时     |
| 修改 rds.rs 辅助函数                   | 1 小时     |
| 修改 row\_from\_value              | 0.5 小时   |
| 添加单元测试                           | 1.5 小时   |
| **总计**                           | **5 小时** |

