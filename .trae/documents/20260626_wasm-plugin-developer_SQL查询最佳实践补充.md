# wasm-plugin-developer 技能 SQL 查询最佳实践补充方案

## 摘要

在 `wasm-plugin-developer` 技能中新增 WASM 插件 SQL 查询最佳实践章节，替代当前文档中陈旧的数据库操作示例（使用 `params: Option<JsonValue>` 的旧模式）。新内容直接引用 `cmx-core` 提供的 `DataValue`、`dv!` 宏、`ParamsBuilder` 等工具，参考 `cmx-sql-execution` 技能的核心实践（但不引用该技能），确保 WASM 边界内外的类型安全和 NULL 绑定正确性。

## 当前状态分析

### 现有问题

1. **旧模式仍在示例代码中**：当前 `wasm-plugin-developer/SKILL.md` 的参考资料部分未涉及 SQL 查询的具体写法，而 `cmx-plugin-demo` 中的 `handlers/database.rs` 仍使用 `params: Some(serde_json::Value::Array(params))` 的旧路径，NULL 经 JSON 会退化为无类型 `DataValue::Null`。
2. **缺少 WASM 场景的 DataValue 指导**：`cmx-core` 的 `database.rs` 已定义 `DbRequest.data_values: Option<Vec<DataValue>>` 字段，但技能文档未向开发者说明如何正确构造该字段。
3. **动态 UPDATE 无推荐方案**：WASM 插件中手写动态 UPDATE 时，开发者仍可能手动管理 `$N` 占位符编号，易出错。

### 关键源文件确认

| 文件 | 职责 |
|------|------|
| `crates/libs/cmx-core/src/wasm_types/database.rs` | `DbRequest` / `DbResponse` 定义，`data_values` 字段说明 |
| `crates/libs/cmx-core/src/model/cell.rs` | `DataValue`、`SqlTypeMarker`、`From<Option<T>>`、`dv!` 宏 |
| `crates/libs/cmx-core/src/model/builder.rs` | `ParamsBuilder`（纯域工具，无 sqlx 依赖，wasm 可用） |
| `crates/libs/cmx-plugin-demo/src/handlers/database.rs` | 当前旧模式示例（须在新技能文档中被替代） |
| `crates/libs/cmx-plugin-sdk/src/host_calls.rs` | `HostCaller::db_query` / `db_execute` MsgPack 封装 |

## 变更计划

### 变更 1：在 SKILL.md "二、代码架构概览"的 HostFunctions trait 表格后补充 `data_values` 字段说明

**文件**：`.trae/skills/wasm-plugin-developer/SKILL.md`

**内容**：在 2.2 节 HostFunctions trait 表格后、2.3 节前，插入一段 `DbRequest` 参数传递规范的说明，强调 `data_values` 优先于 `params`。

> **数据库操作规范**：`DbRequest` 的 `db_id` 字段应使用 `manifest.json` 中 `plugin.datasource_id` 的值。参数传递优先使用 `data_values: Option<Vec<DataValue>>`，确保带类型 NULL（`NullTyped`）在跨 WASM 边界时不被 JSON 退化。旧字段 `params: Option<JsonValue>` 仅用于向后兼容。

### 变更 2：新增"五、SQL 查询最佳实践"完整章节

**文件**：`.trae/skills/wasm-plugin-developer/SKILL.md`

**插入位置**：在"四、参考资料"之前（或文档末尾新增），作为技能的核心新增内容。

**章节结构**：

#### 5.1 参数传递：使用 `data_values` 而非 `params`

- 说明 `params` JSON 路径的问题（NULL 退化为无类型 `DataValue::Null`）
- `data_values` 通过 MsgPack 直接传输 `Vec<DataValue>`，保留 `NullTyped(SqlTypeMarker::Int)` 等类型信息
- 宿主端优先级：`data_values` > `params` > 无参数

#### 5.2 `DataValue` 基础构造

- 列出 `DataValue::String`、`Int`、`Bool`、`Float`、`Decimal`、`DateTime`、`Date`、`Uuid`、`Json`、`Binary`、`Array` 等基础用法
- 强调 WASM 插件可直接使用 `cmx_core::model::cell::DataValue`

#### 5.3 `From<Option<T>>` 糖：消除冗长 NULL 处理

- 对比旧写法 `.map(DataValue::X).unwrap_or(DataValue::Null)` 与新写法 `.into()`
- 列出各 Option 类型的转换规则（`Option<String>` → Null，`Option<i64>` → NullTyped(Int) 等）
- **语义核对**：`None → 0` vs `None → NULL` 的区别，必须保留原语义

#### 5.4 `dv!` 宏：批量构造参数

- 语法：`dv![]`、`dv![id, name, count]`、`dv!(null Uuid)`
- 与 `vec![]` 对比：自动 `Into<DataValue>`，Option 直接传入
- 适用于所有 `db_query` / `db_execute` 的 `data_values` 字段

#### 5.5 `ParamsBuilder`：动态 UPDATE SET 子句

