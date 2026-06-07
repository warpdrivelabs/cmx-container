# 微服务 RPC 调用架构 — 流程详解

> 本文档详细描述 cmx-container 微服务框架中服务发现、服务实例订阅、RPC 调用的完整流程，
> 帮助开发人员快速理解代码架构和数据流转。

---

## 一、整体架构概览

```mermaid
graph TB
    subgraph "服务 A（调用方）"
        A1[业务代码]
        A2[RpcClient trait]
        A3[VoloGrpcClient]
        A4[RegistryAwareDiscover]
        A5[ServiceInstanceCache]
        A6[ServiceListSyncer]
    end

    subgraph "cmx-registry-config"
        B1[ServiceRegistry trait]
        B2[NacosRegistry]
        B3[NacosInstanceListener]
        B4[GlobalServiceInstanceCache]
    end

    subgraph "Nacos Server"
        C1[服务列表]
        C2[实例列表]
        C3[变更推送]
    end

    subgraph "服务 B（被调用方）"
        D1[gRPC Server]
        D2[CmxOrchestratorServiceImpl]
        D3[ServiceInvoker]
        D4[RuntimeInvoker]
    end

    A1 -->|调用| A2
    A2 -->|实现| A3
    A3 -->|发现实例| A4
    A4 -->|读取缓存| A5
    A5 -->|共享| B4
    A6 -->|定时拉取服务列表| B1
    B1 -->|实现| B2
    B2 -->|get_service_list| C1
    B2 -->|subscribe| C2
    C3 -->|推送变更| B3
    B3 -->|更新缓存| A5

    A3 -->|gRPC 调用| D1
    D1 -->|路由| D2
    D2 -->|execute_service| D3
    D2 -->|call_function| D4
```

---

## 二、核心组件说明

| 组件 | 所在 Crate | 职责 |
|------|-----------|------|
| `ServiceRegistry` trait | cmx-registry-config | 注册中心抽象接口（注册/注销/查询/订阅） |
| `NacosRegistry` | cmx-registry-config | Nacos 注册中心实现 |
| `NacosInstanceListener` | cmx-registry-config | Nacos 实例变更监听器 |
| `ServiceInstanceCache` | cmx-registry-config | 通用服务实例内存缓存（注册中心无关） |
| `GlobalServiceInstanceCache` | cmx-registry-config | 全局缓存单例（OnceLock） |
| `ServiceListSyncer` | cmx-registry-config | 服务列表定时同步器 |
| `RpcClient` trait | cmx-traits | RPC 调用统一接口（策略模式） |
| `VoloGrpcClient` | cmx-rpc | gRPC 协议的 RpcClient 实现 |
| `RegistryAwareDiscover` | cmx-rpc | 桥接 ServiceInstanceCache ↔ volo Discover |
| `CmxOrchestratorServiceImpl` | cmx-rpc | gRPC 服务端实现 |
| `CmxServiceOrchestrator` | cmx-rpc-gen | volo-build 生成的 protobuf 代码 |

---

## 三、服务启动初始化流程

```mermaid
sequenceDiagram
    participant Main as web-server main.rs
    participant Infra as init_infra()
    participant RPC as init_rpc()
    participant Registry as NacosRegistry
    participant Cache as ServiceInstanceCache
    participant Syncer as ServiceListSyncer
    participant Server as gRPC Server

    Main->>Infra: 1. 初始化注册中心
    Infra->>Registry: create_registry_with_cache()
    Registry-->>Infra: (registry, cache)
    Infra->>Cache: GlobalServiceInstanceCache::set(cache)

    Main->>RPC: 2. 初始化 RPC 子系统
    RPC->>Cache: GlobalServiceInstanceCache::get()
    RPC->>RPC: create_rpc_client(&rpc, cache, registry)
    RPC->>RPC: GlobalRpcClient::set(rpc_client)

    RPC->>Server: 3. tokio::spawn(start_grpc_server)

    RPC->>Registry: 4. 缓存预热 warmup_services
    loop 每个 warmup 服务
        RPC->>Registry: subscribe_instances(svc, callback)
        Registry->>Registry: cache.subscribe(svc, callback)
        Registry->>Registry: query_instances(svc) → cache.update()
        Registry->>Registry: naming.subscribe(svc, listener)
    end

    RPC->>Syncer: 5. 启动服务列表定时同步
    RPC->>Syncer: syncer.mark_subscribed(warmup_services)
    RPC->>Syncer: tokio::spawn(syncer.run())
```

