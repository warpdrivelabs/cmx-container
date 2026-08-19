# cmx-core

> CMX 核心数据模型和类型定义模块。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

cmx-core 是 cmx-container 项目的核心数据模型层，定义了服务编排、插件系统、数据库操作等模块所需的基础数据结构和类型。

它是分层架构的最底层（与 cmx-utils 并列），被 cmx-traits、cmx-runtime、cmx-service 等上层 crate 依赖，也被 cmx-flowengine、cmx-portalservice 等外部 workspace 通过 path 引用；cmx-plugin-sdk（WASM 插件 SDK）重导出其类型供插件使用。

## 快速开始

### 安装

```toml
[dependencies]
cmx-core = "0.1.12"
```

可选 feature：`openapi`（启用 utoipa，为部分类型生成 OpenAPI schema）。

### 核心示例

```rust
use cmx_core::{
    FunctionInput, FunctionOutput, SVRContext,
    DbRequest, DbResponse,
    CallServiceRequest, CallServiceResponse,
    DataValue, ParamsBuilder,
};
```

## 核心功能与特性

| 模块 | 说明 |
|------|------|
| `model::service` | 服务编排模型（SVRContext / FunctionInput / FunctionOutput / ServiceOrchestration / ServiceFlow / ServiceDefinition） |
| `model::data` | 数据集模型（Schema / Row / DataSet / RowDataSet）与请求参数（GetParams / UpdatePayload 等） |
| `model::cell` | 带类型的单元格值（DataValue / SqlParam / SqlTypeMarker） |
| `model::builder` | `ParamsBuilder` — 动态 UPDATE SET 子句与占位参数构造器（WASM 内可用） |
| `model::iam` | 权限/角色模型（PermissionDeniedError / RoleRequirement / PermissionRegistry / Role / RoleGroup / User / PermissionTreeNode / RegisteredPermission 等） |
| `model::meta` | 插件清单（PluginDefinition）与表结构元数据（TableDefine / ColumnDefine / FieldType） |
| `model::domain` | 领域模型（DomainEntity） |
| `model::module` | 模块清单（ModuleManifest / FormDefinition / MenuDefinition） |
| `wasm_types` | 宿主与 WASM 插件之间交换的类型（数据库 / 缓存 / 插件与服务调用 / 执行步骤 / IAM 身份与权限查询） |
| `error` | 错误类型定义 |

## 模块结构

```
cmx-core
├── src/
│   ├── lib.rs              # 库入口
│   ├── error.rs            # 错误类型定义
│   ├── model/              # 业务数据模型
│   │   ├── builder.rs      # ParamsBuilder
│   │   ├── cell.rs         # DataValue / SqlParam / SqlTypeMarker
│   │   ├── data/           # 数据集与请求参数模型
│   │   ├── domain/         # 领域模型（DomainEntity / DomainEntityManager）
│   │   ├── iam/            # 权限/角色模型
│   │   ├── meta/           # 元数据模型（plugin.rs / table.rs）
│   │   ├── module/         # 模块清单模型
│   │   └── service/        # 服务编排模型
│   └── wasm_types/         # WASM 类型定义
│       ├── cache.rs        # 缓存请求/响应
│       ├── common.rs       # WasmFunctionRequest / WasmFunctionResponse
│       ├── context.rs      # WasmContext 等运行时上下文
│       ├── database.rs     # DbRequest / DbResponse
│       ├── execution.rs    # ExecutionStep / StepStatus / OrchestrationError
│       ├── iam.rs          # cmx:iam 宿主函数类型（WasmUserDetails / WasmEffectivePermissions / IamRequest 等）
│       └── plugin.rs       # 插件/服务调用请求与响应
└── Cargo.toml
```

## 使用指南

### 一、函数输入输出类型

#### 1.1 FunctionInput 结构体