- 问题：手写动态 UPDATE 时 SQL 和 params 顺序双重一致易漂移
- 解决：`ParamsBuilder::new(start_offset)` 自动管理 `$N` 编号
- API：`set`、`set_opt`（None 跳过）、`set_opt_null`（None 写入 NULL）
- 占位符编号策略：SET 从 `$1` 起，WHERE 参数放 params 最后
- WASM 可用性说明：纯 Rust 域工具，无 sqlx 依赖，可直接在 handlers 中使用

#### 5.6 带类型 NULL：`NullTyped`

- 问题：PostgreSQL prepare 时 `DataValue::Null` 绑定为 `None::<String>`，对 INTEGER/TIMESTAMP/UUID 列类型不匹配
- 解决：`DataValue::NullTyped(SqlTypeMarker::Int)` 等
- `dv!(null Uuid)` 语法
- 何时手动使用：条件分支、显式构造 NULL 参数

#### 5.7 事务模式

- 开启事务：`txn_id` 的获取方式（由服务编排传入或宿主侧提供）
- `DbRequest.txn_id: Some(txn_id)`  vs `None`
- 在 handlers 中透传 `input.context.txn_id`

#### 5.8 从 `DbResponse.dataset` 提取结果

- `DbResponse.dataset: Option<DataSet>`
- 遍历行：`dataset.iter()`，`row.get_by_name_as::<String>(schema, "id")`
- 提取单行：`dataset.iter().next()`，`row.to_json_value(schema)`
- 提取整列 Vec

#### 5.9 完整示例：查询 + INSERT + 动态 UPDATE

提供三个 WASM 插件 handlers 层级的完整代码示例：

1. **参数化查询**（`db_query` + `data_values` + `dv!` + `From<Option<T>>`）
2. **INSERT**（`db_execute` + `dv!` + 事务 `txn_id`）
3. **动态 UPDATE**（`ParamsBuilder` + `set_opt` + `data_values`）

所有示例基于 `cmx-plugin-demo` 的 `handlers/database.rs` 风格重写，但使用 `data_values` 替代 `params`。

#### 5.10 反模式

- ❌ 使用 `params: Some(serde_json::Value::Array(...))`（新代码）
- ❌ 手动 `.map(DataValue::X).unwrap_or(DataValue::Null)`（冗长且整型 NULL 丢失类型）
- ❌ 手动管理占位符编号
- ❌ 盲目把 `unwrap_or(0)` 改成 `.into()`（语义改变）
- ❌ 在 `vec![]` 中混用 `.into()` 和裸值导致类型推断歧义

### 变更 3：更新"三、技能使用指引"表格（可选）

**文件**：`.trae/skills/wasm-plugin-developer/SKILL.md`

在"三、技能使用指引"的表格中，若已有数据库相关任务行，确保指向本章节的内部锚点正确。若表格不涉及 SQL 写法，可不改。

### 变更 4：更新"四、参考资料"表格

**文件**：`.trae/skills/wasm-plugin-developer/SKILL.md`

在参考资料表格中新增 SQL 查询最佳实践相关的源文件引用：

| `crates/libs/cmx-core/src/model/cell.rs` | `DataValue`、`SqlTypeMarker`、`dv!` 宏、`From<Option<T>>` |
| `crates/libs/cmx-core/src/model/builder.rs` | `ParamsBuilder`（动态 SET 子句构造器） |
| `crates/libs/cmx-core/src/wasm_types/database.rs` | `DbRequest` / `DbResponse` 结构定义 |

## 假设与决策

1. **不引用 `cmx-sql-execution` 技能**：用户明确要求不能写"使用这个技能"，所有 SQL 规范内容以内联方式直接写入 `wasm-plugin-developer` 技能，代码引用指向 `cmx-core` 源文件。
2. **不修改实际插件工程代码**：本次仅修改 `.trae/skills/wasm-plugin-developer/SKILL.md` 技能文档，不修改 `cmx-plugin-demo` 等实际工程源码（除非用户后续要求）。
3. **保持渐进式披露风格**：与现有 SKILL.md 风格一致，示例代码精简但完整，表格和决策树辅助理解。
4. **ParamsBuilder WASM 可用性**：经确认 `builder.rs` 第一行注释即说明"纯域构造工具，无 sqlx 依赖，wasm 可用"，可直接推荐。

## 验证步骤

1. 读取修改后的 `SKILL.md`，确认新增章节的 markdown 格式正确、锚点无冲突。
2. 检查示例代码中所有 `DbRequest` 构造均使用 `data_values` 而非 `params`。
3. 检查 `dv!` 宏示例语法正确（与 `cmx-core/src/model/cell.rs` 中的宏定义一致）。
4. 检查 `ParamsBuilder` 示例的 `start_offset` 和占位符编号逻辑正确（与 `builder.rs` 实现一致）。
5. 确认未出现"使用 `cmx-sql-execution` 技能"等引用其他技能的文本。
