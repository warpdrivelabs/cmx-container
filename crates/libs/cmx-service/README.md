# cmx-service

> 企业级通用服务层，作为插件编排的执行引擎，协调 PluginQuery 和 RuntimeInvoker  完成请求处理。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

cmx-service 是 cmx-container 项目的服务编排层，负责服务编排执行、事务管理、服务注册与查询等核心功能。

编排执行按 `service_key` 从服务查询加载编排定义（Flow JSON），从 start 节点沿边遍历执行，直到 end 节点或出错。对外暴露方式：HTTP 皮肤 `crates/libs/cmx-apis/cmx-common-api` 直接使用 `Orchestrator` 执行编排；gRPC 皮肤 `crates/libs/cmx-rpcs/cmx-orchestrator-rpc` 通过依赖注入组合编排能力（不直接依赖本 crate）。

## 快速开始

### 安装

```toml
[dependencies]
cmx-service = "0.1.12"
```

### 核心示例

```rust
use cmx_service::{Orchestrator, ExecuteOptions};
use cmx_core::SVRContext;
use std::sync::Arc;

let orchestrator = Orchestrator::new(
    runtime_invoker,           // Arc<dyn RuntimeInvoker>
    plugin_query,              // Arc<dyn PluginQuery>
    service_query,             // Arc<dyn ServiceQuery>
    "primary".to_string(),     // 默认数据库 ID
);

let result = orchestrator
    .execute_service("order-service", svr_context, ExecuteOptions::new(false))
    .await?;
// result: OrchestrationResult { success, output, steps, total_elapsed_us, error, ... }
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| Orchestrator | 编排执行器，支持服务编排 JSON、事务框、多分支节点 |
| ServiceRegistry | 服务注册中心，提供服务信息的内存缓存 |
| ServiceRepository | 服务仓储层，提供服务定义的数据库访问（cmx_service_define 表） |
| ServiceQueryImpl | ServiceQuery trait 实现（注册表缓存优先，回落数据库） |
| ServiceStorageImpl | ServiceStorage trait 实现 |
| ServiceInvokerImpl | ServiceInvoker trait 实现（供 WASM 插件等经全局单例回调编排） |
| ServiceLifecycleListener | 生命周期监听器，订阅插件生命周期事件并同步服务缓存 |

`handler.rs` / `service.rs` / `request.rs` 为预留模块（未在 lib.rs 挂载），供后续单次调用场景启用。

## 服务编排特性

- **线性流程执行**: start -> func -> func -> end
- **事务框支持**: 多个函数在同一个数据库事务中执行（子节点通过 `parent` 字段指向事务框节点）
- **多分支路由**: switch 节点根据返回值选择执行路径（分支选项经 `options` 与边端口路由）
- **SVRContext 上下文传递**: 初始入参、请求头、各步骤输出在函数间传递
- **调试模式**: 执行到指定节点时暂停，返回插件详情与 code-server URL（配合 cmx-debug）

## 模块结构

```
cmx-service
├── src/
│   ├── lib.rs                  # 库入口 + 全局单例（GlobalServiceQuery/Storage/Registry）
│   ├── error.rs                # 错误类型定义
│   ├── orchestrator/           # 编排执行器
│   │   ├── executor.rs         # Orchestrator 核心（execute_service）
│   │   ├── flow_navigator.rs   # 流程图遍历
│   │   ├── node_handler.rs     # 节点执行
│   │   ├── transaction_manager.rs # 事务框管理
│   │   ├── debug_prepare.rs    # 调试暂停准备
│   │   ├── old.rs             # 历史实现（未在 mod.rs 挂载，不参与编译）
│   │   └── types.rs            # 执行选项/结果类型
│   ├── registry.rs             # 服务注册中心
│   ├── repository.rs           # 服务仓储层
│   ├── service_invoker_impl.rs # ServiceInvoker 实现
│   ├── service_query_impl.rs   # ServiceQuery 实现
│   ├── service_storage_impl.rs # ServiceStorage 实现
│   ├── lifecycle_listener.rs   # 生命周期监听器
│   └── sample-flow.json        # 编排 JSON 完整示例
└── Cargo.toml
```

## 使用指南

### 一、编排执行器 (Orchestrator)

#### 1.1 创建编排执行器

```rust
use cmx_service::Orchestrator;
use cmx_traits::{runtime::RuntimeInvoker, plugin::PluginQuery, service::ServiceQuery};
use std::sync::Arc;