```rust
use cmx_core::{FunctionInput, SVRContext};
use serde_json::json;
use std::collections::HashMap;

/// 函数输入结构体 — 固定入参格式
/// 所有服务编排中的函数都使用此结构体作为入参
/// 字段：input（业务输入 JSON）、context（SVRContext）、binary_data（二进制附件）
let mut binary_data = HashMap::new();
binary_data.insert("file".to_string(), vec![0x00, 0xFF, 0x12]);

let input = FunctionInput {
    input: json!({"action": "process", "data": "test"}),
    context,       // SVRContext，见 1.3
    binary_data,
};

// 便捷构造：直接从 JSON 值或任意可序列化类型构造
let input = FunctionInput::from_value(json!({"action": "process"}), context);
let input = FunctionInput::from_input(my_request, context);

// 读取：as_json_value() / as_str()
```

#### 1.2 FunctionOutput 结构体

```rust
use cmx_core::FunctionOutput;
use serde_json::json;

/// 函数输出结构体 — 固定出参格式
/// 字段：result（JSON 结果）、binary_data（二进制附件）
let output = FunctionOutput::new(json!({
    "result": "processed",
    "id": "12345"
}));

// 链式附带二进制数据
let output = FunctionOutput::from_json(json!({"result": "processed"}))
    .with_binary("thumbnail", vec![0x00, 0x01, 0x02]);

// 从任意可序列化结果构造（另有 from_value 别名）
let output = FunctionOutput::from_result(MyResult { code: 0 });
```

#### 1.3 SVRContext 结构体

```rust
use cmx_core::{SVRContext, FunctionInput};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

/// 服务调用上下文 — 在服务编排中持续传递
let mut headers = HashMap::new();
headers.insert("Content-Type".to_string(), "application/json".to_string());
let mut context = SVRContext::new(
    json!({"user_id": 123}),       // 初始输入
    headers,                        // 请求头
    Utc::now(),                     // 请求进入时间
    "req-001".to_string(),          // 请求 ID
);

// 设置事务 ID（用于事务框）
context.set_txn_id("tx-456".to_string());

// 记录前序步骤输出（set / add / remove / clear 系列）
context.set_step_output("step_1", json!({"status": "ok", "data": "result1"}));
context.set_step_output("step_2", json!({"status": "ok", "data": "result2"}));

// 从 FunctionInput 读取上下文
fn process(input: &FunctionInput) {
    let initial = &input.context.initial_input;
    let headers = &input.context.headers;
    let txn_id = &input.context.txn_id;

    // 读取前序步骤输出
    if let Some(step1) = input.context.get_step_output("step_1") {
        println!("Step 1 result: {:?}", step1);
    }
}
```

`SVRContext` 还携带 `auth_context: Option<AuthContext>`（由认证中间件或 gRPC interceptor 注入，含 user_id / username / roles / permissions 等），并提供 `require_permission` / `require_any_permission` / `require_role` 等权限检查方法（见第七章）。

### 二、WASM 类型定义

#### 2.1 DbRequest 数据库请求

```rust
use cmx_core::wasm_types::DbRequest;
use cmx_core::DataValue;
use serde_json::json;

/// 数据库请求结构体
let db_req = DbRequest {
    sql: "SELECT id, name, email FROM users WHERE id = $1 AND status = $2".to_string(),
    // 新式带类型参数（推荐）：NULL 通过 DataValue::NullTyped(SqlTypeMarker) 保留类型
    data_values: Some(vec![
        DataValue::Int(123),
        DataValue::String("active".to_string()),
    ]),
    // 旧式 JSON 参数（向后兼容）；与 data_values 同时设置时 data_values 优先
    params: Some(json!([123, "active"])),
    dataset_id: Some("default".to_string()),
    db_id: Some("postgres".to_string()), // 未指定时使用默认数据库
    txn_id: Some("tx-789".to_string()),  // 在指定事务中执行
};
```

