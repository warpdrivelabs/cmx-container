---
name: "wasm-plugin-developer"
description: "WASM 插件开发指南，介绍工程结构、目录规范、manifest.json、代码架构和技能使用指引。Invoke when 用户需要创建、开发或理解 WASM 插件工程结构时。"
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

## 五、SQL 查询最佳实践

WASM 插件通过 `HostFunctions::db_query` / `db_execute` 与宿主数据库交互。参数构造应优先使用 `cmx-core` 提供的 `DataValue` 体系，避免 JSON 路径导致的 NULL 类型丢失问题。

### 5.1 参数传递：使用 `data_values` 而非 `params`

`DbRequest` 提供两个参数字段：

| 字段 | 类型 | 用途 | 推荐度 |
|------|------|------|--------|
| `data_values` | `Option<Vec<DataValue>>` | 带类型参数（含 `NullTyped`） | ★ 推荐 |
| `params` | `Option<JsonValue>` | JSON 参数数组（向后兼容） | 仅维护 |

**为什么优先使用 `data_values`？**

```rust
// ❌ 旧路径：NULL 经 JSON 退化为无类型 DataValue::Null
let req = DbRequest {
    sql: "INSERT INTO t (id, optional_int) VALUES ($1, $2)".into(),
    params: Some(serde_json::json!([id, null])), // NULL 无类型！
    ..Default::default()
};

// ✅ 新路径：MsgPack 直接传输 Vec<DataValue>，保留 NullTyped 类型信息
let req = DbRequest {
    sql: "INSERT INTO t (id, optional_int) VALUES ($1, $2)".into(),
    data_values: Some(vec![
        DataValue::String(id),
        DataValue::NullTyped(SqlTypeMarker::Int), // 带类型 NULL
    ]),
    ..Default::default()
};
```

宿主端执行优先级：`data_values` > `params` > 无参数。当两个字段同时设置时，`data_values` 优先生效。

### 5.2 `DataValue` 基础构造

```rust
use cmx_core::model::cell::DataValue;

let params = vec![
    DataValue::String(id.clone()),           // TEXT / VARCHAR
    DataValue::Int(count),                   // BIGINT / INT
    DataValue::Bool(enabled),                // BOOLEAN
    DataValue::Float(rate),                  // DOUBLE PRECISION
    DataValue::Decimal(amount),              // NUMERIC
    DataValue::DateTime(created_at),         // TIMESTAMPTZ
    DataValue::Date(birth_date),             // DATE
    DataValue::Uuid(uuid),                   // UUID
    DataValue::Binary(bytes),                // BYTEA
    DataValue::Json(json_string),            // JSONB
    DataValue::Null,                         // NULL（绑定为 None::<String>）
];
```

### 5.3 `From<Option<T>>` 糖：消除冗长 NULL 处理

`cmx-core` 为 `DataValue` 实现了 `From<Option<T>>`，消除 `.map(DataValue::X).unwrap_or(DataValue::Null)` 冗长模式：

```rust
// ❌ 旧写法（冗长，且 NULL 丢失类型）
let params = vec![
    name.map(DataValue::String).unwrap_or(DataValue::Null),
    sort_order.map(DataValue::Int).unwrap_or(DataValue::Null), // NULL 无类型！
];

// ✅ 新写法（.into() 配合 From<Option<T>>）
let params: Vec<DataValue> = vec![
    name.into(),        // Option<String> → DataValue::String 或 Null
    sort_order.into(),  // Option<i64> → DataValue::Int 或 NullTyped(Int)
];
```

**关键规则**：

- `Option<String>.into()` → `DataValue::String` 或 `DataValue::Null`
- `Option<i64>.into()` → `DataValue::Int` 或 `DataValue::NullTyped(Int)`
- `Option<bool>.into()` → `DataValue::Bool` 或 `DataValue::NullTyped(Bool)`
- `Option<Uuid>.into()` → `DataValue::Uuid` 或 `DataValue::NullTyped(Uuid)`
- `Option<DateTime<Utc>>.into()` → `DataValue::DateTime` 或 `DataValue::NullTyped(Timestamp)`
- `Option<NaiveDate>.into()` → `DataValue::Date` 或 `DataValue::NullTyped(Date)`
- `Option<Decimal>.into()` → `DataValue::Decimal` 或 `DataValue::NullTyped(Decimal)`

**语义核对：None → 0 vs None → NULL**

