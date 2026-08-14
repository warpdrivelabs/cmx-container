# cmx-rpc

> 基于 volo-grpc 的 RPC **基础设施核心库**（纯共享设施层）：服务发现桥接、重试、出/入站鉴权、客户端共享基础设施、Bundle 装配接口、gRPC Server 启动器。
>
> 具体领域的 gRPC 皮肤（client / server impl / Bundle）**不在本 crate**——见 `cmx-rpcs/*` 皮肤 crate。

## 三层架构（契约中心化 · 实现归域 · 装配显式）

| 层 | crate | 职责 |
|------|-------|------|
| proto 契约 | `cmx-rpc-gen` | 集中管理全部 `.proto`，volo-build 生成类型 |
| **基础设施（本 crate）** | `cmx-rpc` | Bundle trait / GrpcInfrastructure / with_retry / apply_auth_metadata / AuthVerifier / factory / server_runner / GlobalRpcClient 守卫 |
| 皮肤 | `cmx-rpcs/cmx-orchestrator-rpc`、`cmx-rpcs/cmx-resource-rpc` | 各领域 client 访问器 + server impl + Bundle |
| 组装 | `cmx-platform-app` | **显式收集皮肤 Bundle 列表**传入 `init_rpc`——主应用提供哪些 RPC 服务的唯一决定点 |

依赖方向：皮肤 → cmx-rpc + cmx-rpc-gen + cmx-traits；皮肤**不依赖业务 service crate**（业务实现经 `ServerDeps` 由组装层注入）。

## 如何新增一个 gRPC 服务（标准步骤 SOP）

1. **加 proto**：在 `cmx-rpc-gen/idl/<域>/` 新建 `.proto`（定义 service + 消息，`package cmx`）。
2. **注册 entry**：`cmx-rpc-gen/volo.yml` 加 entry（`filename` 决定生成文件名，建议 `<service_snake>.rs`）。
3. **重导出**：`cmx-rpc-gen/src/lib.rs` 加 `pub mod <域> { include!(...) }` + 便捷别名模块（如 `pub mod xxx_proto { pub use ...; }`），然后 `cargo check -p cmx-rpc-gen` 验证生成。
4. **新建皮肤 crate**：`crates/libs/cmx-rpcs/cmx-<域>-rpc/`，三个文件：
   - `src/client.rs`：OnceLock 单例 + `<域>_client()` 访问器 + `*GrpcClient`（impl 对应 cmx-traits trait，复用 `cmx_rpc::{GrpcInfrastructure, with_retry, apply_auth_metadata, safe_parse_json}`）+ `<域>Bundle`（impl `RpcServiceBundle`）+ proto ↔ 领域模型转换。
   - `src/server.rs`：`*ServerImpl`（impl volo 生成的 service trait，入口经 `cmx_rpc::{AuthVerifier, verify_request}` 鉴权 + `context_scope::scope_full` 透传委托身份）。
   - `src/lib.rs`：`pub use client::{<域>Bundle, <域>_client}; pub use server::*ServerImpl;`
5. **workspace 注册**：cmx-container 根 `Cargo.toml` 的 `members` + `[workspace.dependencies]` 加一行（`version = "0.1.12", registry = "nora"`）。
6. **trait 抽象**（可选）：若新领域需要新的客户端/服务端抽象，先在 `cmx-traits/src/rpc/` 定义 trait，皮肤 crate 实现之；`ServerDeps` 需要新依赖字段时在 `cmx-rpc/src/bundle.rs` 加字段（评估耦合代价，见其 doc 演进路线）。
7. **组装层注册**：`cmx-platform-app` 加 `cmx-<域>-rpc` 依赖，并在 `run_platform` 的 `rpc_bundles` 列表加 `Box::new(cmx_<域>_rpc::<域>Bundle)`。**这一行决定主应用对外提供该服务**。
8. **消费方调用**：依赖 `cmx-<域>-rpc`，用 `cmx_<域>_rpc::<域>_client()`（前置 `cmx_rpc::GlobalRpcClient::is_initialized()` 守卫防 panic）。
9. **验证**：三 workspace（cmx-container / cmx-portalservice / cmx-flowengine）`cargo check` + `cargo clippy`。

## 模块结构

