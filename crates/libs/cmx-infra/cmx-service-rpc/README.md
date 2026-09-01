# cmx-service-rpc

> 微服务间**东西向调用统一基座**：服务目录（`[service_rpc]`，取代旧 `[center_client]` + `[rpc]` 两段）+ HTTP 传输（熔断 / 幂等重试 / 打点 / 出站鉴权注入），gRPC 客户端/服务端设施经 feature 门控可选并入（吸收自已退役的 `cmx-rpc`）。
>
> 把「服务间怎么找到对方、怎么通信、怎么带身份」从各服务散装的 reqwest / 手写选例 / 私有重试里收拢为一处——**消费方只说"调哪个键 + 什么路径"**，定位、传输、凭证、韧性策略全由基座统一。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

## 快速开始

```toml
[dependencies]
cmx-service-rpc = { workspace = true }        # 默认 feature = ["http"]，五引擎零 volo 依赖
# 需要 gRPC 时（客户端或服务端）：
# cmx-service-rpc = { workspace = true, features = ["grpc-server"] }
```

服务目录配置（消费方 toml）：

```toml
[service_rpc.services]
flow = { url = "http://127.0.0.1:8091", discovery = "cmx-flow-server" }   # url 优先，删掉即切服务发现
mdm  = { url = "http://127.0.0.1:8095" }
```

调用方代码：

```rust
use cmx_service_rpc::RpcRequest;

// 1) 声明式调用（推荐）：post + JSON body，返回值自动解标准信封（code == 0）
let view: InstanceView = cmx_service_rpc::call_api(
    RpcRequest::post("flow", "/api/flow/v1/instances").json_body(&start_req)?
).await?;

// 2) 只要成功/失败（无返回体语义）
cmx_service_rpc::call_api_unit(
    RpcRequest::post("mdm", "/api/mdm/flow/callback").raw_body(body, "application/json")
).await?;

// 3) 反代/自持 resolver 场景：只取定位（每次调用现解析，跟随实例变化）
let base = cmx_service_rpc::locator("flow").and_then(|l| l.resolve());
```

初始化：`cmx-service-base` 的 `init_infra()` 末尾自动 `init_and_warm()`（装配目录 + 预热 discovery 订阅 + 启动快照日志），业务方无需手工调 `init()`。

## 核心特性

- **per-key 服务目录**：`[service_rpc.services]` 单表，键 → `url` 静态基址 / `discovery` Nacos 服务名 / `transport` 传输覆盖。url 与 discovery 并存时 **url 优先**（★回滚形态：注册中心全 Mock + url 直连即纯静态部署）。新增微服务只加一行 toml，零代码。
- **fail-fast 判定矩阵**（启动即报错，拒绝静默错配）：

  | 配置形态 | 行为 |
  |---|---|
  | `[service_rpc]` 段缺失 | 合法空目录（全内嵌 / 零出站形态） |
  | discovery-only 键 + 注册中心未启用 + 无 url | 报错，错误信息列出全部键清单 |
  | 旧段 `[center_client]` / `[rpc]` 残留 | 报错提示迁移（不做兼容读取） |
  | grpc 键未开 `grpc-client` feature | 运行时 `NoBinding`（配置了 gRPC 传输但本进程未编译该能力） |

- **调用生命周期**：熔断检查（单键 5 连败开放 / 10s 冷却半开）→ 定位（每次重试重新选例）→ 传输（总超时 = 键级 `timeout_ms` ?? 全局 30000ms）→ 状态映射（401/403 → `AuthRejected`，非 2xx → `Remote`）→ 幂等重试（仅 `idempotent()` 标记 + `Unavailable` 连接级失败；`Timeout` 不重试）→ 打点（`stats()` / `breaker_snapshot()`）。
- **鉴权链出站注入**：`X-API-Key`（`[service_auth].outgoing_api_key`）+ `X-Delegated-User-Token: Bearer`（OBO：`with_token` 显式参数优先，task-local 兜底）+ `X-Request-Id`。
- **feature 门控**：`default = ["http"]`（reqwest）；`grpc-client`（volo 客户端）；`grpc-server = ["grpc-client"]`。feature unification 不跨独立 workspace——五个引擎仓只依赖 http 形态，编译图零 volo。