`DataValue` 变体（15 种）：`Null` / `NullTyped(SqlTypeMarker)` / `Bool` / `Int` / `Float` / `String` / `Decimal` / `DateTime` / `Date` / `Binary` / `Array(Vec<DataValue>)` / `Json` / `Uuid` / `ShortStr(SmolStr)` / `LongStr(SmolStr)`；另有别名 `CellValue = DataValue`。SQL 绑定专用枚举 `SqlParam`（`Null(SqlTypeMarker)` / `Bool` / `Int` / `Float` / `Text` / `Decimal` / `Timestamp` / `Date` / `Uuid` / `Json(String)` / `Binary` / `Array(Vec<SqlParam>)`）可与 `DataValue` 通过 `From`/`Into` 互通。

#### 2.2 DbResponse 数据库响应

```rust
use cmx_core::wasm_types::DbResponse;

/// 数据库响应结构体（查询返回 dataset，写操作返回 affected_rows）
let db_resp = DbResponse {
    success: true,
    affected_rows: Some(3),  // 写操作返回影响行数
    dataset: None,           // 查询操作返回 Option<DataSet>
    txn_id: Some("tx-789".to_string()),
    error: None,
};
```

#### 2.3 缓存请求与响应

```rust
use cmx_core::wasm_types::{CacheGetRequest, CacheSetRequest, CacheResponse};
use serde_json::json;

/// 缓存读取请求
let cache_get = CacheGetRequest { key: "user:123".to_string() };

/// 缓存写入请求（值为任意 JSON 类型，可选 TTL）
let cache_set = CacheSetRequest {
    key: "user:123".to_string(),
    value: json!({"name": "张三", "age": 30}),
    ttl_seconds: Some(3600),
};

/// 缓存操作响应
let resp = CacheResponse {
    success: true,
    value: Some(json!({"name": "张三"})),
    exists: Some(true),
    error: None,
};
```

#### 2.4 插件与服务调用

```rust
use cmx_core::wasm_types::{
    CallServiceRequest, CallServiceResponse, PluginFunRequest, PluginFunCallResponse,
    PluginInfoResponse,
};
use serde_json::json;

/// 按服务 key 调用编排服务
let call_req = CallServiceRequest {
    service_key: "order-service".to_string(),
    input: json!({"user_id": 123}),
    include_steps: Some(true), // 返回各节点执行步骤
    debug: None,
    debug_node_id: None,
    debug_params: None,
    server_name: None,
};
// 响应：CallServiceResponse { success, output, steps: Vec<ExecutionStep>,
//       total_elapsed_us, error: Option<OrchestrationError> }

/// 调用指定插件的指定函数
let fun_req = PluginFunRequest {
    plugin_id: "validator-plugin".to_string(),
    function_name: "validate".to_string(),
    input: json!({"user_id": 123}),
    initial_input: None,
    debug: None,
    server_name: None,
};
// 响应：PluginFunCallResponse { success, result, elapsed_us, error }
// 另有 PluginInfoResponse { plugin_id, db_id, txn_id, request_id, tenant_id }
```

### 三、WASM 函数请求响应（泛型信封）

```rust
use cmx_core::wasm_types::{WasmFunctionRequest, WasmFunctionResponse};
use serde_json::json;

/// 泛型函数请求信封：context 由宿主注入，data 为具体业务负载
let req: WasmFunctionRequest<serde_json::Value> = WasmFunctionRequest {
    context: wasm_context, // WasmContext { request_id, tenant_id, db_id, txn_id, plugin_id }
    data: json!({"action": "run"}),
};

/// 泛型函数响应信封
let resp: WasmFunctionResponse<serde_json::Value> = WasmFunctionResponse {
    success: true,
    data: Some(json!({"done": true})),
    error: None,
};
```

### 四、领域与数据集模型

```rust
use cmx_core::model::domain::DomainEntity;

/// 领域实体：id + 名称 + 可选行数据集
let entity = DomainEntity::new(
    "user-123".to_string(),
    "用户".to_string(),
    row_data_set, // RowDataSet
);
// 也可直接构造 DomainEntity { id, name, dataset: None }
```

`model::data::dataset` 提供行式数据集模型：`Schema`（字段列表）、`Row`、`DataSet`（含 `rows` / `inserted` / `updated` / `deleted` 变更追踪集合与 `total` 分页总数）、`RowDataSet`，以及 `DataSetBuilder` / `RowBuilder` 构建器与 `ColumnarCodec` 列式编解码。

