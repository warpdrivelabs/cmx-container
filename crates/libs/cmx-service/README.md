# cmx-service — 企业级通用服务层

插件编排的执行引擎，协调 `PluginQuery` 和 `RuntimeInvoker` 完成请求处理。

## 目录

- [模块概述](#模块概述)
- [设计思想](#设计思想)
- [代码结构](#代码结构)
- [核心类型](#核心类型)
- [使用指南](#使用指南)
- [编排执行](#编排执行)
- [依赖约束](#依赖约束)

---

## 模块概述

`cmx-service` 是 CMX 插件系统的服务编排层，提供：

- **服务调用** — 单次 WASM 函数调用
- **编排执行** — 多步骤流程编排
- **生命周期监听** — 响应插件激活/停用事件
- **HTTP Handler** — 提供 HTTP 接口封装

---

## 设计思想

### 1. 依赖倒置原则

`cmx-service` 不依赖 `cmx-plugin`，通过 trait 对象交互：

```
cmx-service ──► cmx-traits ◄── cmx-plugin (PluginQuery)
                    ◄── cmx-runtime (RuntimeInvoker)
```

### 2. 服务编排模式

支持两种调用模式：

1. **单次调用** — 直接调用 WASM 函数
2. **编排执行** — 多步骤流程，支持步骤间数据传递

### 3. 生命周期监听

实现 `PluginLifecycleListener` trait，在插件激活时自动加载 WASM 模块：

```rust
#[async_trait]
impl PluginLifecycleListener for CmxService {
    async fn on_plugin_activated(&self, event: LifecycleEvent) {
        // 自动加载 WASM 模块
    }
}
```

---

## 代码结构

```
crates/libs/cmx-service/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口
│   ├── service.rs          # CmxService 核心服务
│   ├── orchestrator.rs     # Orchestrator 编排执行器
│   ├── handler.rs          # ServiceHandler HTTP 处理器
│   ├── request.rs          # 请求/响应类型
│   └── error.rs            # ServiceError 错误类型
└── tests/
    └── service_test.rs      # 单元测试
```

---

## 核心类型

### CmxService

核心服务结构，持有 trait 对象引用：

```rust
pub struct CmxService {
    plugin_query: Arc<dyn PluginQuery>,
    runtime: Arc<dyn RuntimeInvoker>,
    config: ServiceConfig,
}
```

### Orchestrator

编排执行器，支持多步骤流程：

```rust
pub struct Orchestrator {
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
}
```

### 请求/响应类型

```rust
// 单次调用请求
pub struct InvokeRequest {
    pub plugin_id: String,
    pub function_name: String,
    pub input: serde_json::Value,
    pub db_id: Option<String>,
    pub request_id: Option<String>,
    pub tenant_id: Option<String>,
}

// 单次调用响应
pub struct InvokeResponse {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub elapsed_us: u64,
    pub fuel_consumed: u64,
    pub error: Option<String>,
}

// 编排执行请求
pub struct OrchestrateRequest {
    pub orchestration: Orchestration,
    pub initial_input: serde_json::Value,
    pub db_id: Option<String>,
    pub request_id: Option<String>,
    pub tenant_id: Option<String>,
}
```

---

## 使用指南

### 1. 创建服务实例

```rust
use cmx_service::{CmxService, ServiceConfig, ServiceHandler};
use cmx_traits::{PluginQuery, RuntimeInvoker};
use std::sync::Arc;

// 使用默认配置
let service = CmxService::with_defaults(
    plugin_query,  // Arc<dyn PluginQuery>
    runtime,        // Arc<dyn RuntimeInvoker>
);

// 或使用自定义配置
let config = ServiceConfig {
    invoke_timeout_ms: 60000,
    max_retries: 5,
    enable_orchestration_cache: true,
};
let service = CmxService::new(plugin_query, runtime, config);
```

### 2. 单次调用

```rust
use cmx_service::InvokeRequest;

let request = InvokeRequest {
    plugin_id: "my-plugin".to_string(),
    function_name: "handle_request".to_string(),
    input: json!({"data": "value"}),
    db_id: Some("main-db".to_string()),
    request_id: Some("req-001".to_string()),
    tenant_id: None,
};

let response = service.invoke(&request).await?;

if response.success {
    println!("输出: {:?}", response.output);
    println!("耗时: {} μs", response.elapsed_us);
}
```

### 3. 通过 HTTP Handler 使用

```rust
use cmx_service::ServiceHandler;
use axum::{Router, extract::State, Json};

// 创建 Handler
let handler = ServiceHandler::from_components(plugin_query, runtime);

// 在路由中使用
async fn handle_call(
    State(handler): State<Arc<ServiceHandler>>,
    Json(req): Json<InvokeRequest>,
) -> Json<InvokeResponse> {
    Json(handler.handle_invoke(req).await)
}
```

---

## 编排执行

### 编排定义

```rust
use cmx_service::{Orchestration, OrchestrationStep, StepInput};

let orchestration = Orchestration {
    id: "order-flow".to_string(),
    name: "订单处理流程".to_string(),
    description: Some("处理订单的完整流程".to_string()),
    steps: vec![
        // 步骤1: 验证订单
        OrchestrationStep {
            step_id: "validate".to_string(),
            plugin_id: "validator-plugin".to_string(),
            function_name: "validate_order".to_string(),
            input: StepInput::Static { 
                value: json!({"order_id": "12345"}) 
            },
            parallel: false,
            condition: None,
        },
        // 步骤2: 处理订单（引用前一步骤输出）
        OrchestrationStep {
            step_id: "process".to_string(),
            plugin_id: "order-plugin".to_string(),
            function_name: "process_order".to_string(),
            input: StepInput::Reference { 
                step_id: "validate".to_string(),
                path: Some("data".to_string()),
            },
            parallel: false,
            condition: None,
        },
    ],
};
```

### StepInput 类型

| 类型 | 说明 |
|------|------|
| `Static` | 静态 JSON 值 |
| `Reference` | 引用前序步骤输出，支持 JSON 路径 |
| `Merge` | 合并多个来源 |

### 执行编排

```rust
use cmx_service::OrchestrateRequest;

let request = OrchestrateRequest {
    orchestration,
    initial_input: json!({}),
    db_id: Some("main-db".to_string()),
    request_id: Some("req-002".to_string()),
    tenant_id: None,
};

let response = handler.handle_orchestration(request).await;

println!("成功: {}", response.success);
println!("总耗时: {} μs", response.total_elapsed_us);

for step in response.step_results {
    println!("步骤 {}: {}", step.step_id, 
        if step.success { "成功" } else { "失败" });
}
```

---

## 依赖约束

### 允许的依赖

- `cmx-core` — 基础类型
- `cmx-traits` — trait 定义
- `cmx-database` — 直接 SQL 执行（非 WASM）

### 禁止的依赖

- ❌ `cmx-plugin` — 通过 `PluginQuery` trait 交互
- ❌ `cmx-runtime` — 通过 `RuntimeInvoker` trait 交互

### 依赖图

```
cmx-service
├── cmx-core
├── cmx-traits
│   └── cmx-core
├── cmx-database
│   ├── cmx-core
│   └── cmx-traits
└── tokio (异步运行时)
```

---

## 错误处理

```rust
pub enum ServiceError {
    PluginNotFound(String),
    PluginNotActive(String),
    WasmNotLoaded(String),
    InvokeFailed(String),
    OrchestrationFailed { step_id: String, message: String },
    InputParseError(String),
    OutputSerializeError(String),
    DatabaseError(String),
    TraitError(TraitError),
    InternalError(String),
}
```

---

## 配置选项

```rust
pub struct ServiceConfig {
    /// 默认调用超时（毫秒）
    pub invoke_timeout_ms: u64,  // 默认: 30000
    
    /// 最大重试次数
    pub max_retries: u32,        // 默认: 3
    
    /// 是否启用编排缓存
    pub enable_orchestration_cache: bool,  // 默认: true
}
```

---

*文档版本: 1.0.0*
*最后更新: 2026-04-02*