## 模块结构

```
cmx-service-rpc/src
├── config.rs          # ServiceRpcConfig / ServerConfig / ServiceEntry（try_load 含旧段检测）
├── directory.rs       # ServiceDirectory / Locator（Static|Discovery）/ 选例 / warmup / validate
├── invoke.rs          # RpcRequest / RpcResponse / HttpMethod / Body / unwrap_envelope / call_api*
├── transport/         # Transport trait + HttpTransport（feature = "http"；关时 NoopTransport 占位）
├── guard.rs           # BreakerGuard（单键熔断：5 连败开放 / 10s 半开）
├── obs.rs             # KeyStats（calls / transport_failures / total_dur_ms）
├── error.rs           # ServiceRpcError（Unavailable/Timeout/AuthRejected/Remote/Decode/NoBinding）
├── lib.rs             # ServiceRpcHandle + init/install/global*/locator/resolve_base + call_api*
└── grpc/              # feature = "grpc-client" / "grpc-server"（吸收自 cmx-rpc）
    ├── bundle.rs      #   RpcServiceBundle + ServerDeps + ServerRegistration（OCP 装配接口）
    ├── factory.rs     #   init_rpc_clients(…, bundles)
    ├── server_runner.rs  # start_grpc_server(port, bundles, deps, ready_tx)
    ├── global.rs      #   GlobalRpcClient 初始化守卫
    ├── discover.rs    #   RegistryAwareDiscover（注册中心缓存 ↔ volo Discover 桥接）
    ├── error.rs       #   RpcFrameworkError
    ├── client/        #   GrpcInfrastructure / with_retry / apply_auth_metadata / safe_parse_json
    └── server/        #   AuthVerifier / verify_request / VerifiedAuth（入站鉴权）
```

## 公共 API 速览

| API | 说明 |
|-----|------|
| `init()` / `init_and_warm()` / `install(handle)` | 装配全局句柄（`init_infra` 已自动接线；单测用 `install` 注入 mock transport） |
| `global()` / `global_arc()` / `global_or_err(key)` | 全局句柄访问 |
| `locator(key) -> Option<Locator>` | 取定位器（反代 / 页面投递共享；`resolver_fn()` 固化闭包） |
| `resolve_base(key) -> Option<String>` | 现解析一个可用基址（healthy 过滤 + weight 加权 + `http_port` 元数据优先） |
| `RpcRequest::post/get(key, path)` + builder | 请求构造（`json_body` / `raw_body` / `multipart` / `query` / `with_token` / `idempotent` / `timeout`） |
| `call_api::<T>(req)` / `call_api_unit(req)` | 声明式调用：解标准信封（`code == 0`）→ `T`；unit 变体只要成功 |
| `handle.call_api::<T>(req)` | 句柄级调用（测试注入 mock transport 用） |
| `ServiceRpcError` | 统一错误：`key()` 归属键 + `is_transport_failure()` 熔断口径 |
| `stats()` / `breaker_snapshot()` | 每键打点 / 熔断状态快照 |

## 配置（`[service_rpc]` 段）

