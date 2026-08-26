---
name: wasm-plugin-developer
description: WASM 插件开发指南：工程结构、目录规范、manifest.json、三层代码架构、HostFunctions 宿主能力与 SQL 查询差异。当用户需要创建、开发或理解 WASM 插件工程结构、写插件函数、配插件 metadata/seeddata 时必用。
---

# WASM 插件开发指南

> 基于 cmx-container 插件平台的 WASM 插件开发完整指南，采用渐进式披露方式组织。

---

## 一、工程目录结构概览

### 1.1 标准目录树

```
my-plugin/
├── manifest.json              # 插件清单文件（必须）
├── Cargo.toml                 # Rust 项目配置
├── .cargo/config.toml         # Cargo 构建配置
├── .vscode/launch.json        # VS Code 调试配置
├── config/                    # 表定义配置（注册表结构和种子数据）
│   └── {name}_config.json
├── metadata/                  # 表结构定义（DDL 元数据）
│   └── {name}_tables.json
├── seeddata/                  # 种子数据（初始化数据）
│   ├── {table_name}_seed.json
│   └── {table_name}_seed.csv
├── servicedata/               # 服务编排流程定义（每个文件对应一个接口）
│   ├── save_xxx.json
│   ├── list_xxx.json
│   └── get_xxx_detail.json
├── formdata/                  # 表单配置（预留）
├── menudata/                  # 菜单配置（预留）
├── permdata/                  # 权限数据（预留）
├── flowdata/                  # 流程定义数据（预留）
├── mcpdata/                   # MCP/Skills 配置（预留）
└── src/                       # Rust 源码
    ├── lib.rs                 # 模块入口
    ├── host.rs                # HostFunctions trait 定义
    ├── models/                # 业务模型（按实体拆分）
    │   ├── mod.rs             # 模块导出 + SDK 类型重导出
    │   ├── common.rs          # 通用模型
    │   └── {entity}.rs        # 业务实体模型（按需创建）
    ├── handlers/              # 业务处理逻辑（按业务实体拆分）
    │   ├── mod.rs             # PluginCore<H> 定义
    │   └── {entity}.rs        # 业务实体的全部操作（按需创建）
    ├── extism/                # Extism 适配层（与 handlers/ 一一对应）
    │   ├── mod.rs             # ExtismHost 实现
    │   └── {entity}.rs        # 对应 handlers/ 的 #[plugin_fn] 入口
    └── tests/                 # 测试（与 handlers/ 一一对应）
        ├── mod.rs             # 公共测试工具
        └── {entity}.rs        # 对应 handlers/ 的单元测试
```

> **plugin_id 命名约束**：只能使用下划线 `_` 分隔，禁止使用连字符 `-`。
>
> - 正确：`cmx_account`、`order_plugin`、`test_plugin`
> - 错误：`cmx-account`、`order-plugin`、`test-plugin`

**src/ 目录拆分原则**：

- `handlers/`、`extism/`、`tests/` 的子文件按**业务实体**拆分，每个实体文件包含该实体的全部操作
- 例如一个"账户"实体文件中可包含：账户查询、创建、更新、删除、缓存操作、业务校验等所有账户相关逻辑
- `{entity}.rs` 是占位符，开发者根据实际业务创建对应文件，文件名不限
- 当插件只有一个业务实体时，每个目录下只有一个业务文件
- 当插件有多个业务实体时，每个实体对应一个文件
- `extism/` 和 `tests/` 的文件划分与 `handlers/` 保持一一对应
- `models/` 的实体文件与 `handlers/` 的实体文件对应，`common.rs` 存放跨实体共享的通用模型

### 1.2 目录用途速查表

| 目录 | 用途 | 必须性 | 参考技能 |
|------|------|--------|---------|
| `config/` | 表定义配置清单，注册表结构和种子数据关系 | 推荐 | plugin-metadata-generator |
| `metadata/` | 表结构定义（列、索引、主键等 DDL 元数据） | 推荐 | plugin-metadata-generator |
| `seeddata/` | 插件安装时自动执行的初始化数据 | 推荐 | plugin-metadata-generator |
| `servicedata/` | 服务编排流程定义，每个文件对应一个接口 | 推荐 | service-orchestration-generator |
| `formdata/` | 前端表单配置 | 预留 | — |
| `menudata/` | 前端菜单配置 | 预留 | — |
| `permdata/` | 权限数据配置 | 预留 | — |
| `flowdata/` | 流程定义数据 | 预留 | — |
| `mcpdata/` | MCP/Skills 配置 | 预留 | — |

---

## 二、代码架构概览

### 2.1 三层分离模式

