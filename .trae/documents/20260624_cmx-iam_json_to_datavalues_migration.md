# cmx-iam: execute_sql_with_json / query_sql_with_json → DataValues 迁移方案

## 概要

将 `cmx-iam` crate 中所有 `execute_sql_with_json` 调用改为 `execute_sql_with_datavalues`，所有 `query_sql_with_json` 调用改为 `query_sql_with_datavalues`。参数类型从 `serde_json::Value`（`Value::Array(vec![...])`）改为 `Vec<DataValue>`。

**cmx-auth** 中无任何 `*_with_json` 调用，无需修改。

---

## 当前状态分析

### API 签名差异

| 方法 | json 版本参数 | datavalues 版本参数 |
|------|--------------|-------------------|
| execute | `params: serde_json::Value` | `params: Vec<DataValue>` |
| query | `params: serde_json::Value` | `params: Vec<DataValue>` |

两者返回值相同（execute → `Result<u64>`，query → `Result<DataSet>`）。

### 涉及文件（8 个，共 47 处调用）

| 文件 | execute 调用数 | query 调用数 | 已导入 DataValue | 保留 `use serde_json::Value` |
|------|:---:|:---:|:---:|:---:|
| `src/user/service.rs` | 8 | 0 | 是 | 是（GenericCrudService 用） |
| `src/role/service.rs` | 4 | 2 | 否 | 是（GenericCrudService 用） |
| `src/role_group/service.rs` | 1 | 2 | 否 | 是（GenericCrudService 用） |
| `src/permission/service.rs` | 2 | 3 | 否 | 是（GenericCrudService 用） |
| `src/scheduler.rs` | 1 | 1 | 否 | 否（可移除） |
| `src/rule/enforcer.rs` | 0 | 3 | 否 | 否（可移除） |
| `src/rule/service.rs` | 7 | 8 | 否 | 否（可移除） |
| `src/iam_checker.rs` | 0 | 4 | 否 | 否（可移除） |

### 参数构建模式汇总

需处理的 `serde_json::Value` → `DataValue` 转换模式：

| serde_json::Value 模式 | DataValue 模式 |
|------------------------|---------------|
| `Value::String(x)` | `DataValue::String(x)` |
| `Value::Number(priority.into())` (i64) | `DataValue::Int(priority)` |
| `Value::Number((size as i64).into())` | `DataValue::Int(size as i64)` |
| `Value::Null` | `DataValue::Null` |
| `Value::Array(vec![...])` 外层包装 | 去掉外层，直接 `vec![...]` |
| `Value::Array(vec![Value::Array(...)])` 嵌套数组 | `vec![DataValue::Array(vec![...])]` |
| `Vec<Value>` 动态构建后 `Value::Array(params)` | `Vec<DataValue>` 动态构建，直接传 `params` |
| `.map(Value::String).unwrap_or(Value::Null)` | `.map(DataValue::String).unwrap_or(DataValue::Null)` |
| `Value::Array(vec![])` 空参数 | `vec![]` 或 `Vec::<DataValue>::new()` |

---

## 修改方案

### 通用步骤（每个文件）

1. **添加 DataValue 导入**（若尚未导入）：在 `use serde_json::Value;` 之前或之后添加 `use cmx_core::model::cell::DataValue;`
2. **函数名替换**：`execute_sql_with_json` → `execute_sql_with_datavalues`，`query_sql_with_json` → `query_sql_with_datavalues`
3. **参数构建替换**：按上表模式转换所有 `Value::xxx` → `DataValue::xxx`，去掉 `Value::Array(...)` 外层包装
4. **导入清理**：若文件中 `serde_json::Value` 不再被其他代码引用（如 GenericCrudService 调用），则移除 `use serde_json::Value;`

### 文件 1: `src/user/service.rs`（8 处 execute）

