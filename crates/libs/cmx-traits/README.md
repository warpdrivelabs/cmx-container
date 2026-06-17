# cmx-traits

> 跨模块 trait 接口抽象层，定义 cmx-container 项目中所有跨模块交互的 trait 接口。

## 项目简介

本 crate 作为模块间解耦的核心枢纽，各业务模块（cmx-plugin, cmx-service, cmx-runtime）仅依赖本 crate 的 trait 定义，通过 trait 对象交互，不直接依赖彼此的 crate。

## 快速开始

### 安装

```toml
[dependencies]
cmx-traits = "0.1.0"
```

### 核心示例

```rust
use cmx_traits::runtime::{RuntimeInvoker, WasmInvokeResult, InvokeContext};

async fn invoke_wasm(
    invoker: &dyn RuntimeInvoker,
    wasm_bytes: &[u8],
    func_name: &str,
    input: &[u8],
) -> WasmInvokeResult {
    let context = InvokeContext::default();
    invoker.invoke(wasm_bytes, func_name, input, &context).await
}
```

## 核心功能与特性

| 接口 | 模块路径 | 说明 |
|------|---------|------|
| `PluginQuery` | `cmx_traits::plugin` | 插件状态查询（cmx-service 查询 cmx-plugin） |
| `RuntimeInvoker` | `cmx_traits::runtime` | WASM 运行时调用（cmx-service 调用 cmx-runtime） |
| `PluginLifecycleListener` | `cmx_traits::plugin` | 生命周期事件监听（cmx-plugin 通知 cmx-service） |
| `HostFunctionProvider` | `cmx_traits::runtime` | 宿主函数注册（各模块向 cmx-runtime 注册宿主函数） |
| `ServiceQuery` | `cmx_traits::service` | 服务信息查询 |
| `ServiceStorage` | `cmx_traits::service` | 服务定义存储 |
| `AuthService` | `cmx_traits::auth` | 认证服务统一接口 |
| `PermissionChecker` | `cmx_traits::iam` | 权限校验器 |
| `RpcClient` | `cmx_traits::rpc` | RPC 调用统一接口 |
| `EventBus` | `cmx_traits::event_bus` | 全局事件总线 |

## 模块结构

```
cmx-traits
├── src/
│   ├── lib.rs                    # 库入口，仅声明 pub mod
│   ├── error.rs                  # 通用错误类型（TraitError, HostFuncError）
│   ├── auth/                     # 认证领域
│   │   ├── mod.rs
│   │   ├── error.rs              # AuthError
│   │   ├── policy.rs             # AuthPolicy
│   │   ├── service.rs            # AuthService
│   │   ├── storage_query.rs      # AuthStorageQuery
│   │   └── user_query.rs         # UserAuthQuery
│   ├── iam/                      # IAM 领域
│   │   ├── mod.rs
│   │   ├── data_scope.rs         # DataScope
│   │   └── permission_checker.rs # PermissionChecker
│   ├── plugin/                   # 插件领域
│   │   ├── mod.rs
│   │   ├── query.rs              # PluginQuery
│   │   └── lifecycle.rs          # PluginLifecycleListener
│   ├── runtime/                  # WASM 运行时领域
│   │   ├── mod.rs
│   │   ├── invoker.rs            # RuntimeInvoker
│   │   ├── host_func.rs          # HostFunctionProvider
│   │   ├── invoke_context.rs     # InvokeContext
│   │   └── global.rs             # GlobalRuntime
│   ├── service/                  # 服务领域
│   │   ├── mod.rs
│   │   ├── query.rs              # ServiceQuery
│   │   ├── storage.rs            # ServiceStorage
│   │   ├── invoker.rs            # ServiceInvoker
│   │   └── global_invoker.rs     # GlobalServiceInvoker
│   ├── rpc/                      # RPC 领域
│   │   ├── mod.rs
│   │   └── client.rs             # RpcClient
│   └── event_bus/                # 事件总线
│       ├── mod.rs
│       ├── bus.rs
│       ├── global.rs
│       └── types.rs
└── Cargo.toml
```

## 主要模块说明

### `runtime`

定义 `RuntimeInvoker` trait，用于 WASM 运行时调用。

```rust
pub trait RuntimeInvoker: Send + Sync {
    async fn invoke(
        &self,
        wasm_bytes: &[u8],
        func_name: &str,
        input: &[u8],
        context: &InvokeContext,
    ) -> WasmInvokeResult;
}
```

### `plugin`

定义 `PluginQuery` trait，用于查询插件状态。

### `plugin::lifecycle`

定义 `PluginLifecycleListener` trait，用于生命周期事件监听。

### `event_bus`

提供全局事件总线机制，支持事件发布/订阅模式。

## 使用指南

### 一、RuntimeInvoker 接口

#### 1.1 基础调用

