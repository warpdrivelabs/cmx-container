# cmx-core

> CMX 核心数据模型和类型定义模块。

## 项目简介

cmx-core 是 cmx-container 项目的核心数据模型层，定义了服务编排、插件系统、数据库操作等模块所需的基础数据结构和类型。

## 快速开始

### 安装

```toml
[dependencies]
cmx-core = "0.1.0"
```

### 核心示例

```rust
use cmx_core::{
    FunctionInput, FunctionOutput, SVRContext,
    DbRequest, DbResponse,
    ServiceCallRequest, ServiceCallResponse,
};
```

## 核心功能与特性

| 模块 | 说明 |
|------|------|
| `model` | 核心业务数据模型 |
| `wasm_types` | WASM 运行时相关类型 |
| `error` | 错误类型定义 |

## 模块结构

```
cmx-core
├── src/
│   ├── lib.rs              # 库入口
│   ├── error.rs            # 错误类型定义
│   ├── model/              # 业务数据模型
│   │   ├── data/           # 数据操作模型
│   │   ├── domain/         # 领域模型
│   │   ├── meta/           # 元数据模型
│   │   └── service/        # 服务编排模型
│   └── wasm_types/         # WASM 类型定义
│       ├── mod.rs
│       ├── cache.rs
│       ├── common.rs
│       ├── context.rs
│       ├── database.rs
│       ├── plugin.rs
└── Cargo.toml
```

## 使用指南

### 一、函数输入输出类型

#### 1.1 FunctionInput 结构体

```rust
use cmx_core::{FunctionInput, FunctionOutput, SVRContext};
use serde_json::{json, Value};
use std::collections::HashMap;

/// 函数输入结构体 — 固定入参格式
/// 所有服务编排中的函数都使用此结构体作为入参
let mut context = SVRContext::new(json!("initial input"));
context.headers.insert("Authorization".to_string(), "Bearer token".to_string());

let mut binary_data = HashMap::new();
binary_data.insert("file".to_string(), vec![0x00, 0xFF, 0x12]);

let input = FunctionInput {
    input: json!({"action": "process", "data": "test"}),
    context,
    binary_data,
};
```

#### 1.2 FunctionOutput 结构体

```rust
use cmx_core::FunctionOutput;

/// 函数输出结构体 — 固定出参格式
let mut output = FunctionOutput::success(json!({
    "result": "processed",
    "id": "12345"
}));

// 添加二进制数据
output.add_binary("thumbnail", vec![0x00, 0x01, 0x02]);
```

#### 1.3 SVRContext 结构体

```rust
use cmx_core::{SVRContext, FunctionInput};
use serde_json::json;

/// 服务调用上下文 — 在服务编排中持续传递
let mut context = SVRContext::new(json!({
    "user_id": 123,
    "request_id": "req-001"
}));

// 设置事务 ID（用于事务框）
context.set_txn_id("tx-456");

// 添加 HTTP 请求头
context.headers.insert("Content-Type".to_string(), "application/json".to_string());

// 添加前序步骤输出
context.set_step_output("step_1", json!({"status": "ok", "data": "result1"}));
context.set_step_output("step_2", json!({"status": "ok", "data": "result2"}));

// 从 FunctionInput 获取上下文
fn process(input: &FunctionInput) {
    let initial = &input.context.initial_input;
    let headers = &input.context.headers;
    let txn_id = &input.context.txn_id;

    // 获取前序步骤输出
    if let Some(step1_output) = input.context.get_step_output("step_1") {
        println!("Step 1 result: {:?}", step1_output);
    }
}
```

### 二、WASM 类型定义

#### 2.1 DbRequest 数据库请求

```rust
use cmx_core::wasm_types::{DbRequest, ParamValue};

/// 数据库请求结构体
let db_req = DbRequest {
    sql: "SELECT id, name, email FROM users WHERE id = $1 AND status = $2".to_string(),
    params: Some(vec![
        ParamValue::Int64(123),
        ParamValue::String("active".to_string()),
    ]),
    dataset_id: Some("default".to_string()),
    db_id: Some("postgres".to_string()),
    txn_id: Some("tx-789".to_string()),
};
```

#### 2.2 DbResponse 数据库响应

```rust
use cmx_core::wasm_types::DbResponse;

/// 数据库响应结构体
let db_resp = DbResponse {
    rows: vec![
        serde_json::json!({"id": 123, "name": "张三", "email": "zhangsan@example.com"}),
        serde_json::json!({"id": 456, "name": "李四", "email": "lisi@example.com"}),
    ],
    rows_affected: 0,
    last_insert_id: None,
};
```

