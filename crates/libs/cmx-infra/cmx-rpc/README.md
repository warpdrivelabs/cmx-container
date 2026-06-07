# cmx-rpc

> 基于 volo-grpc 的企业级 RPC 框架核心库，提供服务发现、负载均衡、gRPC 客户端/服务端封装，业务层只需关注服务名、方法标识和参数，所有服务发现与负载均衡由框架内部处理。

[![Version](https://img.shields.io/badge/version-0.1.8-blue.svg)]
[![License](https://img.shields.io/badge/license-MIT-green.svg)]

## 快速开始

### 安装

```toml
[dependencies]
cmx-rpc = { workspace = true }
```

### 核心示例

```rust
use cmx_rpc::{create_rpc_client, GlobalRpcClient, start_grpc_server, RpcConfig};
use cmx_registry_config::registry::ServiceInstanceCache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = Arc::new(ServiceInstanceCache::new());
    let registry = /* 获取注册中心实例 */;
    let config = RpcConfig::default();

    // 创建 RPC 客户端并注册全局单例
    let client = create_rpc_client(&config, cache, registry)?;
    GlobalRpcClient::set(client)?;

    // 启动 gRPC 服务端
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        start_grpc_server(9090, service_invoker, runtime_invoker, ready_tx).await
    });
    // 等待服务就绪
    ready_rx.await?;

    Ok(())
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| gRPC 客户端 | 基于 volo-grpc 封装，支持超时、重试、连接池 |
| 服务发现 | 桥接注册中心缓存与 volo Discover trait，支持实例变更通知 |
| 负载均衡 | 通过 volo 内置负载均衡器自动选择服务实例 |
| gRPC 服务端 | 封装 CmxServiceOrchestrator 服务实现，一键启动 |
| 全局客户端管理 | OnceLock 实现的全局单例，任意位置访问 |
| 工厂模式 | 根据协议类型创建客户端，支持扩展新协议 |
| 超时与重试 | 基于 volo rpc_timeout/connect_timeout + 指数退避重试 |
| 结构化日志 | tracing + #[instrument] 全链路追踪 |

## 模块结构

```
cmx-rpc
├── client           # gRPC 客户端实现（VoloGrpcClient）
├── config           # 配置定义（RpcConfig, GrpcConfig, HttpRestConfig）
├── discover         # 注册中心感知的服务发现（RegistryAwareDiscover）
├── error            # 错误类型定义（RpcFrameworkError）
├── factory          # RPC 客户端工厂（create_rpc_client）
├── global           # 全局 RPC 客户端管理（GlobalRpcClient）
├── server           # gRPC 服务端实现（CmxOrchestratorServiceImpl）
└── server_runner    # gRPC 服务启动器（start_grpc_server）
```

### 主要模块说明

#### `client`

VoloGrpcClient 是核心客户端，实现了 `RpcClient` trait，提供两个核心调用方法：
- `call_service` — 执行服务编排
- `call_function` — 调用插件函数

客户端内部通过 `RegistryAwareDiscover` 实现服务发现，自动选择可用实例。采用 double-check locking 防止并发重复创建客户端，支持指数退避重试（仅对可重试错误重试）。

#### `discover`

RegistryAwareDiscover 桥接 `ServiceInstanceCache`（注册中心缓存）与 volo 的 `Discover` trait。通过 `async-broadcast` 通道实现实例变更通知，驱动 volo 负载均衡器更新。支持实例的 added/updated/removed 精确 diff 通知。

#### `server`

CmxOrchestratorServiceImpl 实现了 gRPC 生成的 `CmxServiceOrchestrator` trait，桥接 `ServiceInvoker` 和 `RuntimeInvoker`，将业务逻辑暴露为 gRPC 服务。业务错误封装在响应体中，不返回 gRPC Status 错误。

#### `global`

GlobalRpcClient 使用 `OnceLock` 实现全局单例，提供 `set`/`get`/`is_initialized` 三个方法。`set` 重复调用返回 `GlobalRpcClientAlreadySetError`。

## 使用指南

### 一、配置与初始化

#### 1.1 配置 RPC

```rust
use cmx_rpc::RpcConfig;

// 通过配置文件反序列化（推荐）
let config: RpcConfig = ConfigManager::global().get_as("rpc")?;

// 或手动构建
use cmx_rpc::{RpcConfig, GrpcConfig, HttpRestConfig};

let config = RpcConfig {
    enabled: true,
    protocol: "grpc".to_string(),
    grpc: GrpcConfig {
        port: 9090,
        timeout_ms: 5000,              // RPC 调用超时（毫秒）
        connect_timeout_ms: 3000,      // 连接超时（毫秒）
        retry_count: 0,                // 重试次数
        default_group: None,           // 默认服务分组
        default_clusters: vec![],      // 默认集群列表
    },
    http_rest: HttpRestConfig::default(),
    warmup_services: vec!["user-service".to_string()],
    service_sync_interval_secs: 30,
};
```

**GrpcConfig 字段说明：**

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `port` | `u16` | — | gRPC 服务监听端口 |
| `timeout_ms` | `u64` | 5000 | RPC 调用超时时间（毫秒），通过 volo rpc_timeout 设置 |
| `connect_timeout_ms` | `u64` | 3000 | 连接超时时间（毫秒），通过 volo connect_timeout 设置 |
| `retry_count` | `usize` | 0 | 重试次数（仅对可重试错误重试：UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED） |
| `default_group` | `Option<String>` | None | 默认服务分组（用于 query_instances 过滤） |
| `default_clusters` | `Vec<String>` | [] | 默认集群列表（用于 query_instances 过滤） |

#### 1.2 创建 RPC 客户端

```rust
use cmx_rpc::{create_rpc_client, RpcConfig};
use cmx_registry_config::registry::ServiceInstanceCache;
use std::sync::Arc;

let config = RpcConfig::default();
let cache = Arc::new(ServiceInstanceCache::new());
let registry = /* Arc<dyn ServiceRegistry> */;

// 根据协议创建客户端（目前仅支持 grpc）
let client = create_rpc_client(&config, cache, registry)?;
```

#### 1.3 设置全局客户端

```rust
use cmx_rpc::{GlobalRpcClient, GlobalRpcClientAlreadySetError};
use std::sync::Arc;

// 设置全局客户端（只能调用一次，重复调用返回 GlobalRpcClientAlreadySetError）
match GlobalRpcClient::set(client) {
    Ok(()) => println!("全局 RPC 客户端设置成功"),
    Err(GlobalRpcClientAlreadySetError) => println!("全局 RPC 客户端已初始化"),
}

// 在任意位置获取全局客户端
let client = GlobalRpcClient::get();

// 检查是否已初始化
if GlobalRpcClient::is_initialized() {
    println!("RPC 客户端已就绪");
}
```

### 二、服务调用

#### 2.1 调用服务编排

```rust
use cmx_rpc::GlobalRpcClient;
use cmx_traits::{RpcClient, ServiceInvokeOptions};
use serde_json::json;

let client = GlobalRpcClient::get();

// 调用远程服务编排
let response = client
    .call_service(
        "target-service",                   // 目标服务名
        "service-key-123",                  // 服务标识
        json!({"key": "value"}),            // 输入参数（serde_json::Value）
        ServiceInvokeOptions::default(),
    )
    .await?;

if response.success {
    println!("执行成功: {:?}", response.output);
    for step in &response.steps {
        println!("步骤 {} ({}) - 状态: {:?}", step.node_name, step.node_type, step.status);
    }
}
```

#### 2.2 调用插件函数

```rust
use cmx_rpc::GlobalRpcClient;
use cmx_traits::RpcClient;
use serde_json::json;

let client = GlobalRpcClient::get();

// 调用远程插件函数
let result = client
    .call_function(
        "target-service",                   // 目标服务名
        "plugin-id-456",                    // 插件 ID
        "process_data",                     // 函数名
        json!({"input": "data"}),           // 输入参数（serde_json::Value）
    )
    .await?;

if result.success {
    println!("函数返回: {:?}", result.result);
}
```

### 三、服务端

#### 3.1 启动 gRPC 服务

```rust
use cmx_rpc::start_grpc_server;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service_invoker: Arc<dyn ServiceInvoker> = /* 获取 ServiceInvoker */;
    let runtime_invoker: Arc<dyn RuntimeInvoker> = /* 获取 RuntimeInvoker */;

    // 启动 gRPC 服务，监听 9090 端口
    // ready_tx 用于通知调用方服务已就绪
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        start_grpc_server(9090, service_invoker, runtime_invoker, ready_tx).await
    });

    // 等待服务就绪（建议加超时）
    let _ = ready_rx.await;

    Ok(())
}
```

#### 3.2 自定义服务实现

```rust
use cmx_rpc::CmxOrchestratorServiceImpl;
use std::sync::Arc;

// 创建服务实现，注入 ServiceInvoker 和 RuntimeInvoker
let service = CmxOrchestratorServiceImpl::new(
    service_invoker,
    runtime_invoker,
);

// 服务会自动处理 gRPC 请求：
// - ExecuteService → 调用 ServiceInvoker::invoke_service
// - CallFunction → 调用 RuntimeInvoker::invoke
```

### 四、服务发现

#### 4.1 创建服务发现器

```rust
use cmx_rpc::RegistryAwareDiscover;
use cmx_registry_config::registry::ServiceInstanceCache;
use std::sync::Arc;

let cache = Arc::new(ServiceInstanceCache::new());

// 创建发现器
let discover = RegistryAwareDiscover::new(cache.clone());

// 启动对特定服务的实例变更监听
discover.start_watch("user-service");
```

#### 4.2 服务发现工作原理

```rust
// RegistryAwareDiscover 实现了 volo 的 Discover trait：
// 1. discover() — 从 ServiceInstanceCache 获取服务实例列表
// 2. watch() — 返回变更接收端，volo 负载均衡器监听此通道
// 3. start_watch() — 注册回调到缓存，实例变更时通过 broadcast 通道通知
//
// 实例变更 diff 机制：
// - added: 新增的实例（地址不在旧列表中）
// - removed: 移除的实例（地址不在新列表中）
// - updated: 地址相同但 weight/tags 变化的实例
//
// 数据流：
// ServiceInstanceCache → callback → diff → async_broadcast → volo LoadBalancer → 更新连接池
```

### 五、重试机制

#### 5.1 可重试错误

客户端仅对以下 gRPC 错误码进行重试：

| 错误码 | 说明 |
|--------|------|
| `UNAVAILABLE` | 服务不可达 |
| `DEADLINE_EXCEEDED` | 超时 |
| `RESOURCE_EXHAUSTED` | 限流场景，重试可能成功 |
| `ABORTED` | 事务中止，可重试 |

业务错误（INVALID_ARGUMENT、NOT_FOUND、PERMISSION_DENIED 等）不会重试。

#### 5.2 指数退避

```rust
// 重试退避序列：50ms → 100ms → 200ms → 400ms → 800ms（上限）
// 退避时间不超过剩余时间预算
// 总时间预算 = timeout_ms 配置值
//
// 示例：retry_count=3, timeout_ms=5000
// 第1次调用失败 → 等待 50ms → 第2次调用失败 → 等待 100ms → 第3次调用失败 → 等待 200ms → 第4次调用
// 如果总耗时超过 5000ms，直接返回 Timeout 错误
```

### 六、错误处理

#### 6.1 框架错误类型

```rust
use cmx_rpc::RpcFrameworkError;

// RpcFrameworkError 包含三种变体：
match error {
    RpcFrameworkError::ServerStartFailed(msg) => {
        // gRPC 服务启动失败
        eprintln!("服务启动失败: {}", msg);
    }
    RpcFrameworkError::RegistryNotInitialized => {
        // 注册中心未初始化
        eprintln!("请先初始化注册中心");
    }
    RpcFrameworkError::DiscoveryFailed(msg) => {
        // 服务发现失败
        eprintln!("服务发现失败: {}", msg);
    }
}
```

#### 6.2 调用错误处理

```rust
use cmx_traits::RpcError;

match client.call_service("svc", "key", json!({}), options).await {
    Ok(response) => {
        if response.success {
            println!("成功: {:?}", response.output);
        } else {
            // 业务层错误，封装在响应体中
            if let Some(error) = response.error {
                println!("业务错误: {}", error.message);
            }
        }
    }
    Err(RpcError::Timeout(msg)) => {
        eprintln!("调用超时: {}", msg);
    }
    Err(RpcError::NoAvailableInstance(msg)) => {
        eprintln!("无可用实例: {}", msg);
    }
    Err(RpcError::RpcCallFailed(msg)) => {
        eprintln!("RPC 调用失败: {}", msg);
    }
    Err(RpcError::UnsupportedProtocol(msg)) => {
        eprintln!("不支持的协议: {}", msg);
    }
    Err(e) => {
        eprintln!("其他错误: {}", e);
    }
}
```

### 七、完整集成示例

```rust
use cmx_rpc::{create_rpc_client, GlobalRpcClient, start_grpc_server, RpcConfig};
use cmx_registry_config::registry::ServiceInstanceCache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化注册中心缓存
    let cache = Arc::new(ServiceInstanceCache::new());
    let registry = /* 初始化注册中心 */;

    // 2. 创建并注册全局 RPC 客户端
    let config = RpcConfig::default();
    let client = create_rpc_client(&config, cache.clone(), registry)?;
    GlobalRpcClient::set(client)?;

    // 3. 启动 gRPC 服务端
    let service_invoker = /* 获取 ServiceInvoker */;
    let runtime_invoker = /* 获取 RuntimeInvoker */;

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let port = config.grpc.port;
    tokio::spawn(async move {
        if let Err(e) = start_grpc_server(port, service_invoker, runtime_invoker, ready_tx).await {
            eprintln!("gRPC Server 运行失败: {}", e);
        }
    });

    // 等待服务就绪
    tokio::time::timeout(std::time::Duration::from_secs(3), ready_rx).await??;

    // 4. 缓存预热
    for service_name in &config.warmup_services {
        let instances = registry.query_instances(
            service_name,
            config.grpc.default_group.as_deref(),
            config.grpc.default_clusters.clone(),
        ).await?;
        if !instances.is_empty() {
            cache.update(service_name, instances);
        }
    }

    Ok(())
}
```

## 公共 API 速览

| 类型 | 模块 | 说明 |
|------|------|------|
| `VoloGrpcClient` | client | gRPC 客户端，实现 `RpcClient` trait |
| `RpcConfig` | config | RPC 总配置 |
| `GrpcConfig` | config | gRPC 配置 |
| `HttpRestConfig` | config | HTTP REST 配置（预留） |
| `RegistryAwareDiscover` | discover | 注册中心感知的服务发现 |
| `RpcFrameworkError` | error | 框架层错误 |
| `create_rpc_client` | factory | 客户端工厂函数 |
| `GlobalRpcClient` | global | 全局客户端单例 |
| `GlobalRpcClientAlreadySetError` | global | 重复初始化错误 |
| `CmxOrchestratorServiceImpl` | server | gRPC 服务端实现 |
| `start_grpc_server` | server_runner | gRPC 服务启动函数 |

## 常见问题

### Q: 支持哪些 RPC 协议？

**A**: 目前仅支持 gRPC 协议（基于 volo-grpc）。HTTP REST 协议为预留接口，尚未实现。通过 `create_rpc_client` 工厂函数根据 `RpcConfig.protocol` 字段选择协议。

### Q: 服务发现如何工作？

**A**: `RegistryAwareDiscover` 桥接 `ServiceInstanceCache`（由 `cmx-registry-config` 维护的注册中心缓存）与 volo 的 `Discover` trait。当服务实例变更时，通过 `async-broadcast` 通道通知 volo 负载均衡器更新连接池。支持 added/updated/removed 精确 diff。

### Q: 调用超时如何配置？

**A**: 在 `GrpcConfig` 中设置 `timeout_ms`（默认 5000ms）和 `connect_timeout_ms`（默认 3000ms）。`timeout_ms` 通过 volo 的 `rpc_timeout` 设置，`connect_timeout_ms` 通过 volo 的 `connect_timeout` 设置。重试时总时间不超过 `timeout_ms`。

### Q: 重试机制如何工作？

**A**: 通过 `GrpcConfig.retry_count` 设置重试次数（默认 0 不重试）。仅对可重试错误（UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED）重试，采用指数退避（50ms→100ms→200ms→400ms→800ms），总时间不超过 `timeout_ms` 预算。

### Q: 全局客户端是否线程安全？

**A**: 是。`GlobalRpcClient` 内部使用 `std::sync::OnceLock`，保证只初始化一次且线程安全。`get()` 返回 `&'static Arc<dyn RpcClient>`，可在任意线程安全访问。重复调用 `set()` 返回 `GlobalRpcClientAlreadySetError`。

### Q: 服务端业务错误如何返回？

**A**: 服务端不返回 gRPC Status 错误，而是将业务错误包装在响应体的 `error` 字段中（如 `ExecuteServiceResponse.error`）。这确保业务层错误不会中断 gRPC 连接。仅输入参数格式错误（如 JSON 解析失败）返回 gRPC INVALID_ARGUMENT Status。

### Q: 客户端缓存如何管理？

**A**: `VoloGrpcClient` 内部使用 `RwLock<HashMap>` 缓存每个服务的 gRPC 客户端实例，采用 double-check locking 防止并发重复创建。当缓存中没有目标服务实例时，会主动通过 `ServiceRegistry.query_instances` 拉取并缓存。
