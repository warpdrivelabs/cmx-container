# cmx-orchestrator-rpc

> 服务编排域的 **gRPC 皮肤**（thin crate）：基于 volo-grpc 提供 `call_service` / `call_function` 的客户端访问器、服务端实现与装配 Bundle 三件套，业务实现经 `ServerDeps` 由组装层注入。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-orchestrator-rpc` 位于 `crates/libs/cmx-rpcs/` 归域目录下——该目录与 `cmx-apis/`（HTTP 皮肤）对称，集中收录各业务域的 **gRPC 皮肤** crate。皮肤 crate 只做「协议适配 + 装配」，不含业务逻辑：本 crate 实现 `cmx_traits::rpc::ServiceOrchestrationClient` trait 的 volo-grpc 版本，服务端则把 gRPC 请求桥接到 `ServiceInvoker`（编排执行）与 `FunctionInvoker`（插件函数调用）两个 trait——**不依赖任何业务 service crate**，具体实现由组装层（如 cmx-platform-app）通过 `cmx_service_rpc::grpc::bundle::ServerDeps` 注入。

三件套结构：

- **client 访问器** `orchestrator_client()`：`OnceLock` 领域全局，返回 `&'static Arc<dyn ServiceOrchestrationClient>`；未初始化时 panic（须先经 `cmx_service_rpc::grpc::init_rpc_clients` 初始化，调用前用 `cmx_service_rpc::grpc::GlobalRpcClient::is_initialized` 守卫）。
- **服务端实现** `CmxOrchestratorServerImpl`：实现 proto 生成的 `CmxServiceOrchestrator` trait，两个 RPC 方法 `execute_service` / `call_function`。
- **装配 Bundle** `OrchestratorBundle`：实现 `cmx_service_rpc::grpc::bundle::RpcServiceBundle`（`init_client` + `build_server`），由组装层显式注册进 `init_rpc`。