```
handlers/（纯业务逻辑，按业务实体拆分）
  ↓ 通过泛型 H: HostFunctions
host.rs（抽象接口）
  ↑ impl HostFunctions for ExtismHost
extism/（Extism 适配，与 handlers/ 一一对应）
```

**设计原则**：
- `handlers/` 不知道 Extism 的存在，只依赖 `HostFunctions` trait
- `extism/` 是薄适配层，仅做 `ExtismHost → HostCaller` 的委托
- `tests/` 使用 `mockall` 自动生成 `MockHostFunctions`

### 2.2 HostFunctions trait（13 个宿主能力）

| 方法 | 类别 | 说明 |
|------|------|------|
| `log_info / log_error / log_debug / log_warn` | 日志 | 四级日志 |
| `db_query` | 数据库 | 执行 SELECT 查询 |
| `db_execute` | 数据库 | 执行 INSERT/UPDATE/DELETE |
| `cache_get / cache_set / cache_delete` | 缓存 | 缓存读写删除 |
| `call_plugin` | 插件调用 | 调用本插件函数 |
| `call_remote_plugin` | 插件调用 | 调用远程插件函数 |
| `call_service_by_key` | 服务编排 | 调用本服务编排接口 |
| `call_remote_service` | 服务编排 | 调用远程服务编排接口 |

> **数据库操作规范**：`DbRequest` 的 `db_id` 字段应使用 `manifest.json` 中 `plugin.datasource_id` 的值，确保数据库操作使用插件关联的数据源。参数传递优先使用 `data_values: Option<Vec<DataValue>>`，确保带类型 NULL（`NullTyped`）在跨 WASM 边界时不被 JSON 退化。旧字段 `params: Option<JsonValue>` 仅用于向后兼容。

### 2.3 服务编排数据流规范

在服务编排中，节点间数据通过 `current_output` 链式传递，但不同场景需要使用不同的数据源：

| 数据源 | 访问方式 | 适用场景 |
|--------|---------|---------|
| 上一步输出 | `input.input` | 前序节点输出包含所需字段 |
| 原始输入 | `input.context.initial_input` | 前序节点输出不含所需字段，需要原始业务参数 |
| 指定步骤输出 | `input.context.get_step_output("node_id")` | 需要特定步骤的输出 |
| 事务ID | `input.context.txn_id` | 事务操作 |

**核心原则**：

1. **switch 节点的返回值仅用于路由判断**，不会传递给下一个节点（执行器自动恢复 current_output）
2. **当前节点应从哪里获取数据，取决于前序节点的输出是否包含所需字段**：
   - 包含 → 直接使用 `input.input`
   - 不包含 → 从 `input.context.initial_input` 或 `get_step_output("node_id")` 获取
3. **每个节点的输出都会缓存到 `step_outputs`**，任意节点都可通过 `get_step_output("node_id")` 访问

**数据获取决策树**：

```
当前节点需要什么数据？
├── 前序节点输出包含所需字段 → input.input
├── 前序节点输出不含所需字段
│   ├── 需要原始业务参数 → input.context.initial_input
│   └── 需要特定步骤的输出 → input.context.get_step_output("node_id")
└── 事务ID → input.context.txn_id
```

---

## 三、技能使用指引

在开发插件时，根据不同任务使用对应技能：

| 任务                                              | 使用技能                                | 必须性    |
|-------------------------------------------------|-------------------------------------|--------|
| 编写插件函数文档注释（extism_layer.rs或者有#[plugin_fn]属性的函数） | **plugin-fn-doc**                   | **必须** |
| 生成服务编排流程（servicedata/）                          | **service-orchestration-generator** | 推荐     |
| 生成表结构定义（metadata/）和种子数据（seeddata/）              | **plugin-metadata-generator**       | 推荐     |

### 3.1 典型开发流程

1. 确定业务需求，设计数据表结构
2. 使用 **plugin-metadata-generator** 生成 `metadata/` 和 `seeddata/` 文件
3. 创建 `config/` 配置文件，注册表定义和种子数据
4. 编写 `src/` 代码（models/ → host.rs → handlers/ → extism/ → tests/）
5. 使用 **service-orchestration-generator** 生成 `servicedata/` 服务编排
6. 编写 `manifest.json` 插件清单
7. 使用 **plugin-fn-doc** 规范化函数文档注释
8. 编译验证：`cargo test` + `cargo build --release --target wasm32-wasip1 --features extism`

---
## 四、SQL 查询最佳实践（WASM 差异速览）

