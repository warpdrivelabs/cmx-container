# Volo 框架使用教程

> 基于 cmx-rpc-gen 与 cmx-service-rpc 实战经验的 Volo / Volo-gRPC 新手入门文档。
>
> 适用版本：`volo 0.12.x`、`volo-grpc 0.12.x`、`pilota 0.13.x`
>
> 读者对象：刚接触 Rust RPC 框架的开发者

---

## 目录

- [一、写在前面](#一写在前面)
- [二、什么是 Volo](#二什么是-volo)
- [三、核心概念速览](#三核心概念速览)
- [四、Rust 异步前置知识](#四rust-异步前置知识)
- [五、环境准备](#五环境准备)
- [六、第一个 gRPC 服务](#六第一个-grpc-服务)
- [七、IDL（Protobuf）编写规范](#七idlprotobuf编写规范)
- [八、volo-build 代码生成机制](#八volo-build-代码生成机制)
- [九、服务端开发详解](#九服务端开发详解)
- [十、客户端开发详解](#十客户端开发详解)
- [十一、服务发现（Discover）](#十一服务发现discover)
- [十二、负载均衡](#十二负载均衡)
- [十三、中间件（Motore / Service）](#十三中间件motore--service)
- [十四、超时、重试、连接池](#十四超时重试连接池)
- [十五、错误处理](#十五错误处理)
- [十六、cmx-rpc-gen 实战解读](#十六cmx-rpc-gen-实战解读)
- [十七、cmx-service-rpc 实战解读](#十七cmx-service-rpc-实战解读)
- [十八、整体串联：从 IDL 到全链路调用](#十八整体串联从-idl-到全链路调用)
- [十九、调试与排错](#十九调试与排错)
- [二十、常见问题 FAQ](#二十常见问题-faq)
- [二十一、参考资源](#二十一参考资源)

---

## 一、写在前面

Volo 是字节跳动服务框架团队开源的 Rust RPC 框架，最大的特点是：

1. **基于 AFIT（Async Functions in Traits）和 RPITIT（Return Position Impl Trait in Traits）**
   避免 `async_trait` 宏带来的 `Box<dyn>` 动态分发开销。
2. **高性能**：在 4C 限制下极限 QPS 达到 35W。
3. **易用性**：`volo` CLI 一键初始化项目 + 编译期生成代码。
4. **可扩展**：基于 `Motore` 中间件抽象，服务发现、负载均衡都按统一 Service 接口实现。
5. **强类型**：用 Protobuf / Thrift IDL 描述接口，编译期生成 Rust 类型，零手写胶水代码。

**volo-grpc** 是 Volo 框架的 gRPC 协议实现，对应 Thrift 版本叫 `volo-thrift`。本教程聚焦在 gRPC。

**cmx-rpc-gen** 与 **cmx-service-rpc** 是本项目对 Volo 的工程化封装：

- `cmx-rpc-gen`：从 Protobuf IDL 生成 Rust 类型和服务 trait。
- `cmx-service-rpc`：在生成的代码之上封装服务发现、负载均衡、客户端工厂、全局单例、桥接 ServiceInvoker 等。

阅读完本教程，你将掌握：

- 如何用 Protobuf 描述一个 gRPC 服务。
- 如何让 Volo 在编译期生成 Rust 代码。
- 如何实现一个 gRPC Server 和 Client。
- 如何在微服务架构下让 Client 自动从注册中心发现服务并负载均衡。
- 如何阅读和扩展 cmx-service-rpc / cmx-rpc-gen。

---

## 二、什么是 Volo

### 2.1 框架定位

Volo 不是单一的工具，而是一个**框架家族**：

| 组件               | 作用                                                                 |
| ------------------ | -------------------------------------------------------------------- |
| `volo`             | 核心框架：定义 Service、Client、Layer、中间件、上下文、地址、负载均衡等抽象 |
| `volo-grpc`        | gRPC 协议实现：基于 `volo` + `pilota` 提供 gRPC Server/Client         |
| `volo-thrift`      | Thrift 协议实现                                                     |
| `volo-build`       | 编译期 IDL → Rust 代码生成器（build 脚本中使用）                     |
| `volo-cli`         | 命令行工具：`volo init` / `volo idl add`                             |
| `pilota`           | 纯 Rust 实现的 Thrift/Protobuf 编译器和编解码库（不依赖 `protoc`）     |
| `motore`           | 中间件抽象层（Volo 自己实现的 Tower-like）                           |
| `metainfo`         | 请求级别的元信息透传（trace、tag、user 等）                          |

### 2.2 架构分层

```
┌────────────────────────────────────────────────────────┐
│  业务层：用户实现的 gRPC Service / 调用方                 │
├────────────────────────────────────────────────────────┤
│  volo-grpc：ServiceBuilder、Client、Server、Status、Code │
├────────────────────────────────────────────────────────┤
│  volo：Service / Context / Layer / LoadBalance / Discover│
├────────────────────────────────────────────────────────┤
│  pilota：Protobuf 编解码、IDL 编译                        │
├────────────────────────────────────────────────────────┤
│  motore：中间件 Service 抽象                              │
├────────────────────────────────────────────────────────┤
│  tokio / hyper / h2：异步运行时 + HTTP/2                 │
└────────────────────────────────────────────────────────┘
```

### 2.3 与 gRPC（tonic）的区别

| 维度            | tonic（gRPC 官方）       | volo-grpc（字节）                                  |
| --------------- | ------------------------ | -------------------------------------------------- |
| 中间件抽象      | `tower::Service`         | `motore::Service`（基于 AFIT/RPITIT，无 Box）       |
| Service trait   | 必须用 `async_trait`     | 直接 `async fn`，零开销                             |
| 协议支持        | 仅 gRPC                  | Thrift + gRPC（共用核心抽象）                       |
| IDL 编译        | `tonic-build` + `protoc` | `volo-build` + `pilota`（纯 Rust，无外部依赖）      |
| 服务发现        | 需要手写 Resolver        | 内建 `Discover` trait + `LoadBalancer`              |
| 性能            | 优秀                     | 4C 下 35W QPS（官方数据）                          |

**结论**：如果你只在 Rust 内做 gRPC，tonic 够用；如果你需要**统一 gRPC + Thrift**、**更好的服务发现/负载均衡抽象**、**不依赖 protoc**，选 Volo。

---

## 三、核心概念速览

在写代码前，先记住下面这些名词，本教程会反复用到：

| 名词                       | 含义                                                                         |
| -------------------------- | ---------------------------------------------------------------------------- |
| **IDL**                    | Interface Definition Language，接口描述语言。常见 Protobuf / Thrift          |
| **Pilota**                 | 字节自研的 IDL 编译器和编解码库，纯 Rust 实现                                 |
| **Service Trait**          | 服务端要实现的 trait，由代码生成器从 IDL 产生                                |
| **Client / ClientBuilder** | 调用方使用的客户端，由代码生成器从 IDL 产生                                  |
| **Request / Response**     | volo-grpc 的请求/响应包装，包含 metadata、extensions                          |
| **Status / Code**          | gRPC 错误码（OK / InvalidArgument / NotFound 等）                            |
| **Context**                | volo 的请求上下文，承载 trace、超时、元信息                                  |
| **Endpoint**               | volo 的目标服务描述（含 service_name、address 等）                          |
| **Layer / Service**        | Motore 的中间件抽象（类似 Tower）                                            |
| **Discover**               | 服务发现 trait，根据 service_name 找出可用实例列表                          |
| **Instance**               | volo 的服务实例描述（address、weight、tags）                                |
| **LoadBalancer**           | 负载均衡器，从多个 Instance 中选一个                                         |
| **Change**                 | 服务实例变更事件（added/updated/removed）                                    |

这些概念对初学者很抽象，下面通过示例逐一展开。

---

## 四、Rust 异步前置知识

Volo 强烈依赖 Rust 异步生态。开始前请确认你熟悉：

### 4.1 `async` / `.await`

```rust
async fn hello() -> String {
    "hello".to_string()
}

#[tokio::main]
async fn main() {
    let s = hello().await;
    println!("{}", s);
}
```

### 4.2 Tokio 运行时

Volo 服务端默认使用 `tokio`。需要在 `main` 函数上标记 `#[volo::main]` 或 `#[tokio::main]`（Volo 提供自己的宏，会做更多初始化工作，例如注册全局 service_name）。

### 4.3 Trait 中的异步函数（AFIT）

从 Rust 1.75 起，可以直接在 trait 中使用 `async fn`，**不需要** `async_trait` 宏。Volo 的核心 trait 全部使用这种写法：

```rust
// Volo 风格
pub trait CmxServiceOrchestrator {
    async fn execute_service(
        &self,
        req: volo_grpc::Request<ExecuteServiceRequest>,
    ) -> Result<volo_grpc::Response<ExecuteServiceResponse>, volo_grpc::Status>;
}
```

> 注意：AFIT 有一些限制（不能 dyn 化等），但对 RPC 框架影响不大。

### 4.4 智能指针

需要熟练掌握 `Arc<T>`（多线程共享所有权）、`Mutex<T>` / `RwLock<T>`、channel（`tokio::mpsc` / `async_broadcast`）。

### 4.5 thiserror / anyhow

错误处理推荐用 `thiserror` 定义业务错误类型（cmx 项目的规范要求），用 `anyhow` 在 main / 初始化阶段简化传播。

---

## 五、环境准备

### 5.1 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
# Volo 需要较新的 Rust
rustc --version  # 建议 >= 1.80
```

### 5.2 安装 volo-cli（可选）

```bash
cargo install volo-cli
volo --help
```

> `volo-cli` 是个脚手架工具。**本项目（cmx）不使用 `volo init`**，而是自己手写 IDL + `build.rs` + `volo.yml`，因为这能更好地控制依赖和组织结构。本教程两种方式都讲。

### 5.3 在 workspace 中添加依赖（cmx 规范）

cmx 项目所有依赖都通过 workspace 集中管理。在 `Cargo.toml` 中加入（实际已在 cmx 配好）：

```toml
[workspace.dependencies]
volo = "0.12"
volo-build = "0.12"
volo-grpc = "0.12"
pilota = "0.13"
tokio = { version = "1.41", features = ["full"] }
```

子 crate 使用：

```toml
# crates/libs/cmx-rpc-gen/Cargo.toml
[build-dependencies]
volo-build = { workspace = true }

[dependencies]
volo-grpc = { workspace = true }
volo = { workspace = true }
pilota = { workspace = true }
```

> ⚠️ cmx 规范要求每个依赖上方必须有**单行注释**（不能分组注释），例如 `# RPC 框架`。

---

## 六、第一个 gRPC 服务

我们直接看 cmx 项目的 `cmx-rpc-gen`，它就是一个**最小可用的 Volo gRPC 服务**。

### 6.1 目录结构

```
crates/libs/cmx-rpc-gen/
├── Cargo.toml           # 包定义 + 依赖
├── build.rs             # 编译期代码生成入口
├── volo.yml             # 代码生成配置
├── idl/
│   └── cmx_service.proto    # Protobuf IDL
└── src/
    └── lib.rs           # 重导出生成代码
```

### 6.2 步骤 1：写 Protobuf IDL

`idl/cmx_service.proto`：

```protobuf
syntax = "proto3";

package cmx;

// 服务编排 gRPC 服务
service CmxServiceOrchestrator {
  // 执行服务编排
  rpc ExecuteService(ExecuteServiceRequest) returns (ExecuteServiceResponse);
  // 调用插件函数
  rpc CallFunction(CallFunctionRequest) returns (CallFunctionResponse);
}

message ExecuteServiceRequest {
  string service_key = 1;       // 服务标识
  string input = 2;             // 输入（JSON 字符串）
  bool include_steps = 3;
  bool debug = 4;
  optional string debug_node_id = 5;
  map<string, string> debug_params = 6;
}

message ExecuteServiceResponse {
  bool success = 1;
  optional string output = 2;   // 输出（JSON 字符串）
  // ...
}
```

要点：

- `syntax = "proto3"`：使用 Protobuf v3 语法。
- `package cmx`：包名，影响生成的 Rust 模块路径。
- `service` 块：定义 gRPC 接口，方法名首字母大写（Protobuf 规范）。
- `message` 块：定义请求/响应消息，字段编号（`= 1`）一旦确定不要再改。
- 复杂 JSON 数据用 `string` 字段传递（见后面 FAQ）。

### 6.3 步骤 2：写 `volo.yml`（代码生成配置）

```yaml
entries:
  proto:
    filename: cmx_service_orchestrator.rs   # 生成的文件名
    protocol: protobuf                     # 协议类型
    services:
      - idl:
          source: local                    # IDL 来自本地
          path: idl/cmx_service.proto      # proto 文件路径
          includes:
            - idl                          # import 搜索路径
```

字段解释：

- `entries`：可定义多个 entry（一个 entry 对应一个生成文件）。
- `filename`：生成的 Rust 文件名（在 OUT_DIR 中）。
- `protocol`：支持 `thrift` 和 `protobuf`。
- `services`：要包含的服务。
  - `idl.source`：`local`（本地文件）或 `git`（远程仓库）。
  - `idl.path`：相对 build.rs 的路径。
  - `idl.includes`：proto 中 `import` 时的搜索路径。

### 6.4 步骤 3：写 `build.rs`

```rust
fn main() {
    volo_build::Builder::protobuf().write().unwrap();
}
```

`Builder::protobuf()` 等价于 `Builder::default()`，因为 volo-build 默认就支持 Protobuf。`.write()` 会读取 `volo.yml`，生成代码到 `OUT_DIR`。

> `OUT_DIR` 是 Cargo 在编译时设置的环境变量，指向 `target/debug/build/<crate>-<hash>/out/`。

### 6.5 步骤 4：重导出生成代码

`src/lib.rs`：

```rust
//! cmx-rpc-gen — volo-build 生成的 gRPC 代码重导出

pub mod cmx {
    pub mod cmx_service_orchestrator {
        include!(concat!(env!("OUT_DIR"), "/cmx_service_orchestrator.rs"));
    }
}
```

这段代码做了三件事：

1. `pub mod cmx` 对应 proto 的 `package cmx;`。
2. `pub mod cmx_service_orchestrator` 对应生成的文件名（`volo.yml` 的 `filename`）。
3. `include!` 是宏，在编译时把 OUT_DIR 中的生成文件原样嵌入。

### 6.6 步骤 5：编译

```bash
cargo build -p cmx-rpc-gen
```

Cargo 会：

1. 编译 `build.rs`。
2. `build.rs` 调用 `volo_build`，生成 `target/debug/build/cmx-rpc-gen-xxx/out/cmx_service_orchestrator.rs`。
3. 编译 `lib.rs` 时通过 `include!` 把生成文件嵌入。
4. 编译 `cmx-service-rpc`，后者 `use` 上面这些模块。

### 6.7 生成代码长什么样？

虽然不需要细读，但你应该大致了解生成代码的结构（volo-grpc 0.12 风格）：

```rust
// 自动生成于 OUT_DIR
pub mod cmx_service_orchestrator {
    // --- 消息类型 ---
    #[derive(Debug, Clone, PartialEq)]
    pub struct ExecuteServiceRequest {
        pub service_key: FastStr,
        pub input: FastStr,
        pub include_steps: bool,
        // ...
    }

    // --- 服务端 Trait ---
    #[volo::service]
    pub trait CmxServiceOrchestrator {
        async fn execute_service(
            &self,
            __request: Request<ExecuteServiceRequest>,
        ) -> Result<Response<ExecuteServiceResponse>, Status>;

        async fn call_function(
            &self,
            __request: Request<CallFunctionRequest>,
        ) -> Result<Response<CallFunctionResponse>, Status>;
    }

    // --- 客户端 ---
    pub struct CmxServiceOrchestratorClient { /* ... */ }
    pub struct CmxServiceOrchestratorClientBuilder { /* ... */ }

    // --- 服务端桩 ---
    pub struct CmxServiceOrchestratorServer<S> { inner: S }

    // --- 编译期元类型 ---
    pub struct CmxServiceOrchestratorRequestRecv;
    pub struct CmxServiceOrchestratorResponseSend;
}
```

关键点：

- **消息**用普通 `struct` 表示（不是 `pub struct Foo { ... }` 加 `#[derive]` 即可）。
- **服务端 trait** 用 `#[volo::service]` 宏修饰，可以直接 `async fn`。
- **客户端**自带 `*ClientBuilder`，配置地址、超时、middleware 等。
- **服务端桩** `*Server<S>` 用于 `ServiceBuilder::new(Server::new(impl))`。

> 字符串字段的类型是 `pilota::FastStr`，可以认为等价于 `Arc<str>`（带小字符串优化，零拷贝）。

---

## 七、IDL（Protobuf）编写规范

### 7.1 字段编号

```protobuf
int64 id = 1;        // 编号一旦确定不能改
string name = 2;
```

新增字段必须使用**未使用的编号**，删除字段后编号**不要再复用**（兼容性考虑）。

### 7.2 字段类型映射

| Protobuf 类型 | Rust 类型（volo-grpc 0.12）                  |
| ------------- | -------------------------------------------- |
| `int32`       | `i32`                                        |
| `int64`       | `i64`                                        |
| `uint32`      | `u32`                                        |
| `uint64`      | `u64`                                        |
| `bool`        | `bool`                                       |
| `string`      | `pilota::FastStr`（构造时可传 `String`）     |
| `bytes`       | `bytes::Bytes`                               |
| `map<K,V>`    | `pilota::AHashMap<K, V>`                     |
| `repeated T`  | `Vec<T>`                                     |
| `optional T`  | `Option<T>`                                  |
| `enum`        | 整数常量（不生成 Rust enum 除非指定插件）    |

### 7.3 复杂 JSON 怎么传？

Protobuf 缺少原生 JSON 值类型，所以复杂结构常用 `string` + JSON 字符串：

```protobuf
message CallServiceRequest {
  string input = 1;  // 客户端用 to_string() 序列化 JSON，服务端用 from_str() 反序列化
}
```

这是 **cmx-service-rpc 项目的实际做法**。优点是简单、跨语言；缺点是没有类型校验。

### 7.4 注释

`//` 单行注释会**保留到生成代码**中，作为生成的 Rust 结构体的文档注释。cmx-service-rpc 的 IDL 全部写了方法注释，对应 HTTP API。

---

## 八、volo-build 代码生成机制

### 8.1 工作流

```
volo.yml  ──┐
            ├─→  volo_build::Builder ──→ pilota-build ──→ OUT_DIR/*.rs
idl/*.proto──┘
```

`volo-build` 内部调用 `pilota-build`，后者做两件事：

1. **解析 IDL**：把 `.proto` 解析成内部 IR（中间表示）。
2. **生成 Rust 代码**：根据 IR 生成对应的消息结构体、Service trait、Client/Server。

### 8.2 何时重新生成？

Cargo 检测到 `build.rs`、`volo.yml`、IDL 文件变更时，会重新执行 `build.rs`。也就是说**修改 IDL 后只需重新 `cargo build`**，不需要手动跑任何命令。

### 8.3 增量编译

如果 IDL 很大，编译时间会变长。可以通过 `volo.yml` 的 `dedups` 字段去重。

### 8.4 不使用 `volo-cli` 的好处

- 不需要全局安装。
- 不需要 `volo init` 拉一堆模板。
- 完全掌控目录结构。
- 与 cmx 的代码风格保持一致。

### 8.5 在子 crate 中使用

`cmx-service-rpc` 直接 `use cmx_rpc_gen::cmx::cmx_service_orchestrator::*;` 即可，就像普通模块一样。

---

## 九、服务端开发详解

### 9.1 最简服务端

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::*;
use volo_grpc::server::ServiceBuilder;

pub struct MyService;

impl CmxServiceOrchestrator for MyService {
    async fn execute_service(
        &self,
        req: volo_grpc::Request<ExecuteServiceRequest>,
    ) -> Result<volo_grpc::Response<ExecuteServiceResponse>, volo_grpc::Status> {
        let inner = req.into_inner();
        tracing::info!("service_key = {}", inner.service_key);

        let resp = ExecuteServiceResponse {
            success: true,
            output: Some(r#"{"result":"ok"}"#.into()),
            steps: vec![],
            total_elapsed_us: 100,
            error: None,
        };
        Ok(volo_grpc::Response::new(resp))
    }

    async fn call_function(
        &self,
        req: volo_grpc::Request<CallFunctionRequest>,
    ) -> Result<volo_grpc::Response<CallFunctionResponse>, volo_grpc::Status> {
        let _ = req.into_inner();
        Ok(volo_grpc::Response::new(CallFunctionResponse::default()))
    }
}
```

要点：

- `impl` 自动生成的 trait，**方法签名不能改**。
- 返回 `volo_grpc::Status` 表示**框架层错误**（连接、协议、参数无效等）。**业务错误**通常不放在 `Status` 里，而是放在响应体的字段（如 `success: false`、`error: Some(...)`），这样 gRPC 连接不会中断。
- 业务逻辑用 `tracing` 记录日志。

### 9.2 cmx 风格：注入业务依赖

参考 `cmx-service-rpc/src/server.rs` 的 `CmxOrchestratorServiceImpl`：

```rust
pub struct CmxOrchestratorServiceImpl {
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
}

impl CmxOrchestratorServiceImpl {
    pub fn new(
        service_invoker: Arc<dyn ServiceInvoker>,
        runtime_invoker: Arc<dyn RuntimeInvoker>,
    ) -> Self { ... }
}
```

通过 `Arc<dyn Trait>` 注入依赖，避免与具体实现耦合，便于测试和替换。

### 9.3 启动服务

参考 `cmx-service-rpc/src/server_runner.rs`：

```rust
use volo::net::Address;
use volo_grpc::server::ServiceBuilder;

pub async fn start_grpc_server(
    port: u16,
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
) -> Result<(), RpcFrameworkError> {
    let addr: std::net::SocketAddr = format!("[::]:{port}").parse()?;

    let service_impl = CmxOrchestratorServiceImpl::new(service_invoker, runtime_invoker);

    // 1. 把服务实现包装成 volo Service
    let service = ServiceBuilder::new(CmxServiceOrchestratorServer::new(service_impl))
        .build::<CmxServiceOrchestratorRequestRecv, CmxServiceOrchestratorResponseSend>();

    // 2. 添加到 Server 并启动
    volo_grpc::server::Server::new()
        .add_service(service)
        .run(Address::Ip(addr))
        .await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    Ok(())
}
```

关键步骤：

1. `ServiceBuilder::new(...).build::<Recv, Send>()`：把 trait impl 包成 volo Service。`Recv` 和 `Send` 是生成代码中的元类型，告诉 volo 用什么 codec 收发消息。
2. `Server::new().add_service(service)`：把 Service 添加到 gRPC Server。
3. `.run(addr)`：绑定地址并运行（实际上内部就是 `hyper::Server`）。

### 9.4 在 main 中启动

参考 `web-server/src/config/rpc.rs`：

```rust
let grpc_port = 9090;
tokio::spawn(async move {
    start_grpc_server(grpc_port, service_invoker, runtime_invoker).await
});
```

注意：

- `start_grpc_server` 是**阻塞**的（`.run().await`），通常用 `tokio::spawn` 放到后台。
- 同一进程内 HTTP（axum）和 gRPC 可以**同时运行**，互不干扰。

### 9.5 优雅停机

Volo 的 `Server::run` 内部监听 SIGINT/SIGTERM，收到信号后等待正在处理的请求完成再退出。生产环境建议配合 Kubernetes 的 preStop hook。

---

## 十、客户端开发详解

### 10.1 最简客户端

```rust
use cmx_rpc_gen::cmx::cmx_service_orchestrator::*;
use std::net::SocketAddr;
use volo_grpc::Request;

#[volo::main]
async fn main() {
    let addr: SocketAddr = "[::1]:9090".parse().unwrap();

    // 静态 client（生产环境推荐 lazy_static 或 OnceLock）
    let client = CmxServiceOrchestratorClientBuilder::new("cmx-orchestrator")
        .address(addr)
        .build();

    let req = ExecuteServiceRequest {
        service_key: "my-service".into(),
        input: r#"{"key":"value"}"#.into(),
        include_steps: true,
        debug: false,
        ..Default::default()
    };

    let resp = client.execute_service(Request::new(req)).await;
    match resp {
        Ok(r) => tracing::info!("success = {}", r.into_inner().success),
        Err(e) => tracing::error!("err = {}", e),
    }
}
```

要点：

- `*ClientBuilder::new("service-name")`：`service-name` 是 volo 服务发现的 key，注册中心按它查询实例。
- `.address(addr)`：直连模式，绕过服务发现（适合开发调试）。
- `.build()`：构造真正的 `Client`。
- 客户端的每个 RPC 方法返回 `Future<Result<Response<T>, Status>>`。
- `Request::new(payload)` 包装请求（也可以用 `Request::new_with_metadata`）。

### 10.2 动态创建客户端

如果目标服务名是变量（cmx 的实际场景），参考 `cmx-service-rpc/src/client.rs`：

```rust
async fn get_client(&self, service_name: &str)
    -> Result<CmxServiceOrchestratorClient, RpcError>
{
    // 1. 从注册中心获取实例
    let instances = self.cache.get(service_name).ok_or(...)?;
    if instances.is_empty() { return Err(...); }

    // 2. 创建自定义 Discover
    let discover = RegistryAwareDiscover::new(self.cache.clone());
    discover.start_watch(service_name);

    // 3. 通过 Discover 构建客户端
    let client = CmxServiceOrchestratorClientBuilder::new(service_name)
        .discover(discover)
        .build();

    Ok(client)
}
```

### 10.3 客户端配置项

`ClientBuilder` 提供的方法（volo-grpc 0.12）：

| 方法                                              | 说明                                       |
| ------------------------------------------------- | ------------------------------------------ |
| `new(service_name)`                               | 必填，service name（用于 discover / 日志） |
| `address(addr)`                                   | 直连地址，跳过服务发现                     |
| `discover(d)`                                     | 自定义服务发现                             |
| `load_balance(lb)`                                | 自定义负载均衡器                           |
| `http2_config(cfg)`                               | HTTP/2 连接参数（keep-alive、窗口等）     |
| `timeout(d)`                                      | 全局超时                                   |
| `send_compress(true)` / `accept_compress(true)`   | 启用 gzip 压缩                             |
| `max_decoding_message_size(n)` / `max_encoding_message_size(n)` | 消息体上限                  |
| `layer(layer)`                                    | 添加中间件                                 |
| `build()`                                         | 构造最终客户端                             |

### 10.4 callopt：单次调用配置

volo-grpc 提供了 `CallOpt`，可以在**单次调用**时覆盖默认设置：

```rust
use volo_grpc::client::CallOpt;
use std::time::Duration;

client.execute_service_with_callopt(
    req,
    CallOpt::default()
        .with_timeout(Duration::from_secs(2)),
).await;
```

但生成代码中的方法名是 `execute_service`（不带 `_with_callopt`），如需自定义，可以直接在生成的 trait 上扩展。

### 10.5 cmx 的全局客户端

cmx-service-rpc 把"通过 service_name 找实例 + 超时控制 + 重试"封装成了 `RpcClient` trait，业务层完全不感知 volo 的存在：

```rust
use cmx_service_rpc::grpc::GlobalRpcClient;
use cmx_traits::RpcClient;

let client = GlobalRpcClient::get();
let resp = client.call_service(
    "cmx-orchestrator",            // service_name
    "my-service",                  // service_key
    serde_json::json!({"a": 1}),   // input
    Default::default(),
).await?;
```

详见 `cmx-service-rpc/src/factory.rs` 和 `cmx-service-rpc/src/global.rs`。

---

## 十一、服务发现（Discover）

### 11.1 Discover Trait

volo 的服务发现抽象非常简单：

```rust
pub trait Discover: Send + Sync + 'static {
    type Key: Hash + Eq + Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    fn discover<'s>(
        &'s self,
        endpoint: &'s Endpoint,
    ) -> impl Future<Output = Result<Vec<Arc<Instance>>, Self::Error>> + Send;

    fn key(&self, endpoint: &Endpoint) -> Self::Key;

    fn watch(&self, keys: Option<&[Self::Key]>) -> Option<Receiver<Change<Self::Key>>>;
}
```

- `discover(endpoint)`：根据 endpoint（带 service_name）查出当前所有实例。
- `watch(keys)`：返回变更通知的接收端，volo 内部用它驱动 LoadBalancer 更新。
- `key(endpoint)`：endpoint 的 key（一般就是 service_name）。

### 11.2 cmx 的实现：`RegistryAwareDiscover`

`cmx-service-rpc/src/discover.rs` 把 `ServiceInstanceCache`（来自 `cmx-registry-config`）桥接到 volo 的 Discover。它的核心在于**算 diff**——把新旧实例列表对比，精确地告诉 volo "哪些新增、哪些删除、哪些变化"。

```rust
pub struct RegistryAwareDiscover {
    cache: Arc<ServiceInstanceCache>,
    change_tx: Sender<Change<FastStr>>,
    /// 实例变更通知接收端（watch 时克隆共享）
    change_rx: RwLock<Option<Receiver<Change<FastStr>>>>,
}

impl Clone for RegistryAwareDiscover {
    fn clone(&self) -> Self {
        Self {
            cache: self.cache.clone(),
            change_tx: self.change_tx.clone(),
            // 克隆时把 receiver 共享（多个 watch 可以共享同一个 rx）
            change_rx: RwLock::new(
                self.change_rx.read().expect("change_rx 锁中毒").as_ref().cloned()
            ),
        }
    }
}

impl RegistryAwareDiscover {
    pub fn new(cache: Arc<ServiceInstanceCache>) -> Self { ... }

    pub fn start_watch(&self, service_name: &str) {
        let tx = self.change_tx.clone();
        let service_name = service_name.to_string();
        let cache_for_closure = self.cache.clone();

        // 注册回调到 ServiceInstanceCache
        self.cache.subscribe(
            &service_name,
            Arc::new(move |svc_name, new_instances| {
                let new_volo = instances_to_volo(new_instances);

                // 1. 拉取旧实例（来自缓存里的上一份）
                let old_volo = cache_for_closure.get(svc_name)
                    .map(|old| instances_to_volo(&old))
                    .unwrap_or_default();

                // 2. 按 address 算 diff
                let old_addrs: HashSet<_> = old_volo.iter().map(|i| i.address.clone()).collect();
                let new_addrs: HashSet<_> = new_volo.iter().map(|i| i.address.clone()).collect();

                let added: Vec<_> = new_volo.iter()
                    .filter(|i| !old_addrs.contains(&i.address))
                    .cloned().collect();
                let removed: Vec<_> = old_volo.iter()
                    .filter(|i| !new_addrs.contains(&i.address))
                    .cloned().collect();
                // updated: 地址相同但 weight/tags 变化的实例
                let updated: Vec<_> = new_volo.iter()
                    .filter(|new_i| {
                        old_addrs.contains(&new_i.address) && old_volo.iter().any(|old_i| {
                            old_i.address == new_i.address &&
                            (old_i.weight != new_i.weight || old_i.tags != new_i.tags)
                        })
                    })
                    .cloned().collect();

                // 3. 广播带 diff 的 Change
                let change = Change {
                    key: FastStr::new(svc_name),
                    all: new_volo,
                    added,
                    updated,
                    removed,
                };
                if let Err(e) = tx.try_broadcast(change) {
                    tracing::warn!(error = %e, "实例变更广播失败: 通道已满或无接收者");
                }
            }),
        );
    }
}

fn instances_to_volo(instances: &[ServiceInstance]) -> Vec<Arc<Instance>> {
    instances.iter().filter_map(|i| {
        let addr: std::net::SocketAddr = match format!("{}:{}", i.ip, i.port).parse() {
            Ok(a) => a,
            Err(e) => {
                // 地址解析失败时打 warn 日志，不静默丢弃
                tracing::warn!(
                    service_name = %i.service_name, ip = %i.ip, port = i.port,
                    error = %e, "跳过地址解析失败的实例"
                );
                return None;
            }
        };
        Some(Arc::new(Instance {
            address: Address::Ip(addr),
            weight: (i.weight * 100.0) as u32,
            tags: i.metadata.iter()
                .map(|(k, v)| (Cow::Owned(k.clone()), Cow::Owned(v.clone())))
                .collect(),
        }))
    }).collect()
}

impl Discover for RegistryAwareDiscover {
    type Key = FastStr;
    type Error = LoadBalanceError;

    async fn discover(&self, endpoint: &Endpoint)
        -> Result<Vec<Arc<Instance>>, Self::Error>
    {
        let service_name = endpoint.service_name_ref().to_string();
        match self.cache.get(&service_name) {
            Some(instances) if !instances.is_empty() => Ok(instances_to_volo(&instances)),
            _ => {
                // 缓存为空时返回错误，让 volo 走错误处理路径
                // （volo-grpc 会映射为 Status::Unavailable）
                Err(LoadBalanceError::Discover(
                    format!("service not found in cache: {}", service_name).into(),
                ))
            }
        }
    }

    fn key(&self, endpoint: &Endpoint) -> Self::Key {
        endpoint.service_name()
    }

    fn watch(&self, _keys: Option<&[Self::Key]>) -> Option<Receiver<Change<Self::Key>>> {
        // 每次 watch 返回 rx 的克隆，允许多个 volo 组件同时订阅
        self.change_rx.read().expect("change_rx 锁中毒")
            .as_ref().map(|rx| rx.clone())
    }
}
```

设计要点：

- **从 `ServiceInstanceCache` 读**：`cache.get()` 是纯内存操作。
- **变更通知用 `async-broadcast` 通道**：`change_tx` 发送给 volo，`change_rx` 让 volo 订阅。
- **`start_watch` 注册回调**：当注册中心（Nacos / Mock）推过来新实例列表时，cmx 把它转换为 volo 的 `Change` 并广播。
- **diff 计算**：`start_watch` 内做 added / updated / removed 分类，volo LoadBalancer 收到后能精确更新内部状态（添加新连接、关闭失效连接、刷新 metadata）。
- **discover 返回错误**：缓存为空时返回 `LoadBalanceError::Discover`，避免 volo 在空列表上"看似成功"地选不到实例。`change_rx` 通过 `clone()` 共享，允许多次 watch。
- **地址解析失败可见化**：`instances_to_volo` 会用 `tracing::warn!` 记录跳过的实例，方便排查。

### 11.3 自定义 Discover

如果你接的是 Consul、etcd、Kubernetes DNS，只需要实现 Discover trait 的三个方法：

```rust
pub struct MyConsulDiscover { client: ConsulClient }

impl Discover for MyConsulDiscover {
    type Key = FastStr;
    type Error = LoadBalanceError;

    async fn discover(&self, endpoint: &Endpoint)
        -> Result<Vec<Arc<Instance>>, Self::Error>
    {
        let svc = endpoint.service_name_ref();
        let nodes = self.client.catalog_service(svc).await
            .map_err(|e| LoadBalanceError::Discover(Box::new(e)))?;

        Ok(nodes.into_iter().map(|n| Arc::new(Instance {
            address: Address::Ip(format!("{}:{}", n.address, n.service_port).parse().unwrap()),
            weight: 100,
            tags: Default::default(),
        })).collect())
    }

    fn key(&self, endpoint: &Endpoint) -> Self::Key { endpoint.service_name() }

    fn watch(&self, _keys: Option<&[Self::Key]>) -> Option<Receiver<Change<Self::Key>>> {
        None  // 不实现 watch 也行，volo 会轮询 discover()
    }
}
```

### 11.4 直连（无注册中心）

开发环境可以用 `.address()` 直连，跳过服务发现：

```rust
let client = CmxServiceOrchestratorClientBuilder::new("cmx-orchestrator")
    .address("[::1]:9090".parse().unwrap())
    .build();
```

---

## 十二、负载均衡

### 12.1 默认负载均衡

volo 的 `RandomLoadBalance` 是默认实现（随机 + 权重）。`PickFirstLoadBalance`（总是选第一个）也常用。

### 12.2 自定义负载均衡器

volo 提供 `LoadBalance` trait：

```rust
pub trait LoadBalance: Send + Sync + 'static {
    fn get_picker(
        &self,
        endpoint: &Endpoint,
        instances: Arc<Vec<Arc<Instance>>>,
    ) -> Arc<dyn Picker>;
}

pub trait Picker: Send + Sync + 'static {
    fn pick(&self, req: MaybeRequest) -> Option<Arc<Instance>>;
    fn picker_info(&self) -> PickerInfo;
}
```

实现 `LoadBalance` 和 `Picker` 即可自定义策略（轮询、最小连接、一致性哈希等）。

### 12.3 替换负载均衡

```rust
use volo::loadbalance::random::RandomLoadBalance;

let client = CmxServiceOrchestratorClientBuilder::new("cmx-orchestrator")
    .load_balance(RandomLoadBalance::new())
    .discover(my_discover)
    .build();
```

### 12.4 cmx 怎么做

cmx-service-rpc **不显式指定** 负载均衡，使用 volo 默认（随机+权重）。后续可改为一致性哈希（按 service_key 分片）或金丝雀（按 tag 路由）。

---

## 十三、中间件（Motore / Service）

### 13.1 概念

Volo 的中间件抽象叫 **Layer + Service**，来自 `motore` crate。Layer 包裹 Service，Service 处理请求：

```rust
// 类似 Tower：Layer 产生新的 Service
pub trait Layer<S> {
    type Service;
    fn layer(self, inner: S) -> Self::Service;
}
```

中间件可用于：日志、Trace、超时、限流、熔断、重试、权限、压缩等。

### 13.2 客户端中间件

```rust
use motore::Service;
use volo::context::Context;

pub struct LogLayer;

impl<S> volo::Layer<S> for LogLayer {
    type Service = LogService<S>;
    fn layer(self, inner: S) -> Self::Service {
        LogService { inner }
    }
}

pub struct LogService<S> { inner: S }

impl<S, Cx, Request> Service<Cx, Request> for LogService<S>
where
    S: Service<Cx, Request> + Send + Sync + 'static,
    Cx: Context + Send + Sync + 'static,
    Request: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;

    async fn call(&self, cx: &mut Cx, req: Request) -> Result<Self::Response, Self::Error> {
        let start = std::time::Instant::now();
        let result = self.inner.call(cx, req).await;
        let elapsed = start.elapsed();
        tracing::info!(?elapsed, "rpc call");
        result
    }
}
```

使用：

```rust
let client = CmxServiceOrchestratorClientBuilder::new("svc")
    .layer(LogLayer)
    .build();
```

### 13.3 服务端中间件

```rust
let service = ServiceBuilder::new(impl)
    .layer(LogLayer)
    .build::<Recv, Send>();
```

### 13.4 cmx 中的中间件

cmx-service-rpc **没有用 volo 的 `Layer` 体系**做重试/超时，而是在 `VoloGrpcClient` 内部**手动实现**了：

- **重试 + 指数退避**：见 §14.3，循环 + `is_retryable_error` + `retry_backoff`。
- **总时间预算**：见 §14.2，`deadline` 机制。
- **追踪/日志**：服务端用 `#[instrument(target = "cmx_rpc", name = "grpc_execute_service")]` 自动生成 span；客户端用 `#[instrument(...)]` + `tracing::info!` 记录耗时/重试。

为什么不直接用 volo 的 `RetryLayer`：

- volo 的 `RetryLayer` 不知道 cmx 的"总预算"约束。
- cmx 的重试与 telemetry（`tracing`）能深度集成，每个 attempt 都打日志。
- 自实现更便于控制"哪些错误可重试"。

如果需要分布式 Trace，可以在 `*ClientBuilder` 上加 `tracing_layer`（volo-grpc 自带）或自实现 `Layer`。

---

## 十四、超时、重试、连接池

cmx-service-rpc 在这一块的实现已经从最初版本迭代到**带重试预算、指数退避、客户端缓存、缓存穿透主动拉取**的生产级实现。下面按四个子主题分别讲。

### 14.1 三种超时：RPC 超时、连接超时、总预算

cmx-service-rpc 使用 **volo 原生的双超时配置**：

```rust
let rpc_timeout = Duration::from_millis(self.config.timeout_ms);
let connect_timeout = Duration::from_millis(self.config.connect_timeout_ms);

let client = CmxServiceOrchestratorClientBuilder::new(service_name)
    .discover(discover.clone())
    .rpc_timeout(Some(rpc_timeout))        // 单次 RPC 调用的超时
    .connect_timeout(connect_timeout)      // 建立连接的超时
    .build();
```

| 字段                  | 默认值 | 说明                                                                 |
| --------------------- | ------ | -------------------------------------------------------------------- |
| `timeout_ms`          | 5000   | 单次 gRPC 调用的总耗时上限（含重试预算）。volo 内部通过 `metainfo` 传递到 gRPC 层。|
| `connect_timeout_ms`  | 3000   | 与服务实例建立 HTTP/2 连接的超时。                                    |

### 14.2 总时间预算（含重试）

cmx-service-rpc 的客户端**不是简单的"超时一次"**，而是把整次调用视为一个**总预算**：

```rust
let start = std::time::Instant::now();
let total_budget = Duration::from_millis(self.config.timeout_ms);
let deadline = start + total_budget;  // 总预算截止时间
```

后续每次重试前都会做两件事：

1. 计算剩余预算 `remaining = deadline - now`。
2. 如果 `remaining` 已耗尽 → 立即返回 `RpcError::Timeout("重试预算耗尽: ...")`，不再发起新请求。
3. 如果还有预算 → 在重试前先退避 `min(retry_backoff(attempt), remaining)`。

这样能保证"重试不会让总耗时超过 `timeout_ms`"。

### 14.3 重试：指数退避 + 错误白名单

`VoloGrpcClient` 内置重试逻辑，配置来自 `GrpcConfig.retry_count`（默认 0 即不重试）：

```rust
let max_retries = self.config.retry_count;

for attempt in 0..=max_retries {
    // 1. 检查预算
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() { return Err(Timeout); }
    if attempt > 0 {
        tokio::time::sleep(min(retry_backoff(attempt - 1), remaining)).await;
    }
    // 2. 发起调用
    match client.execute_service(req).await {
        Ok(resp) => return Ok(...),
        Err(e) => {
            // 3. 只有可重试错误才会进下一轮
            if Self::is_retryable_error(&e) && attempt < max_retries {
                continue;
            }
            return Err(RpcError::RpcCallFailed(e.to_string()));
        }
    }
}
```

**退避序列**（`Self::retry_backoff`）：

```rust
fn retry_backoff(attempt: usize) -> Duration {
    let backoff_ms = 50u64.saturating_mul(1u64 << attempt.min(4));
    Duration::from_millis(backoff_ms.min(800))
}
```

- 退避序列：50ms → 100ms → 200ms → 400ms → 800ms（上限）。
- `attempt.min(4)` 防止位移溢出。
- `backoff_ms.min(800)` 防止超出上限。

**可重试错误白名单**（`is_retryable_error`）：

| gRPC Code           | 含义           | 是否重试 |
| ------------------- | -------------- | -------- |
| `UNAVAILABLE`       | 服务不可达     | ✅       |
| `DEADLINE_EXCEEDED` | 调用超时       | ✅       |
| `RESOURCE_EXHAUSTED`| 限流           | ✅       |
| `ABORTED`           | 事务中止       | ✅       |
| `INVALID_ARGUMENT`  | 参数无效       | ❌       |
| `NOT_FOUND`         | 资源不存在     | ❌       |
| `PERMISSION_DENIED` | 权限不足       | ❌       |
| 其他业务错误        | 业务逻辑错误   | ❌       |

### 14.4 配置示例

```toml
[rpc.grpc]
port = 9090
timeout_ms = 5000              # 单次调用上限 5s，含所有重试
connect_timeout_ms = 3000      # 建连上限 3s
retry_count = 2                # 最多重试 2 次（总共 3 次尝试）
default_group = "DEFAULT_GROUP"          # 注册中心查询分组
default_clusters = ["cmx-cluster"]       # 集群过滤
```

### 14.5 客户端缓存：避免重复创建

`VoloGrpcClient` 内部维护 `RwLock<HashMap<service_name, CachedClient>>`：

```rust
pub struct VoloGrpcClient {
    cache: Arc<ServiceInstanceCache>,
    config: GrpcConfig,
    registry: Arc<dyn ServiceRegistry>,
    clients: RwLock<HashMap<String, CachedClient>>,
}

struct CachedClient {
    client: CmxServiceOrchestratorClient,
    _discover: RegistryAwareDiscover,  // 保活 discover 防止 channel 释放
}
```

`get_client` 使用 **double-check locking** 模式：

```rust
async fn get_client(&self, service_name: &str) -> Result<..., RpcError> {
    // 1. 快查：读锁
    if let Some(cached) = self.clients.read().await.get(service_name) {
        return Ok(cached.client.clone());
    }
    // 2. 慢路径：写锁 + 再次检查
    let mut clients = self.clients.write().await;
    if let Some(cached) = clients.get(service_name) {
        return Ok(cached.client.clone());
    }
    // 3. 真正创建
    ...
    clients.insert(service_name.to_string(), cached);
    Ok(client)
}
```

设计要点：

- **同一 service_name 多次调用共享一个 volo Client**（volo Client 内部复用 HTTP/2 连接）。
- **写锁内完成创建**：并发请求不会出现"两个线程都创建了 client"的竞争。
- **CachedClient 里持有 `_discover`**：防止 `discover` 在 `client` 存活期间被 drop。

### 14.6 缓存穿透：主动拉取实例

如果 `get_client` 时 `ServiceInstanceCache` 为空（**冷启动**或**新服务**），会主动调一次 `query_instances` 拉取：

```rust
if self.cache.get(service_name).map_or(true, |v| v.is_empty()) {
    let instances = self.registry.query_instances(
        service_name,
        self.config.default_group.as_deref(),
        self.config.default_clusters.clone(),
    ).await.map_err(|e| RpcError::NoAvailableInstance(...))?;
    self.cache.update(service_name, instances);

    if self.cache.get(service_name).map_or(true, |v| v.is_empty()) {
        return Err(RpcError::NoAvailableInstance(service_name.to_string()));
    }
}
```

这避免了"必须先有预热才能调用"的限制。

### 14.7 volo 原生重试中间件（备选）

除了 cmx 内置的重试，也可以用 volo 的 `RetryLayer`：

```rust
use volo::client::retry::RetryLayer;

let client = CmxServiceOrchestratorClientBuilder::new("svc")
    .layer(RetryLayer::new(...))
    .build();
```

但 cmx 没采用这种方式，原因是：

- volo 的 RetryLayer 不知道 cmx 的"总预算"约束。
- cmx 的重试与 telemetry（`tracing`）能深度集成。

### 14.8 连接池

volo-grpc 内部为每个服务实例维护一个**多路复用**的 HTTP/2 连接（不是为每个请求建连接）。cmx 没有自定义连接池配置，使用 volo 默认行为（`Http2Config` 默认参数）。

如果需要调优窗口、keep-alive：

```rust
use volo_grpc::client::Http2Config;

let client = CmxServiceOrchestratorClientBuilder::new("svc")
    .http2_config(Http2Config {
        keep_alive_interval: Some(Duration::from_secs(30)),
        keep_alive_timeout: Some(Duration::from_secs(5)),
        ..Default::default()
    })
    .build();
```

### 14.9 压缩

```rust
let client = CmxServiceOrchestratorClientBuilder::new("svc")
    .send_compress(true)   // 启用 gzip 压缩请求体
    .accept_compress(true) // 接受 gzip 压缩的响应体
    .build();
```

---

## 十五、错误处理

### 15.1 框架层错误：Status

`volo_grpc::Status` 类似 tonic 的 Status：

```rust
pub struct Status {
    code: Code,
    message: String,
    details: Option<Bytes>,
    metadata: MetadataMap,
}

pub enum Code {
    Ok = 0,
    InvalidArgument = 3,
    NotFound = 5,
    Internal = 13,
    Unauthenticated = 16,
    // ...
}
```

构造方式：

```rust
volo_grpc::Status::new(volo_grpc::Code::InvalidArgument, "输入 JSON 解析失败")
```

注意：**业务错误不要用 Status 返回**（会中断 gRPC 流）。cmx-service-rpc 服务端在 `error` 字段返回业务错误：

```rust
let mut pb_resp = ExecuteServiceResponse::default();
pb_resp.success = false;
pb_resp.error = Some(OrchestrationError { message: "服务不存在".into() });
Ok(volo_grpc::Response::new(pb_resp))
```

### 15.2 业务层错误：thiserror

cmx 项目规范要求**所有自定义 Error 使用 `thiserror`**。例如 `cmx-service-rpc/src/error.rs`：

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RpcFrameworkError {
    #[error("gRPC 服务启动失败: {0}")]
    ServerStartFailed(String),
    #[error("注册中心未初始化")]
    RegistryNotInitialized,
    #[error("服务发现失败: {0}")]
    DiscoveryFailed(String),
}
```

注意 `cmx-traits/src/rpc_client.rs` 中的 `RpcError` 也有类似定义。两者的区别是：

- `RpcFrameworkError`：RPC 框架内部错误（启动、初始化）。
- `RpcError`：RPC **调用**错误（超时、连接失败、协议不支持）。

### 15.3 错误传播

```rust
fn load_config() -> Result<Config, std::io::Error> { ... }

fn start_server() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_config()?;                  // io::Error → Box<dyn Error>
    start_grpc_server(cfg.port, ...).await?;  // RpcFrameworkError → Box<dyn Error>
    Ok(())
}
```

复杂项目可以分多个错误类型，用 `#[from]` 自动转换：

```rust
#[derive(Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Rpc(#[from] RpcFrameworkError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

---

## 十六、cmx-rpc-gen 实战解读

`crates/libs/cmx-rpc-gen/` 整个 crate 几乎没写任何业务代码，它的价值在于**用 volo-build 把 IDL 转成 Rust 模块**。

### 16.1 Cargo.toml

```toml
[package]
name = "cmx-rpc-gen"
version.workspace = true
edition.workspace = true

[build-dependencies]
# RPC 框架代码生成
volo-build = { workspace = true }

[dependencies]
# gRPC 框架
volo-grpc = { workspace = true }
# RPC 框架
volo = { workspace = true }
# IDL 编译框架
pilota = { workspace = true }
```

为什么 `volo-grpc`/`volo`/`pilota` 也要在 `[dependencies]` 里？——因为生成代码的 `include!` 会引用 `pilota::FastStr`、`volo_grpc::Request` 等类型。

### 16.2 为什么需要独立的 `cmx-rpc-gen` crate？

**避免重复生成代码**。如果有多个 crate 需要 `CmxServiceOrchestrator` 的消息类型：

- ❌ 每个 crate 各自 build.rs 跑一次 volo-build。
- ✅ 在 `cmx-rpc-gen` 中生成一次，其他 crate 依赖 `cmx-rpc-gen` 即可。

Volo 官方文档也建议把生成代码独立成一个 crate。

### 16.3 如何扩展 IDL？

1. 编辑 `idl/cmx_service.proto`，新增 rpc 方法和消息。
2. `cargo build -p cmx-rpc-gen`。
3. 在 `cmx-service-rpc` 的 `server.rs` 中实现新方法。
4. 在 `cmx-service-rpc` 的 `client.rs` 中调用新方法。

修改 proto 字段（**已存在的字段**）要小心：

- ✅ 增加新字段（用新编号）。
- ❌ 改变已存在字段的类型或编号（会破坏向后兼容）。

### 16.4 如何查看生成代码？

```bash
# 编译后生成文件路径
target/debug/build/cmx-rpc-gen-xxx/out/cmx_service_orchestrator.rs

# 或者用 cargo expand
cargo install cargo-expand
cargo expand -p cmx-rpc-gen
```

---

## 十七、cmx-service-rpc 实战解读

`crates/libs/cmx-infra/cmx-service-rpc/` 提供了 cmx 对 Volo 的工程化封装，下面拆解每个模块。

### 17.1 模块结构

```
cmx-service-rpc
├── client           # VoloGrpcClient（RpcClient trait 实现，含重试 + 客户端缓存）
├── config           # RpcConfig / GrpcConfig / HttpRestConfig
├── discover         # RegistryAwareDiscover（volo Discover trait 实现，带 diff 通知）
├── error            # RpcFrameworkError（框架启动/初始化错误）
├── factory          # create_rpc_client（按协议创建客户端）
├── global           # GlobalRpcClient（OnceLock 单例）
├── server           # CmxOrchestratorServiceImpl（volo Service trait 实现）
└── server_runner    # start_grpc_server（oneshot 就绪信号 + 启动 gRPC Server）
```

### 17.2 config.rs：配置定义

最新代码已经把所有字段都补齐了：

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct RpcConfig {
    pub enabled: bool,
    pub protocol: String,           // 目前只支持 "grpc"
    pub grpc: GrpcConfig,
    #[serde(default)]
    pub http_rest: HttpRestConfig,  // 预留
    #[serde(default)]
    pub warmup_services: Vec<String>,
    /// 服务列表同步间隔（秒），0 表示禁用定时同步
    #[serde(default = "default_service_sync_interval_secs")]  // 30s
    pub service_sync_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    pub port: u16,
    /// 单次 RPC 调用超时（毫秒），通过 volo rpc_timeout 设置
    #[serde(default = "default_timeout_ms")]                  // 5000ms
    pub timeout_ms: u64,
    /// 连接超时（毫秒），通过 volo connect_timeout 设置
    #[serde(default = "default_connect_timeout_ms")]          // 3000ms
    pub connect_timeout_ms: u64,
    /// 重试次数（仅对可重试错误：UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED）
    #[serde(default)]
    pub retry_count: usize,
    /// 默认服务分组（用于 query_instances 过滤，None 表示不按分组过滤）
    #[serde(default)]
    pub default_group: Option<String>,
    /// 默认集群列表（用于 query_instances 过滤，空表示不过滤）
    #[serde(default)]
    pub default_clusters: Vec<String>,
}
```

通过 `serde::Deserialize` 配合 `config` crate 从 TOML 中读取：

```toml
[rpc]
enabled = true
protocol = "grpc"
warmup_services = ["cmx-orchestrator"]
service_sync_interval_secs = 30

[rpc.grpc]
port = 9090
timeout_ms = 5000
connect_timeout_ms = 3000
retry_count = 2
default_group = "DEFAULT_GROUP"
default_clusters = ["cmx-cluster"]

[rpc.http_rest]
port = 8080
timeout_ms = 5000
```

### 17.3 client.rs：RpcClient 封装

`VoloGrpcClient` 实现了 `cmx_traits::RpcClient`，业务层调用 `call_service` / `call_function` 时**完全不用关心 volo**：

```rust
pub struct VoloGrpcClient {
    cache: Arc<ServiceInstanceCache>,
    config: GrpcConfig,
    registry: Arc<dyn ServiceRegistry>,
    /// 缓存的 gRPC 客户端（service_name → CachedClient）
    clients: RwLock<HashMap<String, CachedClient>>,
}

struct CachedClient {
    client: CmxServiceOrchestratorClient,
    _discover: RegistryAwareDiscover,   // 保活 discover
}

#[async_trait]
impl RpcClient for VoloGrpcClient {
    async fn call_service(
        &self,
        service_name: &str,
        service_key: &str,
        input: Value,
        options: ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, RpcError> {
        let start = Instant::now();
        let total_budget = Duration::from_millis(self.config.timeout_ms);
        let deadline = start + total_budget;
        let client = self.get_client(service_name).await?;

        let max_retries = self.config.retry_count;

        for attempt in 0..=max_retries {
            // 1. 检查总时间预算
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { return Err(RpcError::Timeout("...")); }
            if attempt > 0 {
                let backoff = Self::retry_backoff(attempt - 1);
                tokio::time::sleep(min(backoff, remaining)).await;
            }

            let req = ExecuteServiceRequest { /* ... */ };

            // 2. 发起调用
            match client.execute_service(req).await {
                Ok(resp) => return Ok(proto_to_call_service_response(resp.into_inner())),
                Err(e) => {
                    // 3. 只有可重试错误才会进下一轮
                    if Self::is_retryable_error(&e) && attempt < max_retries {
                        continue;
                    }
                    return Err(RpcError::RpcCallFailed(e.to_string()));
                }
            }
        }
        unreachable!()
    }
}
```

**关键设计要点**：

- **客户端缓存**：内部用 `RwLock<HashMap>` 按 `service_name` 缓存 `CmxServiceOrchestratorClient`，避免每次调用都重建。`get_client` 用 double-check locking 模式。
- **缓存穿透主动拉取**：如果 `ServiceInstanceCache` 为空，会主动调 `registry.query_instances` 拉取，避免冷启动调用失败。
- **总时间预算**：把整次调用视为一个预算，包含所有重试的总耗时，不会超过 `timeout_ms`。
- **重试 + 指数退避**：50ms → 100ms → 200ms → 400ms → 800ms（上限）。
- **错误白名单**：只对 `UNAVAILABLE` / `DEADLINE_EXCEEDED` / `RESOURCE_EXHAUSTED` / `ABORTED` 重试，业务错误不重试。
- **协议无关**：业务层只依赖 `cmx_traits::RpcClient`，不依赖 volo。
- **错误归一化**：把 volo-grpc 的 `Status` 统一映射到 `RpcError`。
- **超时在客户端**：服务端不设超时，所有超时由调用方控制。

### 17.4 server.rs：服务端实现

```rust
pub struct CmxOrchestratorServiceImpl {
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
}

impl CmxServiceOrchestrator for CmxOrchestratorServiceImpl {
    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_execute_service")]
    fn execute_service(
        &self,
        req: volo_grpc::Request<ExecuteServiceRequest>,
    ) -> impl Future<Output = Result<Response<ExecuteServiceResponse>, Status>> + Send {
        let service_invoker = self.service_invoker.clone();
        async move {
            let req = req.into_inner();

            // 1. 解析 JSON
            let input: serde_json::Value = serde_json::from_str(&req.input).map_err(|e| {
                volo_grpc::Status::new(volo_grpc::Code::InvalidArgument, format!("输入 JSON 解析失败: {e}"))
            })?;

            // 2. 构造 ServiceInvokeOptions
            let options = ServiceInvokeOptions {
                include_steps: req.include_steps,
                debug: req.debug,
                debug_node_id: req.debug_node_id.map(|s| s.to_string()),
                debug_params: if req.debug_params.is_empty() { None } else {
                    Some(req.debug_params.into_iter()
                        .map(|(k, v)| (k.to_string(), v.to_string())).collect())
                },
            };

            // 3. 调用业务
            match service_invoker.invoke_service(&req.service_key, input, options).await {
                Ok(resp) => {
                    let mut pb_resp = ExecuteServiceResponse::default();
                    pb_resp.success = resp.success;
                    pb_resp.output = resp.output.map(|v| v.to_string().into());
                    pb_resp.steps = resp.steps.into_iter().map(execution_step_to_proto).collect();
                    pb_resp.total_elapsed_us = resp.total_elapsed_us.unwrap_or(0);
                    pb_resp.error = resp.error.map(|e| OrchestrationError { message: e.message.into() });
                    Ok(Response::new(pb_resp))
                }
                Err(e) => {
                    // 业务错误包装在响应体里，不返回 Status
                    let mut pb_resp = ExecuteServiceResponse::default();
                    pb_resp.success = false;
                    pb_resp.error = Some(OrchestrationError { message: e.to_string().into() });
                    Ok(Response::new(pb_resp))
                }
            }
        }
    }
}

/// 将 StepStatus 转为字符串（注意：与 client 的 parse_step_status 互逆）
fn step_status_to_str(status: &cmx_core::StepStatus) -> &'static str {
    match status {
        cmx_core::StepStatus::Success => "Success",
        cmx_core::StepStatus::Failed => "Failed",
        cmx_core::StepStatus::Skipped => "Skipped",
        cmx_core::StepStatus::DebugPaused => "DebugPaused",
    }
}
```

注意返回类型是 `impl Future<...>`，这是 **RPITIT**（Return Position Impl Trait in Trait）的应用：

```rust
fn execute_service(&self, req: Request<ExecuteServiceRequest>)
    -> impl Future<Output = Result<Response<...>, Status>> + Send
```

这与直接 `async fn` 略有不同：直接 `async fn` 在 trait 中需要 Rust 1.75+，而 `impl Future` 在 trait 中可以兼容更老版本。volo 0.12 的生成代码选择了 `impl Future` 形式以获得更广泛的兼容性。

### 17.5 server_runner.rs：启动入口（含 oneshot 就绪信号）

`start_grpc_server` 现在接受一个 `oneshot::Sender<()>` 用于通知"服务已就绪"：

```rust
pub async fn start_grpc_server(
    port: u16,
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), RpcFrameworkError> {
    let addr: SocketAddr = format!("[::]:{port}").parse()?;
    let service_impl = CmxOrchestratorServiceImpl::new(service_invoker, runtime_invoker);

    // 1. 包成 volo Service
    let service = ServiceBuilder::new(CmxServiceOrchestratorServer::new(service_impl))
        .build::<CmxServiceOrchestratorRequestRecv, CmxServiceOrchestratorResponseSend>();

    // 2. 在 Server.run() 之前发就绪信号
    if ready_tx.send(()).is_err() {
        tracing::warn!("gRPC Server 就绪信号发送失败: 接收端已 drop（启动回调超时？）");
    }

    // 3. 启动并运行（阻塞直到服务退出）
    volo_grpc::server::Server::new()
        .add_service(service)
        .run(Address::Ip(addr))
        .await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(e.to_string()))?;
    Ok(())
}
```

注意：

- `ready_tx.send(())` 在 `Server::run()` 之前调用——这意味着信号表示"参数已就绪、即将 listen"，不保证端口已经 `accept`。
- 这是 fire-and-forget 模式：调用方等不到信号就放弃等待（init 流程不会因为 3s 还没起来就 panic）。
- 接收端在 web-server 的 init 流程里通过 `tokio::time::timeout(3s, ready_rx)` 等待（见 §18）。

注意 `CmxServiceOrchestratorRequestRecv` 和 `CmxServiceOrchestratorResponseSend` 是**编译期元类型**，告诉 volo 怎么编解码请求/响应。它们是生成代码中的空 struct（zero-sized），仅作为类型标签。

### 17.6 global.rs：全局单例

```rust
static GLOBAL_RPC_CLIENT: OnceLock<Arc<dyn RpcClient>> = OnceLock::new();

impl GlobalRpcClient {
    pub fn set(client: Arc<dyn RpcClient>) -> Result<(), String> { ... }
    pub fn get() -> &'static Arc<dyn RpcClient> { ... }
    pub fn is_initialized() -> bool { ... }
}
```

使用 `std::sync::OnceLock`（Rust 1.70+）保证线程安全的全局单例。**只能在初始化时 set 一次**。

### 17.7 factory.rs：协议工厂

**签名变化**：最新代码的 `create_rpc_client` 增加了 `registry` 参数：

```rust
pub fn create_rpc_client(
    config: &RpcConfig,
    cache: Arc<ServiceInstanceCache>,
    registry: Arc<dyn ServiceRegistry>,     // ← 新增参数
) -> Result<Arc<dyn RpcClient>, RpcError> {
    match config.protocol.as_str() {
        "grpc" => {
            tracing::info!(
                protocol = %config.protocol,
                timeout_ms = config.grpc.timeout_ms,
                "创建 gRPC RPC 客户端"
            );
            Ok(Arc::new(VoloGrpcClient::new(
                cache, config.grpc.clone(), registry
            )))
        }
        "http_rest" => Err(RpcError::UnsupportedProtocol("http_rest 协议暂未实现".to_string())),
        other => Err(RpcError::UnsupportedProtocol(other.to_string())),
    }
}
```

`VoloGrpcClient::new` 同样变成 3 参数：

```rust
impl VoloGrpcClient {
    pub fn new(
        cache: Arc<ServiceInstanceCache>,
        config: GrpcConfig,
        registry: Arc<dyn ServiceRegistry>,
    ) -> Self {
        Self { cache, config, registry, clients: RwLock::new(HashMap::new()) }
    }
}
```

`registry` 用于缓存穿透时主动 `query_instances`。未来要支持 HTTP REST / GraphQL 等时，只需在此 `match` 中加分支。

### 17.8 discover.rs：注册中心桥接

见 §11.2。核心要点：

- 从 `ServiceInstanceCache` 读实例列表。
- 在 `start_watch` 回调里算 **diff**（added/updated/removed）。
- 把带 diff 的 `Change` 通过 `async-broadcast` 通知 volo。
- 缓存为空时 `discover` 返回 `Err(LoadBalanceError::Discover)`。
- `change_rx` 用 `clone()` 共享，允许多次 watch。

---

## 十八、整体串联：从 IDL 到全链路调用

下面把所有片段组合起来，模拟一次完整的"客户端→服务端"调用。

### 18.1 服务端启动（cmx 实际代码）

`web-server/src/config/rpc.rs` 里的 `init_rpc` 已经演化为"oneshot 就绪信号 + 服务预热 + 定时同步"三段式：

```rust
// web-server/src/config/rpc.rs
pub async fn init_rpc(
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
) -> crate::Result<Option<u16>> {
    let rpc = load_rpc_config().expect("rpc config");

    // 1. 获取共享缓存和注册中心（由 init_infra 早期创建）
    let cache = GlobalServiceInstanceCache::get().clone();
    let registry = cmx_registry_config::GlobalRegistry::get().clone();

    // 2. 创建 RPC 客户端并注册到全局单例
    let rpc_client = create_rpc_client(&rpc, cache, registry)?;
    GlobalRpcClient::set(rpc_client)?;

    // 3. 后台启动 gRPC Server（oneshot 就绪信号 + 最多 3s 等待）
    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
    let _server_handle = tokio::spawn(async move {
        start_grpc_server(
            rpc.grpc.port,
            service_invoker,
            runtime_invoker,
            server_ready_tx,  // ← 把 Sender 传进去
        ).await
    });

    // 最多等 3 秒确认 Server 已就绪
    match tokio::time::timeout(std::time::Duration::from_secs(3), server_ready_rx).await {
        Ok(Ok(())) => info!("gRPC Server 启动成功"),
        Ok(Err(_)) => return Err(Error::ServerSetup("gRPC Server 启动失败".to_string())),
        Err(_) => return Err(Error::ServerSetup("gRPC Server 启动超时".to_string())),
    }

    // 4. 缓存预热：主动查询 warmup_services，写入全局缓存
    if !rpc.warmup_services.is_empty() {
        for service_name in &rpc.warmup_services {
            let instances = registry.query_instances(
                service_name,
                rpc.grpc.default_group.as_deref(),
                rpc.grpc.default_clusters.clone(),
            ).await?;
            if !instances.is_empty() {
                GlobalServiceInstanceCache::get().update(service_name, instances);
            }
        }
    }

    // 5. 启动服务列表定时同步
    if rpc.service_sync_interval_secs > 0 {
        let syncer = cmx_registry_config::ServiceListSyncer::new(
            registry.clone(),
            GlobalServiceInstanceCache::get().clone(),
            rpc.service_sync_interval_secs,
        );
        for svc in &rpc.warmup_services {
            syncer.mark_subscribed(svc);  // 标记预热服务为已订阅，避免重复查询
        }
        tokio::spawn(async move { syncer.run().await });
    }

    Ok(Some(rpc.grpc.port))
}
```

**关键点**：

- **顺序保证**：cache/registry → client → server（oneshot）→ 预热 → 定时同步。
- **就绪信号**：web-server 在收到信号之前不会继续后续初始化（如暴露 HTTP `/health`），避免"端口还没 listen 就被探活"。
- **3 秒超时**：兜底防止 init 卡死。
- **`mark_subscribed`**：避免 `ServiceListSyncer` 重复拉取已经在预热阶段查过的服务。

### 18.2 业务层调用（完全感知不到 volo）

```rust
use cmx_service_rpc::grpc::GlobalRpcClient;
use cmx_traits::{RpcClient, ServiceInvokeOptions};

async fn execute_remote_service() -> anyhow::Result<()> {
    let client = GlobalRpcClient::get();

    let resp = client.call_service(
        "cmx-orchestrator",                              // service_name
        "user-register",                                  // service_key
        serde_json::json!({ "username": "alice" }),      // input
        ServiceInvokeOptions {
            include_steps: false,
            debug: false,
            debug_node_id: None,
            debug_params: None,
        },
    ).await?;

    println!("success = {}, output = {:?}", resp.success, resp.output);
    Ok(())
}
```

### 18.3 内部流程（一次成功调用）

1. 业务层 `client.call_service(...)` → 走到 `VoloGrpcClient::call_service`。
2. 记录 `start = Instant::now()`，`deadline = start + timeout_ms`。
3. `VoloGrpcClient::get_client(service_name)`：
   - 读 `clients` 缓存：命中 → 直接返回。
   - 写锁 + 再次检查：命中 → 返回。
   - 看 `ServiceInstanceCache`：空 → 主动 `registry.query_instances` 拉一次。
   - 创建 `RegistryAwareDiscover` + `CmxServiceOrchestratorClientBuilder::new(svc).discover(...).rpc_timeout(...).connect_timeout(...).build()`，缓存到 `clients` HashMap。
4. 构造 `ExecuteServiceRequest`（`service_key_fs`、`input`、`include_steps`、`debug`、`debug_node_id`、`debug_params`）。
5. `client.execute_service(req).await` → volo 内部用 `RegistryAwareDiscover::discover()` 拿实例列表 → `LoadBalancer::pick` 选一个实例 → HTTP/2 over TCP 发送。
6. 对端 gRPC Server 收到，调用 `CmxOrchestratorServiceImpl::execute_service`：
   - 解析 `req.input` 为 `serde_json::Value`。
   - 调 `service_invoker.invoke_service(service_key, input, options)`。
   - 结果包成 `ExecuteServiceResponse`。
7. 客户端收到响应 → `proto_to_call_service_response(...)` 转成 `CallServiceResponse`。
8. 业务层拿到结果。

### 18.4 失败与重试流程

如果是可重试错误（`UNAVAILABLE` / `DEADLINE_EXCEEDED` / `RESOURCE_EXHAUSTED` / `ABORTED`）：

```
attempt 0: 调一次 → 失败
  → 计算 remaining = deadline - now
  → remaining > 0 → tokio::sleep(50ms)
attempt 1: 调一次 → 失败
  → tokio::sleep(100ms)
attempt 2: 调一次 → 失败
  → retry_count=2 已用尽 → 退出循环 → Err(RpcError::RpcCallFailed)
```

如果总预算耗尽（`remaining == 0`）：

```
attempt 1: 调一次 → 失败
  → remaining 已为 0 → 立即退出 → Err(RpcError::Timeout("重试预算耗尽: ..."))
```

### 18.5 数据流（复杂 JSON）

```
业务层 Value
    ↓ serde_json::to_string
JSON 字符串
    ↓ input: pilota::FastStr
Protobuf 编码
    ↓ h2 frame
字节流（TCP）
    ↓ h2 frame
Protobuf 解码
    ↓ input: pilota::FastStr
JSON 字符串
    ↓ serde_json::from_str
业务层 Value（服务端）
```

> 为什么用 string 传 JSON？——Protobuf 没有原生 JSON 值类型，嵌套 message 也可以但定义繁琐。cmx-service-rpc 选择 string 简化 IDL。

### 18.6 启动时序图

```
web-server::main()
  │
  ├── init_infra()  ─── 创建 GlobalServiceInstanceCache, GlobalRegistry
  │
  ├── init_rpc()
  │     │
  │     ├── create_rpc_client(&rpc, cache, registry)
  │     │     └── VoloGrpcClient::new(cache, GrpcConfig, registry)
  │     │
  │     ├── GlobalRpcClient::set(client)
  │     │
  │     ├── tokio::spawn(start_grpc_server(..., server_ready_tx))
  │     │     │
  │     │     └── start_grpc_server:
  │     │           ├── ServiceBuilder::new(...).build::<Recv, Send>()
  │     │           ├── ready_tx.send(())    ← 发就绪信号
  │     │           └── Server::run(addr)    ← 阻塞
  │     │
  │     ├── tokio::time::timeout(3s, ready_rx).await
  │     │
  │     ├── for svc in warmup_services: cache.update(...)
  │     │
  │     └── tokio::spawn(ServiceListSyncer::new(...).run())
  │
  └── axum::serve(...)  ─── HTTP 服务
```

---

## 十九、调试与排错

### 19.1 启用 Volo 内部日志

```bash
RUST_LOG=info cargo run
RUST_LOG=volo=debug,volo_grpc=debug cargo run
```

### 19.2 grpcurl 测试 gRPC

安装 `grpcurl`：

```bash
# macOS
brew install grpcurl
# Linux
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest
```

volo-grpc **默认不开启反射**，grpcurl 无法枚举服务。两种办法：

1. **手动指定 proto**：

   ```bash
   grpcurl -plaintext \
       -import-path crates/libs/cmx-rpc-gen/idl \
       -proto cmx_service.proto \
       [::1]:9090 cmx.CmxServiceOrchestrator/ExecuteService \
       -d '{"service_key":"my-service","input":"{}","include_steps":false,"debug":false,"debug_params":{}}'
   ```

2. **开启反射**：在 `volo.yml` 添加 `reflection: true`（volo-grpc 0.12 的 beta 特性）。

### 19.3 常见错误

| 现象                                    | 可能原因                          |
| --------------------------------------- | --------------------------------- |
| `NoAvailableInstance`                  | service_name 没注册到注册中心      |
| `Connection refused`                    | 服务端没启动 / 防火墙 / 端口错    |
| `Status::unavailable`                  | 上游 gRPC 服务不可用              |
| `Status::deadline_exceeded`             | 调用超时（业务逻辑太慢）          |
| `Status::invalid_argument`              | IDL 解析失败（如 input 不是 JSON）|
| `Status::internal`                     | 服务端 panic / 业务抛错           |
| 注册中心有实例但 volo 选不到           | Discover 没调用 `start_watch`    |

### 19.4 用 tracing 跟踪调用

`cmx-service-rpc/src/client.rs` 已经用 `#[instrument]` 装饰了 `call_service` / `call_function`：

```rust
#[instrument(target = "cmx_rpc", skip(self, input), fields(service_name, service_key))]
async fn call_service(...) { ... }
```

只要初始化 `tracing_subscriber`：

```rust
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

就能看到完整调用链。

---

## 二十、常见问题 FAQ

### Q1：`VoloGrpcClient::new` 应该是几个参数？

A：**3 个参数** —— `cache`、`config`、`registry`。最新代码已经统一为：

```rust
VoloGrpcClient::new(cache, config, registry)
```

`create_rpc_client` 同样：

```rust
create_rpc_client(&rpc, cache, registry)
```

如果发现旧文档或示例写的是 2 参数，那是历史版本。

### Q2：为什么客户端首次调用很慢？

A：首次 `get_client` 时会从 `ServiceInstanceCache` 读实例。如果实例还没同步完，会先做一次 `registry.query_instances`（缓存穿透主动拉取），所以会**慢几百毫秒**。可以在 web-server 启动时配置 `RpcConfig.warmup_services` 做预热。

### Q3：服务注册了但 volo 选不到实例？

A：检查：

1. `RegistryAwareDiscover::start_watch(service_name)` 是否被调用。`VoloGrpcClient::get_client` 内部会自动调用。
2. 注册中心返回的实例字段是否完整（`port == 0` 会被 `parse()` 过滤掉，并打 warn 日志）。
3. `discover` 缓存为空时返回 `Err(LoadBalanceError::Discover)`，volo 会映射为 `Status::Unavailable`，最终在 `VoloGrpcClient` 里被归一化为 `RpcError::RpcCallFailed`。
4. `change_rx` 是否被 `take()` 走了——新代码用 `clone()` 共享，不存在这个问题。

### Q4：gRPC Server 启动失败 `Address already in use`？

A：端口被占用。`lsof -i :9090`（Linux）查看，`kill <pid>` 或换端口。

如果端口可用但 `init_rpc` 报"gRPC Server 启动超时"：检查 `start_grpc_server` 内的 `ready_tx.send(())` 是否在 `Server::run()` 之前。最新代码已经把 `ready_tx.send(())` 移到 `Server::run()` 之前，应该不会卡 3 秒。

### Q5：如何同时支持 gRPC + HTTP REST？

A：当前 cmx-service-rpc **只支持 gRPC**。HTTP REST 在 web-server 单独用 axum 暴露（`/api/service/execute`），内部走相同的 `ServiceInvoker` / `RuntimeInvoker`。`RpcConfig.http_rest` 字段已预留，等 `factory::create_rpc_client` 加 `match` 分支即可启用。如果需要用 gRPC-Web 调浏览器，可以用 volo-grpc 的 `grpc-web` feature（`Cargo.toml` 中 `volo-grpc = { features = ["grpc-web"] }`）。

### Q6：怎么把生成的代码提交到 Git？

A：通常**不提交**（OUT_DIR 在 target 中，gitignore）。但有些团队会提交一份生成的 `.rs` 文件到 git 用于审阅。cmx-rpc-gen 选择不提交。

### Q7：`pilota::FastStr` 和 `String` 怎么互转？

```rust
let s: String = "hello".to_string();
let fs: pilota::FastStr = s.clone().into();        // String → FastStr
let s2: String = fs.to_string();                   // FastStr → String
let fs2 = pilota::FastStr::new("inline literal");  // &str → FastStr（无堆分配）
```

FastStr 的优势是：**小字符串内联存储，无堆分配**；**`from(String)` 走 Arc，clone 廉价**。

### Q8：为什么有些生成的 trait 方法用 `impl Future`，有些用 `async fn`？

A：volo-grpc 0.12 的 Service trait 用了 `impl Future` 形式（兼容更老 Rust 版本的 dyn 化），而 `RpcClient` 等业务 trait 用 `async fn`（更简洁）。两者等价。

### Q9：能不能用 Protobuf 嵌套 message 替代 JSON string？

A：可以。IDL 改成：

```protobuf
message CallInput {
  map<string, string> fields = 1;
}

message CallServiceRequest {
  CallInput input = 1;
}
```

但定义繁琐，且不支持任意 JSON。cmx 选择 string 是因为很多业务场景需要**任意 JSON 结构**。

### Q10：客户端要不要复用一个 `Client` 实例？

A：**要**。`VoloGrpcClient` 内部已经按 `service_name` 缓存了 volo Client（`clients: RwLock<HashMap<String, CachedClient>>`），所以全局单例 `GlobalRpcClient` 即可。**不要**在每个请求里 `ClientBuilder::new(...).build()`，会绕过缓存导致每次调用都重建连接。

### Q11：重试为什么只重试 UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED？

A：这是 cmx-service-rpc 的**白名单策略**：

- ✅ **UNAVAILABLE / DEADLINE_EXCEEDED**：临时性网络/超时问题，重试大概率成功。
- ✅ **RESOURCE_EXHAUSTED**：限流场景，间隔后大概率能过。
- ✅ **ABORTED**：事务中止，对端已经回滚，重试是安全的。
- ❌ **INVALID_ARGUMENT / NOT_FOUND / PERMISSION_DENIED**：业务逻辑错误，重试无意义。
- ❌ **INTERNAL**：服务 panic，重试可能雪崩。

如果需要自定义重试策略，可以修改 `VoloGrpcClient::is_retryable_error`。

### Q12：总时间预算 vs 每次超时，如何选择？

A：cmx-service-rpc 用**总时间预算**。优点是**总耗时可控**（即使 N 次重试 + 退避，总耗时也不会超过 `timeout_ms`），适合 SLO 严格的场景。缺点是单次重试的 timeout 不固定（如果第一次用掉 4.9s，重试时预算只剩 100ms）。

如果业务希望"每次调用都有固定的 5s 超时"，可以改写为：

```rust
for attempt in 0..=max_retries {
    let result = tokio::time::timeout(
        Duration::from_millis(self.config.timeout_ms),
        client.execute_service(req.clone()),
    ).await;
    // ...
}
```

### Q13：如何用 grpcurl 调试 cmx-service-rpc 服务？

A：volo-grpc **默认不开启反射**，grpcurl 无法枚举服务。两种办法：

1. **手动指定 proto**：

   ```bash
   grpcurl -plaintext \
       -import-path crates/libs/cmx-rpc-gen/idl \
       -proto cmx_service.proto \
       [::1]:9090 cmx.CmxServiceOrchestrator/ExecuteService \
       -d '{"service_key":"my-service","input":"{}","include_steps":false,"debug":false,"debug_params":{}}'
   ```

2. **开启反射**：在 `volo.yml` 添加 `reflection: true`（volo-grpc 0.12 的 beta 特性）。

### Q14：`ServiceListSyncer` 和 `ServiceInstanceCache.subscribe` 是什么关系？

A：两者协同工作：

- **`subscribe`**：当某服务实例列表变化时（注册中心推送），`ServiceInstanceCache` 会调回调通知订阅者。`RegistryAwareDiscover` 订阅后把变化转为 `Change<FastStr>` 广播给 volo。
- **`ServiceListSyncer`**：主动定时拉取"我订阅了哪些服务"，让缓存保持新鲜（兜底 `subscribe` 漏掉的事件）。

`init_rpc` 启动 `ServiceListSyncer` 后，会把 `warmup_services` 标记为 `mark_subscribed`，避免重复拉取。

### Q15：为什么 `CachedClient` 里要持有 `_discover`？

A：`_discover` 字段以 `_` 前缀表示"故意不使用"，实际作用是**保活**。如果 `RegistryAwareDiscover` 实例被 drop，`change_tx` 也跟着 drop，`async-broadcast` 通道就断了——后续注册中心推送的实例变更就无法通知到 volo。把它放在 `CachedClient` 里就能让 volo Client 和 Discover 生命周期绑定，cached client 在，discover 就在。

---

## 二十一、参考资源

### 官方文档

- Volo 概览：<https://www.cloudwego.io/zh/docs/volo/overview/>
- Volo-gRPC Getting Started：<https://www.cloudwego.io/docs/volo/volo-grpc/getting-started/>
- volo.yml 配置：<https://www.cloudwego.io/docs/volo/guide/config>
- volo-build API：<https://docs.rs/volo-build/0.12.3/volo_build/>
- volo-grpc API：<https://docs.rs/volo-grpc/latest/volo_grpc/>
- Volo GitHub：<https://github.com/cloudwego/volo>
- Motore（中间件抽象）：<https://github.com/cloudwego/motore>
- Pilota（IDL 编译器）：<https://github.com/cloudwego/pilota>

### 源码阅读顺序建议

1. `cmx-rpc-gen/build.rs` —— 1 行代码触发生成。
2. `cmx-rpc-gen/volo.yml` —— 生成配置。
3. `cmx-rpc-gen/idl/cmx_service.proto` —— IDL 定义。
4. `target/debug/build/cmx-rpc-gen-*/out/cmx_service_orchestrator.rs` —— 生成代码（重点看 Service trait、ClientBuilder）。
5. `cmx-service-rpc/src/server.rs` —— 服务端实现。
6. `cmx-service-rpc/src/server_runner.rs` —— 服务端启动。
7. `cmx-service-rpc/src/client.rs` —— 客户端实现。
8. `cmx-service-rpc/src/discover.rs` —— 服务发现桥接。
9. `cmx-service-rpc/src/factory.rs` —— 客户端工厂。
10. `cmx-service-rpc/src/global.rs` —— 全局单例。
11. `crates/web/web-server/src/config/rpc.rs` —— 真实集成入口。

### 推荐阅读顺序（Volo 源码）

1. `volo/src/service.rs` —— Service 抽象。
2. `volo/src/layer.rs` —— Layer 抽象。
3. `volo/src/discovery.rs` —— Discover trait。
4. `volo-grpc/src/server/` —— gRPC Server。
5. `volo-grpc/src/client/` —— gRPC Client。
6. `motore/src/service.rs` —— 中间件 Service。
7. `pilota/src/` —— IDL 解析与代码生成。

---

## 写在最后

Volo 的学习曲线比 tonic 略陡（多了 Service/Layer/Discover/LB 等抽象），但换来的是：

- **统一协议抽象**：gRPC 和 Thrift 共享同一套中间件体系。
- **零外部依赖**：纯 Rust 实现 IDL 编译，不依赖 `protoc`。
- **极高性能**：AFIT/RPITIT + 静态分发 + 零拷贝 FastStr。

对 cmx 项目来说，cmx-service-rpc 已经在 Volo 之上做了足够多的封装（`RpcClient` trait、`GlobalRpcClient`、`RegistryAwareDiscover`），**业务层基本不需要关心 volo**。但理解 Volo 的核心机制（IDL 生成、Discover、Service trait）能帮助你：

- 排查 RPC 调用问题。
- 扩展新协议（如 HTTP REST）。
- 实现自定义中间件（如分布式 Trace、金丝雀发布）。

Happy hacking！