服务端收到请求后经 `cmx_traits::auth::context_scope::scope_full` 建立 task_local scope，把鉴权上下文、委托用户 token、request_id 透传给业务层——使**链式跨服务调用可继续 on-behalf-of**（A 经平台调 B，B 内部再调 C 时 C 仍能拿到真实用户身份）。业务失败走 `Ok(success=false)` 响应而非 gRPC Status 错误（客户端拿到结构化错误信息，而非传输层异常）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-service-rpc`（feature `grpc-server`） | 共享 RPC 基础设施（src/grpc/）：`GrpcInfrastructure`（服务发现/超时/重试）、`with_retry`（带 RetryStats）、`apply_auth_metadata`、`safe_parse_json`、`AuthVerifier` / `verify_request` / `VerifiedAuth`、`bundle::{RpcServiceBundle, ServerDeps, ServerRegistration}` |
| `cmx-rpc-gen` | proto 契约 `orchestrator_proto`（`CmxServiceOrchestratorClient/Server`、`ExecuteServiceRequest/Response`、`CallFunctionRequest/Response` 等） |
| `cmx-traits` | trait 抽象层：`ServiceOrchestrationClient`、`ServiceInvoker`、`FunctionInvoker`、`ServiceInvokeOptions`、`FunctionCallResult`、`RpcError`、`step_status`、`context_scope` |
| `cmx-core` | 领域模型：`CallServiceResponse`、`ExecutionStep`、`OrchestrationError`、`SVRContext` |
| `volo-grpc` / `pilota` | gRPC 框架（`FastStr` / `AHashMap` 零拷贝字符串） |
| `tokio` / `async-trait` / `serde_json` / `tracing` / `chrono` / `uuid` | 运行时 / trait 异步 / JSON / 日志 / `SVRContext` 时间戳 / rpc 请求 id |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | workspace 依赖 | 组装层注册 `OrchestratorBundle` 进 `init_rpc`（`rpc_bundles` vec，与 `ResourceDataBundle` 并列），注入 `GlobalServiceInvoker` 与 `FunctionInvoker` |
| `cmx-common-api` | workspace 依赖 | `handlers/service/handler.rs` 的 `call_function_via_rpc` / `execute_service_via_rpc`：前置 `cmx_service_rpc::grpc::GlobalRpcClient::is_initialized()` 守卫后调 `orchestrator_client().call_function(...)` |
| `cmx-plugin` | workspace 依赖 | `host_functions.rs` 中插件宿主函数经 `orchestrator_client()` 发起跨服务编排调用 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| `execute_service` RPC | 客户端 `call_service(service_name, service_key, input, options)` → 远端 `ServiceInvoker::invoke_service`；`ServiceInvokeOptions` 透传 `include_steps` / `debug` / `debug_node_id` / `debug_params` |
| `call_function` RPC | 客户端 `call_function(service_name, plugin_id, function_name, input)` → 远端 `FunctionInvoker::invoke_plugin_function`（封装 RuntimeInvoker + PluginQuery 完整调用链） |
| 客户端缓存 | `service_name → client` 的 RwLock 缓存 + double-check locking 防并发重复建连；discover 由 `GrpcInfrastructure` 共享 |
| 超时与重试 | 两个调用均包 `cmx_service_rpc::grpc::with_retry`（timeout_ms / retry_count 取自 infra），成功/失败日志带 `RetryStats`（elapsed_us / attempts） |
| 出站鉴权 | `apply_auth_metadata` 注入 outbound_service_key（`[service_auth]` 服务凭证） |
| 服务端鉴权 | `AuthVerifier` 可选注入（`with_auth_verifier`）；`verify_request` 从 gRPC metadata 验证，产出 `VerifiedAuth` |
| 委托链透传 | `scope_full(auth_ctx, user_token, request_id, None, ...)` 建 task_local scope，链式调用继续 on-behalf-of |
| StepStatus 统一 | proto 字符串 ↔ enum 经 `cmx_traits::step_status::parse_step_status` / `step_status_to_str` 单一来源转换 |
| JSON 安全解析 | proto 字段反序列化经 `safe_parse_json`（坏 JSON 不 panic，记日志返回 Null） |
| 业务失败不抛 Status | invoker Err 时返回 `Ok(success=false)` + `OrchestrationError.message` 响应 |

---

## 模块结构

```text
cmx-orchestrator-rpc
├── src
│   ├── lib.rs     # 模块声明与三件套导出
│   ├── client.rs  # OrchestratorGrpcClient（ServiceOrchestrationClient impl）+ orchestrator_client() 访问器 + OrchestratorBundle + proto→领域转换
│   └── server.rs  # CmxOrchestratorServerImpl（CmxServiceOrchestrator impl）：鉴权 + scope_full + 桥接两个 invoker
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/client.rs —— 领域全局访问器（OnceLock；未初始化 panic，先用 is_initialized 守卫）
pub fn orchestrator_client() -> &'static Arc<dyn ServiceOrchestrationClient>;

pub struct OrchestratorGrpcClient { /* infra + clients: RwLock<HashMap<service_name, CmxServiceOrchestratorClient>> */ }
impl OrchestratorGrpcClient {
    pub fn new(infra: Arc<GrpcInfrastructure>) -> Self;
}

#[async_trait]
impl ServiceOrchestrationClient for OrchestratorGrpcClient {
    async fn call_service(
        &self, service_name: &str, service_key: &str, input: Value,
        options: cmx_traits::service::ServiceInvokeOptions,
    ) -> Result<cmx_core::CallServiceResponse, RpcError>;

    async fn call_function(
        &self, service_name: &str, plugin_id: &str, function_name: &str, input: Value,
    ) -> Result<cmx_traits::rpc::FunctionCallResult, RpcError>;
}

// src/client.rs —— 装配 Bundle（由组装层注册进 init_rpc）
pub struct OrchestratorBundle;
impl RpcServiceBundle for OrchestratorBundle {
    fn name(&self) -> &'static str;                                   // "orchestrator"
    fn init_client(&self, infra: Arc<GrpcInfrastructure>);            // 建 client 填 OnceLock
    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration;  // 用 deps 组装服务端并注册到 volo server
}