- **导入**：已有 `use cmx_core::model::cell::DataValue;`（L6），保留 `use serde_json::Value;`（GenericCrudService L379 仍用）
- **L436**: `Value::Array(vec![Value::String(user_id.clone())])` → `vec![DataValue::String(user_id.clone())]`
- **L446**: 同上
- **L607**: `Value::Array(vec![Value::String(user_id.clone())])` → `vec![DataValue::String(user_id.clone())]`
- **L618-622**: `Value::Array(vec![Value::String(ur_id), Value::String(user_id.clone()), Value::String(role_id.clone())])` → `vec![DataValue::String(ur_id), DataValue::String(user_id.clone()), DataValue::String(role_id.clone())]`
- **L752-760**: 7 个参数的 vec，含 `reason.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null)` → `.map(|s| DataValue::String(s.to_string())).unwrap_or(DataValue::Null)`
- **L852-854**: 2 个参数 → `DataValue::String(...)`
- **L973-975**: 2 个参数 → `DataValue::String(...)`
- **L1095-1097**: 2 个参数 → `DataValue::String(...)`

### 文件 2: `src/role/service.rs`（4 execute + 2 query）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，保留 `use serde_json::Value;`（GenericCrudService L212, L254 仍用）
- **L160**: `Value::Array(vec![Value::String(data.code.clone())])` → `vec![DataValue::String(data.code.clone())]`
- **L321**: 同模式
- **L331**: 同模式
- **L476**: 同模式
- **L487-491**: 3 个参数的 vec → `DataValue::String(...)`
- **L550**: 同模式
- 所有 `query_sql_with_json` → `query_sql_with_datavalues`，`execute_sql_with_json` → `execute_sql_with_datavalues`

### 文件 3: `src/role_group/service.rs`（1 execute + 2 query）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，保留 `use serde_json::Value;`（GenericCrudService L204, L248 仍用）
- **L293-298**: `Value::Array(role_group_ids.iter().map(|id| Value::String(id.clone())).collect())` → `role_group_ids.iter().map(|id| DataValue::String(id.clone())).collect()`
- **L311-316**: 同上模式
- **L329**: `Value::Array(vec![Value::String(role_group_id.clone())])` → `vec![DataValue::String(role_group_id.clone())]`

### 文件 4: `src/permission/service.rs`（2 execute + 3 query）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，保留 `use serde_json::Value;`（GenericCrudService L228, L270 仍用）
- **L175**: `Value::Array(vec![Value::String(data.code.clone())])` → `vec![DataValue::String(data.code.clone())]`
- **L326, L336**: 同模式
- **L474-493**: 动态构建参数
  - `let mut params: Vec<Value> = Vec::new();` → `let mut params: Vec<DataValue> = Vec::new();`
  - `params.push(Value::String(dc.to_string()));` → `params.push(DataValue::String(dc.to_string()));`（3 处类似）
  - `let params_value = Value::Array(params);` → 删除此行，直接用 `params`
  - 调用处 `query_sql_with_json(..., params_value, ...)` → `query_sql_with_datavalues(..., params, ...)`
- **L531**: `serde_json::Value::Array(vec![])` → `vec![]`（需类型标注 `vec![] as Vec<DataValue>` 或直接 `Vec::<DataValue>::new()`）

### 文件 5: `src/scheduler.rs`（1 execute + 1 query）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，移除 `use serde_json::Value;`
- **L87**: `Value::Array(vec![])` → `vec![]`
- **L115**: `Value::Array(vec![])` → `vec![]`
- 注意：空 vec 需要类型推断。由于 `execute_sql_with_datavalues` 接受 `Vec<DataValue>`，`vec![]` 可自动推断

### 文件 6: `src/rule/enforcer.rs`（3 query）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，移除 `use serde_json::Value;`
- **L111-115**:
  ```rust
  // 前:
  let params = Value::Array(vec![
      subject_type.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
  ]);
  // 后:
  let params: Vec<DataValue> = vec![
      subject_type.map(|s| DataValue::String(s.to_string())).unwrap_or(DataValue::Null),
  ];
  ```