### 初始化步骤详解

1. **`init_infra()`**：创建 `NacosRegistry` + `ServiceInstanceCache`，设置全局缓存单例
2. **`init_rpc()`**：
   - 从全局缓存获取共享 `cache`
   - 通过工厂函数 `create_rpc_client()` 创建 `VoloGrpcClient`
   - 注册到 `GlobalRpcClient` 全局单例
3. **启动 gRPC Server**：在后台 tokio task 中运行，监听独立端口（默认 9090）
4. **缓存预热**：遍历 `warmup_services` 配置，对每个服务调用 `subscribe_instances()`
5. **启动 ServiceListSyncer**：定时轮询注册中心服务列表，发现新服务自动订阅

---

## 四、服务发现与实例订阅流程

### 4.1 三种触发订阅的途径

```mermaid
graph TD
    A[服务实例订阅触发] --> B[缓存预热]
    A --> C[ServiceListSyncer 定时同步]
    A --> D[RPC 调用缓存穿透]

    B -->|warmup_services 配置| E[subscribe_instances]
    C -->|每 N 秒轮询服务列表| E
    D -->|cache.get 为空| E

    E --> F[1. cache.subscribe 注册回调]
    E --> G[2. query_instances 首次拉取]
    E --> H[3. naming.subscribe 注册 Nacos 监听器]

    G --> I[cache.update 写入缓存]
    H --> J[NacosInstanceListener]
    J -->|后续推送| I
```

### 4.2 subscribe_instances 详细流程

```mermaid
sequenceDiagram
    participant Caller as 调用方
    participant Registry as NacosRegistry
    participant Cache as ServiceInstanceCache
    participant Nacos as Nacos SDK
    participant Listener as NacosInstanceListener

    Caller->>Registry: subscribe_instances("order-service", callback)
    Registry->>Cache: cache.subscribe("order-service", callback)
    Note over Cache: 注册回调到 subscribers 列表

    Registry->>Nacos: query_instances("order-service")
    Nacos-->>Registry: [instance1, instance2, ...]
    Registry->>Cache: cache.update("order-service", instances)
    Note over Cache: 写入 cached HashMap + 通知已有订阅者

    alt 首次订阅该服务
        Registry->>Nacos: naming.subscribe("order-service", listener)
        Note over Nacos,Listener: 注册 NacosInstanceListener
        Nacos-->>Listener: 后续实例变更推送
    else 已订阅过
        Note over Registry: 跳过，registered_listeners 去重
    end
```

### 4.3 ServiceListSyncer 定时同步流程

```mermaid
sequenceDiagram
    participant Syncer as ServiceListSyncer
    participant Registry as NacosRegistry
    participant Cache as ServiceInstanceCache

    loop 每 service_sync_interval_secs 秒
        Syncer->>Registry: get_service_list()
        Registry-->>Syncer: ["order-service", "user-service", "pay-service"]

        Note over Syncer: 与 subscribed_services 对比

        alt 发现新服务
            Syncer->>Registry: subscribe_instances("new-service", callback)
            Note over Syncer: 标记为已订阅
        end

        alt 服务下线
            Syncer->>Cache: cache.update("removed-service", vec![])
            Note over Syncer: 保留订阅关系（Nacos 不支持 unsubscribe）
        end
    end
```

---

## 五、Nacos 实例变更实时推送流程

```mermaid
sequenceDiagram
    participant Nacos as Nacos Server
    participant SDK as nacos-sdk
    participant Listener as NacosInstanceListener
    participant Cache as ServiceInstanceCache
    participant Discover as RegistryAwareDiscover
    participant LB as volo 负载均衡器

    Nacos->>SDK: 推送 NamingChangeEvent
    SDK->>Listener: event(Arc<NamingChangeEvent>)
    Listener->>Listener: 过滤 healthy 实例
    Listener->>Listener: convert_from_nacos_instance()
    Listener->>Cache: cache.update("order-service", instances)

    Note over Cache: 两件事同时发生：
    Note over Cache: 1. 更新 cached HashMap
    Note over Cache: 2. 遍历 subscribers 回调

    Cache->>Discover: callback(service_name, instances)
    Discover->>Discover: instances_to_volo(instances)
    Discover->>Discover: 构建 Change { key, all, added, ... }
    Discover->>LB: change_tx.try_broadcast(change)

    Note over LB: 负载均衡器收到变更通知<br/>下次调用自动路由到新实例
```