| 字段 | 默认 | 说明 |
|------|------|------|
| `default_transport` | `"http"` | 全局传输缺省（`http` / `grpc`）；键级 `transport` 可覆盖 |
| `nacos_group` | `"DEFAULT_GROUP"` | discovery 定位键共用的 Nacos 分组 |
| `timeout_ms` | `30000` | HTTP 全局总超时；键级可覆盖 |
| `retry_max` | `1` | 幂等请求的最大重试次数（连接级 `Unavailable` 才重试） |
| `services.<key>.url` | — | 静态基址（纯基址不含路径；与 discovery 并存时优先） |
| `services.<key>.discovery` | — | Nacos 服务名（选例：healthy + weight 加权 + `http_port` 元数据） |
| `services.<key>.transport` | 全局缺省 | 该键传输覆盖（`grpc` 键必须配 `discovery`，url 无效） |
| `services.<key>.timeout_ms` / `retry_max` | 全局 | 键级覆盖 |
| `server.enabled` | `false` | gRPC 服务端总开关（原 `[rpc].enabled`） |
| `server.protocol` | `"grpc"` | 仅支持 grpc |
| `server.grpc.*` | — | 原 `[rpc.grpc]` 全字段（port / timeout_ms / connect_timeout_ms / retry_count / warmup_services…） |

环境变量覆盖：`SERVICE_RPC__SERVICES__<KEY>__<FIELD>` 等（旧前缀 `CENTER_CLIENT__` 已废弃，残留即启动报错）。完整字段手册见仓根 `config/CONFIG_MANUAL.md`「服务间统一调用目录（service_rpc）」章。

出站凭证：`[service_auth].outgoing_api_key` 由基座统一注入 `X-API-Key`；委托用户经 `RpcRequest::with_token` 或 task-local 传播。

## gRPC 模块（feature 门控）

gRPC 客户端/服务端设施按 feature 并入本 crate（原 `cmx-infra/cmx-rpc` 整体吸收，模块路径 `cmx_service_rpc::grpc::*`，旧类型名不变）：

- `grpc-client`：`GrpcInfrastructure` / `with_retry` / `apply_auth_metadata` / `RegistryAwareDiscover` / `GlobalRpcClient`。
- `grpc-server`（含 client）：`RpcServiceBundle` / `ServerDeps` / `start_grpc_server` / `AuthVerifier`。

重试语义：仅重试 `UNAVAILABLE` / `DEADLINE_EXCEEDED` / `RESOURCE_EXHAUSTED` / `ABORTED`，指数退避 50ms→800ms 封顶、总预算 `timeout_ms`；业务错误立即失败。服务端业务错误封装在响应体 `error` 字段（不中断连接），仅输入格式错误返回 `INVALID_ARGUMENT`。

新增一个 gRPC 服务的 SOP（九步：proto → volo.yml entry → 别名重导出 → 新建皮肤 crate → workspace 注册 → trait 抽象 → 组装层 `rpc_bundles` 注册 → 消费方守卫调用 → 三 workspace check）沿用原流程，基础设施引用一律改 `cmx_service_rpc::grpc::{…}`；volo 框架入门见 [VOLO_GUIDE.md](VOLO_GUIDE.md)。

## 常见问题

**Q: 旧 `[center_client]` / `[rpc]` 段还能用吗？**
A: 不能。`try_load` 检测到旧段残留立即报错（错误信息带迁移对应关系）：`[center_client]` → `[service_rpc]`（`services` 单表原样搬）；`[rpc]` → `[service_rpc.server]`。

**Q: 五个引擎仓会引入 volo 吗？**
A: 不会。feature unification 以独立 workspace 为界，引擎仓依赖默认 feature（纯 http），volo 仅在显式开 `grpc-*` 的编译图（门户 platform-app）里出现。

**Q: 反代和声明式调用什么关系？**
A: 同一目录两个消费面——反代薄壳（`cmx-proxy-core`）拿 `locator(key).resolver_fn()` 按请求现解析基址做透明转发；声明式调用（`call_api` / 契约 SDK）走完整生命周期（熔断/重试/打点）。配置一份，两端共享。

**Q: 单测怎么隔离网络？**
A: `ServiceRpcHandle::with_transport(config, Arc::new(mock))` 构造后 `install()` 注入全局——`Transport` trait 是唯一 IO 边界，mock 它即可全链路断言。