#### 2.3 CacheRequest 缓存请求

```rust
use cmx_core::wasm_types::CacheRequest;

/// 缓存获取请求
let cache_get = CacheRequest {
    key: "user:123".to_string(),
    value: None,
    ttl_seconds: None,
};

/// 缓存设置请求
let cache_set = CacheRequest {
    key: "user:123".to_string(),
    value: Some(r#"{"name":"张三","age":30}"#.to_string()),
    ttl_seconds: Some(3600),
};
```

#### 2.4 ServiceCallRequest 服务调用请求

```rust
use cmx_core::wasm_types::ServiceCallRequest;

/// 服务调用请求结构体
let call_req = ServiceCallRequest {
    service_id: "user-service".to_string(),
    function_name: "get_user".to_string(),
    input: json!({"user_id": 123}),
    trace_id: Some("trace-001".to_string()),
    timeout_ms: Some(5000),
};
```

### 三、宿主函数类型

#### 3.1 HostFunctionInput

```rust
use cmx_core::wasm_types::HostFunctionInput;

/// 宿主函数输入
let host_input = HostFunctionInput {
    func_name: "db_query".to_string(),
    args: vec![
        serde_json::json!({"sql": "SELECT * FROM users"}),
    ],
};
```

#### 3.2 HostFunctionOutput

```rust
use cmx_core::wasm_types::HostFunctionOutput;

/// 宿主函数输出
let host_output = HostFunctionOutput {
    success: true,
    result: Some(json!({"rows": [], "count": 0})),
    error: None,
};
```

### 四、领域模型

#### 4.1 Entity 实体

```rust
use cmx_core::model::domain::{Entity, EntityId};

/// 领域实体
let entity = Entity {
    id: EntityId::new("user", 123),
    name: "张三".to_string(),
    created_at: chrono::Utc::now(),
    updated_at: chrono::Utc::now(),
    metadata: HashMap::new(),
};
```

#### 4.2 Plugin 插件

```rust
use cmx_core::model::domain::Plugin;
use cmx_core::model::domain::plugin::PluginStatus;

/// 插件信息
let plugin = Plugin {
    id: "my-plugin".to_string(),
    name: "我的插件".to_string(),
    version: "1.0.0".to_string(),
    status: PluginStatus::Active,
    install_path: "/plugins/my-plugin/1.0.0".to_string(),
    manifest: None,
};
```

### 五、服务编排模型

#### 5.1 ServiceOrchestration

```rust
use cmx_core::model::service::{ServiceOrchestration, ServiceNode};

/// 服务编排定义
let orchestration = ServiceOrchestration {
    id: "order-service".to_string(),
    name: "订单服务".to_string(),
    version: "1.0.0".to_string(),
    nodes: vec![
        ServiceNode {
            id: "start".to_string(),
            node_type: "start".to_string(),
            next: Some("validate".to_string()),
            ..Default::default()
        },
        ServiceNode {
            id: "validate".to_string(),
            node_type: "func".to_string(),
            plugin: Some("validator-plugin".to_string()),
            function: Some("validate".to_string()),
            next: Some("process".to_string()),
            ..Default::default()
        },
        ServiceNode {
            id: "process".to_string(),
            node_type: "func".to_string(),
            plugin: Some("order-plugin".to_string()),
            function: Some("create_order".to_string()),
            next: Some("end".to_string()),
            ..Default::default()
        },
        ServiceNode {
            id: "end".to_string(),
            node_type: "end".to_string(),
            ..Default::default()
        },
    ],
};
```

#### 5.2 FlowExecution 流程执行

```rust
use cmx_core::model::service::{FlowExecution, FlowStatus, StepResult};

/// 流程执行记录
let execution = FlowExecution {
    id: "exec-001".to_string(),
    orchestration_id: "order-service".to_string(),
    status: FlowStatus::Running,
    current_node: "validate".to_string(),
    started_at: chrono::Utc::now(),
    finished_at: None,
    step_results: vec![
        StepResult {
            node_id: "start".to_string(),
            status: "completed".to_string(),
            input: json!({}),
            output: json!({}),
            started_at: chrono::Utc::now(),
            finished_at: chrono::Utc::now(),
            error: None,
        },
    ],
    context: serde_json::json!({}),
};
```

### 六、元数据模型

#### 6.1 TableDefine 表定义

