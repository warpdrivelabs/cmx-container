# DataValue 反序列化收紧与 SQL 参数绑定一致性方案

## 一、Summary

`DatabaseManager::execute_sql_with_json` / `query_sql_with_json` 当前以 `serde_json::Value` 数组为入参，调用链路为：

```
json_to_data_values(json)
  └─> serde_json::from_value::<DataValue>(v)   // 每个元素
        └─> DataValue::Deserialize  // cmx-core/src/model/cell.rs L115
              ├─> 尝试 DateTime::parse_from_rfc3339
              ├─> 尝试 NaiveDate::parse_from_str
              ├─> 尝试 Uuid::parse_str           ← 关键：UUID 字符串 → DataValue::Uuid
              ├─> 尝试 B64: 前缀解码
              └─> 默认 DataValue::String
  └─> bind_data_value_postgres/mysql/sqlite    // 绑定到 sqlx
        ├─> DataValue::Uuid(v) => query.bind(*v)   // PostgreSQL 当作 Uuid 类型发送
        ├─> DataValue::Uuid(v) => query.bind(v.to_string())  // MySQL/SQLite 当作字符串
        └─> DataValue::Null  => query.bind(None::<String>)
```

**问题根因**：`DataValue::Deserialize` 对普通字符串做了"过激的类型推断"，导致以下场景出错：

1. **UUID 字符串错配**：`varchar(64)` 类型的 ID 列收到 `"550e8400-..."` 时，被反序列化为 `DataValue::Uuid`；PostgreSQL 的 `bind_data_value_postgres` 按 Uuid 类型编码，与 `varchar` 列类型不兼容（"invalid input syntax for type uuid" 反过来 / 或者数据库驱动直接报类型不匹配）。
2. **None 绑定的占位类型不统一**：`DataValue::Null` 当前统一 `query.bind(None::<String>)`，虽然 sqlx 会以 `NULL` 发送，但占位 `String` 在多类型列场景下有歧义隐患。本次按用户决策保持现状。
3. **`json_to_data_values` 透传 `DataValue::Deserialize` 行为**，未做任何字符串策略控制，导致 SQL 参数层与 DataValue 默认反序列化行为耦合。

**用户决策（已确认）**：

- ✅ 不新增 `_strict` / `_smart` 变体方法；`query_sql_with_json` / `execute_sql_with_json` 公共签名保持不变。
- ✅ 同步重写 `DataValue::Deserialize` 收紧字符串推断。
- ✅ `DataValue::Null` 绑定保持 `query.bind(None::<String>)`，由 sqlx 自行推断。

## 二、Current State Analysis

### 2.1 关键文件
- [`crates/libs/cmx-core/src/model/cell.rs`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-core/src/model/cell.rs) — `DataValue` enum 及 `Serialize`/`Deserialize`（L29-191），推断逻辑在 L115-189
- [`crates/libs/cmx-infra/cmx-database/src/executor/mod.rs`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/executor/mod.rs) — `json_to_data_values`（L200-209）、`bind_data_value_*`（L129-197）
- [`crates/libs/cmx-infra/cmx-database/src/transaction/core.rs`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/core.rs) — `execute_with_json`（L537-541）、`query_with_json`（L603-607）
- [`crates/libs/cmx-infra/cmx-database/src/manager/mod.rs`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/manager/mod.rs) — `execute_sql_with_json` / `query_sql_with_json`（L243、L305）
- [`crates/libs/cmx-core/src/model/data/dataset/rds.rs`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-core/src/model/data/dataset/rds.rs) — `from_json_value`（L291）、`json_value_to_typed_data`（L705-820），后者已按 FieldType 显式转换，前者仍走通用反序列化

### 2.2 现有调用模式
- 多数调用方（`cmx-iam/user/service.rs`、`cmx-iam/user_auth_query_impl.rs`、`cmx-service/repository.rs`）传 `Value::Array(vec![Value::String(s)])`，期望 `s` 作为字符串绑定 —— 本次改造正是要恢复这一语义。
- `cmx-database/src/host_functions.rs` 的 WASM 宿主函数也会调用 `*_with_json`，但参数语义同 Rust 调用方（数组元素类型决定绑定类型），收紧推断对其无负面影响，反而更安全。