### 五、服务编排模型

#### 5.1 ServiceOrchestration 与 Flow JSON

服务编排来自服务设计器的 Flow JSON，反序列化为 `ServiceOrchestration { name, code, description, flow, source_str }`，其中 `flow: ServiceFlow { nodes, edges }`：

- `ServiceNode { id, node_type, parent, meta, data }`
  - `node_type` 为 `skylake-*` 系列（如 `skylake-start` / `skylake-end` / `skylake-func` / `skylake-switch` / `skylake-transaction`）
  - `parent` 指向所属事务框节点 ID（用于事务嵌套）
  - `data: Option<NodeData>` 携带 `name`、`node_meta: Option<NodeNodeMeta>`（plugin_id / plugin_name / plugin_version / function_name / database_id）、`inputs` / `outputs` / `options`
- `ServiceEdge { source_node_id, source_port_id, target_node_id, target_port_id }`（端口用于条件分支路由）

```json
{
  "name": "订单服务",
  "code": "order-service",
  "flow": {
    "nodes": [
      { "id": "start",    "node_type": "skylake-start", "meta": {} },
      { "id": "validate", "node_type": "skylake-func", "meta": {},
        "data": { "name": "校验订单", "nodeMeta": {
          "pluginId": "validator-plugin", "pluginName": "校验插件",
          "pluginVersion": "1.0.0", "functionName": "validate", "databaseId": null
        } } }
    ],
    "edges": [
      { "sourceNodeID": "start", "sourcePortID": "out",
        "targetNodeID": "validate", "targetPortID": "in" }
    ]
  }
}
```

`ServiceDefinition` 则是服务注册表中的行（id / app_id / service_key / service_name / plugin_id / status / version / domain、application、module 归属编码与名称等），由 cmx-service 负责加载与注册。

#### 5.2 执行步骤与状态

```rust
use cmx_core::wasm_types::{ExecutionStep, StepStatus, OrchestrationError};
use serde_json::json;

/// 执行步骤记录（每个节点一条，随 CallServiceResponse 返回）
let step = ExecutionStep {
    node_id: "validate".to_string(),
    node_name: "校验订单".to_string(),
    node_type: "skylake-func".to_string(),
    status: StepStatus::Success, // Success | Failed | Skipped | DebugPaused
    output: Some(json!({"valid": true})),
    elapsed_us: 1250,
    error: None,
    previous_output: None, // 失败步骤记录失败前的数据上下文，便于排错
};

/// 编排错误摘要（失败步骤的详细信息统一记录在 steps 数组中）
let err = OrchestrationError { message: "节点执行失败".to_string() };
```

### 六、元数据模型

#### 6.1 表结构元数据

`model::meta::table` 提供建表元数据（供插件安装时初始化表结构）：

- `TableDefine`：`table_name` / `display_name` / `columns` / `primary_keys` / `indexes` / `version` / `partition_type` / `partition_columns` / `extensions` 等
- `ColumnDefine`：`name` / `label` / `field_type` / `is_primary_key` / `is_nullable` / `default_value` / `length` / `precision` / `scale` / `db_type` / `ordinal` / 外键信息（`is_foreign_key` / `foreign_key_table` / `foreign_key_column`）/ `extensions`
- `IndexDefine`（支持 `#[derive(Default)]`）与 `IndexKind`、`PartitionType`

```rust
use cmx_core::model::meta::FieldType;

/// 字段类型枚举（13 种）
FieldType::String;  FieldType::Int;      FieldType::Float;
FieldType::Decimal; FieldType::DateTime; FieldType::Date;
FieldType::Bool;    FieldType::Text;     FieldType::Binary;
FieldType::Array;   FieldType::Json;     FieldType::Uuid;
FieldType::Unknown; // null 值字段反序列化时类型未知
```

#### 6.2 插件清单元数据