WASM 插件通过 `HostFunctions::db_query` / `db_execute` 与宿主数据库交互。**DataValue 构造 / `From<Option<T>>` 糖 / `dv!` 宏 / `ParamsBuilder` / `NullTyped` / 反模式全集的通用规范以 [cmx-sql-execution](../cmx-sql-execution/SKILL.md) 技能为共享真源**（其 [references/datavalue-and-params.md](../cmx-sql-execution/references/datavalue-and-params.md)、[references/wasm-boundary-antipatterns.md](../cmx-sql-execution/references/wasm-boundary-antipatterns.md)），本节只列 **WASM 侧差异**：

### 4.1 参数传递：`DbRequest` 用 `data_values` 而非 `params`

| 字段 | 类型 | 用途 | 推荐度 |
|------|------|------|--------|
| `data_values` | `Option<Vec<DataValue>>` | 带类型参数（含 `NullTyped`） | ★ 推荐 |
| `params` | `Option<JsonValue>` | JSON 参数数组（向后兼容） | 仅维护 |

```rust
// ❌ 旧路径：NULL 经 JSON 退化为无类型 DataValue::Null
params: Some(serde_json::json!([id, null]))
// ✅ 新路径：MsgPack 直接传输 Vec<DataValue>，保留 NullTyped 类型信息
data_values: Some(vec![DataValue::String(id), DataValue::NullTyped(SqlTypeMarker::Int)])
```

宿主端执行优先级：`data_values` > `params` > 无参数。

### 4.2 WASM 可用性

`ParamsBuilder` / `dv!` / `DataValue` 均为**纯 Rust 域构造工具，无 sqlx 依赖**，可直接在 WASM 插件 handlers 中使用（`crates/libs/cmx-core/src/model/{cell,builder}.rs`）。

### 4.3 事务：txn_id 从编排上下文透传

WASM 插件不自己开启事务——事务 ID 由服务编排上下文传入，经 `input.context.txn_id` 取出透传给 `DbRequest.txn_id`：

```rust
let db_request = DbRequest {
    sql: "INSERT INTO cmx_order (id, name) VALUES ($1, $2)".into(),
    data_values: Some(dv![id, name]),
    txn_id: input.context.txn_id.clone(),  // 透传事务 ID；非事务则 None
    ..Default::default()
};
let resp = self.host.db_execute(db_request)?;
```

### 4.4 从 `DbResponse.dataset` 提取结果

```rust
let resp = self.host.db_query(db_request)?;
if !resp.success { return Err(format!("查询失败: {}", resp.error.unwrap_or_default())); }
let dataset = resp.dataset.ok_or("查询结果为空")?;
let schema = dataset.schema.as_ref();
for row in dataset.iter() {
    let id: String = row.get_by_name_as::<String>(schema, "id").unwrap_or_default();
    let name: Option<String> = row.get_by_name_as::<String>(schema, "name");
}
```

通用 DataSet 提取姿势（遍历 / 单行 / 整列）详见 `../cmx-sql-execution/references/transactions-and-dataset.md`。

### 4.5 完整示例

三个端到端示例（参数化查询 / INSERT+事务透传 / ParamsBuilder 动态 UPDATE）见 [references/sql-examples.md](references/sql-examples.md)。

### 4.6 WASM 特有反模式

| 反模式 | 说明 |
|--------|------|
| ❌ `params: Some(serde_json::Value::Array(...))` | WASM 侧 JSON 路径退化 NULL 类型，新代码一律 `data_values` |

其余反模式（手写 unwrap_or / 手动占位符 / 语义误改等）见 `../cmx-sql-execution/references/wasm-boundary-antipatterns.md`。

---

## 五、参考资料

当需要创建工程、编写 manifest.json 或编写代码时，读取详细规范：

| 场景 | 参考文档 |
|------|---------|
| 创建插件工程、配置目录结构、编写 manifest.json | [project-structure.md](references/project-structure.md) |
| 了解代码架构详情、SDK 类型、函数注释规范、Cargo.toml 配置 | [project-structure.md](references/project-structure.md) |
| WASM 插件内 SQL 三个完整示例（查询/INSERT+事务/动态 UPDATE） | [sql-examples.md](references/sql-examples.md) |
| `DataValue`、`SqlTypeMarker`、`dv!` 宏、`From<Option<T>>`、`ParamsBuilder` 的**使用规范** | `../cmx-sql-execution/SKILL.md`（共享真源）+ 其 references |
| `ParamsBuilder`（动态 SET 子句构造器） | `crates/libs/cmx-core/src/model/builder.rs` |
| `DbRequest` / `DbResponse` 结构定义 | `crates/libs/cmx-core/src/wasm_types/database.rs` |
