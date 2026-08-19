# cmx-traits

> 跨模块 trait 接口抽象层，定义 cmx-container 项目中所有跨模块交互的 trait 接口 。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

本 crate 作为模块间解耦的核心枢纽，各业务模块（cmx-plugin、cmx-service、cmx-runtime）仅依赖本 crate 的 trait 定义，通过 trait 对象交互，不直接依赖彼此的 crate；同时提供 `GlobalRuntime` / `GlobalEventBus` / `GlobalServiceInvoker` 等全局单例访问点，支持依赖注入与单元测试 mock。

## 快速开始

### 安装

```toml
[dependencies]
cmx-traits = "0.1.12"
```

### 核心示例

```rust
use cmx_traits::runtime::{RuntimeInvoker, WasmInvokeResult};

async fn invoke_wasm(
    invoker: &dyn RuntimeInvoker,
    plugin_id: &str,
    function_name: &str,
    input: &[u8],
) -> Result<WasmInvokeResult, cmx_traits::TraitError> {
    invoker.invoke(plugin_id, function_name, input).await
}
```

## 核心功能与特性

| 接口 | 模块路径 | 说明 |
|------|---------|------|
| `RuntimeInvoker` | `cmx_traits::runtime` | WASM 运行时调用（cmx-service 调用 cmx-runtime） |
| `HostFunctionProvider` | `cmx_traits::runtime` | 宿主函数注册（各模块向 cmx-runtime 注册宿主函数） |
| `PluginQuery` | `cmx_traits::plugin` | 插件状态查询（cmx-service 查询 cmx-plugin） |
| `PluginLifecycleListener` | `cmx_traits::plugin` | 生命周期事件监听（cmx-plugin 通知 cmx-service / cmx-runtime） |
| `ServiceQuery` / `ServiceStorage` / `ServiceInvoker` | `cmx_traits::service` | 服务查询 / 存储 / 编排执行 |
| `FunctionInvoker` | `cmx_traits::function_invoker` | 插件函数调用（单函数粒度） |
| `AuthService` / `AuthPolicy` | `cmx_traits::auth` | 认证服务统一接口 |
| `UserAuthQuery` / `AuthStorageQuery` | `cmx_traits::auth` | 认证数据查询 |
| `PermissionChecker` | `cmx_traits::iam` | 权限校验器 |
| `CodeMinter` | `cmx_traits::code` | 业务编码生成器 |
| `ServiceOrchestrationClient` / `ResourceDataClient` | `cmx_traits::rpc` | 跨服务 RPC 调用（gRPC 皮肤契约） |
| `ResourceDataImporter` | `cmx_traits::resource` | 平台资源数据导入（表单/菜单/权限等定义包） |
| `EventBus` | `cmx_traits::event_bus` | 全局事件总线 |

## 模块结构