fn create_orchestrator(
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    service_query: Arc<dyn ServiceQuery>,
) -> Orchestrator {
    Orchestrator::new(
        runtime,
        plugin_query,
        service_query,
        "primary".to_string(), // 默认数据库 ID（事务框未指定 databaseId 时使用）
    )
    // 可选：Builder 方式覆盖默认库
    // .with_db_id("orders-db")
}
```

#### 1.2 执行服务编排

执行入口是 `execute_service(service_key, svr_context, options)`：按 service_key 先取 `ServiceDefinition` 校验存在，再取编排定义执行。

```rust
use cmx_service::{Orchestrator, ExecuteOptions};
use cmx_core::SVRContext;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

async fn execute_service(
    orchestrator: &Orchestrator,
    service_key: &str,
) -> Result<(), cmx_service::ServiceError> {
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    let svr_context = SVRContext::new(
        json!({"user_id": 123, "action": "create_order"}),
        headers,
        Utc::now(),
        "req-001".to_string(),
    );

    // include_steps=false：成功时仅返回最终结果（生产推荐）；
    // 执行失败时无论设置如何都会返回 steps 便于排错
    let result = orchestrator
        .execute_service(service_key, svr_context, ExecuteOptions::new(false))
        .await?;

    println!("success: {}", result.success);
    println!("output: {:?}", result.output);
    println!("total: {}us", result.total_elapsed_us);
    Ok(())
}
```

`OrchestrationResult` 字段：`success` / `output`（最终输出，失败时 None）/ `steps: Vec<ExecutionStep>` / `total_elapsed_us` / `error: Option<OrchestrationError>` / `debug_triggered` / `debug_prepare_result`。

#### 1.3 调试模式执行

`ExecuteOptions::with_debug(debug, debug_node_id, debug_params)`：当 `debug=true` 且指定 `debug_node_id` 时，编排器执行到目标节点会暂停，返回 `DebugPrepareResult`（code-server URL、插件详情、节点信息等，供前端发起调试会话，配合 cmx-debug 使用）。

```rust
use cmx_service::ExecuteOptions;

let options = ExecuteOptions::new(true) // 返回步骤数据
    .with_debug(true, Some("validate_input".to_string()), None);

let result = orchestrator.execute_service("order-service", svr_context, options).await?;
if result.debug_triggered == Some(true) {
    let prep = result.debug_prepare_result.unwrap();
    println!("code-server: {}", prep.code_server_url);
    println!("plugin: {} v{}", prep.plugin_id, prep.plugin_version);
}
```

### 二、服务编排 JSON 结构

编排定义来自服务设计器（Flow JSON），顶层为 `{ name, code, description, flow: { nodes, edges } }`，完整示例见 `src/sample-flow.json`。节点类型为 `skylake-*` 系列，节点字段使用 `type`（Rust 侧为 `node_type`）、`data.nodeMeta`（camelCase）等 serde 重命名。

#### 2.1 线性流程

```json
{
  "name": "用户注册服务",
  "code": "user-register",
  "flow": {
    "nodes": [
      { "id": "start", "type": "skylake-start", "meta": {} },
      {
        "id": "create_user",
        "type": "skylake-func",
        "meta": {},
        "data": {
          "name": "创建用户",
          "nodeMeta": {
            "pluginId": "user-plugin",
            "pluginName": "用户插件",
            "pluginVersion": "1.0.0",
            "functionName": "create",
            "databaseId": null
          },
          "inputs": [], "outputs": []
        }
      },
      { "id": "end", "type": "skylake-end", "meta": {} }
    ],
    "edges": [
      { "sourceNodeID": "start", "sourcePortID": "out", "targetNodeID": "create_user", "targetPortID": "in" },
      { "sourceNodeID": "create_user", "sourcePortID": "out", "targetNodeID": "end", "targetPortID": "in" }
    ]
  }
}
```

#### 2.2 多分支流程（skylake-switch）

switch 节点在 `data.options` 中声明分支出口（端口名），边的 `sourcePortID` 对应命中的分支：

```json
{
  "id": "route_check",
  "type": "skylake-switch",
  "meta": {},
  "data": {
    "name": "路由判断",
    "nodeMeta": {
      "pluginId": "bbgl", "pluginName": "路由插件", "pluginVersion": "1.0.0",
      "functionName": "route_check", "databaseId": null
    },
    "options": ["1", "2", "3"],
    "inputs": [], "outputs": []
  }
}
```

```json
{ "sourceNodeID": "route_check", "sourcePortID": "1", "targetNodeID": "branch_1_func", "targetPortID": "in" }
```

#### 2.3 事务流程（skylake-transaction）

事务框是一个节点（`type: "skylake-transaction"`，`nodeMeta.databaseId` 指定事务库），框内函数节点通过 `parent` 字段指向事务框节点 ID；执行时共享同一 `txn_id`，任一节点失败整体回滚。

```json
{
  "id": "transaction_box",
  "type": "skylake-transaction",
  "meta": {},
  "data": {
    "name": "事务处理框",
    "nodeMeta": {
      "pluginId": "", "pluginName": "", "pluginVersion": "",
      "functionName": "", "databaseId": "primary"
    },
    "inputs": [], "outputs": []
  }
}
```

### 三、服务注册中心 (ServiceRegistry)

`ServiceRegistry` 是纯内存缓存（service_key -> ServiceDefinition，附 orchestration JSON 缓存与插件索引）。

```rust
use cmx_service::ServiceRegistry;
use std::collections::HashMap;