```
cmx-rpc/src
├── bundle.rs          # RpcServiceBundle trait + ServerDeps + ServerRegistration（OCP 装配接口）
├── factory.rs         # init_rpc_clients(…, bundles)——迭代外部传入的 Bundle 初始化客户端
├── server_runner.rs   # start_grpc_server(port, bundles, deps, ready_tx)
├── global.rs          # GlobalRpcClient 初始化状态守卫（is_initialized）
├── config.rs          # RpcConfig / GrpcConfig / HttpRestConfig
├── discover.rs        # RegistryAwareDiscover（注册中心缓存 ↔ volo Discover 桥接）
├── error.rs           # RpcFrameworkError
├── client/
│   ├── mod.rs         # safe_parse_json
│   ├── infra.rs       # GrpcInfrastructure（Discover 缓存/超时/重试配置/出站凭证）
│   ├── retry.rs       # with_retry（指数退避 + 总预算，含单测）
│   └── auth_outbound.rs  # apply_auth_metadata（三层出站凭证注入）
└── server/
    ├── mod.rs
    └── auth_layer.rs  # AuthVerifier / verify_request / VerifiedAuth（入站鉴权）
```

## 公共 API 速览

| API | 说明 |
|-----|------|
| `init_rpc_clients(config, cache, registry, outbound_key, bundles)` | 初始化传入 Bundle 的客户端，返回 bundles 供 server 注册 |
| `start_grpc_server(port, bundles, deps, ready_tx)` | fold 迭代 Bundle 注册服务并启动（先绑端口再发就绪信号） |
| `bundle::{RpcServiceBundle, ServerDeps, ServerRegistration}` | 皮肤 crate 实现的装配接口 |
| `GrpcInfrastructure` | 客户端共享设施（服务发现缓存、超时/重试配置、出站凭证） |
| `with_retry` / `RetryStats` | 带总预算的指数退避重试 |
| `apply_auth_metadata` | 出站三层凭证注入（X-API-Key / X-Delegated-User-Token / X-Request-Id） |
| `AuthVerifier` / `verify_request` / `VerifiedAuth` | 入站鉴权（服务身份必备 + 委托用户可选） |
| `safe_parse_json` | JSON 容错解析（失败降级 Null + warn 日志） |
| `GlobalRpcClient::is_initialized` | 全局初始化守卫（消费方调用访问器前先检查） |
| `RpcConfig` / `GrpcConfig` | `[rpc]` 配置段 |

## 配置（`[rpc]` 段）

| 字段 | 默认 | 说明 |
|------|------|------|
| `enabled` | false | RPC 总开关（关闭时 `init_rpc` 直接跳过，全本地调用） |
| `protocol` | "grpc" | 目前仅支持 grpc |
| `grpc.port` | — | gRPC 监听端口 |
| `grpc.timeout_ms` | 30000 | 单次调用超时 + 重试总预算 |
| `grpc.connect_timeout_ms` | 3000 | 连接超时 |
| `grpc.retry_count` | 0 | 重试次数（仅对 UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED） |
| `warmup_services` | [] | 启动时预订阅的服务名列表 |

出站服务凭证：`[service_auth].outgoing_api_key`（`cmx_sk_xxx`），由 `apply_auth_metadata` 注入 `X-API-Key`。

## 重试机制

仅重试 `UNAVAILABLE` / `DEADLINE_EXCEEDED` / `RESOURCE_EXHAUSTED` / `ABORTED`；指数退避 50ms→100ms→200ms→400ms→800ms（上限），总时间不超过 `timeout_ms` 预算。业务错误（INVALID_ARGUMENT 等）立即失败。

> 注意：`with_retry` 的闭包只能返回原始 `volo_grpc::Status`；`into_inner`/proto 转换必须在重试返回后做一次（避免重试分支重复消费 response）——见 `client/retry.rs` 模块文档。

## 服务端业务错误约定

业务错误不返回 gRPC Status，而是封装在响应体 `error` 字段中（不中断连接）；仅输入格式错误（如 JSON 解析失败）返回 `INVALID_ARGUMENT`。服务端入口统一：鉴权 → `context_scope::scope_full`（透传委托用户 + request_id，支持链式 on-behalf-of）→ 调用注入的 trait 实现。

## 常见问题

**Q: 主应用如何裁剪 RPC 能力（精简版/独立微服务）？**
A: 只改 `cmx-platform-app` 的 `rpc_bundles` 列表——增删 `Box::new(<域>Bundle)` 即增删对外 gRPC 服务，cmx-rpc 与皮肤 crate 零改动。

**Q: 为什么皮肤不放进业务 service crate（如 cmx-biz）？**
A: 业务核心保持纯净、不染 volo-grpc（与 HTTP 层薄 `*-api` crate 同一策略）。皮肤依赖 cmx-traits 抽象，业务实现经 `ServerDeps` 注入。

**Q: 旧路径 `cmx_rpc::orchestrator_client()` 还在吗？**
A: 不在。已随皮肤迁至 `cmx_orchestrator_rpc::orchestrator_client()` / `cmx_resource_rpc::resource_data_client()`。`GlobalRpcClient` 守卫仍在 `cmx-rpc`。