```rust
// 语义 A：None 表示 0（有默认值）
data.sort_order.unwrap_or(0).into()  // → DataValue::Int(0)

// 语义 B：None 表示 NULL（数据库存 NULL）
data.sort_order.into()  // → DataValue::NullTyped(Int)
```

### 5.4 `dv!` 宏：批量构造参数

`dv!` 宏基于 `Into<DataValue>` trait 驱动，适合批量构造参数：

```rust
use cmx_core::dv;

// 空参数
let params: Vec<DataValue> = dv!();

// 批量构造（每个 expr 须 Into<DataValue>）
let params = dv![
    id.clone(),                    // String → DataValue::String
    data.code.clone(),             // String
    data.sort_order.unwrap_or(0),  // i64 → DataValue::Int
    data.description.clone(),      // Option<String> → DataValue::String 或 Null
    data.parent_id.clone(),        // Option<String>
];

// 显式带类型的 NULL（非 Vec，返回单个 DataValue）
let null_uuid: DataValue = dv!(null Uuid);  // → NullTyped(Uuid)
```

> **`dv!` vs `vec![]`：** `dv!` 的优势在于 `Option<T>` 直接传入即自动 `.into()`，而 `vec![]` 需要每个元素显式 `.into()`。简单场景（2-3 个参数）可用 `vec![a.into(), b.into()]`，复杂场景用 `dv!` 更简洁。

### 5.5 `ParamsBuilder`：动态 UPDATE SET 子句

手写动态 UPDATE 时，「SQL SET 子句顺序」与「params Vec push 顺序」必须双重一致，极易出错：

```rust
// ❌ 旧模式（易错：idx 漂移、sets 和 params 顺序不一致）
let mut sets: Vec<String> = Vec::new();
let mut params: Vec<DataValue> = vec![DataValue::String(rule_id.to_string())]; // WHERE $1
let mut idx = 2;
if let Some(name) = data.name {
    sets.push(format!("name = ${idx}"));
    params.push(DataValue::String(name));
    idx += 1;
}
// ...
```

**解决：`ParamsBuilder` 自动管理编号**

```rust
use cmx_core::ParamsBuilder;

// SET 从 $1 起，WHERE id 参数放最后
let mut b = ParamsBuilder::new(0);  // start_offset = 0 → SET 从 $1 起
b.set_opt("name", data.name)              // Option<String> → None 跳过该列
 .set_opt("priority", data.priority)      // Option<i64> → None 跳过
 .set_opt("status", data.status);         // Option<i64>
let (set_clause, mut params) = b.build();

if set_clause.is_empty() {
    return Err("未提供任何更新字段".into());
}

// WHERE id 参数放最后，占位符编号 = SET 参数数 + 1
let where_idx = params.len() + 1;
params.push(DataValue::String(rule_id.to_string()));
let sql = format!(
    "UPDATE cmx_order SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
);
```

**`ParamsBuilder` API**：

| 方法 | 说明 |
|------|------|
| `new(start_offset)` | 创建 builder，占位符从 `start_offset + 1` 起编号 |
| `set(col, val)` | 必填列赋值，val 须 `Into<DataValue>` |
| `set_opt(col, val)` | 可选列赋值，**None 跳过该列**（不加入 SET） |
| `set_opt_null(col, val)` | 可选列赋值，**None 写入 NULL**（显式置 NULL） |
| `build()` | 返回 `(set_clause: String, params: Vec<DataValue>)` |
| `len()` / `is_empty()` | 查询当前赋值数 |
| `next_placeholder()` | 查询下一个占位符编号 |

**`set_opt` vs `set_opt_null`**：

```rust
// set_opt: None → 跳过该列（不更新）
b.set_opt("name", None::<String>);  // SET 子句不含 name

// set_opt_null: None → 写入 SET name = NULL（显式置 NULL）
b.set_opt_null("deleted_at", None::<DateTime<Utc>>);  // SET deleted_at = $N (NullTyped)
```

> **WASM 可用性**：`ParamsBuilder` 是纯 Rust 域构造工具，无 sqlx 依赖，可直接在 WASM 插件 handlers 中使用（定义于 `crates/libs/cmx-core/src/model/builder.rs`）。

### 5.6 带类型 NULL：`NullTyped`

PostgreSQL prepare 时，占位符需要知道目标列类型。`DataValue::Null` 绑定为 `None::<String>`，对 INTEGER/TIMESTAMP/UUID 等非字符串列会类型不匹配。