- **L177**: `Value::Array(vec![Value::String(user_id.to_string())])` → `vec![DataValue::String(user_id.to_string())]`
- **L204-207**: 嵌套数组
  ```rust
  // 前:
  let role_id_array = Value::Array(
      role_ids.iter().map(|id| Value::String(id.clone())).collect(),
  );
  let params = Value::Array(vec![role_id_array]);
  // 后:
  let role_id_array = DataValue::Array(
      role_ids.iter().map(|id| DataValue::String(id.clone())).collect(),
  );
  let params = vec![role_id_array];
  ```
- **L236**: `Value::Array(vec![Value::String(user_id.to_string())])` → `vec![DataValue::String(user_id.to_string())]`

### 文件 7: `src/rule/service.rs`（7 execute + 8 query，共 15 处）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，移除 `use serde_json::Value;`（无 GenericCrudService 调用）
- **L343, L361**: `Value::Array(vec![Value::String(rule_id.to_string())])` → `vec![DataValue::String(rule_id.to_string())]`
- **L390-392**: 嵌套数组
  ```rust
  // 前:
  let params = Value::Array(vec![Value::Array(
      ids.iter().map(|i| Value::String(i.clone())).collect(),
  )]);
  // 后:
  let params = vec![DataValue::Array(
      ids.iter().map(|i| DataValue::String(i.clone())).collect(),
  )];
  ```
- **L494-509**: INSERT 规则参数（8 个字段）
  - `Value::String(...)` → `DataValue::String(...)`（5 处）
  - `.map(Value::String).unwrap_or(Value::Null)` → `.map(DataValue::String).unwrap_or(DataValue::Null)`（2 处：violation_message, description）
  - `Value::Number(priority.into())` → `DataValue::Int(priority)`（priority 为 i64）
- **L524-528**: INSERT 规则项参数（3 个 `Value::String`）→ `DataValue::String`
- **L599-646**: 动态 UPDATE 参数
  - `let mut params: Vec<Value> = vec![Value::String(rule_id.to_string())];` → `let mut params: Vec<DataValue> = vec![DataValue::String(rule_id.to_string())];`
  - `params.push(Value::String(name));` → `params.push(DataValue::String(name));`（多处类似）
  - `params.push(Value::Number(priority.into()));` → `params.push(DataValue::Int(priority));`
  - `params.push(Value::Number(status.into()));` → `params.push(DataValue::Int(status));`
  - L646: `Value::Array(params)` → 直接传 `params`
- **L670**: `Value::Array(vec![Value::String(rule_id.to_string())])` → `vec![DataValue::String(rule_id.to_string())]`
- **L729**: `Value::Array(vec![])` → `vec![]`
- **L754-756**: `Value::Number((size as i64).into())` → `DataValue::Int(size as i64)`，`Value::Number(offset.into())` → `DataValue::Int(offset)`
- **L782-784**: `Value::String(rule_id.to_string())` + `Value::Number(status.into())` → `DataValue::String(...)` + `DataValue::Int(status)`
- **L850-853**: 3 个 `Value::String` → `DataValue::String`
- **L889-891**: 嵌套数组
  ```rust
  // 前:
  let params = Value::Array(vec![
      Value::Array(item_ids.iter().map(|i| Value::String(i.clone())).collect()),
      Value::String(rule_id.to_string()),
  ]);
  // 后:
  let params = vec![
      DataValue::Array(item_ids.iter().map(|i| DataValue::String(i.clone())).collect()),
      DataValue::String(rule_id.to_string()),
  ];
  ```
- **L948, L974**: `Value::Array(vec![Value::String(uid.clone())])` → `vec![DataValue::String(uid.clone())]`
- **L999**: `Value::Array(vec![])` → `vec![]`

### 文件 8: `src/iam_checker.rs`（4 query + 1 辅助函数签名修改）