```
cmx-traits
├── src/
│   ├── lib.rs                    # 库入口，仅声明 pub mod
│   ├── error.rs                  # 通用错误类型（TraitError, HostFuncError, SetSystemAuthError）
│   ├── auth/                     # 认证领域
│   │   ├── context_scope.rs      # 请求级认证上下文作用域（RequestAuth / current_auth 等）
│   │   ├── error.rs              # AuthError
│   │   ├── policy.rs             # AuthPolicy
│   │   ├── service.rs            # AuthService 及 UserInfo/TokenPair/Credentials 等
│   │   ├── storage_query.rs      # AuthStorageQuery
│   │   └── user_query.rs         # UserAuthQuery 及 UserAuthData/ApiKeyData 等
│   ├── code/                     # 业务编码生成（CodeMinter / GlobalCodeMinter）
│   ├── function_invoker.rs       # FunctionInvoker / FunctionInvokeResult
│   ├── step_status.rs            # StepStatus 字符串编解码（跨模块单一来源）
│   ├── iam/                      # IAM 领域
│   │   ├── data_scope.rs         # DataScope
│   │   └── permission_checker.rs # PermissionChecker
│   ├── plugin/                   # 插件领域
│   │   ├── lifecycle.rs          # PluginLifecycleListener / PluginLifecyclePayload / plugin_events
│   │   └── query.rs              # PluginQuery / PluginSnapshot / PluginFilter
│   ├── resource/                 # 平台资源领域
│   │   ├── importer.rs           # ResourceDataImporter
│   │   ├── dto.rs                # 导入/清理/列表 DTO
│   │   └── category/form/menu/permission/table.rs  # 各类资源定义
│   ├── runtime/                  # WASM 运行时领域
│   │   ├── global.rs             # GlobalRuntime
│   │   ├── host_func.rs          # HostFunctionProvider / HostFunctionDef / ValType
│   │   ├── invoke_context.rs     # InvokeOptions / InvokeContext（调用深度与循环检测）
│   │   └── invoker.rs            # RuntimeInvoker / WasmInvokeResult
│   ├── service/                  # 服务领域
│   │   ├── global_invoker.rs     # GlobalServiceInvoker
│   │   ├── invoker.rs            # ServiceInvoker / ServiceInvokeOptions
│   │   ├── query.rs              # ServiceQuery / ServicePageFilter / ServicePageResult
│   │   └── storage.rs            # ServiceStorage / SaveServiceVersionParams
│   ├── rpc/                      # RPC 领域
│   │   ├── orchestrator.rs       # ServiceOrchestrationClient
│   │   ├── resource_data.rs      # ResourceDataClient
│   │   ├── error.rs              # RpcError
│   │   └── types.rs              # FunctionCallResult 等
│   └── event_bus/                # 事件总线
│       ├── bus.rs                # EventBus
│       ├── global.rs             # GlobalEventBus
│       └── types.rs              # EventTopic / EventPayload / EventHandler
└── Cargo.toml
```

## 主要模块说明

### `runtime`

定义 `RuntimeInvoker` trait，用于 WASM 运行时调用（按插件 ID 调用已加载模块的导出函数）。

```rust
#[async_trait::async_trait]
pub trait RuntimeInvoker: Send + Sync {
    async fn invoke(&self, plugin_id: &str, function_name: &str, input: &[u8])
        -> Result<WasmInvokeResult, TraitError> { /* 默认转发到 invoke_with_options */ }
    async fn invoke_with_options(&self, plugin_id: &str, function_name: &str,
        input: &[u8], options: &InvokeOptions) -> Result<WasmInvokeResult, TraitError>;
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError>;
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError>;
    async fn is_loaded(&self, plugin_id: &str) -> bool;
}
```

`WasmInvokeResult` 为结构体：`{ output: Vec<u8>, elapsed_us: u64, fuel_consumed: Option<u64> }`。

### `plugin`

定义 `PluginQuery` trait（get_plugin / is_installed / is_active / get_wasm_path / list_plugins），用于查询插件安装与激活状态。

### `plugin::lifecycle`

定义 `PluginLifecycleListener` trait（on_plugin_installed / on_plugin_upgraded / on_plugin_uninstalled / on_plugin_downgraded），用于生命周期事件监听。

### `event_bus`

提供全局事件总线机制，支持事件发布/订阅模式；handler 为同步闭包（`Arc<dyn Fn(EventTopic, EventPayload)>`），异步派发由总线内部 spawn 完成。

## 使用指南

### 一、RuntimeInvoker 接口

#### 1.1 基础调用

```rust
use cmx_traits::runtime::RuntimeInvoker;

async fn call_wasm_function(
    invoker: &dyn RuntimeInvoker,
    plugin_id: &str,
    func_name: &str,
    input_data: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let result = invoker.invoke(plugin_id, func_name, input_data).await?;
    Ok(result.output)
}
```

#### 1.2 InvokeOptions 与调用深度控制

`InvokeOptions` 携带调用选项：`timeout`（默认 30s，`DEFAULT_TIMEOUT`）、`max_depth`（默认 8，`DEFAULT_MAX_DEPTH`）、`debug`。

`InvokeContext` 是线程局部的调用深度与循环检测管理器：进入嵌套调用前用 `enter(plugin_id, function_name, max_depth)` 取得 RAII 守卫 `InvokeGuard`（Drop 时自动退出），深度超限或检测到同插件同函数循环时返回 `InvokeGuardError`（`DepthExceeded` / `CycleDetected`）。