```rust
use cmx_traits::runtime::{
    RuntimeInvoker,
    WasmInvokeResult,
    InvokeContext,
};

async fn call_wasm_function(
    invoker: &dyn RuntimeInvoker,
    wasm_bytes: &[u8],
    func_name: &str,
    input_data: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let context = InvokeContext::default();

    let result = invoker
        .invoke(wasm_bytes, func_name, input_data, &context)
        .await?;

    Ok(result.output)
}
```

#### 1.2 使用 InvokeContext 传递元数据

```rust
use cmx_traits::runtime::InvokeContext;

let mut context = InvokeContext::default();

// 设置调用追踪 ID
context.set_trace_id("trace-12345");

// 设置调用超时时间（毫秒）
context.set_timeout(5000);

// 设置是否为调试模式
context.set_debug(true);

// 添加自定义上下文数据
context.set("user_id", "user-001");
context.set("request_id", "req-456");

let result = invoker.invoke(wasm_bytes, func_name, input, &context).await?;
```

#### 1.3 实现自定义 RuntimeInvoker

```rust
use cmx_traits::runtime::{
    RuntimeInvoker, WasmInvokeResult, InvokeContext,
};
use async_trait::async_trait;

struct CustomRuntimeInvoker {
    engine: MyWasmEngine,
}

#[async_trait]
impl RuntimeInvoker for CustomRuntimeInvoker {
    async fn invoke(
        &self,
        wasm_bytes: &[u8],
        func_name: &str,
        input: &[u8],
        context: &InvokeContext,
    ) -> WasmInvokeResult {
        // 1. 加载 WASM 模块
        let module = self.engine.load_module(wasm_bytes).await;

        // 2. 准备调用参数
        let params = prepare_wasm_params(input, context);

        // 3. 执行调用
        match self.engine.call(&module, func_name, params).await {
            Ok(output) => WasmInvokeResult::Success(output),
            Err(e) => WasmInvokeResult::Failure(1, e.to_string()),
        }
    }
}
```

### 二、PluginQuery 接口

#### 2.1 查询插件状态

```rust
use cmx_traits::plugin::{PluginQuery, PluginSnapshot};

async fn check_plugin_status(
    query: &dyn PluginQuery,
    plugin_id: &str,
) -> Result<Option<PluginStatus>, Box<dyn std::error::Error>> {
    let status = query.get_plugin_status(plugin_id).await?;
    Ok(status)
}

async fn list_active_plugins(
    query: &dyn PluginQuery,
) -> Result<Vec<PluginInfo>, Box<dyn std::error::Error>> {
    let plugins = query.list_plugins().await?;
    let active: Vec<PluginInfo> = plugins
        .into_iter()
        .filter(|p| p.status == PluginStatus::Active)
        .collect();
    Ok(active)
}
```

#### 2.2 获取插件详细信息

```rust
use cmx_traits::PluginQuery;

async fn get_plugin_details(
    query: &dyn PluginQuery,
    plugin_id: &str,
) -> Result<Option<PluginInfo>, Box<dyn std::error::Error>> {
    let info = query.get_plugin_info(plugin_id).await?;
    if let Some(plugin) = info {
        println!("Plugin: {} v{}", plugin.id, plugin.version);
        println!("Status: {:?}", plugin.status);
        println!("Installed at: {:?}", plugin.installed_at);
    }
    Ok(info)
}
```

### 三、PluginLifecycleListener 接口

#### 3.1 监听插件生命周期事件

```rust
use cmx_traits::plugin::{
    PluginLifecycleListener,
    PluginLifecyclePayload,
    plugin_events,
};
use async_trait::async_trait;

struct PluginEventHandler;

#[async_trait]
impl PluginLifecycleListener for PluginEventHandler {
    async fn on_installed(&self, payload: &PluginLifecyclePayload) {
        tracing::info!(
            "Plugin {} installed at {:?}",
            payload.plugin_id,
            payload.install_path
        );
    }

    async fn on_activated(&self, payload: &PluginLifecyclePayload) {
        tracing::info!("Plugin {} activated", payload.plugin_id);
    }

    async fn on_deactivated(&self, payload: &PluginLifecyclePayload) {
        tracing::info!("Plugin {} deactivated", payload.plugin_id);
    }

    async fn on_upgraded(&self, payload: &PluginLifecyclePayload) {
        tracing::info!(
            "Plugin {} upgraded to {}",
            payload.plugin_id,
            payload.version
        );
    }

    async fn on_uninstalled(&self, payload: &PluginLifecyclePayload) {
        tracing::info!("Plugin {} uninstalled", payload.plugin_id);
    }
}
```

#### 3.2 批量注册监听器

```rust
use cmx_traits::PluginLifecycleListener;

let handler = Arc::new(PluginEventHandler {});

// 注册单个事件监听
GlobalPluginManager::get()
    .await
    .register_lifecycle_listener(handler.clone())
    .await;
```

### 四、ServiceQuery 与 ServiceStorage 接口

#### 4.1 查询服务定义

