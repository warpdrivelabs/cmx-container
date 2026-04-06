# CMX 新模块使用指南

本文档介绍解耦重构后新增的核心模块及其使用方法。

---

## 目录

1. [cmx-traits — Trait 抽象层](#1-cmx-traits--trait-抽象层)
2. [cmx-runtime — WASM 运行时引擎](#2-cmx-runtime--wasm-运行时引擎)
3. [cmx-service — 企业服务层](#3-cmx-service--企业服务层)
4. [HostFunctionProvider 实现](#4-hostfunctionprovider-实现)
5. [PluginQuery 实现](#5-pluginquery-实现)
6. [工程依赖关系](#6-工程依赖关系)
7. [快速开始示例](#7-快速开始示例)

---

## 1. cmx-traits — Trait 抽象层

### 1.1 模块概述

`cmx-traits` 是整个解耦架构的核心枢纽，定义了所有跨模块交互的 trait 接口。它不依赖任何业务模块，仅依赖 `cmx-core`（基础类型）。

### 1.2 核心 Trait

#### PluginQuery — 插件查询接口

```rust
use cmx_traits::{PluginQuery, PluginSnapshot, PluginFilter};

/// 查询插件信息
async fn example(plugin_query: &dyn PluginQuery) {
    // 根据ID查询插件
    let snapshot = plugin_query.get_plugin("my-plugin").await?;
    
    // 检查插件是否激活
    let is_active = plugin_query.is_active("my-plugin").await?;
    
    // 获取 WASM 文件路径
    let wasm_path = plugin_query.get_wasm_path("my-plugin").await?;
    
    // 列出所有已激活插件
    let active_plugins = plugin_query.list_active_plugins().await?;
    
    // 按条件筛选插件
    let filter = PluginFilter {
        status: Some("activated".to_string()),
        domain_code: Some("finance".to_string()),
        ..Default::default()
    };
    let plugins = plugin_query.list_plugins(&filter).await?;
}
```

#### RuntimeInvoker — WASM 调用接口

```rust
use cmx_traits::{RuntimeInvoker, CallerData, WasmInvokeResult};

/// 调用 WASM 函数
async fn invoke_wasm(runtime: &dyn RuntimeInvoker) {
    // 构建调用上下文
    let caller_data = CallerData::new("plugin-id", "db-id")
        .with_request_id("req-123")
        .with_tenant_id("tenant-001");
    
    // 调用 WASM 函数
    let input = br#"{"action": "process"}"#;
    let result: WasmInvokeResult = runtime.invoke(
        "plugin-id",
        "handle_request",
        input,
        &caller_data
    ).await?;
    
    println!("输出: {:?}", result.output);
    println!("耗时: {} μs", result.elapsed_us);
}
```

#### HostFunctionProvider — 宿主函数注册接口

```rust
use cmx_traits::{HostFunctionProvider, WasmLinker, HostFuncError};

/// 自定义宿主函数提供者
struct MyHostFunctions;

impl HostFunctionProvider for MyHostFunctions {
    fn namespace(&self) -> &str {
        "cmx:my_module"
    }
    
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
        // 注册带返回值的函数
        let my_func = Box::new(|caller, input| {
            // input 是已从 WASM 内存读取的字节数据
            let output = process_input(input);
            Ok(output)
        });
        linker.define("cmx:my_module", "my_func", my_func)?;
        
        Ok(())
    }
    
    fn provided_functions(&self) -> Vec<&str> {
        vec!["cmx:my_module/my_func"]
    }
}
```

#### PluginLifecycleListener — 生命周期监听接口

```rust
use cmx_traits::{PluginLifecycleListener, LifecycleEvent};

struct MyLifecycleListener;

#[async_trait]
impl PluginLifecycleListener for MyLifecycleListener {
    async fn on_plugin_activated(&self, event: LifecycleEvent) {
        println!("插件激活: {}", event.plugin_id);
        // 加载 WASM 模块到运行时...
    }
    
    async fn on_plugin_deactivated(&self, event: LifecycleEvent) {
        println!("插件停用: {}", event.plugin_id);
        // 卸载 WASM 模块...
    }
    
    async fn on_plugin_uninstalled(&self, event: LifecycleEvent) {
        println!("插件卸载: {}", event.plugin_id);
        // 清理资源...
    }
}
```

### 1.3 数据类型

#### CallerData — WASM 调用上下文

```rust
use cmx_traits::CallerData;

let caller = CallerData::new("plugin-id", "database-id")
    .with_txn_id("txn-123")           // 事务ID
    .with_request_id("req-456")       // 请求ID
    .with_tenant_id("tenant-789")     // 租户ID
    .with_extra("key", json!({"value": 1})); // 扩展字段
```

#### PluginSnapshot — 插件快照

```rust
pub struct PluginSnapshot {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub install_path: String,
    pub wasm_path: Option<String>,
    pub plugin_type: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}
```

---

## 2. cmx-runtime — WASM 运行时引擎

### 2.1 模块概述

`cmx-runtime` 基于 wasmtime 实现 WASM 模块的加载、实例化和调用。它实现了 `RuntimeInvoker` trait，并管理宿主函数的注册。

### 2.2 全局单例使用

```rust
use cmx_runtime::{GlobalWasmEngine, WasmEngineConfig};
use cmx_traits::RuntimeInvoker;

// 初始化运行时
fn init_runtime() {
    let config = WasmEngineConfig {
        fuel_limit: Some(1_000_000),
        enable_cache: true,
    };
    GlobalWasmEngine::initialize(config).expect("初始化失败");
}

// 获取运行时实例
async fn use_runtime() {
    let engine = GlobalWasmEngine::get().await;
    
    // 加载模块
    engine.load_module("my-plugin", std::path::Path::new("./plugin.wasm")).await.unwrap();
    
    // 调用函数
    let caller_data = CallerData::new("my-plugin", "default");
    let result = engine.invoke("my-plugin", "main", b"input", &caller_data).await.unwrap();
}
```

### 2.3 注册宿主函数

```rust
use cmx_runtime::GlobalWasmEngine;
use cmx_database::host_functions::DatabaseHostFunctions;
use cmx_buffer::host_functions::BufferHostFunctions;
use cmx_utils::host_functions::LoggingHostFunctions;

async fn register_host_functions(
    db_manager: Arc<DatabaseManager>,
    cache_manager: Arc<CacheManager>,
) {
    let mut engine = GlobalWasmEngine::get_mut().await;
    
    // 注册数据库宿主函数
    engine.register_provider(Box::new(DatabaseHostFunctions::new(db_manager)));
    
    // 注册缓存宿主函数
    engine.register_provider(Box::new(BufferHostFunctions::new(cache_manager)));
    
    // 注册日志宿主函数
    engine.register_provider(Box::new(LoggingHostFunctions::new()));
}
```

### 2.4 配置选项

```rust
pub struct WasmEngineConfig {
    /// Fuel 限制（防止无限循环）
    pub fuel_limit: Option<u64>,
    /// 是否启用编译缓存
    pub enable_cache: bool,
    /// 最大实例数
    pub max_instances: usize,
}
```

---

## 3. cmx-service — 企业服务层

### 3.1 模块概述

`cmx-service` 是插件编排的执行引擎，协调 `PluginQuery` 和 `RuntimeInvoker` 完成请求处理。它实现了 `PluginLifecycleListener`，可响应插件生命周期事件。

### 3.2 基本使用

```rust
use cmx_service::{CmxService, ServiceConfig, ServiceHandler};
use cmx_traits::{PluginQuery, RuntimeInvoker};
use std::sync::Arc;

/// 创建服务实例
fn create_service(
    plugin_query: Arc<dyn PluginQuery>,
    runtime: Arc<dyn RuntimeInvoker>,
) -> ServiceHandler {
    // 方式一：使用默认配置
    ServiceHandler::from_components(plugin_query, runtime)
    
    // 方式二：自定义配置
    let config = ServiceConfig {
        invoke_timeout_ms: 60000,
        max_retries: 5,
        enable_orchestration_cache: true,
    };
    let service = Arc::new(CmxService::new(plugin_query, runtime, config));
    ServiceHandler::new(service)
}
```

### 3.3 单次调用

```rust
use cmx_service::{InvokeRequest, InvokeResponse};

async fn invoke_plugin(handler: &ServiceHandler) {
    let request = InvokeRequest {
        plugin_id: "my-plugin".to_string(),
        function_name: "handle_request".to_string(),
        input: json!({"data": "value"}),
        db_id: Some("main-db".to_string()),
        request_id: Some("req-001".to_string()),
        tenant_id: None,
    };
    
    let response: InvokeResponse = handler.handle_invoke(request).await;
    
    if response.success {
        println!("输出: {:?}", response.output);
        println!("耗时: {} μs", response.elapsed_us);
    } else {
        eprintln!("错误: {:?}", response.error);
    }
}
```

### 3.4 编排执行

```rust
use cmx_service::{OrchestrateRequest, Orchestration, OrchestrationStep, StepInput};

async fn execute_orchestration(handler: &ServiceHandler) {
    let orchestration = Orchestration {
        id: "order-flow".to_string(),
        name: "订单处理流程".to_string(),
        description: Some("处理订单的完整流程".to_string()),
        steps: vec![
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
    
    let request = OrchestrateRequest {
        orchestration,
        initial_input: json!({}),
        db_id: Some("main-db".to_string()),
        request_id: Some("req-002".to_string()),
        tenant_id: None,
    };
    
    let response = handler.handle_orchestrate(request).await;
    
    println!("成功: {}", response.success);
    println!("总耗时: {} μs", response.total_elapsed_us);
    for step in response.step_results {
        println!("步骤 {}: {} ({} μs)", 
            step.step_id, 
            if step.success { "成功" } else { "失败" },
            step.elapsed_us
        );
    }
}
```

### 3.5 生命周期监听

```rust
use cmx_traits::PluginLifecycleListener;

// CmxService 实现了 PluginLifecycleListener
async fn setup_lifecycle(service: Arc<CmxService>) {
    // 在插件激活时自动加载 WASM 模块
    service.on_plugin_activated(LifecycleEvent {
        plugin_id: "new-plugin".to_string(),
        version: "1.0.0".to_string(),
        wasm_path: Some(std::path::PathBuf::from("./plugins/new-plugin/main.wasm")),
        timestamp: chrono::Utc::now(),
    }).await;
}
```

---

## 4. HostFunctionProvider 实现

### 4.1 cmx-utils — LoggingHostFunctions

提供日志记录能力：

```rust
use cmx_utils::host_functions::LoggingHostFunctions;
use cmx_traits::HostFunctionProvider;

let logger = LoggingHostFunctions::new();

// 提供的函数：
// - cmx:log/info  — 记录 info 级别日志
// - cmx:log/warn  — 记录 warn 级别日志
// - cmx:log/error — 记录 error 级别日志
```

**WASM 端调用示例：**
```wat
(import "cmx:log" "info" (func $log_info (param i32 i32) (result i32)))
```

### 4.2 cmx-database — DatabaseHostFunctions

提供数据库操作能力：

```rust
use cmx_database::host_functions::DatabaseHostFunctions;
use cmx_database::DatabaseManager;
use std::sync::Arc;

let host_functions = DatabaseHostFunctions::new(db_manager);

// 提供的函数：
// - cmx:database/execute_sql   — 执行写操作 SQL
// - cmx:database/query_sql     — 执行查询 SQL
// - cmx:database/txn_begin     — 开启事务
// - cmx:database/txn_commit    — 提交事务
// - cmx:database/txn_rollback  — 回滚事务
```

**请求格式（JSON）：**
```json
{
    "sql": "SELECT * FROM users WHERE id = ?",
    "params": [1],
    "dataset_id": "default"
}
```

### 4.3 cmx-buffer — BufferHostFunctions

提供 Redis 缓存操作能力：

```rust
use cmx_buffer::host_functions::BufferHostFunctions;
use cmx_buffer::cache::CacheManager;
use std::sync::Arc;

let host_functions = BufferHostFunctions::new(cache_manager);

// 提供的函数：
// - cmx:buffer/cache_get    — 读取缓存
// - cmx:buffer/cache_set    — 写入缓存
// - cmx:buffer/cache_delete — 删除缓存
```

**请求格式（JSON）：**
```json
{
    "key": "user:123",
    "value": "{\"name\": \"John\"}",
    "ttl_seconds": 3600
}
```

**注意：** 所有缓存键自动添加 `plugin:{plugin_id}:` 前缀，实现插件间隔离。

### 4.4 cmx-plugin — PluginHostFunctions

提供插件间调用能力：

```rust
use cmx_plugin::host_functions::PluginHostFunctions;
use cmx_traits::RuntimeInvoker;
use std::sync::Arc;

let host_functions = PluginHostFunctions::new(runtime);

// 提供的函数：
// - cmx:plugin/call_service — 调用另一个插件的服务
// - cmx:plugin/get_info     — 获取当前插件信息
```

**请求格式（JSON）：**
```json
{
    "target_plugin_id": "order-plugin",
    "function_name": "calculate_total",
    "input": {"items": [...]}
}
```

---

## 5. PluginQuery 实现

### 5.1 cmx-plugin — PluginManager 实现

`PluginManager` 已实现 `PluginQuery` trait：

```rust
use cmx_plugin::core::manager::PluginManager;
use cmx_traits::PluginQuery;
use std::sync::Arc;

async fn use_plugin_query() {
    let manager = PluginManager::new(settings).await.unwrap();
    let plugin_query: Arc<dyn PluginQuery> = Arc::new(manager);
    
    // 现在可以通过 trait 接口使用
    let plugin = plugin_query.get_plugin("my-plugin").await.unwrap();
}
```

### 5.2 类型转换

```rust
// PluginInfo -> PluginSnapshot (自动转换)
// PluginRecord -> PluginSnapshot (自动转换，包含 wasm_path)
// cmx-traits::PluginFilter -> cmx-plugin::PluginFilter (自动转换)
```

---

## 6. 工程依赖关系

### 6.1 依赖图

```
┌─────────────────────────────────────────────────────────────────┐
│                        web-server                                │
│                    (应用入口/组装层)                              │
└───────────────────────────┬─────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            │               │               │
            ▼               ▼               ▼
     ┌──────────┐    ┌──────────┐    ┌──────────┐
     │ cmx-api  │    │cmx-plugin│    │cmx-service│
     │(HTTP API)│    │(插件管理) │    │(服务编排) │
     └────┬─────┘    └────┬─────┘    └────┬─────┘
          │               │               │
          │               │               │
          └───────────────┼───────────────┘
                          │
                          ▼
              ┌───────────────────────┐
              │     cmx-traits        │
              │   (Trait 抽象层)       │
              │  - PluginQuery        │
              │  - RuntimeInvoker     │
              │  - HostFunctionProvider│
              │  - LifecycleListener  │
              └───────────┬───────────┘
                          │
        ┌─────────────────┼─────────────────┐
        │                 │                 │
        ▼                 ▼                 ▼
 ┌────────────┐    ┌────────────┐    ┌────────────┐
 │cmx-runtime │    │cmx-database│    │ cmx-buffer │
 │(WASM引擎)  │    │ (数据库)    │    │  (缓存)    │
 └─────┬──────┘    └─────┬──────┘    └─────┬──────┘
       │                 │                 │
       └─────────────────┼─────────────────┘
                         │
                         ▼
              ┌───────────────────────┐
              │      cmx-core         │
              │   (基础类型定义)       │
              └───────────────────────┘
```

### 6.2 各 Crate 依赖关系

| Crate | 依赖 | 被依赖 |
|-------|------|--------|
| **cmx-traits** | cmx-core | cmx-runtime, cmx-database, cmx-buffer, cmx-plugin, cmx-utils, cmx-service |
| **cmx-runtime** | cmx-core, cmx-traits, wasmtime | cmx-service, web-server |
| **cmx-service** | cmx-core, cmx-traits, cmx-database | web-server |
| **cmx-database** | cmx-core, cmx-traits, sqlx | cmx-service, cmx-plugin, web-server |
| **cmx-buffer** | cmx-traits, redis | cmx-plugin, web-server |
| **cmx-plugin** | cmx-traits, cmx-database, cmx-buffer | cmx-api, web-server |
| **cmx-utils** | cmx-traits | cmx-service, web-server |

### 6.3 解耦前后对比

**解耦前：**
```
cmx-plugin ──────► cmx-runtime ──────► wasmtime
     │                  ▲
     └──────────────────┘  (循环依赖风险)
```

**解耦后：**
```
cmx-plugin ──────► cmx-traits ◄────── cmx-runtime
     │                  │
     └── 通过 trait 调用 ──┘
```

### 6.4 宿主函数注册流程

```
┌──────────────────────────────────────────────────────────────┐
│                      web-server 初始化                        │
└────────────────────────────┬─────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│  1. 创建 WasmEngine                                           │
│  2. 注册各模块的 HostFunctionProvider:                         │
│     - DatabaseHostFunctions (cmx-database)                   │
│     - BufferHostFunctions (cmx-buffer)                       │
│     - LoggingHostFunctions (cmx-utils)                       │
│     - PluginHostFunctions (cmx-plugin)                       │
└────────────────────────────┬─────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│  3. 创建 CmxService                                           │
│     - 注入 PluginQuery (PluginManager)                       │
│     - 注入 RuntimeInvoker (WasmEngine)                       │
└────────────────────────────┬─────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│  4. 将 CmxService 注册为 PluginLifecycleListener              │
│     → 插件激活时自动加载 WASM 模块                            │
└──────────────────────────────────────────────────────────────┘
```

---

## 7. 快速开始示例

### 7.1 完整初始化流程

```rust
use std::sync::Arc;
use cmx_runtime::{GlobalWasmEngine, WasmEngineConfig};
use cmx_database::{DatabaseManager, host_functions::DatabaseHostFunctions};
use cmx_buffer::{CacheManager, host_functions::BufferHostFunctions};
use cmx_utils::host_functions::LoggingHostFunctions;
use cmx_plugin::core::manager::PluginManager;
use cmx_service::{CmxService, ServiceHandler};
use cmx_traits::{PluginQuery, RuntimeInvoker, PluginLifecycleListener};

async fn initialize_system() -> ServiceHandler {
    // 1. 初始化基础设施
    let db_manager = Arc::new(DatabaseManager::new(db_config));
    let cache_manager = Arc::new(CacheManager::new(cache_config));
    
    // 2. 初始化 WASM 运行时
    GlobalWasmEngine::initialize(WasmEngineConfig::default());
    {
        let mut engine = GlobalWasmEngine::get_mut().await;
        engine.register_provider(Box::new(DatabaseHostFunctions::new(db_manager.clone())));
        engine.register_provider(Box::new(BufferHostFunctions::new(cache_manager.clone())));
        engine.register_provider(Box::new(LoggingHostFunctions::new()));
    }
    
    // 3. 初始化插件管理器
    let plugin_manager = Arc::new(PluginManager::new(plugin_settings).await.unwrap());
    
    // 4. 创建服务层
    let runtime: Arc<dyn RuntimeInvoker> = GlobalWasmEngine::get_arc();
    let plugin_query: Arc<dyn PluginQuery> = plugin_manager.clone();
    
    let service = Arc::new(CmxService::with_defaults(plugin_query, runtime));
    let handler = ServiceHandler::new(service);
    
    // 5. 注册生命周期监听
    // 当插件激活时，CmxService 会自动加载 WASM 模块
    
    handler
}
```

### 7.2 处理 HTTP 请求

```rust
use axum::{Router, Json, extract::State};
use cmx_service::{ServiceHandler, InvokeRequest};

async fn handle_plugin_call(
    State(handler): State<Arc<ServiceHandler>>,
    Json(request): Json<InvokeRequest>,
) -> Json<InvokeResponse> {
    Json(handler.handle_invoke(request).await)
}

fn create_router(handler: Arc<ServiceHandler>) -> Router {
    Router::new()
        .route("/invoke", post(handle_plugin_call))
        .with_state(handler)
}
```

---

## 附录：错误处理

### TraitError

```rust
pub enum TraitError {
    PluginNotFound(String),      // 插件未找到
    PluginNotActive(String),     // 插件未激活
    WasmLoadFailed(String),      // WASM 加载失败
    WasmInvokeFailed(String),    // WASM 调用失败
    WasmNotLoaded(String),       // WASM 未加载
    OrchestrationFailed(String), // 编排执行失败
    Internal(String),            // 内部错误
}
```

### ServiceError

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

### HostFuncError

```rust
pub enum HostFuncError {
    RegistrationFailed { namespace: String, name: String, reason: String },
    ExecutionFailed { namespace: String, name: String, reason: String },
    MemoryOutOfBounds { offset: u32, len: u32 },
    InvalidParam(String),
}
```

---

*文档版本: 1.0.0*
*最后更新: 2026-04-02*
