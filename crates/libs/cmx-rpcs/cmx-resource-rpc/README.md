# cmx-resource-rpc

> 资源数据管理域的 **gRPC 皮肤**（thin crate）：基于 volo-grpc 提供 `import_resource_data` / `cleanup_resource_data` / `list_resource_data` 的客户端访问器、服务端实现与装配 Bundle 三件套，承担插件/模块资源（menu/perm/form/flow 四类 ZIP 包）的跨服务导入。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-resource-rpc` 位于 `crates/libs/cmx-rpcs/` 归域目录下（与 `cmx-apis/` HTTP 皮肤对称的 gRPC 皮肤集中地）。它实现 `cmx_traits::rpc::ResourceDataClient` trait 的 volo-grpc 版本，服务端把 gRPC 请求桥接到 `cmx_traits::resource::ResourceDataImporter` trait——**不依赖业务 service crate**，具体导入器（各服务的菜单/权限/表单/流程资源落库实现）由组装层经 `cmx_service_rpc::grpc::bundle::ServerDeps.data_importer` 注入。

典型链路：插件安装任务把资源打成 ZIP（如 `perm` 权限包），经本 crate 的 `import_resource_data` 发往目标服务（按 `service_name` 经注册中心发现实例），目标服务的 `ResourceDataImporter` 按 upsert 语义导入并返回 created/updated/deleted 计数。

**重试策略是本 crate 最重要的设计决策**：`import_resource_data` / `cleanup_resource_data` **不走 `cmx_service_rpc::grpc::with_retry`**（与 orchestrator-rpc 相反）。源码注释详述了三点理由：

1. 传输 ZIP 二进制大包（默认上限 4MB），重试需保证下游导入幂等；
2. 大包重试放大带宽与下游负载，4MB 上限下网络抖动概率高，盲目重试易雪崩；
3. import 由插件安装流程驱动，失败可由上层重试整个安装任务。

路线：未来若引入幂等 token + 分片上传，可启用有限重试。`list_resource_data` 为轻量查询，同样直连不重试。

错误映射上，`status_to_rpc_error` 保留认证/权限类别不坍缩：`Unauthenticated` → `RpcError::Unauthenticated`、`PermissionDenied` → `RpcError::PermissionDenied`、其余 → `RpcError::RpcCallFailed`——调用方（如插件安装任务）可据此区分「凭证问题」与「传输故障」。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-service-rpc`（feature `grpc-server`） | 共享 RPC 基础设施（src/grpc/）：`GrpcInfrastructure`、`apply_auth_metadata`、`AuthVerifier` / `verify_request` / `VerifiedAuth`、`bundle::{RpcServiceBundle, ServerDeps, ServerRegistration}` |
| `cmx-rpc-gen` | proto 契约 `resource_data_proto`（`CmxResourceDataServiceClient/Server`、`ImportResourceDataRequest`、`CleanupResourceDataRequest`、`ListResourceDataRequest` 等） |
| `cmx-traits` | trait 抽象层：`ResourceDataClient`、`ResourceDataImporter`、`ResourceDataImportRequest/CleanupRequest/ImportResult/ListResult`、`ResourceDataCategory`、`RpcError` |
| `volo-grpc` | gRPC 框架（客户端 Builder / 服务端 ServiceBuilder / Status） |
| `tokio` / `async-trait` / `tracing` | 运行时 / trait 异步 / 结构化日志 |

注意依赖刻意精简：无 serde_json / chrono / uuid / pilota / cmx-core（资源域无编排语义，纯数据搬运）。

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | workspace 依赖 | 组装层注册 `ResourceDataBundle` 进 `init_rpc`（与 `OrchestratorBundle` 并列），注入平台资源数据导入器 |
| `cmx-plugin` | workspace 依赖 | `service/remote_importers.rs` 的 `send_via_grpc`：`resource_data_client().import_resource_data(&service_name, request)` 发包，及 `list_resource_data` 查询远端资源清单 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 导入 RPC | `import_resource_data(service_name, request)`：ZIP 字节包发往目标服务按 upsert 导入，返回 `{ success, message, created_count, updated_count, deleted_count }` |
| 清理 RPC | `cleanup_resource_data(service_name, request)`：按类别+域/应用/模块（+插件三元组）删除远端资源 |
| 查询 RPC | `list_resource_data(service_name, request)`：远端资源清单以 JSON 字节（`json_data: Vec<u8>`）返回 |
| 服务端参数校验 | category 经 `ResourceDataCategory::parse_from_str`（有效值 menu/perm/form/flow）；`domain_code/application_code/module_code` 所有类别必填；**Perm 类别**还需 `plugin_id/app_id/version` 非空；list 需 `module_code` 非空 |
| 缺依赖降级 | `data_importer` 未配置（`CmxResourceDataServerImpl::new(None)`）时返回 `success=false` 响应而非 panic——适合纯编排节点 |
| 不重试设计 | import/cleanup 不走 `with_retry`（见简介），失败立即返回；list 同样直连 |
| 错误类别保留 | `status_to_rpc_error`：Unauthenticated / PermissionDenied 各自映射，不坍缩为 RpcCallFailed |
| 委托链透传 | 服务端 `scope_full(auth_ctx, user_token, request_id, None, ...)` 建 task_local scope，importer 内部可 `current_auth()` 取调用者身份 |
| 客户端缓存 | `service_name → client` RwLock 缓存 + double-check locking（同 orchestrator-rpc 模式） |

---

## 模块结构