`model::meta::plugin` 定义插件清单模型：`PluginDefinition`（id / name / version / main_file / source_path / table_config_files / supported_databases / 域-应用-模块归属 / vendor 信息等）、`PluginService`（插件暴露的服务：service_id / name / entry_point 等）、`PluginDependency`（插件间依赖与版本约束）。

### 七、错误与权限类型

#### 7.1 CoreError

```rust
use cmx_core::CoreError;

/// CoreError 目前仅一个变体：全局单例重复初始化
let e = CoreError::AlreadyInitialized("global config".to_string());
```

#### 7.2 IAM 权限错误

```rust
use cmx_core::{PermissionDeniedError, RoleRequirement};

/// 权限检查与角色检查的统一错误类型
match deny {
    // 未认证（auth_context 缺失）
    PermissionDeniedError::Unauthenticated => {}
    // 权限不足
    PermissionDeniedError::Permission { user_id, permission } => {}
    // 角色不足（单角色）
    PermissionDeniedError::Role { user_id, role } => {}
    // 角色不足（多角色，requirement 区分 AND/OR 语义）
    PermissionDeniedError::Roles { user_id, requirement, roles } => {
        // requirement: RoleRequirement::All | RoleRequirement::Any
    }
}
// 辅助方法：deny.is_unauthenticated()
```

`AuthContext::new(user_id, username)` 构造认证上下文后，可用 `has_permission` / `has_role` / `require_permission` / `require_all_permissions` / `require_any_permission` / `require_role` / `require_all_roles` / `require_any_role` 做检查；`system:all` 权限与 `admin` 角色拥有短路放行语义。

### 八、完整使用示例

```rust
use cmx_core::{DataValue, DbRequest, DbResponse, FunctionInput, FunctionOutput, SVRContext};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

/// 订单输入数据
#[derive(Deserialize)]
struct OrderInput {
    user_id: i64,
    items: Vec<OrderItem>,
    total: f64,
}

#[derive(Deserialize)]
struct OrderItem {
    product_id: String,
    quantity: i32,
    price: f64,
}

/// 服务编排函数示例：在 WASM 插件中处理订单
fn process_order(input: &FunctionInput) -> Result<FunctionOutput, String> {
    // 1. 解析业务输入
    let order: OrderInput = serde_json::from_value(input.input.clone())
        .map_err(|e| format!("invalid input: {e}"))?;
    if order.items.is_empty() {
        return Err("order must have at least one item".into());
    }

    // 2. 构造数据库请求（data_values 携带类型信息，优先于 params）
    let db_req = DbRequest {
        sql: "INSERT INTO orders (user_id, total, status) VALUES ($1, $2, $3) RETURNING id"
            .to_string(),
        params: None,
        data_values: Some(vec![
            DataValue::Int(order.user_id),
            DataValue::Float(order.total),
            DataValue::String("created".into()),
        ]),
        dataset_id: None,
        db_id: None,                          // 使用默认数据库
        txn_id: input.context.txn_id.clone(), // 事务框内则沿用事务
    };

    let db_resp = call_db(db_req)?;
    if !db_resp.success {
        return Err(db_resp.error.unwrap_or_else(|| "db error".into()));
    }

    // 3. 构造函数输出（可附带二进制数据）
    Ok(FunctionOutput::new(json!({
        "status": "created",
        "affected_rows": db_resp.affected_rows.unwrap_or(0),
        "created_at": Utc::now().to_rfc3339(),
    }))
    .with_binary("receipt", b"receipt-bytes".to_vec()))
}

/// 宿主侧构造入参：SVRContext 4 参数 + FunctionInput::from_value
fn build_input() -> FunctionInput {
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    let context = SVRContext::new(json!({"user_id": 123}), headers, Utc::now(), "req-001".to_string());
    FunctionInput::from_value(json!({"action": "process"}), context)
}

fn call_db(req: DbRequest) -> Result<DbResponse, String> {
    // 实际实现中经 HostCaller 调用宿主函数（见 cmx-plugin-sdk）
    Ok(DbResponse {
        success: true,
        affected_rows: Some(1),
        dataset: None,
        txn_id: None,
        error: None,
    })
}
```