// src/server.rs —— 服务端实现
pub struct CmxOrchestratorServerImpl { /* service_invoker + function_invoker + auth_verifier */ }
impl CmxOrchestratorServerImpl {
    pub fn new(
        service_invoker: Arc<dyn ServiceInvoker>,
        function_invoker: Arc<dyn FunctionInvoker>,
    ) -> Self;
    pub fn with_auth_verifier(mut self, verifier: AuthVerifier) -> Self;
}

impl cmx_rpc_gen::orchestrator_proto::CmxServiceOrchestrator for CmxOrchestratorServerImpl {
    fn execute_service(&self, req: volo_grpc::Request<ExecuteServiceRequest>)
        -> impl Future<Output = Result<volo_grpc::Response<ExecuteServiceResponse>, volo_grpc::Status>> + Send;
    fn call_function(&self, req: volo_grpc::Request<CallFunctionRequest>)
        -> impl Future<Output = Result<volo_grpc::Response<CallFunctionResponse>, volo_grpc::Status>> + Send;
}
```

---

## 使用示例

### 场景一：组装层注册 Bundle（真实用法，参考 `cmx-platform-app/src/lib.rs`）

```rust
use cmx_orchestrator_rpc::OrchestratorBundle;

// rpc_bundles vec 与 ResourceDataBundle 并列，一起交给 init_rpc：
let rpc_bundles: Vec<Box<dyn cmx_service_rpc::grpc::bundle::RpcServiceBundle>> =
    vec![Box::new(OrchestratorBundle), Box::new(cmx_resource_rpc::ResourceDataBundle)];

// init_rpc 内部会：遍历 bundles 调 init_client(基建) 填各自 OnceLock；
// build_server(&ServerDeps { service_invoker, function_invoker, auth_verifier, .. })
// 把业务实现注入服务端并挂到 volo gRPC server。
init_rpc(rpc_bundles, global_service_invoker, function_invoker, importer, auth).await?;
```

### 场景二：HTTP handler 转发到 gRPC（真实用法，参考 `cmx-common-api` 的 `call_function_via_rpc`）

```rust
use serde_json::json;

// 守卫：RPC 层未启用（未配注册中心）时优雅降级，而非 panic
if !cmx_service_rpc::grpc::GlobalRpcClient::is_initialized() {
    return Err(bad_request("RPC 未初始化"));
}

// 领域全局访问器 → 按 service_name 经注册中心发现远端实例
let result = cmx_orchestrator_rpc::orchestrator_client()
    .call_function(&server_name, &req.plugin_id, &req.function_name, json!({ "arg": 1 }))
    .await?;

// FunctionCallResult { success, result: Option<Value>, elapsed_us, error }
if result.success {
    println!("插件函数输出: {:?}（{}µs）", result.result, result.elapsed_us);
}
```

### 场景三：带调试选项的服务编排调用

```rust
use cmx_traits::service::ServiceInvokeOptions;
use std::collections::HashMap;

let options = ServiceInvokeOptions {
    include_steps: true,                       // 响应携带逐步 ExecutionStep（node_id/status/耗时）
    debug: true,                               // 开调试模式
    debug_node_id: Some("approve_node_1".into()), // 只调试指定节点
    debug_params: Some(HashMap::from([(
        "mock_user".into(),
        "zhangsan".into(),
    )])),
};

let resp = cmx_orchestrator_rpc::orchestrator_client()
    .call_service("mdm-service", "order_sync", serde_json::json!({ "orderId": 42 }), options)
    .await?;

for step in &resp.steps {
    // StepStatus 已从 proto 字符串统一解析为 enum（cmx-traits 单一来源）
    println!("{} [{}] {}µs", step.node_id, cmx_traits::step_status::step_status_to_str(&step.status), step.elapsed_us);
}
```

---

## Features

无 `[features]`，本 crate 为薄皮肤，不含可选编译特性。新增一个 gRPC 服务的标准步骤见 `cmx-service-rpc/README.md`。