- **导入**：添加 `use cmx_core::model::cell::DataValue;`，移除 `use serde_json::Value;`
- **L85**: 修改 `exists_check` 函数签名
  ```rust
  // 前:
  async fn exists_check(&self, sql: &str, params: Value, label: &str) -> Result<bool, TraitError>
  // 后:
  async fn exists_check(&self, sql: &str, params: Vec<DataValue>, label: &str) -> Result<bool, TraitError>
  ```
- **L88**: `query_sql_with_json` → `query_sql_with_datavalues`
- **L228**: `Value::Array(vec![Value::String(role_id.to_string())])` → `vec![DataValue::String(role_id.to_string())]`
- **L278**: `Value::Array(vec![Value::String(user_id.to_string())])` → `vec![DataValue::String(user_id.to_string())]`
- **L406**: 同上
- **L447**: 同上

---

## 假设与决策

1. **cmx-auth 无需修改**：已确认 `cmx-infra/cmx-auth` 中无 `execute_sql_with_json` / `query_sql_with_json` 调用。
2. **GenericCrudService 调用不改**：用户仅要求修改 `execute_sql_with_json` 和 `query_sql_with_json`。`GenericCrudService::get/update` 中的 `Value::String(...)` 参数保持不变。
3. **`Value::Number` 全部为整数**：所有 `Value::Number(x.into())` 的 `x` 均为 `i64` 类型，统一映射到 `DataValue::Int(x)`。
4. **空 vec 类型推断**：`vec![]` 在传给 `execute_sql_with_datavalues` / `query_sql_with_datavalues` 时可自动推断为 `Vec<DataValue>`，无需显式类型标注。若编译器无法推断，则添加 `Vec::<DataValue>::new()`。
5. **保留 `use serde_json::Value;` 的判断**：4 个文件（user/service.rs, role/service.rs, role_group/service.rs, permission/service.rs）因 GenericCrudService 调用仍需 `Value`，保留导入；其余 4 个文件移除。

---

## 验证步骤

1. **编译检查**：`rtk cargo check -p cmx-iam` 确认无编译错误
2. **Clippy 检查**：`rtk cargo clippy -p cmx-iam` 确认无警告（特别是 unused import）
3. **全文搜索**：`grep -r "execute_sql_with_json\|query_sql_with_json" crates/libs/cmx-iam/src/` 确认结果为空
4. **测试运行**：`rtk cargo test -p cmx-iam` 确认测试通过

---

## 后续增强（2026-06-25）

本迁移方案已由 [`20260625_cmx-sql-param-封装重构方案.md`](./20260625_cmx-sql-param-封装重构方案.md) 增强解决以下遗留问题：

1. **NULL 丢失类型**：新增 `SqlTypeMarker` 枚举与 `DataValue::NullTyped(SqlTypeMarker)` 变体，绑定到非字符串列（INTEGER/TIMESTAMP/UUID 等）时显式声明目标类型，避免 PostgreSQL prepare 类型不匹配。
2. **Optional 构造繁琐**：为 `DataValue` 实现 `From<Option<T>>`（T 覆盖 String/i64/i32/f64/bool/Uuid/DateTime/NaiveDate/Decimal），消除 `.map(DataValue::X).unwrap_or(DataValue::Null)` 冗长模式。整型/时间/Uuid 的 None 走 `NullTyped(对应类型)`，字符串 None 走 `Null`。
3. **占位符漂移**：新增 `ParamsBuilder`（cmx-core，wasm 可用）自动管理 `$N` 占位符编号，消除动态 UPDATE 中「SET 子句顺序」与「params push 顺序」双重一致的漂移风险。
4. **wasm 边界带类型 NULL**：`DbRequest` 新增 `data_values: Option<Vec<DataValue>>` 字段，宿主 `do_query`/`do_execute` 识别后走 `*_with_datavalues` 路径，让 plugin 直接传带类型 NULL（`NullTyped` 序列化为 `$null:Type` 字符串前缀，跨 JSON/MsgPack 往返保留类型）。

cmx-iam 的 `permission/service.rs` 与 `rule/service.rs` 已作为首批重构验证完成。
