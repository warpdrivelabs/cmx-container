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
use cmx_registry_config::cache::ServiceInstanceCache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 RPC 客户端并注册全局单例
    let config = RpcConfig::default();
    let cache = Arc::new(ServiceInstanceCache::new());
    let registry = /* 获取注册中心实例 */;

    let client = create_rpc_client(&config, cache, registry)?;
    GlobalRpcClient::set(client)?;

    // 启动 gRPC 服务端
    start_grpc_server(9090, service_invoker, runtime_invoker).await?;
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
| 超时控制 | 基于 tokio::time::timeout 的调用超时 |
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

客户端内部通过 `RegistryAwareDiscover` 实现服务发现，自动选择可用实例。

#### `discover`

RegistryAwareDiscover 桥接 `ServiceInstanceCache`（注册中心缓存）与 volo 的 `Discover` trait。通过 `async-broadcast` 通道实现实例变更通知，驱动 volo 负载均衡器更新。

#### `server`

CmxOrchestratorServiceImpl 实现了 gRPC 生成的 `CmxServiceOrchestrator` trait，桥接 `ServiceInvoker` 和 `RuntimeInvoker`，将业务逻辑暴露为 gRPC 服务。

## 使用指南

### 一、配置与初始化

#### 1.1 配置 RPC

```rust
use cmx_rpc::RpcConfig;

// 通过配置文件反序列化（推荐）
let config: RpcConfig = serde::Deserialize::deserialize(toml_value)?;

// 或手动构建
use cmx_rpc::{RpcConfig, GrpcConfig, HttpRestConfig};

let config = RpcConfig {
    enabled: true,
    protocol: "grpc".to_string(),
    grpc: GrpcConfig {
        port: 9090,
        timeout_ms: 5000,
        retry_count: 0,
        pool_size: 4,
    },
    http_rest: HttpRestConfig::default(),
    warmup_services: vec!["user-service".to_string()],
    service_sync_interval_secs: 30,
};
```

#### 1.2 创建 RPC 客户端

```rust
use cmx_rpc::{create_rpc_client, RpcConfig};
use cmx_registry_config::cache::ServiceInstanceCache;
use std::sync::Arc;

let config = RpcConfig::default();
let cache = Arc::new(ServiceInstanceCache::new());
let registry = /* Arc<dyn ServiceRegistry> */;

// 根据协议创建客户端（目前仅支持 grpc）
let client = create_rpc_client(&config, cache, registry)?;
```

#### 1.3 设置全局客户端

```rust
use cmx_rpc::GlobalRpcClient;
use std::sync::Arc;

// 设置全局客户端（只能调用一次）
GlobalRpcClient::set(client)?;

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
use cmx_traits::{RpcClient, CallServiceOptions};

let client = GlobalRpcClient::get();

// 调用远程服务编排
let response = client
    .call_service(
        "target-service",       // 目标服务名
        "service-key-123",      // 服务标识
        r#"{"key": "value"}"#,  // 输入参数（JSON 字符串）
        CallServiceOptions::default(),
    )
    .await?;

if response.success {
    println!("执行成功: {:?}", response.output);
    for step in &response.steps {
        println!("步骤 {} ({}) - 状态: {}", step.node_name, step.node_type, step.status);
    }
}
```

#### 2.2 调用插件函数

```rust
use cmx_rpc::GlobalRpcClient;
use cmx_traits::RpcClient;

let client = GlobalRpcClient::get();

// 调用远程插件函数
let result = client
    .call_function(
        "target-service",       // 目标服务名
        "plugin-id-456",        // 插件 ID
        "process_data",         // 函数名
        r#"{"input": "data"}"#, // 输入参数（JSON 字符串）
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
    start_grpc_server(9090, service_invoker, runtime_invoker).await?;

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
use cmx_registry_config::cache::ServiceInstanceCache;
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
// 当注册中心的服务实例发生变化时：
// ServiceInstanceCache → callback → async_broadcast → volo LoadBalancer → 更新连接池
```

### 五、错误处理

#### 5.1 框架错误类型

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

#### 5.2 调用错误处理

```rust
use cmx_traits::RpcError;

match client.call_service("svc", "key", "{}", options).await {
    Ok(response) => {
        if response.success {
            println!("成功: {:?}", response.output);
        } else {
            // 业务层错误，封装在响应体中
            println!("业务错误: {:?}", response.error);
        }
    }
    Err(RpcError::Timeout) => {
        eprintln!("调用超时");
    }
    Err(RpcError::ConnectionFailed(msg)) => {
        eprintln!("连接失败: {}", msg);
    }
    Err(RpcError::UnsupportedProtocol(msg)) => {
        eprintln!("不支持的协议: {}", msg);
    }
    Err(e) => {
        eprintln!("其他错误: {}", e);
    }
}
```

### 六、完整集成示例

```rust
use cmx_rpc::{create_rpc_client, GlobalRpcClient, start_grpc_server, RpcConfig};
use cmx_registry_config::cache::ServiceInstanceCache;
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

    // 3. 启动 gRPC 服务端（同时作为客户端和服务端）
    let service_invoker = /* 获取 ServiceInvoker */;
    let runtime_invoker = /* 获取 RuntimeInvoker */;

    start_grpc_server(9090, service_invoker, runtime_invoker).await?;

    Ok(())
}
```

## 常见问题

### Q: 支持哪些 RPC 协议？

**A**: 目前仅支持 gRPC 协议（基于 volo-grpc）。HTTP REST 协议为预留接口，尚未实现。通过 `create_rpc_client` 工厂函数根据 `RpcConfig.protocol` 字段选择协议。

### Q: 服务发现如何工作？

**A**: `RegistryAwareDiscover` 桥接 `ServiceInstanceCache`（由 `cmx-registry-config` 维护的注册中心缓存）与 volo 的 `Discover` trait。当服务实例变更时，通过 `async-broadcast` 通道通知 volo 负载均衡器更新连接池。

### Q: 调用超时如何配置？

**A**: 在 `GrpcConfig` 中设置 `timeout_ms` 字段（默认 5000ms）。客户端使用 `tokio::time::timeout` 实现超时控制，超时返回 `RpcError::Timeout`。

### Q: 全局客户端是否线程安全？

**A**: 是。`GlobalRpcClient` 内部使用 `std::sync::OnceLock`，保证只初始化一次且线程安全。`get()` 返回 `&'static Arc<dyn RpcClient>`，可在任意线程安全访问。

### Q: 服务端业务错误如何返回？

**A**: 服务端不返回 gRPC Status 错误，而是将业务错误包装在响应体的 `error` 字段中（如 `ExecuteServiceResponse.error`）。这确保业务层错误不会中断 gRPC 连接。