### 2.3 反向影响面评估
`DataValue::Deserialize` 还被以下位置使用，需要同步检查：

| 位置 | 当前用法 | 改造后影响 |
| --- | --- | --- |
| `cmx-database/src/transaction/core.rs` L538, L604 | `serde_json::from_value::<DataValue>(params)` | 行为变更：UUID/DateTime 字符串变 `String`。**符合目标** |
| `cmx-database/src/executor/mod.rs` L200 `json_to_data_values` | 同上 | **核心改造点** |
| `cmx-core/src/model/data/dataset/rds.rs` L302 `from_json_value` | 通用反序列化（无 FieldType 上下文） | 行为变更：UUID 字段变 `String`。**可接受**：建议调用方改用 `json_value_to_typed_data`（已存在） |
| `cmx-core/src/model/data/dataset/rds.rs` L705-820 各 `FieldType` 分支 | 仅 `Unknown` 走通用反序列化 | 不受影响 |
| `cmx-core/src/model/cell.rs` 单元测试 | 多个测试依赖旧推断 | **需要同步更新测试断言** |

## 三、Proposed Changes

### 3.1 重写 `DataValue::Deserialize`（[cell.rs L115-191](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-core/src/model/cell.rs#L115-L191)）

**新的字符串推断规则（保守模式）**：

| JSON 形态 | 反序列化结果 | 说明 |
| --- | --- | --- |
| `null` | `DataValue::Null` | 维持 |
| `true` / `false` | `DataValue::Bool` | 维持 |
| JSON Number（i64/f64） | `DataValue::Int` / `DataValue::Float` | 维持 |
| 字符串以 `"B64:"` 开头 | `DataValue::Binary` (base64 解码) | **保留**：显式前缀无歧义 |
| 字符串以 `{` / `[` 开头且是合法 JSON | `DataValue::Json(s)` | **保留**：显式 JSON 容器无歧义 |
| **其他所有字符串** | `DataValue::String(s)` | **新增**：去掉 UUID/DateTime/NaiveDate/Decimal 自动推断 |
| JSON 数组（全部 0-255 数字） | `DataValue::Binary` | 维持 |
| JSON 数组（其他） | `DataValue::Array` | 维持 |
| JSON 对象 | 错误 | 维持 |

**同步调整**：
- 文档注释（L109-114）更新推断规则说明。
- `JsonValue::Object(_)` 分支维持返回错误。
- 单元测试中依赖旧推断的用例需要更新断言（`test_uuid_deserialization` 等）。

### 3.2 重写 `json_to_data_values`（[executor/mod.rs L199-209](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/executor/mod.rs#L199-L209)）

策略二选一（**采用 a**，更显式）：

**a. 直接复用收紧后的 `DataValue::Deserialize`**：
- 行为与新的 `DataValue::Deserialize` 强一致，避免双轨维护。
- 删除或保留 `ParamValue::from_json` 作为兼容垫片，但不再被 `json_to_data_values` 调用。

**b. 在 `json_to_data_values` 中独立实现"SQL 参数友好"的转换**：
- 数组元素统一为：`Null/Bool/Int/Float/String/Json/Binary` 六种基本类型，遇到 Array 转 `DataValue::Array`。
- 不再依赖 `DataValue::Deserialize`，避免后续 `DataValue` 演进时影响 SQL 路径。

**选择 a**，理由：
- `DataValue::Deserialize` 是 DataValue 唯一的反序列化入口，所有调用方都从一处获益。
- `ParamValue` 在 `executor/mod.rs` 中已是 dead code 候选，保留可作"智能推断"兼容入口但不再被 SQL 路径使用。

### 3.3 验证 & 调整反向影响

- `cmx-core/src/model/data/dataset/rds.rs` L302 的 `from_json_value`：
  - 改前注释强调"基于 Schema 字段类型做精确转换"，并增加 fallback：若 `serde_json::from_value::<DataValue>` 失败，按 `field.field_type` 走 `json_value_to_typed_data` 二次尝试。
  - 这样即使收紧后丢失类型推断，调用方也不会因此失败。