```rust
// ❌ 问题：NULL 绑定到非 TEXT 列
DataValue::Null  // 绑定为 None::<String> → INTEGER 列 prepare 类型不匹配！

// ✅ 解决：显式声明 NULL 的目标类型
DataValue::NullTyped(SqlTypeMarker::Int)  // 绑定为 None::<i64> → INTEGER 列类型正确
```

**`SqlTypeMarker` 枚举**：

```rust
pub enum SqlTypeMarker {
    Bool,       // BOOLEAN
    Int,        // BIGINT / INTEGER
    Float,      // DOUBLE PRECISION / REAL
    Decimal,    // NUMERIC
    Text,       // TEXT / VARCHAR
    Timestamp,  // TIMESTAMPTZ
    Date,       // DATE
    Uuid,       // UUID
    Json,       // JSONB
    Binary,     // BYTEA
}
```

大多数场景 `From<Option<T>>` 会自动产生正确的 `NullTyped`，仅在以下场景需要手动构造：

```rust
use cmx_core::model::cell::{DataValue, SqlTypeMarker};

// 条件分支：根据情况传 NULL
let parent_id_param = if has_parent {
    DataValue::String(parent_id)
} else {
    DataValue::NullTyped(SqlTypeMarker::Text)  // 显式 TEXT 类型 NULL
};

// 或用 dv! 宏的 null 语法
let null_uuid: DataValue = cmx_core::dv!(null Uuid);
```

### 5.7 事务模式

WASM 插件中，事务 ID 由服务编排上下文传入，通过 `input.context.txn_id` 获取并透传：

```rust
// 非事务执行
let db_request = DbRequest {
    sql: "SELECT id, name FROM cmx_order WHERE status = $1".into(),
    data_values: Some(dv![status]),
    txn_id: None,  // 无事务
    ..Default::default()
};
let resp = self.host.db_query(db_request)?;

// 事务内执行（txn_id 从 input.context 透传）
let db_request = DbRequest {
    sql: "INSERT INTO cmx_order (id, name) VALUES ($1, $2)".into(),
    data_values: Some(dv![id, name]),
    txn_id: input.context.txn_id.clone(),  // 透传事务 ID
    ..Default::default()
};
let resp = self.host.db_execute(db_request)?;
```

### 5.8 从 `DbResponse.dataset` 提取结果

```rust
let resp = self.host.db_query(db_request)?;
if !resp.success {
    return Err(format!("查询失败: {}", resp.error.unwrap_or_default()));
}

let dataset = resp.dataset.ok_or("查询结果为空")?;
let schema = dataset.schema.as_ref();

// 遍历行
let mut orders = Vec::new();
for row in dataset.iter() {
    let id: String = row.get_by_name_as::<String>(schema, "id").unwrap_or_default();
    let name: Option<String> = row.get_by_name_as::<String>(schema, "name");
    let count: i64 = row.get_by_name_as::<i64>(schema, "count").unwrap_or(0);
    orders.push((id, name, count));
}

// 提取单行（首行）
let row = dataset.iter().next().ok_or("记录不存在")?;
let json_val = row.to_json_value(schema);
let order: Order = serde_json::from_value(json_val)?;

// 提取整列为 Vec
let ids: Vec<String> = dataset.iter()
    .filter_map(|row| row.get_by_name_as::<String>(schema, "id"))
    .collect();
```

### 5.9 完整示例

#### 示例 1：参数化查询（`db_query` + `data_values` + `dv!`）

```rust
use cmx_core::dv;
use cmx_core::model::cell::DataValue;

impl<H: HostFunctions> PluginCore<H> {
    pub fn query_orders(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: OrderQueryRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(OrderQueryRequest {
                order_id: None,
                customer_name: None,
                status: None,
            });

        let mut sql = "SELECT id, customer_name, product_name, quantity, status FROM cmx_order WHERE 1=1".to_string();
        let mut params: Vec<DataValue> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref order_id) = request.order_id {
            sql.push_str(&format!(" AND id = ${param_idx}"));
            param_idx += 1;
            params.push(DataValue::String(order_id.clone()));
        }
        if let Some(ref customer_name) = request.customer_name {
            sql.push_str(&format!(" AND customer_name = ${param_idx}"));
            param_idx += 1;
            params.push(DataValue::String(customer_name.clone()));
        }
        if let Some(ref status) = request.status {
            sql.push_str(&format!(" AND status = ${param_idx}"));
            param_idx += 1;
            params.push(DataValue::String(status.clone()));
        }

        let db_request = DbRequest {
            sql,
            data_values: if params.is_empty() { None } else { Some(params) },
            dataset_id: None,
            db_id: None,
            txn_id: None,
            params: None,  // 新代码不使用 params
        };

        let db_response = self.host.db_query(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "dataset": db_response.dataset,
        })))
    }
}
```

