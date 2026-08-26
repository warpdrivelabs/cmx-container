# cmx-platform-app

> 平台总装配器（原 cmx-portal-app）：把 CMX 平台所有业务域的路由聚合到一起、按序执行基础设施初始化并起 HTTP 服务的最上层组装 crate，供各微服务 bin 以一行 `run_platform(banner)` 拉起完整平台。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-platform-app` 是 cmx-container 平台的**总装配层**（组装 crate，非可执行 bin）。它把原 web-server `main()` 的全部装配逻辑——有序初始化、`CmxAppState` 组装、路由树构建、HTTP serve 与优雅关闭——收成一个库函数 [`run_platform`]，对外只暴露 `cmx_platform_app::run_platform(banner).await` 一个入口。各微服务 bin（如 cmx-portalservice 的 `cmx-portal-server` :8080）作为薄壳，仅定义自己的 banner 字符画后调用它。

### 设计要点

- **一处装配，处处起服**：依赖约 50 个平台 crate（8+ 个 `*-api` 路由模块、IAM/审计/认证链、RPC 皮肤、任务中心、AI 代理等）。各独立微服务 bin 跨 workspace path 只引它一个，即牵入完整平台依赖树。它必须留在 cmx-container（与被装配的平台 crate 内聚）。
- **初始化顺序 load-bearing**：审计依赖数据源、IAM 依赖审计、系统身份在 finalize 之前……顺序不可随意调换。基础设施经公用包 `cmx-service-base` 的包装配（wasm/plugins/crypto/storage/services/Nacos/rpc…），门户专属组装（如 `build_function_invoker` 绑 cmx-biz）留在本 crate 的 `config/`。
- **门户仅留反代薄壳**：流程/报表/规则/模型中心/主数据五个引擎均为独立微服务（编译期不依赖引擎源码）。各按 `[center_client.services]` 的服务定位键（flow/report/rules/model/mdm）决定——配了（`url` 静态基址或 `discovery` Nacos 服务名）就挂反代 Module + 页面反代层，没配就不挂该模块路由（五者均无进程内嵌兜底）；前端与其余装配全零改动。
- **框架级配置统一**：日志配置改走 `ChassisConfig::load("cmx-server", "portal-server.toml")` 直读 toml `[server]` 段 + `SERVER__*` 环境变量覆盖；前端 dist 托管路径改读统一 `[assets]` 段（`assets.web_*_dist`）。
- **对偶关系**：与 `cmx-flow-app`（流程微服务装配核，在独立 workspace cmx-flowengine）成对——同一套装配模式，不同服务身份。

---

## 与其他 crate 的关系

### 上游依赖（选列）

| 依赖 | 用途 |
|------|------|
| `cmx-service-base` | 基础设施公用包装配（开 `redis/config-manager/crypto/debug/event-bus/storage/plugins/services/wasm/registry-config/rpc` 全套 feature） |
| `cmx-web-chassis` | 纯框架骨架：日志初始化、banner、serve + 优雅关闭 |
| `cmx-web-monitor` | 技术监控（`/_mon`）：身份读取器、拓扑 provider、活体探测 |
| `cmx-rpt-api` / `cmx-rule-api` / `cmx-flow-api` | 报表/规则/流程的纯反代 Module + 页面反代层（在此合并进主路由，破环） |
| `cmx-model-proxy` / `cmx-mdm-proxy` | 模型中心（`/api/{dct,dict,doc,model,definitions,flexible-combination,code}/*`）/ 主数据（`/api/mdm/*`）的纯反代 Module + 页面反代层（引擎已抽为独立微服务 cmx-model / cmx-mdm，门户不依赖引擎源码） |
| `cmx-storage-api` / `cmx-ai-api` / `cmx-job-api` | 存储/AI/异步任务中心的 HTTP 路由 Module（在此合并，cmx-api 不依赖它们） |
| `cmx-biz-api` / `cmx-plugin-api` / `cmx-iam-api` | 域/应用/菜单/数据源、插件市场、IAM/认证路由 |
| `cmx-iam` / `cmx-auth` / `cmx-audit` / `cmx-biz` | IAM 服务、认证服务、审计日志器、业务执行核心（组装注入） |
| `cmx-orchestrator-rpc` / `cmx-resource-rpc` | RPC 皮肤 Bundle 来源：依赖哪些域的 `*-rpc` = 对外提供哪些 gRPC 服务 |
| `cmx-job-core` / `cmx-job-store-pg` | 异步任务中心（PG 持久化 + claim/heartbeat/reaper 三循环） |
| `utoipa` / `utoipa-swagger-ui` | OpenApi 聚合与 Swagger UI（merge 各域 `*ApiDoc` 切片） |

### 下游使用方（谁依赖 cmx-platform-app）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-portalservice/crates/cmx-portal-server` | `cmx-platform-app = { path = "../cmx-container/crates/libs/cmx-platform-app", version = "0.1.12" }`（经 workspace 传递） | 门户主应用 bin：定义门户专属 banner 后调 `cmx_platform_app::run_platform(banner)` 组装出完整 `cmx-portal-server`（:8080） |
| cmx-container 内其他 crate | **无** | 它是最顶层组装 crate，容器内无下游 |

### 在整体架构中的位置

```text
┌────────────────────────────────────────────────────────────┐
│  微服务 bin 薄壳（cmx-portal-server 等）                     │
│  定义 banner → cmx_platform_app::run_platform(banner)       │
└──────────────┬─────────────────────────────────────────────┘
               ▼
┌────────────────────────────────────────────────────────────┐
│  cmx-platform-app（总装配层）                                │
│  ① 有序 init（infra→center_client 快照/预热→crypto→cache→   │
│     datasources→…→IAM 链→RPC→AppState→AI→jobs→router）      │
│  ② CmxAppState 注入（plugin/runtime/service/auth/iam…）     │
│  ③ 路由聚合（各 *-api Module + 五引擎按配置反代挂载）       │
│  ④ serve + 优雅关闭（委托 cmx-web-chassis）                  │
└──────────────┬─────────────────────────────────────────────┘
               ▼
     cmx-service-base / 各 *-api / cmx-iam / cmx-rpc ……
```

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 平台起服入口 | `run_platform(banner)`：init + AppState + 路由 + serve + 优雅关闭一气呵成 |
| 有序初始化 | 基础设施→服务定位快照/上游预热→数据链→IAM/认证链→RPC→AppState→AI→任务中心，顺序敏感 |
| 全域路由聚合 | 15 个 Module 无条件 merge + flow/report/rules/model/mdm 五者按 `[center_client.services]` 配置反代挂载 |
| OpenAPI 聚合 | 以 `ApiDoc` 为基底 merge 各域 `*ApiDoc` 切片（DCT/DOC/MDM 切片随引擎迁出，门户不再合并），挂 `/swagger-ui` |
| 服务依赖拓扑 | `service_topology()` 枚举各能力 embedded/proxy 真源，喂 `/_mon` 拓扑面板与活体探测 |
| 中间件栈 | 权限→监控→认证→上下文→追踪→100MB 请求体限制→CORS→压缩（排除 octet-stream 流式端点） |
| 前端静态托管 | `/portal`（SPA 回退）、`/html`（SPA 回退）、`/shared`（纯静态）按统一 `[assets]` 段配置挂 dist |
| RPC 装配 | `OrchestratorBundle` + `ResourceDataBundle` 显式注册，`build_function_invoker()` 绑 cmx-biz 后注入 cmx-rpc |
| gRPC 端口上报 | `init_rpc` 返回 gRPC 端口并注入注册中心 metadata |

---

## 模块结构

```text
cmx-platform-app
├── src
│   ├── lib.rs               # run_platform 入口：有序 init 编排 + serve + 优雅关闭
│   ├── app_state.rs         # build_app_state：向 CmxAppState 注入各子系统 trait 实例
│   ├── error.rs             # Error / Result（ConfigError/ServerSetup/DatasourceInit/…）
│   ├── router.rs            # build_router：API 路由 + 中间件栈 + 静态托管组装
│   ├── routes.rs            # routes()：全域 Module 聚合 + 五引擎反代切换 + OpenAPI merge
│   └── config/
│       ├── mod.rs           # WebConfig 单例 / AppIdentity / DeployMode
│       ├── audit.rs         # build_audit_logger：审计日志器（依赖数据源，须在 datasources 后）
│       ├── auth.rs          # init_auth_service / init_system_identity
│       ├── cache.rs         # init_cache：读 ConfigManager 得 RedisConfig 再委托 cmx-service-base
│       ├── datasource.rs    # init_datasources：sqlx 数据源建池 + SysDatasource 持久化（portal 专属）
│       ├── iam.rs           # init_iam_services / finalize_iam_state / run_permission_check
│       ├── jobs.rs          # init_job_center：异步任务中心（M3 分布式态）
│       ├── migration.rs     # init_database_migrations：数据库迁移
│       ├── nacos.rs         # （历史遗留：未被 mod.rs 声明，当前不参与编译）
│       ├── rpc.rs           # build_function_invoker（绑 cmx-biz）/ load_outgoing_credential（绑 cmx-plugin）
│       └── runtime.rs       # init_runtime：Extism 引擎 + IAM/plugin host-fn provider 注入
└── Cargo.toml
```

---

## 关键类型 / API

> **可见性说明**：`lib.rs` 仅 `pub use self::error::{Error, Result}` 并暴露 `run_platform`；
> `config` / `routes` / `router` / `app_state` 均为私有 `mod`，其函数仅供 `run_platform` 内部编排。
> 以下签名均为源码真实定义，用于说明装配结构与内部协作；外部如需独立复用某段初始化，
> 应改用下沉后的 `cmx-service-base` 同名函数或将对应模块开放为 `pub`。

### 入口（`lib.rs`，对外唯一 API）

```rust
/// 平台服务入口：装配并运行平台聚合服务（src/lib.rs）
pub async fn run_platform(banner: cmx_web_chassis::BannerSpec) -> Result<()>;

/// 优雅关闭超时（秒）：toml [server].graceful_timeout_secs > 默认 10s（env 覆盖 SERVER__GRACEFUL_TIMEOUT_SECS 经 ConfigManager env 层自动生效）
fn graceful_shutdown_timeout() -> Duration; // 私有
```

### 路由（`routes.rs` / `router.rs`，crate 内部模块）

```rust
/// 全部 API 路由（src/routes.rs）：15 个 Module merge + 五引擎反代切换
pub fn routes() -> Router<CmxAppState>;

/// Swagger UI + 聚合 OpenAPI 路由（/swagger-ui、/api-docs/openapi.json）
pub fn get_swagger_routes() -> Router;

/// 流程引擎是否反代态（读 [center_client.services].flow）
pub fn flow_is_proxied() -> bool;

/// 服务依赖拓扑：各能力 embedded/proxy 的真源清单（喂 cmx-web-monitor）
pub fn service_topology() -> Vec<cmx_web_monitor::ServiceDep>;

/// 组装完整路由树：/api 嵌套 + 中间件栈 + /_mon + 静态 fallback + dist 托管（src/router.rs）
pub fn build_router(app_state: CmxAppState, web_config: &WebConfig) -> Router;
```

### AppState 与配置（`app_state.rs` / `config/mod.rs`，crate 内部模块）

```rust
/// 注入 plugin_query / runtime_invoker / service_query / service_storage /
/// storage_service / auth_service / iam_state / resource_data_importer / definition_importers
pub fn build_app_state(
    auth_service: Arc<dyn AuthService>,
    iam_state: Arc<IamState>,
    resource_data_importer: Option<Arc<dyn ResourceDataImporter>>,
    definition_importers: Option<Arc<DefinitionImporterBundle>>,
) -> CmxAppState;                                    // src/app_state.rs

pub struct WebConfig { pub web_folder: String }      // src/config/mod.rs
pub fn init_web_config() -> ConfigResult<()>;        // 从 ConfigManager 读 web_folder
pub fn web_config() -> ConfigResult<&'static WebConfig>;

pub struct AppIdentity { pub domain_code: String, pub application_code: String, pub module_code: String }
pub fn load_app_identity() -> AppIdentity;           // [app] 节，缺省 "default"
pub fn load_deploy_mode() -> DeployMode;             // [deploy] mode，缺省 Mono
```

### 错误（`error.rs`）

```rust
pub type Result<T> = core::result::Result<T, Error>;

pub enum Error {
    ConfigError(String),     // 配置加载/解析/验证失败
    ServerSetup(String),     // 地址绑定、RPC 初始化等
    DatasourceInit(String),  // 数据源连接/注册
    RuntimeInit(String),     // WASM 引擎/宿主函数注册
    PluginInit(String),      // 插件管理器
    ServiceInit(String),     // 服务管理器/调用器
    StorageInit(String),     // 文件存储
    Migration(String),       // 数据库迁移
    Io(#[from] std::io::Error),
}
```

---

## 使用示例

### 一、微服务薄壳起服（cmx-portal-server 模式）

```rust
use cmx_web_chassis::BannerSpec;

/// 门户专属字符画（MEGA PORTAL，区别于 flow/report/mdm 各自的 banner）。
fn portal_banner() -> BannerSpec {
    BannerSpec::defaults("portal")
        .tagline("CMX 门户 · 平台聚合服务")
}

#[tokio::main]
async fn main() {
    // 薄壳只定义 banner，其余全部装配（有序 init + 路由 + serve）交给总装配器：
    // - dotenvy、日志、Nacos/配置中心、数据源、IAM 链、RPC、任务中心……
    // - 监听地址取 server.host / server.port（缺省 0.0.0.0:8080）
    if let Err(e) = cmx_platform_app::run_platform(portal_banner()).await {
        eprintln!("平台启动失败: {e}");
        std::process::exit(1);
    }
}
```

### 二、按配置切换「内嵌 / 反代」引擎路由

流程/报表/规则/模型/主数据五引擎的部署形态由 `[center_client.services]` 的 per-key 定位决定，前端与其余装配零改动：

```toml
# 配置文件（节选）——配了即反代到独立微服务；不配则不挂该模块路由
[center_client.services]
flow   = { url = "http://127.0.0.1:8081" }        # /api/flow/*  → cmx-flow-server
report = { discovery = "cmx-rpt-server" }         # /api/report-design/* 等 → Nacos 选例（HTTP 选例按 weight 加权随机）
rules  = { url = "http://127.0.0.1:8083" }        # /api/rules/* → cmx-rule-server
model  = { url = "http://127.0.0.1:8093" }        # /api/{dct,dict,doc,model,…}/* → cmx-model-server
mdm    = { url = "http://127.0.0.1:8095" }        # /api/mdm/* → cmx-mdm-server
# 定位二选一：url（静态基址）/ discovery（Nacos 服务名）；不同键可混用。
# 服务间调用另可按键配 transport = "http" | "grpc"（反代恒走 HTTP，配 grpc 仅 warn）。
```

```rust
// 装配层读取部署形态（run_platform 内部即如此判定；flow_is_proxied 为 crate 内部 pub fn，
// 经开放模块后亦可外部使用）：
if cmx_platform_app::routes::flow_is_proxied() {
    // 反代态：本进程不启动内嵌引擎 poller，/api/flow/* 与流程页面请求转发远程
    tracing::info!("流程引擎：独立微服务模式");
}

// /_mon 拓扑面板数据源：真实反映 routes() 的装配决策（embedded/proxy + 目标地址）
for dep in cmx_platform_app::routes::service_topology() {
    println!("{} [{}] target={:?}", dep.label, dep.mode, dep.target);
}
```

### 三、初始化步骤的复用边界（说明性）

`config` 模块当前为**私有**，其函数（`init_cache` / `init_datasources` / `build_audit_logger` …）仅供
`run_platform` 内部编排，外部不可直接调用。需要单独复用某段初始化时，按下沉原则选择替代：

```rust
// ✅ 通用部分：直接用 cmx-service-base 的同名函数（本 crate 内部也是这么做的）
// Redis 初始化（cmx-service-base 版本不吃 ConfigManager，自己传 RedisConfig）
cmx_service_base::init_cache(redis_config).await?;
// tokio-postgres 数据源注册
let cfg = cmx_service_base::BaseConfig::from_toml_path("flow-server.toml")?;
cmx_service_base::register_pg_datasources(&cfg.databases).await?;

// ⚠️ portal 专属部分（读 ConfigManager 的 init_cache 包装、cmx_sys_datasource 持久化、
//    IAM 链等）留在本 crate 私有 config 模块中——外部复用需先把它开放为 pub（小改动），
//    或等待进一步下沉。这是刻意的：这些逻辑绑定了 portal 的配置链与部署形态。
```

### 四、自定义 RPC 皮肤集（裁剪 gRPC 能力）

```rust
// 主应用提供的 RPC 服务 = 此处显式收集的 Bundle 列表（run_platform 内的写法）：
// 依赖哪个域的 *-rpc crate 并注册其 Bundle，即对外提供哪个 gRPC 服务；
// 裁剪能力只需增删本列表，cmx-rpc 与皮肤 crate 零改动。
let rpc_bundles: Vec<Box<dyn cmx_rpc::bundle::RpcServiceBundle>> = vec![
    Box::new(cmx_orchestrator_rpc::OrchestratorBundle),
    Box::new(cmx_resource_rpc::ResourceDataBundle),
];

let grpc_port = cmx_service_base::init_rpc(
    rpc_bundles,
    cmx_traits::service::GlobalServiceInvoker::get().clone(),
    // build_function_invoker（绑 cmx-biz）是本 crate 私有 config::rpc 的函数，
    // run_platform 内部构造后注入；外部自定义 bin 需自行构造 BizFunctionInvoker
    function_invoker,
    resource_data_importer.clone(),
    Some(auth_service.clone()),                          // None = 不启用 gRPC 鉴权
).await?;
```

---

## 关键设计决策

### 1. 为什么总装配层是库而不是 bin？

各微服务（门户/报表/主数据…）需要**同一套装配**但**不同的 banner 与服务身份**。库化后 bin 只剩十几行；若做成 bin 则每个微服务都要 fork 一份 main。`BannerSpec` 由调用方传入，非终端环境自动降级纯文本，避免 ANSI 污染日志。

### 2. 初始化顺序为何 load-bearing？

- `build_audit_logger` 依赖 `DatabaseManager` → 必须在 `init_datasources` 之后；
- `init_auth_service` 消费 `user_auth_query`（IAM 产出）→ IAM 在前；
- `init_system_identity`（后台任务的 system_auth()）必须在 `finalize_iam_state` 之前；
- `init_rpc` 需要 `GlobalServiceInvoker` → 必须在 `init_service_invoker` 之后；
- `init_storage` 依赖 DB manager 已起（文件元信息管理）。

### 3. 反代切换为什么放在 routes.rs 而不是各 *-api？

`cmx-api` 不依赖各业务域 crate（避免循环依赖）；`*-api` 反代 Module 在本 crate 的 `routes()` 里合并。配置读取（`CenterClientConfig::load`）与装配决策收敛于此，`service_topology()` 与 `flow_is_proxied()` 消费同一真源，监控面板不猜。

### 4. 压缩层为什么排除 `application/octet-stream`？

单据流式端点（`/api/doc/data/tokio-zmc-stream`）用 chunked 分帧边算边发（O(单行)内存），压缩层会缓冲整流并可能截断 gzip 尾（丢 trailer → 浏览器 `net::ERR_*`），故排除二进制流保持真流式；列式 JSON 单据包仍享受 gzip/br 高压缩比。

### 5. portal 专属组装与通用基础设施如何切分？

`config/rpc.rs` 只留 portal 专属：`build_function_invoker()`（绑 cmx-biz `BizFunctionInvoker`）与 `load_outgoing_credential()`（绑 cmx-plugin `Credential`）；通用部分（`init_rpc` 本体、Nacos init/shutdown、wasm/crypto/storage 等）已下沉 `cmx-service-base`，本 crate 经包装委托调用。

---

## 常见问题

### Q1: `run_platform` 到底初始化了多少步？

按代码顺序：dotenvy → 日志（chassis，`ChassisConfig::load("cmx-server", "portal-server.toml")`）→ `init_infra`（Nacos/配置中心/全局配置）→ `log_center_client_snapshot`（服务定位配置快照）+ `warm_proxy_upstreams`（discovery 目标订阅预热）→ crypto → cache → datasources → storage → web-monitor（服务名/身份/采样器/拓扑/探测）→ flow 部署形态判定 → web_config → debug → runtime(WASM) → event_bus → services → plugins → service_invoker → audit_logger → iam_services → auth_service → system_identity → finalize_iam_state → 权限校验 → RPC → AppState → AI 子系统 → 任务中心 → build_router → bind → serve → shutdown_infra。注：编码引擎注入（init_code_engine）与 MDM 分发引擎启动已随模型中心/主数据迁至独立微服务（cmx-model-server / cmx-mdm-server），门户不再执行。

### Q2: `config/nacos.rs` 为什么没出现在模块导出里？

`config/mod.rs` 未声明 `pub mod nacos`，该文件是历史遗留、当前**不参与编译**；Nacos 注册/配置中心逻辑已下沉至 `cmx-service-base::init_infra` / `shutdown_infra`（feature `registry-config`）。

### Q3: gRPC 服务默认开吗？

不开。需配置 `[rpc] enabled = true` 且 `protocol = "grpc"`，`init_rpc` 才会起 gRPC Server（并返回端口注入注册中心 metadata）；否则跳过并返回 `None`。

### Q4: 前端 dist 托管的三个路径分别是什么？

`/portal` → CMXPortalManager/dist（SPA，未命中回退 index.html）；`/html` → CMXHTMLDesigner/dist（SPA）；`/shared` → cmx-ui5-runtime/dist（纯静态，缺文件 404 不回退）。路径由统一 `[assets]` 段的 `assets.web_portal_dist` / `assets.web_html_dist` / `assets.web_shared_dist` 配置给出，未配置则跳过（开发走 vite 代理）。

### Q5: 与 cmx-flow-app 是什么关系？

对偶关系：本 crate 是**平台聚合服务**的装配核（原 web-server），`cmx-flow-app` 是**流程微服务**的装配核（独立 workspace cmx-flowengine）。二者装配模式一致，服务身份不同；平台侧通过 `FlowProxyModule` 反代对接 flow-server。

### Q6: 设计器保存业务域页面怎么处理归属？

F3-save：门户 `POST /api/html-pages` 按页面 id 归属分流——属主引擎的页面（如 `portal.model.*` / `portal.mdm.*`）整包反代到属主服务的同名端点（引擎侧经 `cmx-form::serve` 的 F3-save 写路径落自有资产工作区），其余落门户本地存储；batch 取页同理按归属扇出。详见 cmx-common-api 的 pages handlers。