---

## 六、RPC 调用完整流程

### 6.1 call_service 流程（执行服务编排）

```mermaid
sequenceDiagram
    participant Biz as 业务代码
    participant Client as VoloGrpcClient
    participant Cache as ServiceInstanceCache
    participant Registry as NacosRegistry
    participant Discover as RegistryAwareDiscover
    participant VoloLB as volo 负载均衡
    participant Server as gRPC Server (服务B)
    participant Invoker as ServiceInvoker

    Biz->>Client: call_service("order-service", "create_order", input, options)

    Client->>Cache: cache.get("order-service")

    alt 缓存为空（缓存穿透）
        Client->>Registry: subscribe_instances("order-service", callback)
        Registry-->>Client: Ok
        Client->>Cache: cache.get("order-service")
        alt 仍为空
            Client-->>Biz: Err(NoAvailableInstance)
        end
    end

    Client->>Discover: new(cache) + start_watch("order-service")
    Note over Discover: 注册缓存变更回调到 async_broadcast

    Client->>VoloLB: CmxServiceOrchestratorClientBuilder::new("order-service").discover(discover).build()

    Client->>Client: 构建 ExecuteServiceRequest
    Client->>Server: client.execute_service(req) [带超时]

    Server->>Invoker: service_invoker.invoke_service(service_key, input, options)
    Invoker-->>Server: CallServiceResponse
    Server-->>Client: ExecuteServiceResponse

    Client->>Client: proto_to_call_service_response() 类型转换
    Client-->>Biz: Ok(CallServiceResponse)
```

### 6.2 call_function 流程（调用插件函数）

```mermaid
sequenceDiagram
    participant Biz as 业务代码
    participant Client as VoloGrpcClient
    participant Server as gRPC Server (服务B)
    participant Invoker as RuntimeInvoker
    participant WASM as WASM Runtime

    Biz->>Client: call_function("order-service", "plugin-1", "calc", input)

    Note over Client: 服务发现流程同 6.1

    Client->>Client: 构建 CallFunctionRequest
    Client->>Server: client.call_function(req) [带超时]

    Server->>Invoker: runtime_invoker.invoke("plugin-1", "calc", input_bytes)
    Invoker->>WASM: 执行 WASM 函数
    WASM-->>Invoker: WasmInvokeResult
    Invoker-->>Server: WasmInvokeResult
    Server-->>Client: CallFunctionResponse

    Client->>Client: 转换为 FunctionCallResult
    Client-->>Biz: Ok(FunctionCallResult)
```

---

## 七、gRPC Server 请求处理流程

```mermaid
sequenceDiagram
    participant Remote as 远程调用方
    participant Grpc as volo-grpc Server
    participant Impl as CmxOrchestratorServiceImpl
    participant SvcInvoker as ServiceInvoker
    participant RtInvoker as RuntimeInvoker

    alt ExecuteService
        Remote->>Grpc: gRPC 请求 ExecuteService
        Grpc->>Impl: execute_service(Request)
        Impl->>Impl: 解析 JSON input
        Impl->>Impl: 构建 ServiceInvokeOptions
        Impl->>SvcInvoker: invoke_service(service_key, input, options)
        SvcInvoker-->>Impl: CallServiceResponse
        Impl->>Impl: 转换为 protobuf ExecuteServiceResponse
        Impl-->>Grpc: Response
        Grpc-->>Remote: gRPC 响应
    else CallFunction
        Remote->>Grpc: gRPC 请求 CallFunction
        Grpc->>Impl: call_function(Request)
        Impl->>RtInvoker: invoke(plugin_id, function_name, input_bytes)
        RtInvoker-->>Impl: WasmInvokeResult
        Impl->>Impl: 转换为 protobuf CallFunctionResponse
        Impl-->>Grpc: Response
        Grpc-->>Remote: gRPC 响应
    end
```