```rust
use cmx_traits::service::ServiceQuery;
use cmx_core::model::service::ServiceDefinition;

async fn find_service(
    query: &dyn ServiceQuery,
    service_id: &str,
) -> Result<Option<ServiceDefinition>, Box<dyn std::error::Error>> {
    let service = query.get_service(service_id).await?;
    Ok(service)
}

async fn list_services_by_plugin(
    query: &dyn ServiceQuery,
    plugin_id: &str,
) -> Result<Vec<ServiceInfo>, Box<dyn std::error::Error>> {
    let services = query.list_services().await?;
    let plugin_services: Vec<ServiceInfo> = services
        .into_iter()
        .filter(|s| s.plugin_id == plugin_id)
        .collect();
    Ok(plugin_services)
}
```

#### 4.2 保存服务定义

```rust
use cmx_traits::service::ServiceStorage;
use cmx_core::model::service::ServiceDefinition;

async fn save_service_definition(
    storage: &dyn ServiceStorage,
    definition: ServiceDefinition,
) -> Result<(), Box<dyn std::error::Error>> {
    storage.save_service(&definition).await?;
    tracing::info!("Service {} saved", definition.id);
    Ok(())
}

async fn delete_service(
    storage: &dyn ServiceStorage,
    service_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    storage.delete_service(service_id).await?;
    tracing::info!("Service {} deleted", service_id);
    Ok(())
}
```

### 五、EventBus 事件总线

#### 5.1 发布事件

```rust
use cmx_traits::event_bus::GlobalEventBus;
use cmx_traits::plugin::plugin_events;

#[tokio::main]
async fn main() {
    // 发布插件安装事件
    let payload = serde_json::json!({
        "plugin_id": "my-plugin",
        "version": "1.0.0",
        "install_path": "/plugins/my-plugin/1.0.0"
    });

    GlobalEventBus::get()
        .publish(plugin_events::INSTALLED, payload)
        .await;

    println!("Event published");
}
```

#### 5.2 订阅事件

```rust
use cmx_traits::event_bus::{GlobalEventBus, EventBus, EventHandler};
use cmx_traits::plugin::plugin_events;
use std::sync::Arc;

struct MyEventHandler;

#[async_trait::async_trait]
impl EventHandler for MyEventHandler {
    async fn handle(&self, event: &str, payload: serde_json::Value) {
        tracing::info!("Received event: {} with payload: {:?}", event, payload);
    }
}

#[tokio::main]
async fn main() {
    let handler: Arc<dyn EventHandler> = Arc::new(MyEventHandler {});

    // 订阅特定事件
    GlobalEventBus::get()
        .subscribe(plugin_events::INSTALLED, handler.clone())
        .await;

    // 也可以使用通配符订阅所有事件
    GlobalEventBus::get()
        .subscribe("*", handler.clone())
        .await;
}
```

#### 5.3 取消订阅

```rust
use cmx_traits::GlobalEventBus;

#[tokio::main]
async fn main() {
    // 取消订阅
    GlobalEventBus::get()
        .unsubscribe(plugin_events::INSTALLED)
        .await;
}
```

### 六、全局单例访问

#### 6.1 全局运行时

```rust
use cmx_traits::GlobalRuntime;

async fn use_global_runtime() {
    // 检查运行时是否已初始化
    if GlobalRuntime::is_initialized() {
        let runtime = GlobalRuntime::get().await;
        // 使用运行时...
    }
}
```

#### 6.2 全局服务查询器

```rust
use cmx_traits::GlobalServiceQuery;

async fn use_global_service_query() {
    // 设置全局服务查询器
    let query_impl = MyServiceQueryImpl::new();
    GlobalServiceQuery::set(Arc::new(query_impl)).unwrap();

    // 获取全局服务查询器
    let query = GlobalServiceQuery::get().unwrap();
    let services = query.list_services().await.unwrap();
}
```

### 七、错误处理

```rust
use cmx_traits::error::{TraitError, HostFuncError};

// TraitError 用于 trait 方法返回的错误
impl From<TraitError> for std::io::Error {
    fn from(err: TraitError) -> Self {
        match err {
            TraitError::NotFound(msg) => {
                std::io::Error::new(std::io::ErrorKind::NotFound, msg)
            }
            TraitError::InvalidInput(msg) => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
            }
            TraitError::Internal(msg) => {
                std::io::Error::new(std::io::ErrorKind::Other, msg)
            }
        }
    }
}

// HostFuncError 用于宿主函数调用错误
fn handle_host_func_error(err: HostFuncError) {
    match err {
        HostFuncError::NotFound(func_name) => {
            eprintln!("Host function not found: {}", func_name);
        }
        HostFuncError::InvalidParams(msg) => {
            eprintln!("Invalid parameters: {}", msg);
        }
        HostFuncError::ExecutionFailed(msg) => {
            eprintln!("Execution failed: {}", msg);
        }
    }
}
```