```rust
use cmx_core::model::meta::{TableDefine, ColumnDefine, ColumnType};

/// 表定义
let table = TableDefine {
    name: "users".to_string(),
    schema: Some("public".to_string()),
    columns: vec![
        ColumnDefine {
            name: "id".to_string(),
            column_type: ColumnType::BigInt,
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        ColumnDefine {
            name: "name".to_string(),
            column_type: ColumnType::Varchar(255),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
        ColumnDefine {
            name: "email".to_string(),
            column_type: ColumnType::Varchar(255),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
        ColumnDefine {
            name: "created_at".to_string(),
            column_type: ColumnType::Timestamp,
            nullable: false,
            default_value: Some("NOW()".to_string()),
            is_primary_key: false,
            is_auto_increment: false,
        },
    ],
    indexes: vec![],
    foreign_keys: vec![],
};
```

### 七、错误类型

#### 7.1 CoreError

```rust
use cmx_core::error::CoreError;

match result {
    Ok(value) => println!("Result: {:?}", value),
    Err(e) => {
        match e {
            CoreError::NotFound(msg) => {
                eprintln!("Resource not found: {}", msg);
            }
            CoreError::InvalidInput(msg) => {
                eprintln!("Invalid input: {}", msg);
            }
            CoreError::SerializationFailed(msg) => {
                eprintln!("Serialization failed: {}", msg);
            }
            CoreError::DeserializationFailed(msg) => {
                eprintln!("Deserialization failed: {}", msg);
            }
            CoreError::Internal(msg) => {
                eprintln!("Internal error: {}", msg);
            }
        }
    }
}
```

### 八、完整使用示例

```rust
use cmx_core::{
    FunctionInput, FunctionOutput, SVRContext,
    wasm_types::{DbRequest, DbResponse, ParamValue, CacheRequest, ServiceCallRequest},
    error::CoreError,
};
use serde_json::json;
use std::collections::HashMap;

/// 服务编排函数示例
fn process_order(input: &FunctionInput) -> Result<FunctionOutput, CoreError> {
    // 1. 解析输入
    let order_data = input.parse_json::<OrderInput>()?;

    // 2. 检查缓存
    let cache_key = format!("order:{}", order_data.order_id);
    if let Some(cached) = check_cache(&cache_key)? {
        return Ok(FunctionOutput::success(cached));
    }

    // 3. 验证数据
    validate_order(&order_data)?;

    // 4. 数据库操作
    let db_req = DbRequest {
        sql: "INSERT INTO orders (user_id, total, status) VALUES ($1, $2, $3) RETURNING id".to_string(),
        params: Some(vec![
            ParamValue::Int64(order_data.user_id),
            ParamValue::Float(order_data.total),
            ParamValue::String("pending".to_string()),
        ]),
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };

    let db_resp = execute_db(db_req)?;
    let order_id = db_resp.last_insert_id
        .ok_or_else(|| CoreError::Internal("Failed to get order ID".to_string()))?;

    // 5. 构造结果
    let result = json!({
        "order_id": order_id,
        "status": "created",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    // 6. 写入缓存
    set_cache(&cache_key, &result)?;

    // 7. 更新上下文中的步骤输出
    let mut output = FunctionOutput::success(result);
    output.context.set_step_output("create_order", json!({"order_id": order_id}));

    Ok(output)
}

/// 订单输入数据
#[derive(serde::Deserialize)]
struct OrderInput {
    order_id: Option<i64>,
    user_id: i64,
    items: Vec<OrderItem>,
    total: f64,
}

#[derive(serde::Deserialize)]
struct OrderItem {
    product_id: String,
    quantity: i32,
    price: f64,
}

fn validate_order(order: &OrderInput) -> Result<(), CoreError> {
    if order.items.is_empty() {
        return Err(CoreError::InvalidInput("Order must have at least one item".to_string()));
    }
    if order.total <= 0.0 {
        return Err(CoreError::InvalidInput("Order total must be positive".to_string()));
    }
    Ok(())
}

fn check_cache(key: &str) -> Result<Option<serde_json::Value>, CoreError> {
    // 实际实现中调用缓存服务
    Ok(None)
}

fn set_cache(key: &str, value: &serde_json::Value) -> Result<(), CoreError> {
    // 实际实现中调用缓存服务
    Ok(())
}

fn execute_db(req: DbRequest) -> Result<DbResponse, CoreError> {
    // 实际实现中调用数据库服务
    Ok(DbResponse {
        rows: vec![],
        rows_affected: 1,
        last_insert_id: Some(12345),
    })
}
```
