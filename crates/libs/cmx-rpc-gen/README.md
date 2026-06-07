# cmx-rpc-gen

> 基于 volo-build 的 gRPC 代码生成与重导出 crate，从 Protobuf IDL 自动生成 Rust 类型和 gRPC 服务代码，供 cmx-rpc 及其他 crate 统一引用。

[![Version](https://img.shields.io/badge/version-0.1.8-blue.svg)]
[![License](https://img.shields.io/badge/license-MIT-green.svg)]

## 快速开始

### 安装

```toml
[dependencies]
cmx-rpc-gen = { workspace = true }
```

### 核心示例

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::{
    ExecuteServiceRequest, ExecuteServiceResponse,
    CallFunctionRequest, CallFunctionResponse,
    CmxServiceOrchestratorClient,
};

// 构建服务编排请求
let request = ExecuteServiceRequest {
    service_key: "my-service-key".into(),
    input: r#"{"key": "value"}"#.into(),
    include_steps: true,
    debug: false,
    ..Default::default()
};
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| Protobuf 代码生成 | 通过 volo-build 在编译期从 proto 文件生成 Rust 代码 |
| gRPC 服务定义 | 生成 CmxServiceOrchestrator 服务 trait 和客户端 |
| 消息类型生成 | 自动生成所有 protobuf 消息对应的 Rust 结构体 |
| 统一重导出 | 其他 crate 只需依赖 cmx-rpc-gen 即可获取所有生成类型 |
| 零手写代码 | 所有公共 API 均由代码生成器产生，无需手动维护 |

## 模块结构

```
cmx-rpc-gen
├── build.rs                          # volo-build 代码生成入口
├── volo.yml                          # volo-build 构建配置
├── idl/
│   └── cmx_service.proto             # Protobuf IDL 定义
└── src/
    └── lib.rs                        # 重导出生成代码
```

### 生成代码模块

```
cmx::cmx_service_orchestrator
├── ExecuteServiceRequest             # 服务编排执行请求
├── ExecuteServiceResponse            # 服务编排执行响应
├── ExecutionStep                     # 编排执行步骤
├── OrchestrationError                # 编排错误
├── CallFunctionRequest               # 插件函数调用请求
├── CallFunctionResponse              # 插件函数调用响应
├── CmxServiceOrchestrator            # gRPC 服务 trait
└── CmxServiceOrchestratorClient      # gRPC 客户端
```

## 使用指南

### 一、消息类型

#### 1.1 构建服务编排请求

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::ExecuteServiceRequest;

// 基本请求
let request = ExecuteServiceRequest {
    service_key: "order-service".into(),
    input: r#"{"orderId": "12345"}"#.into(),
    include_steps: false,
    debug: false,
    ..Default::default()
};

// 带调试参数的请求
let debug_request = ExecuteServiceRequest {
    service_key: "order-service".into(),
    input: r#"{"orderId": "12345"}"#.into(),
    include_steps: true,
    debug: true,
    debug_node_id: Some("node-3".into()),
    debug_params: vec![
        ("param1".into(), "value1".into()),
        ("param2".into(), "value2".into()),
    ].into_iter().collect(),
};
```

#### 1.2 处理服务编排响应

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::ExecuteServiceResponse;

let response: ExecuteServiceResponse = /* 从 gRPC 调用获取 */;

if response.success {
    // 获取输出结果
    if let Some(output) = response.output {
        println!("输出: {}", output);
    }

    // 遍历执行步骤
    for step in &response.steps {
        println!(
            "步骤: {} ({}) - 状态: {}, 耗时: {}us",
            step.node_name, step.node_type, step.status, step.elapsed_us
        );
    }

    // 总耗时
    println!("总耗时: {}us", response.total_elapsed_us);
} else {
    // 处理编排错误
    if let Some(error) = response.error {
        println!("编排错误: {}", error.message);
    }
}
```

#### 1.3 构建插件函数调用请求

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::CallFunctionRequest;

let request = CallFunctionRequest {
    plugin_id: "my-plugin".into(),
    function_name: "process_data".into(),
    input: r#"{"data": "hello"}"#.into(),
    initial_input: Some(r#"{"original": "input"}"#.into()),
    debug: false,
};
```

#### 1.4 处理插件函数调用响应

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::CallFunctionResponse;

let response: CallFunctionResponse = /* 从 gRPC 调用获取 */;

if response.success {
    if let Some(result) = response.result {
        println!("函数返回: {}", result);
    }
    println!("耗时: {}us", response.elapsed_us);
} else {
    if let Some(error) = response.error {
        println!("调用错误: {}", error);
    }
}
```

### 二、gRPC 服务实现

#### 2.1 实现 gRPC 服务 trait

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::CmxServiceOrchestrator;
use volo_grpc::{Request, Response, Status};

#[derive(Clone)]
struct MyServiceImpl {
    // 业务依赖
}

impl CmxServiceOrchestrator for MyServiceImpl {
    async fn execute_service(
        &self,
        req: Request<ExecuteServiceRequest>,
    ) -> Result<Response<ExecuteServiceResponse>, Status> {
        let inner = req.into_inner();

        // 处理业务逻辑
        let response = ExecuteServiceResponse {
            success: true,
            output: Some(r#"{"result": "ok"}"#.into()),
            steps: vec![],
            total_elapsed_us: 100,
            error: None,
        };

        Ok(Response::new(response))
    }

    async fn call_function(
        &self,
        req: Request<CallFunctionRequest>,
    ) -> Result<Response<CallFunctionResponse>, Status> {
        let inner = req.into_inner();

        let response = CallFunctionResponse {
            success: true,
            result: Some("function result".into()),
            elapsed_us: 50,
            error: None,
        };

        Ok(Response::new(response))
    }
}
```

#### 2.2 使用 gRPC 客户端

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::CmxServiceOrchestratorClient;
use volo_grpc::Request;

// 创建客户端（通常由 cmx-rpc 的 VoloGrpcClient 内部管理）
let client: CmxServiceOrchestratorClient = /* 通过 volo 创建 */;

// 调用 ExecuteService
let request = ExecuteServiceRequest {
    service_key: "my-service".into(),
    input: "{}".into(),
    ..Default::default()
};
let response = client.execute_service(Request::new(request)).await?;

// 调用 CallFunction
let request = CallFunctionRequest {
    plugin_id: "plugin-1".into(),
    function_name: "run".into(),
    input: "{}".into(),
    ..Default::default()
};
let response = client.call_function(Request::new(request)).await?;
```

### 三、Protobuf IDL 定义

#### 3.1 当前服务定义

```protobuf
syntax = "proto3";
package cmx;

service CmxServiceOrchestrator {
  // 执行服务编排（对应 POST /api/service/execute）
  rpc ExecuteService(ExecuteServiceRequest) returns (ExecuteServiceResponse);
  // 调用插件函数（对应 POST /api/service/call）
  rpc CallFunction(CallFunctionRequest) returns (CallFunctionResponse);
}
```

#### 3.2 消息字段说明

**ExecuteServiceRequest：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `service_key` | `string` | 服务标识 |
| `input` | `string` | 输入数据（JSON 字符串） |
| `include_steps` | `bool` | 是否包含执行步骤详情 |
| `debug` | `bool` | 是否启用调试模式 |
| `debug_node_id` | `optional string` | 调试目标节点 ID |
| `debug_params` | `map<string, string>` | 调试参数 |

**ExecuteServiceResponse：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `bool` | 是否执行成功 |
| `output` | `optional string` | 输出结果（JSON 字符串） |
| `steps` | `repeated ExecutionStep` | 执行步骤列表 |
| `total_elapsed_us` | `uint64` | 总耗时（微秒） |
| `error` | `optional OrchestrationError` | 编排错误信息 |

**CallFunctionRequest：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `plugin_id` | `string` | 插件 ID |
| `function_name` | `string` | 函数名 |
| `input` | `string` | 输入数据（JSON 字符串） |
| `initial_input` | `optional string` | 初始输入 |
| `debug` | `bool` | 是否启用调试模式 |

**CallFunctionResponse：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `bool` | 是否调用成功 |
| `result` | `optional string` | 返回结果（JSON 字符串） |
| `elapsed_us` | `uint64` | 耗时（微秒） |
| `error` | `optional string` | 错误信息 |

### 四、修改 IDL 定义

#### 4.1 添加新的 RPC 方法

1. 编辑 `idl/cmx_service.proto`，添加新的 rpc 方法和对应的消息类型
2. 重新编译项目，volo-build 会自动重新生成代码

```protobuf
// 在 service CmxServiceOrchestrator 中添加新方法
rpc HealthCheck(HealthCheckRequest) returns (HealthCheckResponse);

// 添加对应的消息定义
message HealthCheckRequest {
  string service_name = 1;
}

message HealthCheckResponse {
  bool healthy = 1;
  string message = 2;
}
```

#### 4.2 修改构建配置

`volo.yml` 配置说明：

```yaml
entries:
  proto:
    filename: cmx_service_orchestrator.rs  # 生成的文件名
    protocol: protobuf                      # 协议类型
    services:
      - idl:
          source: local                     # IDL 来源（local/远程）
          path: idl/cmx_service.proto       # proto 文件路径
          includes:                         # include 搜索路径
            - idl
```

### 五、与其他 Crate 的关系

```rust
// cmx-rpc-gen 是底层依赖，被以下 crate 使用：

// 1. cmx-rpc — RPC 框架核心库
//    使用 CmxServiceOrchestratorClient 进行 gRPC 调用
//    使用 CmxServiceOrchestrator trait 实现服务端
//    使用所有消息类型进行请求/响应构建

// 2. 其他需要 gRPC 类型的 crate
use cmx_rpc_gen::cmx::cmx_service_orchestrator::*;

// 典型依赖关系：
// cmx-traits (定义 RpcClient trait)
//     ↓
// cmx-rpc (实现 RpcClient，使用 cmx-rpc-gen 的类型)
//     ↓
// cmx-rpc-gen (提供 protobuf 生成代码)
```

## 常见问题

### Q: 为什么需要单独的 cmx-rpc-gen crate？

**A**: 将代码生成与业务逻辑分离是 volo 框架的推荐实践。`cmx-rpc-gen` 负责从 proto 文件生成 Rust 代码，`cmx-rpc` 负责使用这些类型实现业务逻辑。这样多个 crate 可以共享同一份生成代码，避免重复编译。

### Q: 如何查看生成的代码？

**A**: 编译后，生成代码位于 `target/debug/build/cmx-rpc-gen-<hash>/out/cmx_service_orchestrator.rs`。也可以通过 `cargo expand` 查看宏展开后的代码。

### Q: 修改 proto 文件后需要做什么？

**A**: 只需重新编译项目（`cargo build`），volo-build 会在编译期自动检测 proto 文件变更并重新生成代码。无需手动运行任何代码生成命令。

### Q: input 字段为什么使用 string 而不是嵌套消息？

**A**: `input` 字段使用 `string` 类型传输 JSON 字符串，这是 gRPC 传输复杂 JSON 结构的常见做法。客户端将 `serde_json::Value` 序列化为 JSON 字符串传输，服务端反序列化回 `Value`。这避免了 protobuf 缺少原生 JSON 值类型的限制。