// 注册（同时缓存编排 JSON）
registry.register(service_definition, Some(orchestration_json)).await;

// 注销（service_key + plugin_id 双重定位）
registry.unregister("order-service", "order-plugin").await;

// 查询
let def = registry.get("order-service").await;               // Option<ServiceDefinition>
let list = registry.get_by_plugin("order-plugin").await;      // Vec<ServiceDefinition>
let flow = registry.get_orchestration("order-service").await; // Option<serde_json::Value>
let keys = registry.get_all_keys().await;                     // Vec<String>

// 批量重建（先清空再装载，用于应用启动时预热）
registry.load_all(services, orchestrations_map).await;

// 按插件同步（插件安装/升级后刷新其服务）
registry.sync_plugin_services(plugin_id, services, orchestrations).await;
```

### 四、服务仓储层 (ServiceRepository)

`ServiceRepository` 直接经 cmx-database 执行 SQL，读写 `cmx_service_define` 表：

```rust
use cmx_service::ServiceRepository;
use std::sync::Arc;

let repo = ServiceRepository::new(
    Arc::new(database_manager), // cmx_database::DatabaseManager
    "primary".to_string(),      // 默认数据库 ID
);
// .with_db_id("orders-db") 可覆盖

// 保存（或在外层事务中保存：save_service_with_txn）
repo.save_service(&definition).await?;

// 查询（带 app_id 隔离）
let def = repo.get_service("order-service", "app-001").await?;
let all = repo.list_services("app-001").await?;
let by_plugin = repo.get_services_by_plugin("order-plugin").await?;
let (items, total) = repo.page_services(&filter, 1, 20).await?; // ServicePageFilter

// 删除与版本留档
repo.delete_service("order-service", "app-001").await?;
repo.delete_services_by_plugin("order-plugin").await?;
repo.save_service_version(params).await?;
let versions = repo.get_service_versions("order-service", "app-001").await?;
let config = repo.get_service_config("order-service", "app-001").await?;
```

### 五、trait 实现与全局单例

三个 trait 实现：

- `ServiceQueryImpl::new(repository, registry, app_id)` — 实现 `ServiceQuery`（注册表缓存优先，回落数据库）
- `ServiceStorageImpl::new(repository)` — 实现 `ServiceStorage`（保存/删除/版本）
- `ServiceInvokerImpl::new(runtime, plugin_query, service_query, default_db_id)` — 实现 `ServiceInvoker`（供 WASM 插件回调编排服务）

全局单例（`set` 为同步方法，重复设置返回 Err；`get` 返回静态引用，未初始化时 panic）：

```rust
use cmx_service::{
    GlobalServiceQuery, GlobalServiceRegistry, GlobalServiceStorage,
    ServiceQueryImpl, ServiceStorageImpl,
};
use std::sync::Arc;

fn init_globals(
    query_impl: ServiceQueryImpl,
    storage_impl: ServiceStorageImpl,
    registry: Arc<ServiceRegistry>,
) -> Result<(), String> {
    GlobalServiceQuery::set(Arc::new(query_impl))?;
    GlobalServiceStorage::set(Arc::new(storage_impl))?;
    GlobalServiceRegistry::set(registry)?;
    Ok(())
}