```text
cmx-resource-rpc
├── src
│   ├── lib.rs     # 模块声明与三件套导出
│   ├── client.rs  # ResourceDataGrpcClient（ResourceDataClient impl）+ resource_data_client() 访问器 + status_to_rpc_error + ResourceDataBundle（含重试策略文档）
│   └── server.rs  # CmxResourceDataServerImpl（CmxResourceDataService impl）：鉴权 + scope_full + 参数校验 + 桥接 ResourceDataImporter
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/client.rs —— 领域全局访问器（OnceLock；未初始化 panic，先用 is_initialized 守卫）
pub fn resource_data_client() -> &'static Arc<dyn ResourceDataClient>;

pub struct ResourceDataGrpcClient { /* infra + clients: RwLock<HashMap<service_name, CmxResourceDataServiceClient>> */ }
impl ResourceDataGrpcClient {
    pub fn new(infra: Arc<GrpcInfrastructure>) -> Self;
}

#[async_trait]
impl cmx_traits::rpc::ResourceDataClient for ResourceDataGrpcClient {
    async fn import_resource_data(
        &self, service_name: &str, request: ResourceDataImportRequest,
    ) -> Result<ResourceDataImportResult, RpcError>;

    async fn cleanup_resource_data(
        &self, service_name: &str, request: ResourceDataCleanupRequest,
    ) -> Result<ResourceDataImportResult, RpcError>;

    async fn list_resource_data(
        &self, service_name: &str, request: ResourceDataImportRequest,
    ) -> Result<ResourceDataListResult, RpcError>;
}
// 注：ResourceDataImportRequest 字段 = category + domain_code/application_code/module_code
//     + plugin_id/app_id/version + zip_data: Vec<u8>

// src/client.rs —— 装配 Bundle
pub struct ResourceDataBundle;
impl RpcServiceBundle for ResourceDataBundle {
    fn name(&self) -> &'static str;                                   // "resource_data"
    fn init_client(&self, infra: Arc<GrpcInfrastructure>);
    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration;
}

// src/server.rs —— 服务端实现（data_importer 可选：未配置返回 success=false）
pub struct CmxResourceDataServerImpl { /* data_importer: Option<Arc<dyn ResourceDataImporter>> + auth_verifier */ }
impl CmxResourceDataServerImpl {
    pub fn new(data_importer: Option<Arc<dyn ResourceDataImporter>>) -> Self;
    pub fn with_auth_verifier(mut self, verifier: AuthVerifier) -> Self;
}
// impl resource_data_proto::CmxResourceDataService：三个 RPC 方法
//   import_resource_data / cleanup_resource_data / list_resource_data
```

---

## 使用示例

### 场景一：插件安装经 gRPC 推送权限资源（真实用法，参考 `cmx-plugin` 的 `send_via_grpc`）

```rust
use cmx_traits::resource::{ResourceDataCategory, ResourceDataImportRequest};

// 守卫 + 领域全局访问器（cmx-plugin remote_importers 的真实模式）
if cmx_service_rpc::grpc::GlobalRpcClient::is_initialized() {
    let client = cmx_resource_rpc::resource_data_client();
    let request = ResourceDataImportRequest {
        category: ResourceDataCategory::Perm,       // 插件权限包
        domain_code: "fi".into(),
        application_code: "cmxfico".into(),
        module_code: "gl".into(),
        plugin_id: "fi.gl.voucher".into(),
        app_id: "portal".into(),
        version: "1.0.0".into(),
        zip_data: zip_bytes,                        // ZIP 二进制（≤4MB）
    };
    // 发往目标服务（service_name 经注册中心发现实例）；不重试，失败由上层安装任务重试
    let result = client.import_resource_data(&service_name, request).await?;
    println!("导入完成：+{} ~{} -{}", result.created_count, result.updated_count, result.deleted_count);
}
```

### 场景二：查询远端资源清单（list）

```rust
let list_req = ResourceDataImportRequest {
    category: ResourceDataCategory::Menu,           // 查远端菜单资源
    domain_code: "fi".into(),
    application_code: "cmxfico".into(),
    module_code: "gl".into(),
    plugin_id: String::new(),                       // list 不需要插件三元组
    app_id: String::new(),
    version: String::new(),
    zip_data: Vec::new(),
};

let result = cmx_resource_rpc::resource_data_client()
    .list_resource_data(&service_name, list_req)
    .await?;

if result.success {
    // json_data 为远端序列化的资源清单 JSON 字节
    let items: serde_json::Value = serde_json::from_slice(&result.json_data)?;
    println!("远端菜单 {} 条", items.as_array().map(|a| a.len()).unwrap_or(0));
}
```

### 场景三：错误类别的差异化处理

```rust
// status_to_rpc_error 保留类别，调用方可区分凭证与传输故障：
match cmx_resource_rpc::resource_data_client()
    .import_resource_data(&service_name, request)
    .await
{
    Ok(r) if r.success => { /* 落库计数 */ }
    Ok(r) => { /* 业务失败：r.message 有原因（如"Perm 类别导入需要 plugin_id/app_id/version 非空"） */ }
    Err(cmx_traits::rpc::RpcError::Unauthenticated(msg)) => {
        // 凭证问题：提示检查 [service_auth] 出站凭证，重试无意义
    }
    Err(cmx_traits::rpc::RpcError::PermissionDenied(msg)) => {
        // 权限不足：目标服务拒绝了本服务身份
    }
    Err(cmx_traits::rpc::RpcError::RpcCallFailed(msg)) => {
        // 传输/服务故障：可由上层插件安装任务稍后整体重试
    }
    Err(_) => {}
}
```

---

## Features

无 `[features]`，本 crate 为薄皮肤，不含可选编译特性。新增一个 gRPC 服务的标准步骤见 `cmx-service-rpc/README.md`。