```rust
use std::time::Duration;
use cmx_traits::runtime::InvokeOptions;

let options = InvokeOptions::new()
    .with_timeout(Duration::from_secs(5))
    .with_max_depth(16);

let result = invoker
    .invoke_with_options(plugin_id, func_name, input, &options)
    .await?;

// 查询当前线程调用深度 / 循环
let depth = cmx_traits::runtime::InvokeContext::current_depth();
let cyclic = cmx_traits::runtime::InvokeContext::is_cycle(plugin_id, func_name);
```

#### 1.3 实现自定义 RuntimeInvoker

```rust
use cmx_traits::runtime::{RuntimeInvoker, WasmInvokeResult, InvokeOptions, InvokeGuard};
use cmx_traits::TraitError;
use async_trait::async_trait;
use std::path::Path;

struct CustomRuntimeInvoker { /* ... */ }

#[async_trait]
impl RuntimeInvoker for CustomRuntimeInvoker {
    async fn invoke_with_options(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError> {
        // 循环检测 + 深度守卫（第三参数为最大深度，Drop 时自动退出）
        let _guard = InvokeContext::enter(plugin_id, function_name, options.max_depth)
            .map_err(|e| TraitError::Internal(e.to_string()))?;
        // 加载模块并执行调用 ...
        Ok(WasmInvokeResult {
            output: Vec::new(),
            elapsed_us: 0,
            fuel_consumed: None,
        })
    }

    async fn load_module(&self, _plugin_id: &str, _wasm_path: &Path) -> Result<(), TraitError> {
        Ok(())
    }
    async fn unload_module(&self, _plugin_id: &str) -> Result<(), TraitError> { Ok(()) }
    async fn is_loaded(&self, _plugin_id: &str) -> bool { true }
}
```

### 二、PluginQuery 接口

#### 2.1 查询插件状态

```rust
use cmx_traits::plugin::{PluginQuery, PluginFilter};

async fn check_plugin_status(
    query: &dyn PluginQuery,
    plugin_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(query.is_active(plugin_id).await?)
}

async fn list_active_plugins(
    query: &dyn PluginQuery,
) -> Result<Vec<cmx_traits::plugin::PluginSnapshot>, Box<dyn std::error::Error>> {
    let filter = PluginFilter {
        status: Some("activated".to_string()),
        ..Default::default()
    };
    Ok(query.list_plugins(&filter).await?)
}
```

#### 2.2 获取插件快照

```rust
use cmx_traits::plugin::{PluginQuery, PluginSnapshot};

async fn get_plugin_details(
    query: &dyn PluginQuery,
    plugin_id: &str,
) -> Result<Option<PluginSnapshot>, Box<dyn std::error::Error>> {
    let snapshot = query.get_plugin(plugin_id).await?;
    if let Some(p) = snapshot {
        println!("Plugin: {} v{}", p.plugin_id, p.version);
        println!("Status: {}", p.status); // installed / activated / deactivated / error
        println!("WASM: {:?}", p.wasm_path);
    }
    Ok(snapshot)
}
```

`PluginSnapshot` 字段：plugin_id / name / version / status / install_path / wasm_path / plugin_type（如 `wasm`、`rhai`）/ domain_code / application_code / module_code / source_path。

### 三、PluginLifecycleListener 接口

#### 3.1 监听插件生命周期事件

```rust
use cmx_traits::plugin::PluginLifecycleListener;
use async_trait::async_trait;

struct PluginEventHandler;

#[async_trait]
impl PluginLifecycleListener for PluginEventHandler {
    async fn on_plugin_installed(&self, event: cmx_traits::plugin::PluginLifecyclePayload) {
        tracing::info!("Plugin {} v{} installed", event.plugin_id, event.version);
    }

    async fn on_plugin_upgraded(&self, event: cmx_traits::plugin::PluginLifecyclePayload) {
        tracing::info!(
            "Plugin {} upgraded: {} -> {}",
            event.plugin_id,
            event.old_version.as_deref().unwrap_or("?"),
            event.version
        );
    }

    async fn on_plugin_uninstalled(&self, event: cmx_traits::plugin::PluginLifecyclePayload) {
        tracing::info!("Plugin {} uninstalled", event.plugin_id);
    }

    async fn on_plugin_downgraded(&self, event: cmx_traits::plugin::PluginLifecyclePayload) {
        tracing::info!("Plugin {} downgraded to {}", event.plugin_id, event.version);
    }
}
```

