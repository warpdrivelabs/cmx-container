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
use cmx_rpc_gen::orchestrator_proto::{
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
│   ├── orchestrator/
│   │   └── cmx_service.proto         # 服务编排域 IDL
│   └── resource/
│       └── cmx_resource_data.proto   # 资源数据管理域 IDL
└── src/
    └── lib.rs                        # 重导出生成代码 + 便捷别名模块
```

### 生成代码模块

生成类型完整路径位于 `cmx_rpc_gen::cmx::<service>::<service>::cmx` 下，**推荐使用便捷别名**（lib.rs 提供）：

- `cmx_rpc_gen::orchestrator_proto::*` — 服务编排域（ExecuteServiceRequest、CmxServiceOrchestratorClient 等）
- `cmx_rpc_gen::resource_data_proto::*` — 资源数据管理域（ImportResourceDataRequest、CmxResourceDataServiceClient 等）

```
orchestrator_proto（别名）
├── ExecuteServiceRequest             # 服务编排执行请求
├── ExecuteServiceResponse            # 服务编排执行响应
├── ExecutionStep                     # 编排执行步骤
├── OrchestrationError                # 编排错误
├── CallFunctionRequest               # 插件函数调用请求
├── CallFunctionResponse              # 插件函数调用响应
├── CmxServiceOrchestrator            # gRPC 服务 trait
├── CmxServiceOrchestratorClient      # gRPC 客户端
├── CmxServiceOrchestratorServer      # gRPC 服务端注册器
├── CmxServiceOrchestratorRequestRecv # gRPC 请求接收类型
└── CmxServiceOrchestratorResponseSend # gRPC 响应发送类型
```

## 使用指南

### 一、消息类型

#### 1.1 构建服务编排请求

```rust
use cmx_rpc_gen::orchestrator_proto::ExecuteServiceRequest;

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
use cmx_rpc_gen::orchestrator_proto::ExecuteServiceResponse;

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
use cmx_rpc_gen::orchestrator_proto::CallFunctionRequest;

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
use cmx_rpc_gen::orchestrator_proto::CallFunctionResponse;

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
use cmx_rpc_gen::orchestrator_proto::CmxServiceOrchestrator;
use volo_grpc::{Request, Response, Status};

#[derive(Clone)]
struct MyServiceImpl {
    // 业务依赖
}

impl CmxServiceOrchestrator for MyServiceImpl {
    fn execute_service(
        &self,
        req: Request<ExecuteServiceRequest>,
    ) -> impl std::future::Future<
        Output = Result<Response<ExecuteServiceResponse>, Status>,
    > + Send {
        let inner = req.into_inner();
        async move {
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
    }

    fn call_function(
        &self,
        req: Request<CallFunctionRequest>,
    ) -> impl std::future::Future<
        Output = Result<Response<CallFunctionResponse>, Status>,
    > + Send {
        let inner = req.into_inner();
        async move {
            let response = CallFunctionResponse {
                success: true,
                result: Some("function result".into()),
                elapsed_us: 50,
                error: None,
            };

            Ok(Response::new(response))
        }
    }
}
```

#### 2.2 使用 gRPC 客户端

```rust
use cmx_rpc_gen::orchestrator_proto::CmxServiceOrchestratorClient;
use volo_grpc::Request;

// 创建客户端（通常由 cmx-rpc 的 VoloGrpcClient 内部管理）
let client: CmxServiceOrchestratorClient = /* 通过 volo 创建 */;

// 调用 ExecuteService
let request = ExecuteServiceRequest {
    service_key: "my-service".into(),
    input: "{}".into(),
    ..Default::default()
};
let response = client.execute_service(request).await?;

// 调用 CallFunction
let request = CallFunctionRequest {
    plugin_id: "plugin-1".into(),
    function_name: "run".into(),
    input: "{}".into(),
    ..Default::default()
};
let response = client.call_function(request).await?;
```

### 三、Protobuf IDL 定义

#### 3.1 当前服务定义

```protobuf
syntax = "proto3";
package cmx;

// 服务编排 gRPC 服务
service CmxServiceOrchestrator {
  // 执行服务编排（对应 POST /api/service/execute）
  rpc ExecuteService(ExecuteServiceRequest) returns (ExecuteServiceResponse);
  // 调用插件函数（对应 POST /api/service/call）
  rpc CallFunction(CallFunctionRequest) returns (CallFunctionResponse);
}
```

#### 3.2 消息字段说明

**ExecuteServiceRequest：**

| 字段 | 类型 | 序号 | 说明 |
|------|------|------|------|
| `service_key` | `string` | 1 | 服务标识 |
| `input` | `string` | 2 | 输入数据（JSON 字符串） |
| `include_steps` | `bool` | 3 | 是否包含执行步骤详情 |
| `debug` | `bool` | 4 | 是否启用调试模式 |
| `debug_node_id` | `optional string` | 5 | 调试目标节点 ID |
| `debug_params` | `map<string, string>` | 6 | 调试参数 |

**ExecutionStep：**

| 字段 | 类型 | 序号 | 说明 |
|------|------|------|------|
| `node_id` | `string` | 1 | 节点 ID |
| `node_name` | `string` | 2 | 节点名称 |
| `node_type` | `string` | 3 | 节点类型 |
| `status` | `string` | 4 | 执行状态（Success/Failed/Skipped/DebugPaused） |
| `output` | `optional string` | 5 | 输出（JSON 字符串） |
| `elapsed_us` | `uint64` | 6 | 耗时（微秒） |
| `error` | `optional string` | 7 | 错误信息 |
| `previous_output` | `optional string` | 8 | 前置节点输出（JSON 字符串） |

**OrchestrationError：**

| 字段 | 类型 | 序号 | 说明 |
|------|------|------|------|
| `message` | `string` | 1 | 错误消息 |

**ExecuteServiceResponse：**

| 字段 | 类型 | 序号 | 说明 |
|------|------|------|------|
| `success` | `bool` | 1 | 是否执行成功 |
| `output` | `optional string` | 2 | 输出结果（JSON 字符串） |
| `steps` | `repeated ExecutionStep` | 3 | 执行步骤列表 |
| `total_elapsed_us` | `uint64` | 4 | 总耗时（微秒） |
| `error` | `optional OrchestrationError` | 5 | 编排错误信息 |

**CallFunctionRequest：**

| 字段 | 类型 | 序号 | 说明 |
|------|------|------|------|
| `plugin_id` | `string` | 1 | 插件 ID |
| `function_name` | `string` | 2 | 函数名 |
| `input` | `string` | 3 | 输入数据（JSON 字符串） |
| `initial_input` | `optional string` | 4 | 初始输入 |
| `debug` | `bool` | 5 | 是否启用调试模式 |

**CallFunctionResponse：**

| 字段 | 类型 | 序号 | 说明 |
|------|------|------|------|
| `success` | `bool` | 1 | 是否调用成功 |
| `result` | `optional string` | 2 | 返回结果（JSON 字符串） |
| `elapsed_us` | `uint64` | 3 | 耗时（微秒） |
| `error` | `optional string` | 4 | 错误信息 |

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
  orchestrator:
    filename: cmx_service_orchestrator.rs  # 生成的文件名（决定 lib.rs 的 include 路径）
    protocol: protobuf                      # 协议类型
    services:
      - idl:
          source: local                     # IDL 来源（local/远程）
          path: idl/orchestrator/cmx_service.proto  # proto 文件路径（按域分子目录）
          includes:                         # include 搜索路径
            - idl
```

#### 4.3 构建脚本

`build.rs` 使用 `volo_build::ConfigBuilder` 读取 `volo.yml` 配置生成代码：

```rust
fn main() {
    volo_build::ConfigBuilder::default()
        .write()
        .expect("volo-build 失败：请确认 idl/cmx_service.proto 与 volo.yml 配置正确");
}
```

> **注意**：`ConfigBuilder` 会自动读取同目录下的 `volo.yml` 配置文件来生成代码。
> 不要使用 `volo_build::Builder::protobuf()`，那是编程式 API，需要手动调用 `.add_service()` 指定 proto 文件，
> 不会读取 `volo.yml`。

### 五、数据序列化约定

#### 5.1 JSON 字符串传输

`input`/`output`/`result` 字段使用 `string` 类型传输 JSON 字符串，这是 gRPC 传输复杂 JSON 结构的常见做法：

```
业务层 serde_json::Value → to_string() → protobuf string → 传输 → protobuf string → from_str() → Value
```

#### 5.2 步骤状态字符串

`ExecutionStep.status` 字段使用字符串表示，取值范围：

| 状态值 | 说明 |
|--------|------|
| `Success` | 执行成功 |
| `Failed` | 执行失败 |
| `Skipped` | 跳过 |
| `DebugPaused` | 调试暂停 |

### 六、与其他 Crate 的关系

```
cmx-traits (定义 ServiceOrchestrationClient/ResourceDataClient/ServiceInvoker 等抽象)
    ↓
cmx-rpcs/* 皮肤 crate (cmx-orchestrator-rpc / cmx-resource-rpc：client + server impl + Bundle)
    ↓                ↓
cmx-rpc (RPC 基础设施：Bundle trait / 发现 / 重试 / 鉴权 / server_runner)
    ↓
cmx-rpc-gen (提供 protobuf 生成代码，本 crate)
    ↓
volo-build + volo-grpc + pilota (底层 gRPC 框架)
```

**使用场景：**

```rust
// 1. 皮肤 crate（cmx-rpcs/*）— 使用生成类型实现 gRPC client/server 与 Bundle
use cmx_rpc_gen::orchestrator_proto::*;

// 2. 其他需要 gRPC 类型的 crate
use cmx_rpc_gen::resource_data_proto::*;

// 3. 组装层（cmx-platform-app）
//    显式收集皮肤 Bundle 列表传入 init_rpc（主应用 RPC 能力的唯一决定点）
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

### Q: 生成的类型路径是什么？

**A**: 所有生成类型通过 `cmx_rpc_gen::cmx::cmx_service_orchestrator` 路径访问。这对应 proto 文件中的 `package cmx` 和 `service CmxServiceOrchestrator`。
