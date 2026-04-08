# DataValue 序列化/反序列化优化分析计划

## 一、分析范围

- **目标文件**：`cmx-core/src/model/cell.rs`（DataValue 定义及序列化实现）
- **关联文件**：`cmx-core/src/model/data/dataset/rds.rs`（DataSet/Row 序列化）
- **依赖库**：serde, serde_json, chrono, uuid, rust_decimal, base64, smol_str

## 二、当前实现分析

### 2.1 DataValue 枚举（cell.rs:30-51）

当前 DataValue 共有 **14 个变体**：

| 变体 | 存储类型 | 序列化格式 | 反序列化识别方式 |
|------|----------|------------|------------------|
| Null | - | `null` | JSON null |
| Bool | bool | 布尔值 | JSON boolean |
| Int | i64 | 整数 | JSON number → i64 |
| Float | f64 | newtype_struct "Float" | JSON number → f64 |
| String | String | 字符串 | 回退为 String |
| Decimal | Decimal | 字符串 | 回退为 String |
| DateTime | DateTime<Utc> | RFC3339 字符串 | RFC3339 解析 |
| Date | NaiveDate | 日期字符串 | "%Y-%m-%d" 解析 |
| Binary | Vec<u8> | base64 字符串 | 数组[0-255] 或 base64（已注释掉） |
| Array | Vec<DataValue> | JSON 数组 | JSON array（非全0-255） |
| Json | String | 字符串 | 以 `{` 或 `[` 开头的字符串 |
| Uuid | Uuid | 字符串 | UUID 格式解析 |
| ShortStr | SmolStr | 字符串 | 回退为 String |
| LongStr | SmolStr | 字符串 | 回退为 String |

### 2.2 序列化实现问题分析

#### 问题 1：`untagged` 式反序列化的歧义性（严重）

当前实现虽然没有使用 `#[serde(untagged)]`，但手动实现了类似 untagged 的语义，存在大量类型歧义：

**场景 A：UUID 字符串丢失类型**
```
序列化：DataValue::Uuid("550e8400-...") → "\"550e8400-...\""
反序列化："550e8400-..." → DataValue::Uuid(...)  ✅ 可以识别
```

**场景 B：日期字符串丢失类型**
```
序列化：DataValue::Date("2024-01-01") → "\"2024-01-01\""
反序列化："2024-01-01" → DataValue::Date(...)  ✅ 可以识别
```

**场景 C：普通字符串被误识别**
```
序列化：DataValue::String("2024-01-01") → "\"2024-01-01\""
反序列化："2024-01-01" → DataValue::Date(...)  ❌ 类型被错误改变！
```

**场景 D：Decimal 字符串被误识别**
```
序列化：DataValue::Decimal("123.45") → "\"123.45\""
反序列化："123.45" → DataValue::String(...)  ❌ 类型丢失！
```

**场景 E：UUID 字符串被误识别为 Uuid**
```
序列化：DataValue::String("550e8400-e29b-41d4-a716-446655440000") → "\"550e8400-...\""
反序列化："550e8400-..." → DataValue::Uuid(...)  ❌ 类型被错误改变！
```

**场景 F：Json 字段序列化后再反序列化变为 String**
```
序列化：DataValue::Json("{\"key\":\"value\"}") → "\"{\\\"key\\\":\\\"value\\\"}\""  (双层转义)
反序列化：转义后的字符串不以 { 开头 → DataValue::String(...)  ❌ 类型丢失！
```

**核心问题**：序列化→反序列化 **往返不一致（roundtrip lossy）**，数据类型信息在序列化时被丢弃。

#### 问题 2：Binary 类型检测逻辑有缺陷

当前反序列化时，`[0, 1, 2]` 会被识别为 Binary（因为所有元素 ≤255），但 `[1, 2, 300]` 会被识别为 Array。这意味着：

- `Vec<i32>` 类型的数组如果值恰好在 0-255 范围内，会被错误反序列化为 Binary
- Binary 的 base64 解码已被注释掉（代码中有 fixme 注释），说明开发者已意识到此问题

#### 问题 3：Float 序列化使用 newtype_struct

```rust
DataValue::Float(f) => {
    if let Some(n) = serde_json::Number::from_f64(*f) {
        serializer.serialize_newtype_struct("Float", &n)
    } else {
        serializer.serialize_unit()  // NaN/Infinity 变为 null
    }
}
```

- 输出 `{"Float": 1.5}` 而非 `1.5`，与标准 JSON 数字不兼容
- NaN 和 Infinity 被静默丢弃为 null，可能导致数据丢失

#### 问题 4：ShortStr / LongStr 无法区分

序列化后都变为普通字符串，反序列化时都变为 `DataValue::String`，无法还原原始变体。

#### 问题 5：JsonValue::Object 反序列化报错

```rust
JsonValue::Object(_) => {
    Err(serde::de::Error::custom("unexpected object in DataValue"))
}
```

JSON 对象无法直接反序列化为 DataValue，这在嵌套结构中可能造成问题。

### 2.3 DataSet 反序列化中的性能问题（rds.rs:437-460）

```rust
fn row_from_value(v: serde_json::Value, schema: &Schema) -> Result<Row, String> {
    for field in &schema.fields {
        let dv = obj
            .get(&field.name)
            .map(|x: &serde_json::Value| serde_json::from_value::<DataValue>(x.clone()))
            // ...
    }
}
```

**性能问题**：
1. **`.clone()` 每个 Value**：对每个字段都 clone 了 `serde_json::Value`
2. **逐字段 `from_value`**：每个字段独立调用 serde 反序列化，没有利用批量解析
3. **子 DataSet 的 clone**：`serde_json::from_value::<DataSet>(val.clone())` 同样有 clone 开销