---

## 八、数据流转与类型转换

### 8.1 服务实例数据流转

```mermaid
graph LR
    A[NacosServiceInstance<br/>nacos-sdk] -->|convert_from_nacos_instance| B[ServiceInstance<br/>cmx-registry-config]
    B -->|instances_to_volo| C[volo Instance<br/>volo::discovery]
    C -->|负载均衡选择| D[SocketAddr<br/>gRPC 连接]

    style A fill:#f9f,stroke:#333
    style B fill:#bbf,stroke:#333
    style C fill:#bfb,stroke:#333
    style D fill:#fbb,stroke:#333
```

### 8.2 请求/响应类型转换

```mermaid
graph TD
    subgraph "call_service 类型转换"
        A1[serde_json::Value] -->|序列化| A2[FastStr]
        A2 -->|protobuf 传输| A3[ExecuteServiceRequest]
        A3 -->|gRPC| A4[ExecuteServiceResponse]
        A4 -->|proto_to_call_service_response| A5[CallServiceResponse]
    end

    subgraph "call_function 类型转换"
        B1[serde_json::Value] -->|序列化| B2[FastStr]
        B2 -->|protobuf 传输| B3[CallFunctionRequest]
        B3 -->|gRPC| B4[CallFunctionResponse]
        B4 -->|类型转换| B5[FunctionCallResult]
    end
```

---

## 九、关键源码索引

| 流程 | 文件 | 关键方法/结构体 |
|------|------|----------------|
| 注册中心初始化 | `cmx-registry-config/src/registry/mod.rs` | `create_registry_with_cache()` |
| Nacos 实例变更监听 | `cmx-registry-config/src/registry/nacos.rs` | `NacosInstanceListener::event()` |
| 服务实例缓存 | `cmx-registry-config/src/registry/instance_cache.rs` | `ServiceInstanceCache::update()` |
| 服务列表定时同步 | `cmx-registry-config/src/registry/service_list_syncer.rs` | `ServiceListSyncer::sync_once()` |
| RPC 初始化 | `web-server/src/config/rpc.rs` | `init_rpc()` |
| RPC 客户端 | `cmx-rpc/src/client.rs` | `VoloGrpcClient::get_client()` |
| volo Discover 桥接 | `cmx-rpc/src/discover.rs` | `RegistryAwareDiscover` |
| RPC 工厂函数 | `cmx-rpc/src/factory.rs` | `create_rpc_client()` |
| gRPC 服务端 | `cmx-rpc/src/server.rs` | `CmxOrchestratorServiceImpl` |
| gRPC Server 启动 | `cmx-rpc/src/server_runner.rs` | `start_grpc_server()` |
| protobuf 定义 | `cmx-rpc-gen/idl/cmx_service.proto` | `CmxServiceOrchestrator` service |
| RPC trait 定义 | `cmx-traits/src/rpc_client.rs` | `RpcClient` trait |

---

## 十、配置参考

```toml
[rpc]
# 是否启用 RPC
enabled = true
# 通信协议（目前仅支持 "grpc"）
protocol = "grpc"
# 预热服务列表（启动时预先发现的服务名）
warmup_services = ["order-service", "user-service"]
# 服务列表同步间隔（秒，0=禁用，默认 30）
service_sync_interval_secs = 30

[rpc.grpc]
# gRPC Server 监听端口
port = 9090
# 调用超时时间（毫秒）
timeout_ms = 5000
# 重试次数
retry_count = 0
# 连接池大小
pool_size = 4
```

---

## 十一、扩展新 RPC 协议指南

如需支持新的 RPC 协议（如 HTTP/REST），只需：

1. 在 `cmx-rpc/src/` 下新增协议实现（如 `http_rest_client.rs`）
2. 实现 `RpcClient` trait
3. 在 `factory.rs` 的 `create_rpc_client()` 中增加 match 分支
4. 在 `RpcConfig` 中添加新协议的配置段

业务代码无需任何修改，仍通过 `GlobalRpcClient::get().call_service(...)` 调用。