`PluginLifecyclePayload` 字段：app_id / plugin_id / version / old_version（仅升级）/ wasm_path / install_path / timestamp；可用 `new(app_id, plugin_id, version)` 构造并以 `with_old_version` / `with_wasm_path` / `with_install_path` 链式补充。

`plugin_events` 模块定义事件主题常量：`INSTALLED` / `UPGRADED` / `UNINSTALLED` / `DOWNGRADED` / `REINSTALLED` / `LOADED` / `UNLOADED`（形如 `plugin.installed`），监听器通常由装配层订阅事件总线后分发。

### 四、ServiceQuery / ServiceStorage / ServiceInvoker 接口

#### 4.1 查询服务定义

```rust
use cmx_traits::service::{ServiceQuery, ServicePageFilter};

async fn find_service(
    query: &dyn ServiceQuery,
    service_key: &str,
) -> Result<Option<cmx_core::ServiceDefinition>, Box<dyn std::error::Error>> {
    Ok(query.get_service(service_key).await?)
}

async fn page_services(
    query: &dyn ServiceQuery,
) -> Result<cmx_traits::service::ServicePageResult, Box<dyn std::error::Error>> {
    let filter = ServicePageFilter {
        keyword: Some("order".to_string()), // service_key/service_name 模糊匹配
        ..Default::default()
    };
    let page = query.page_services(filter, 1, 20).await?;
    Ok(page) // { items: Vec<ServiceDefinition>, total: u64 }
}
```

另有 `get_services_by_plugin(plugin_id)`、`list_active_services()`、`get_orchestration(service_key)`（取编排定义）。

#### 4.2 存储服务定义

```rust
use cmx_traits::service::ServiceStorage;

async fn save_service_definition(
    storage: &dyn ServiceStorage,
    definition: &cmx_core::ServiceDefinition,
) -> Result<(), Box<dyn std::error::Error>> {
    // 第二个参数为事务 ID（可选），支持与外层事务合并提交
    storage.save_service(definition, None).await?;
    Ok(())
}
```

`ServiceStorage` 还提供 `save_service_version(SaveServiceVersionParams)`（版本留档）、`delete_service(service_key, txn_id)`、`delete_services_by_plugin(plugin_id)`、`get_service_config(...)`。

#### 4.3 执行服务编排

```rust
use cmx_traits::service::{ServiceInvoker, ServiceInvokeOptions};

async fn run_service(
    invoker: &dyn ServiceInvoker,
) -> Result<cmx_core::CallServiceResponse, Box<dyn std::error::Error>> {
    let options = ServiceInvokeOptions {
        include_steps: true,
        debug: false,
        debug_node_id: None,
        debug_params: None,
    };
    Ok(invoker.invoke_service("order-service", serde_json::json!({"user_id": 1}), options).await?)
}
```

### 五、EventBus 事件总线

#### 5.1 发布事件

```rust
use cmx_traits::event_bus::GlobalEventBus;
use cmx_traits::plugin::plugin_events;

fn main() {
    // 发布插件安装事件（publish 异步派发给订阅者；publish_sync 逐个同步执行）
    GlobalEventBus::initialize().ok();
    GlobalEventBus::get();
}
```

```rust
use cmx_traits::event_bus::GlobalEventBus;
use cmx_traits::plugin::plugin_events;

async fn publish_installed() {
    let payload = serde_json::json!({
        "plugin_id": "my-plugin",
        "version": "1.0.0",
        "install_path": "/plugins/my-plugin/1.0.0"
    });

    GlobalEventBus::get()
        .publish(plugin_events::INSTALLED, payload)
        .await;
}
```

#### 5.2 订阅事件

handler 为同步闭包类型 `EventHandler = Arc<dyn Fn(EventTopic, EventPayload) + Send + Sync>`；不支持通配符主题。

```rust
use cmx_traits::event_bus::{GlobalEventBus, EventHandler};
use cmx_traits::plugin::plugin_events;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let handler: EventHandler = Arc::new(|topic, payload| {
        tracing::info!("Received event: {} with payload: {}", topic, payload);
    });

    // 订阅特定主题
    GlobalEventBus::get()
        .subscribe(plugin_events::INSTALLED, handler)
        .await;

    // 观测接口：subscriber_count / topics
    let count = GlobalEventBus::get()
        .subscriber_count(plugin_events::INSTALLED)
        .await;
}
```