### 2.4 `DataValue::Json(String)` 的设计分析

当前 `Json` 变体内部存储 `String`（JSON 字符串），而非结构化的 `serde_json::Value`。

**当前方式的优点**：
- 存储紧凑，只需一次 String 分配
- 与数据库 JSONB/JSON 列交互简单（直接传递字符串）
- 无需解析即可传递给下游

**当前方式的缺点**：
- 序列化时需要双重转义：`"{\"key\":\"value\"}"` → `"\"{\\\"key\\\":\\\"value\\\"}\""`
- 无法进行结构化访问，每次需要 `serde_json::from_str` 解析
- 反序列化时由于转义，`{` 前有 `\"`，不以 `{` 开头，导致识别失败
- 类型安全无法在编译期保证 JSON 结构

## 三、优化建议

### 3.1 方案一：带类型标签的序列化（推荐，解决往返一致性）

将 DataValue 序列化为带类型标签的格式：

```json
// 当前：{"key": "2024-01-01"}  → 反序列化时无法区分 Date 和 String
// 优化后：
{"key": {"$type": "date", "$value": "2024-01-01"}}
{"key": {"$type": "uuid", "$value": "550e8400-..."}}
{"key": {"$type": "json", "$value": {"nested": true}}}
{"key": {"$type": "binary", "$value": "AQID"}}
```

**优点**：完全解决类型歧义，往返一致
**缺点**：JSON 体积增大，与现有前端格式不兼容

### 3.2 方案二：利用 Schema 信息指导反序列化（渐进式优化）

在 DataSet 反序列化时，利用 `FieldType` 信息指导 DataValue 的构建，避免启发式推断：

```rust
fn row_from_value_with_type(v: serde_json::Value, schema: &Schema) -> Result<Row, String> {
    for (i, field) in schema.fields.iter().enumerate() {
        let dv = obj.get(&field.name)
            .map(|x| from_value_by_type(x, &field.field_type))  // 根据 FieldType 决定解析方式
            .transpose()
            .unwrap_or(DataValue::Null);
        values.push(dv);
    }
}
```

**优点**：不改变序列化格式，利用已有 Schema 信息
**缺点**：独立 DataValue 反序列化仍有歧义（但不影响 DataSet 场景）

### 3.3 方案三：修复 Float 序列化

```rust
// 当前
serializer.serialize_newtype_struct("Float", &n)
// 修改为
serializer.serialize_f64(*f)
```

### 3.4 方案四：消除 Binary 误判

移除 "数组元素全为 0-255 则为 Binary" 的启发式规则。Binary 应只通过 base64 字符串反序列化。

### 3.5 DataSet 反序列化性能优化

避免 `.clone()` 每个 `serde_json::Value`，改用 `serde_json::from_value` 的零拷贝方式，或直接在 `Deserialize` 实现中处理。

## 四、DataValue::Json 是否应使用 serde_json::Value

### 4.1 当前实现（Json 变体存储 String）

```rust
pub enum DataValue {
    // ...
    Json(String),     // 存储 JSON 字符串
    // ...
}
```

### 4.2 替代方案（Json 变体存储 serde_json::Value）

```rust
pub enum DataValue {
    // ...
    Json(serde_json::Value),   // 存储结构化 JSON 值
    // ...
}
```

### 4.3 多维度对比

| 维度 | String 方案 | serde_json::Value 方案 |
|------|-------------|----------------------|
| **序列化输出** | `"\"{\\\"key\\\":\\\"value\\\"}\""` (双重转义) | `{"key":"value"}` (原生 JSON) |
| **反序列化识别** | 失败（转义后不以 { 开头） | 直接匹配 Object 变体 |
| **内存占用** | 一次 String 分配 | Value 需要堆分配（Map/Array），但本身就是堆上的 |
| **结构化访问** | 需每次 `serde_json::from_str` 解析 | 直接 `.get("key")` 访问 |
| **数据库交互** | 直接传字符串给 JSONB 列 | 需要 `.to_string()` 转换 |
| **类型安全** | 无编译期检查 | 无编译期检查（Value 是动态类型） |
| **往返一致性** | ❌ 序列化→反序列化丢失类型 | ✅ Value::Object 可直接保留 |
| **Clone 成本** | 低（String clone） | 中等（Value 深拷贝） |

### 4.4 结论

**强烈建议将 `DataValue::Json(String)` 改为 `DataValue::Json(serde_json::Value)`**。

理由：
1. **解决往返一致性问题**：当前 Json 变体序列化后无法正确反序列化回来
2. **消除双重转义**：前端收到的 JSON 更自然、体积更小
3. **简化代码**：无需在每次访问 JSON 内容时重新解析字符串
4. **Object 反序列化问题一并解决**：JsonValue::Object 可映射为 DataValue::Json

## 五、实施路径

### 阶段 1：修复关键问题（低风险）
1. **Float 序列化修复**：`serialize_newtype_struct` → `serialize_f64`
2. **移除 Binary 误判启发式**：删除数组全 0-255 → Binary 的逻辑
3. **修复 Json 序列化往返**：将 `DataValue::Json(String)` 改为 `DataValue::Json(serde_json::Value)`

### 阶段 2：利用 Schema 信息优化（中风险）
4. **row_from_value 使用 FieldType 指导反序列化**：根据字段类型选择正确的解析方式
5. **消除 DataSet 反序列化中的 clone**：优化性能

### 阶段 3：类型标签序列化（可选，需评估兼容性）
6. **引入类型标签格式**：仅在需要类型往返一致性的场景启用