- `cmx-core/src/model/cell.rs` 单元测试：
  - `test_uuid_deserialization`：原断言 "可能解析为 String 或 Uuid" → 收紧为必然 `DataValue::String`。
  - `test_datetime_deserialization_rfc3339` / `test_datetime_deserialization_with_timezone`：原断言"DateTime" → 收紧为 `DataValue::String`。
  - `test_date_deserialization`：原断言 `DataValue::Date` → 收紧为 `DataValue::String`。
  - `test_json_deserialization` 维持（用 `{` 前缀识别 JSON，不受影响）。
  - `test_binary_deserialization` 维持（用 `[1,2,3]` 数组识别 Binary）。

### 3.4 文档同步

- `DataValue` enum 顶部文档注释补充"反序列化推断规则"小节。
- `executor::json_to_data_values` 文档注释说明：字符串不再自动推断为 UUID/DateTime，调用方需要 Uuid/DateTime 时请显式构造 `DataValue::Uuid` / `DataValue::DateTime` 后通过 `execute_sql_with_datavalues` 提交。
- 同步在 `cmx-iam`/`cmx-service` 已有 `*_with_json` 调用处的注释（仅 doc，**不改签名不改实现**）。

## 四、Assumptions & Decisions

1. **不新增 `_strict` 变体方法**：用户明确表示调用方太多，不希望公共 API 增长。收紧行为是默认且唯一的语义。
2. **不修改 `bind_data_value_*` 三个绑定函数**：`DataValue::Null` 绑定 `None::<String>` 保持不变；UUID 在 PostgreSQL 仍然走 `query.bind(*v)`，MySQL/SQLite 走 `v.to_string()` —— 因为这层绑定是 **DataValue 类型 → SQL 编码** 的忠实映射，类型本身正确，绑定本身正确，问题在反序列化层。
3. **不修改 `ParamValue` 与 `ParamValue::from_json`**：保留作为高级"智能推断"兼容入口，但 SQL 参数路径不再走它。如未来需要"宽松模式"，可以从 `ParamValue::from_json` 派生。
4. **`rds.rs::from_json_value` 暂不重写为完全 FieldType 驱动**：本次最小改动，避免触及 ORM/序列化层；只在注释中提示调用方在需要类型保真时使用 `json_value_to_typed_data`。
5. **单元测试更新遵循"行为即契约"**：收紧后的行为是新的契约，测试断言必须更新而不是删除 —— 这样回归保护仍然有效。

## 五、Verification

执行顺序：

1. `cargo check -p cmx-core` —— 编译通过。
2. `cargo test -p cmx-core --lib model::cell::tests` —— cell.rs 单元测试通过（更新后的断言）。
3. `cargo test -p cmx-core --lib model::data::dataset::tests` —— DataSet 相关测试通过。
4. `cargo check -p cmx-database` —— `executor::json_to_data_values` 编译通过。
5. `cargo test -p cmx-database --lib` —— 数据库 crate 内部测试通过。
6. `cargo check -p cmx-iam -p cmx-service -p cmx-api -p web-server` —— 所有调用方编译通过（无 API 变更，预计无修改）。
7. **手工场景验证**（可选）：
   - PostgreSQL：`execute_sql_with_json(db, None, "UPDATE cmx_user SET ... WHERE id = $1", json!([user_uuid_string]))` 成功。
   - PostgreSQL：`execute_sql_with_json(db, None, "UPDATE cmx_user SET ... WHERE deleted_at = $1", json!([null]))` 成功。
   - MySQL / SQLite：同等场景验证。

## 六、Out of Scope

- `execute_sql_with_json` / `query_sql_with_json` 公共签名与行为契约调整（仅文档注释）。
- 新增 `_strict` / `_smart` / `_typed` 等变体方法。
- `bind_data_value_*` 三个绑定函数的占位类型调整。
- `rds.rs::from_json_value` 完全重构为 FieldType 驱动。
- `DataValue::Serialize` 的输出格式调整（保持现状）。
- `DataValue` 自身 enum 变体新增/删除。