#### 示例 2：INSERT（`db_execute` + `dv!` + 事务 `txn_id`）

```rust
use cmx_core::dv;
use cmx_core::model::cell::DataValue;

impl<H: HostFunctions> PluginCore<H> {
    pub fn create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {e}"))?;

        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO cmx_order \
                   (id, customer_name, product_name, quantity, unit_price, status, remark, sort_order) \
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)".to_string();

        // ★ 使用 dv! 宏，Option<T> 直接传入即自动 .into()
        let params = dv![
            id.clone(),
            request.customer_name.clone(),
            request.product_name.clone(),
            request.quantity,
            request.unit_price,
            request.status.clone(),
            request.remark.clone(),        // Option<String> → String 或 Null
            request.sort_order,            // Option<i64> → Int 或 NullTyped(Int)
        ];

        let db_request = DbRequest {
            sql,
            data_values: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),  // 透传事务 ID
            params: None,
        };

        let db_response = self.host.db_execute(db_request)?;
        self.host.log_info(&format!("订单创建成功, 影响行数: {:?}", db_response.affected_rows))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }
}
```

#### 示例 3：动态 UPDATE（`ParamsBuilder` + `set_opt`）

```rust
use cmx_core::ParamsBuilder;
use cmx_core::model::cell::DataValue;

impl<H: HostFunctions> PluginCore<H> {
    pub fn update_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: UpdateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {e}"))?;

        // ★ ParamsBuilder 自动管理占位符，SET 从 $1 起
        let mut b = ParamsBuilder::new(0);
        b.set_opt("customer_name", request.customer_name)
         .set_opt("product_name", request.product_name)
         .set_opt("quantity", request.quantity)
         .set_opt("unit_price", request.unit_price)
         .set_opt("status", request.status)
         .set_opt("remark", request.remark)
         .set_opt("sort_order", request.sort_order);
        let (set_clause, mut params) = b.build();

        if set_clause.is_empty() {
            return Err("未提供任何更新字段".into());
        }

        // WHERE id 放最后
        let where_idx = params.len() + 1;
        params.push(DataValue::String(request.order_id));
        let sql = format!(
            "UPDATE cmx_order SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
        );

        let db_request = DbRequest {
            sql,
            data_values: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
            params: None,
        };

        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }
}
```

### 5.10 反模式

| 反模式 | 说明 |
|--------|------|
| ❌ 使用 `params: Some(serde_json::Value::Array(...))` | 新代码应使用 `data_values`，JSON 路径会退化 NULL 类型 |
| ❌ 手动 `.map(DataValue::X).unwrap_or(DataValue::Null)` | 冗长，且整型 NULL 会丢失类型变成 `DataValue::Null` |
| ❌ 手动管理占位符编号 | 易漂移，应使用 `ParamsBuilder` |
| ❌ 盲目把 `unwrap_or(0)` 改成 `.into()` | `None → 0` 与 `None → NULL` 语义不同，必须保留原语义 |
| ❌ 在 `vec![]` 中混用 `.into()` 和裸值 | 可能导致类型推断歧义，用 `dv!` 或显式标注类型 |

```rust
// ❌ 错误改法：语义从 None→0 变成了 None→NULL
sort_order.into()

// ✅ 正确改法：保留 None→0 语义后再 into()
sort_order.unwrap_or(0).into()
```

---

## 四、参考资料

当需要创建工程、编写 manifest.json 或编写代码时，读取详细规范：

| 场景 | 参考文档 |
|------|---------|
| 创建插件工程、配置目录结构、编写 manifest.json | [project-structure.md](references/project-structure.md) |
| 了解代码架构详情、SDK 类型、函数注释规范、Cargo.toml 配置 | [project-structure.md](references/project-structure.md) |
| `DataValue`、`SqlTypeMarker`、`dv!` 宏、`From<Option<T>>` | `crates/libs/cmx-core/src/model/cell.rs` |
| `ParamsBuilder`（动态 SET 子句构造器） | `crates/libs/cmx-core/src/model/builder.rs` |
| `DbRequest` / `DbResponse` 结构定义 | `crates/libs/cmx-core/src/wasm_types/database.rs` |