#### 5.3 取消订阅

```rust
use cmx_traits::event_bus::GlobalEventBus;
use cmx_traits::plugin::plugin_events;

#[tokio::main]
async fn main() {
    // 按主题移除全部订阅者（无单个 handler 级别的退订）
    GlobalEventBus::get()
        .unsubscribe_all(plugin_events::INSTALLED)
        .await;
}
```

### 六、全局单例访问

全局单例均为同步方法（OnceLock 实现），`set` 重复调用报错、`get` 返回静态引用。

#### 6.1 全局运行时

```rust
use cmx_traits::runtime::GlobalRuntime;
use std::sync::Arc;

fn use_global_runtime(invoker: Arc<dyn cmx_traits::runtime::RuntimeInvoker>) {
    GlobalRuntime::set(invoker).unwrap(); // 应用启动时装配

    if GlobalRuntime::is_initialized() {
        let runtime = GlobalRuntime::get(); // &'static Arc<dyn RuntimeInvoker>
        let _ = runtime.is_loaded("my-plugin");
    }
}
```

#### 6.2 全局服务调用器与编码器

```rust
use cmx_traits::service::GlobalServiceInvoker;
use cmx_traits::code::GlobalCodeMinter;
use std::sync::Arc;

fn setup(invoker: Arc<dyn cmx_traits::service::ServiceInvoker>,
         minter: Arc<dyn cmx_traits::code::CodeMinter>) {
    GlobalServiceInvoker::set(invoker).unwrap();
    GlobalCodeMinter::set(minter).ok();

    let invoker = GlobalServiceInvoker::get(); // &'static Arc<dyn ServiceInvoker>
    let minter = GlobalCodeMinter::get();      // Option<&'static Arc<dyn CodeMinter>>
}
```

### 七、HostFunctionProvider 宿主函数注册

各模块实现 `HostFunctionProvider` 向 cmx-runtime 注册供 WASM 插件调用的宿主函数：

```rust
use cmx_traits::runtime::{HostFunctionDef, HostFunctionProvider};

struct DatabaseHostFunctions;

impl HostFunctionProvider for DatabaseHostFunctions {
    fn namespace(&self) -> &str { "cmx:database" } // 建议格式 cmx:模块名

    fn functions(&self) -> Vec<HostFunctionDef> {
        // msgpack_fn(函数名, 命名空间)；另有 void_fn / no_input / no_output 组合
        vec![HostFunctionDef::msgpack_fn("query", "cmx:database")]
    }

    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, cmx_traits::HostFuncError> {
        // MsgPack 编解码并执行实际业务
        Ok(Vec::new())
    }
}
```

### 八、错误处理

`TraitError` 覆盖跨模块交互的主要错误场景：

```rust
use cmx_traits::error::TraitError;

match err {
    TraitError::PluginNotFound(id) => { /* 插件未安装 */ }
    TraitError::PluginNotActive(id) => { /* 插件未激活 */ }
    TraitError::WasmLoadFailed(msg) | TraitError::WasmInvokeFailed(msg) | TraitError::WasmNotLoaded(msg) => {
        /* WASM 加载/调用失败 */
    }
    TraitError::OrchestrationFailed(msg) => { /* 编排执行失败 */ }
    TraitError::NotFound(msg) | TraitError::Internal(msg) | TraitError::Business(msg)
    | TraitError::Forbidden(msg) | TraitError::AlreadyInitialized(msg) => { /* 通用错误 */ }
    TraitError::RemoteCenter(msg) | TraitError::Rpc(msg) => { /* 远程中心 / RPC 错误 */ }
}
```

`HostFuncError` 用于宿主函数调用错误，变体为结构体形式：`RegistrationFailed { namespace, name, reason }`、`ExecutionFailed { namespace, name, reason }`、`MemoryOutOfBounds { offset, len }`、`InvalidParam(String)`。RPC 相关错误见 `cmx_traits::rpc::RpcError`（ServiceNotFound / NoAvailableInstance / Timeout 等）。