fn use_globals() {
    let query = GlobalServiceQuery::get();      // &'static Arc<dyn ServiceQuery>
    let storage = GlobalServiceStorage::get();  // &'static Arc<dyn ServiceStorage>
    let registry = GlobalServiceRegistry::get();
    assert!(GlobalServiceQuery::is_initialized());
}
```

### 六、生命周期监听 (ServiceLifecycleListener)

`ServiceLifecycleListener` 是具体结构体（非 trait），订阅全局事件总线的插件安装/升级/卸载事件，自动同步服务注册缓存与数据库：

```rust
use cmx_service::ServiceLifecycleListener;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let listener = ServiceLifecycleListener::new(
        Arc::new(query_impl),        // ServiceQuery
        Arc::new(repository),        // ServiceRepository
        Arc::new(service_registry),  // ServiceRegistry
        "app-001".to_string(),       // 过滤非本应用事件
    );
    listener.register().await; // 订阅 plugin.installed / upgraded / uninstalled / downgraded / reinstalled / loaded / unloaded
}
```

### 七、错误处理

`ServiceError` 覆盖编排执行、数据库访问与插件调用的错误场景：

```rust
use cmx_service::ServiceError;

match err {
    ServiceError::PluginNotFound(id) | ServiceError::PluginNotActive(id) => {
        eprintln!("Plugin unavailable: {}", id);
    }
    ServiceError::WasmNotLoaded(id) => {
        eprintln!("WASM module not loaded: {}", id);
    }
    ServiceError::InvokeFailed(msg) => {
        eprintln!("WASM invocation failed: {}", msg);
    }
    ServiceError::OrchestrationFailed { step_id, message } => {
        eprintln!("Step {} failed: {}", step_id, message);
    }
    ServiceError::NodeExecutionFailed { node_id, node_name, node_type, detail } => {
        eprintln!("Node {}({}) [{}] failed: {}", node_name, node_id, node_type, detail);
    }
    ServiceError::TransactionRolledBack { txn_id, reason } => {
        eprintln!("Transaction {} rolled back: {}", txn_id, reason);
    }
    ServiceError::InputParseError(msg)
    | ServiceError::OutputSerializeError(msg)
    | ServiceError::DatabaseError(msg)
    | ServiceError::InternalError(msg) => {
        eprintln!("Service error: {}", msg);
    }
    ServiceError::TraitError(e) => {
        eprintln!("Underlying trait error: {}", e);
    }
}
```

### 八、完整示例

```rust
use cmx_service::{
    ExecuteOptions, GlobalServiceQuery, GlobalServiceRegistry, GlobalServiceStorage,
    Orchestrator, ServiceQueryImpl, ServiceRegistry, ServiceRepository,
    ServiceStorageImpl, ServiceLifecycleListener,
};
use cmx_core::SVRContext;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化各组件（runtime/plugin_query 来自 cmx-runtime / cmx-plugin 装配层）
    let runtime: Arc<dyn cmx_traits::runtime::RuntimeInvoker> = create_runtime()?;
    let plugin_query: Arc<dyn cmx_traits::plugin::PluginQuery> = create_plugin_query()?;
    let repository = Arc::new(ServiceRepository::new(db_manager, "primary".to_string()));
    let registry = Arc::new(ServiceRegistry::new());

    // 2. 初始化全局单例
    let query_impl = ServiceQueryImpl::new(repository.clone(), registry.clone(), "app-001".to_string());
    GlobalServiceQuery::set(Arc::new(query_impl)).map_err(|e| format!("{}", e))?;
    GlobalServiceStorage::set(Arc::new(ServiceStorageImpl::new(repository.clone())))
        .map_err(|e| format!("{}", e))?;
    GlobalServiceRegistry::set(registry.clone()).map_err(|e| format!("{}", e))?;

    // 3. 订阅插件生命周期事件，自动同步服务缓存
    ServiceLifecycleListener::new(
        GlobalServiceQuery::get().clone(),
        repository.clone(),
        registry.clone(),
        "app-001".to_string(),
    )
    .register()
    .await;

    // 4. 创建编排执行器并执行
    let orchestrator = Orchestrator::new(
        runtime,
        plugin_query,
        GlobalServiceQuery::get().clone(),
        "primary".to_string(),
    );

    let svr_context = SVRContext::new(
        json!({"order_id": "ORD-12345", "user_id": 1001}),
        HashMap::new(),
        Utc::now(),
        "req-001".to_string(),
    );

    let result = orchestrator
        .execute_service("order_processing", svr_context, ExecuteOptions::new(true))
        .await?;

    println!("success: {}, elapsed: {}us", result.success, result.total_elapsed_us);
    for step in &result.steps {
        println!("  [{}] {} -> {:?}", step.node_type, step.node_name, step.status);
    }

    Ok(())
}
```
