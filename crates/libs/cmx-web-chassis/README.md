# cmx-web-chassis

> 通用 HTTP 服务骨架（chassis）：把「起一个 axum 微服务」的框架无关部分收成一份可复用代码——分层日志 + 配置三级装配 + 有序启动钩子 + 中间件栈 + 优雅关闭 + 启动横幅，各服务填一个 `ServiceSpec` 调 `run()` 即得同构服务。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-web-chassis` 抽自 web-server 的 main.rs「框架无关切片」，泛型 over 应用状态 `S`，**零 cmx-api / 零平台依赖**——任何 Rust 服务（平台内或平台外）都可用。CMX 拆出多个独立微服务（flow / report / rule / 门户等）后，每个服务的 main 都要重复同一套启动序列；本 crate 把它收口为一份：填 `ServiceSpec`（路由、状态、钩子、banner）→ `run(spec)`。

`run()` 内的统一启动序列：

1. `dotenvy` 载入 `.env`；
2. **分层日志** `init_tracing`：控制台 `CompactFormatter`（带色、紧凑）+ 滚动文件 JSON（按天、非阻塞）——返回的 `WorkerGuard` 必须持有到进程退出，否则文件日志后台线程提前 drop、丢日志；
3. **有序启动钩子**：按注册顺序逐个执行，任一 Err 视为致命（`ChassisError::InitHook`）中止启动；
4. **路由装配**：默认把业务 router nest 到 `/api` 前缀（可 `nest_api(false)` 挂根）；
5. **通用技术监控**（默认开）：`set_service_name` + 后台系统采样器 + 拓扑探测器 + merge `cmx_web_monitor::monitor_routes()`；
6. **中间件栈** `default_layers`：TraceLayer → 100MiB 请求体上限 → CORS permissive → 压缩（排除 `application/octet-stream`）；
7. **绑定 + banner**：打印启动信息框与渐变字符画（非终端降级纯文本）；
8. **serve + 优雅关闭**：SIGINT/SIGTERM 触发，等待活动连接，超时（`graceful_timeout_secs`，默认 10s）兜底继续退出。

配置三级装配（`ChassisConfig::load`）：toml 路径 `CONFIG_FILE` → `{PREFIX}_CONFIG` → 内置默认 → 环境变量 `{PREFIX}_HOST/_PORT/_LOG_DIR/_LOG_LEVEL/_GRACEFUL_SECS` 覆盖（优先级最高）。chassis 只管这些**框架级**配置；服务专属配置（DB URL 等）由各服务自己读。

依赖方向上，chassis 依赖 `cmx-web-monitor`（默认启用的技术监控），而 monitor **不依赖** chassis——无环。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-web-monitor` | 技术监控（`monitor_routes` / `spawn_system_sampler` / `spawn_topology_prober` / `set_service_name`），monitor 不反向依赖 chassis，无环 |
| `axum` | Web 框架（Router / serve / DefaultBodyLimit） |
| `tower-http` | TraceLayer / RequestBodyLimitLayer / CorsLayer / CompressionLayer |
| `tokio` | 运行时 / TcpListener / select 优雅关闭 |
| `tracing` / `tracing-subscriber` / `tracing-appender` | 分层日志（EnvFilter / registry / 滚动文件 appender） |
| `serde` / `toml` | `ChassisConfig` 的 toml 反序列化壳 |
| `dotenvy` | `.env` 载入 |
| `anyhow` | 启动钩子错误类型 |

### 下游使用方（谁依赖本 crate）

| 使用方 | 仓库 | 实际用途 |
|--------|------|---------|
| `cmx-flow-server` | `../cmx-flowengine` | 流程微服务 main：`ServiceSpec::<()>::new("flow", cfg).nest_api(false)...` + 数据源/引擎钩子 |
| `cmx-rpt-server` | `../cmx-report` | 报表微服务启动骨架 |
| `cmx-rule-server` | `../cmx-rulesengine` | 规则引擎微服务启动骨架 |
| `cmx-portal-server` | `../cmx-portalservice` | 门户主服务（部分复用：`init_tracing` / `serve_with_shutdown` / `print_startup_banner`） |
| `cmx-platform-app` | 本仓库 | 平台组装层 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 声明式装配 | `ServiceSpec<S>`：name / config / router / state / nest_api(默认 true) / monitor(默认 true) / banner / init_hooks，builder 链式填充 |
| 有序启动钩子 | `init(name, hook)` 追加；钩子收 `&ServiceMeta`（服务名），返回 `anyhow::Result`；Err 中止启动并带上钩子名 |
| 分层日志 | 控制台 CompactFormatter（时间戳+彩色级别+线程+文件:行号+消息）+ 按天滚动 JSON 文件；RUST_LOG 覆盖 log_level |
| 配置三级装配 | `ChassisConfig::load(service, env_prefix, default_toml)`：默认值 → 可选 toml → 环境变量覆盖 |
| 中间件栈 | `default_layers`：Trace → 请求体 100MiB 上限 → CORS permissive → 压缩（octet-stream 除外） |
| 优雅关闭 | `serve_with_shutdown`：Ctrl+C + Unix SIGTERM select → 等待活动连接 → 超时兜底退出 |
| 启动横幅 | `BannerSpec`：多行字符画 + 标语 + 渐变停靠点 + 签名；终端 ANSI 纵向渐变，非终端降级纯文本；东亚全角字符按 2 列对齐 |
| 默认横幅 | `DEFAULT_ART`（CMX 字符画）+ `DEFAULT_STOPS`（青→蓝→紫→品红：`(0,229,255)`/`(41,121,255)`/`(124,77,255)`/`(255,64,200)`） |
| 零平台依赖 | 不依赖任何 cmx-api/业务 crate；泛型 `S`（默认 `()`）适配各服务状态 |

---

## 模块结构

```text
cmx-web-chassis
├── src
│   ├── lib.rs       # ServiceSpec / InitHook / ServiceMeta / run / init_tracing / default_layers /
│   │                #   print_startup_banner / serve_with_shutdown（BODY_LIMIT = 100 MiB）
│   ├── config.rs    # ChassisConfig（host/port/log_dir/log_file/log_level/graceful_timeout_secs）+ defaults + load 三级装配
│   ├── banner.rs    # BannerSpec（art/tagline/signature/stops）+ Rgb + print + DEFAULT_STOPS/DEFAULT_ART
│   ├── format.rs    # CompactFormatter（tracing_subscriber FormatEvent impl：控制台紧凑带色格式）
│   ├── shutdown.rs  # shutdown_signal()（Ctrl+C + SIGTERM）
│   └── error.rs     # ChassisError { Config, ServerSetup, InitHook{name,source}, Io } / Result<T>
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/lib.rs —— 声明式服务描述
pub type InitHook = Box<dyn for<'a> FnOnce(&'a ServiceMeta)
    -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> + Send>;
pub struct ServiceMeta { pub name: String }

pub struct ServiceSpec<S = ()> { /* name/config/router/state/nest_api/monitor/banner/init_hooks */ }
impl<S: Clone + Send + Sync + 'static> ServiceSpec<S> {
    pub fn new(name: impl Into<String>, config: ChassisConfig) -> Self where S: Default;
    pub fn router(mut self, router: Router<S>) -> Self;
    pub fn state(mut self, state: S) -> Self;
    pub fn nest_api(mut self, yes: bool) -> Self;       // 默认 true：router nest 到 /api
    pub fn monitor(mut self, yes: bool) -> Self;        // 默认 true：/_mon + 采样器 + observe 地基
    pub fn banner(mut self, banner: BannerSpec) -> Self;
    pub fn init<F>(mut self, name: impl Into<String>, hook: F) -> Self;
}

pub async fn run<S>(spec: ServiceSpec<S>) -> Result<()>;  // 日志→钩子→路由→监控→中间件→serve

// 独立可复用的四件（门户等平台服务可直接调）
pub fn init_tracing(config: &ChassisConfig) -> tracing_appender::non_blocking::WorkerGuard;
pub fn default_layers(router: Router) -> Router;
pub fn print_startup_banner(name: &str, config: &ChassisConfig, listener: &TcpListener, banner: &BannerSpec);
pub async fn serve_with_shutdown(listener: TcpListener, app: Router, graceful_secs: u64) -> Result<()>;

// src/config.rs
pub struct ChassisConfig { pub host: String, pub port: u16, pub log_dir: String,
    pub log_file: String, pub log_level: String, pub graceful_timeout_secs: u64 }
impl ChassisConfig {
    pub fn defaults(service: &str) -> Self;                     // 0.0.0.0:8080 / logs / <service>.log / info / 10s
    pub fn load(service: &str, env_prefix: &str, default_toml: &str) -> Self;
}

// src/banner.rs
pub type Rgb = (u8, u8, u8);
pub struct BannerSpec { pub art: String, /* + tagline / signature / stops，builder 式构造 */ }
pub fn print(spec: &BannerSpec);
pub const DEFAULT_STOPS: [Rgb; 4];
pub const DEFAULT_ART: &str;
```

---

## 使用示例

### 场景一：独立微服务完整启动（真实用法，参考 `../cmx-flowengine` 的 flow-server main）

```rust
use cmx_web_chassis::{run, BannerSpec, ChassisConfig, ServiceSpec};

#[tokio::main]
async fn main() -> cmx_web_chassis::Result<()> {
    // 配置三级装配：flow-server.toml 兜底，FLOW_PORT 等环境变量覆盖
    let mut cfg = ChassisConfig::load("flow", "FLOW", "flow-server.toml");
    // 默认端口避让：未显式配置且仍是 8080 时改用服务专属端口
    if std::env::var("FLOW_PORT").is_err() && cfg.port == 8080 {
        cfg.port = 8091;
    }

    let spec = ServiceSpec::<()>::new("flow", cfg)
        .router(my_routes())               // Router<()>：流程 API 路由
        .state(())
        .nest_api(false)                   // flow-server 路由挂根（不经 /api 前缀）
        .banner(BannerSpec::defaults("flow"))  // 渐变横幅（非终端自动降级）
        .init("datasources", |_meta| Box::pin(async {
            register_dbs().await;          // 注册 PG 数据源（供监控页 DB 池面板）
            Ok(())
        }))
        .init("engine", |_meta| Box::pin(async {
            spawn_timer_poller().await     // 起流程定时器轮询
        }));
    run(spec).await
}
```

### 场景二：平台服务复用部分能力（不整体接管）

```rust
// 门户 web-server 有自己的中间件/静态托管，只复用「框架无关下半段」：
let cfg = ChassisConfig::load("portal", "PORTAL", "web-server.toml");

// ① 只要分层日志（guard 持有到 main 结束，否则文件日志丢尾部）
let _guard = cmx_web_chassis::init_tracing(&cfg);

// ② 只要中间件栈（在此基础上再叠认证/权限/cookie）
let app = cmx_web_chassis::default_layers(my_router_with_auth());

// ③ 只要优雅关闭语义（完全组装好的 app + listener 交进来）
let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
cmx_web_chassis::serve_with_shutdown(listener, app, cfg.graceful_timeout_secs).await?;
```

### 场景三：环境变量覆盖配置（部署常用）

```text
# ChassisConfig::load("flow", "FLOW", ...) 的优先级（高→低）：
#   1. FLOW_PORT=8095            ← 环境变量覆盖（部署/编排注入）
#   2. CONFIG_FILE=/etc/cmx.toml ← 统一配置文件（全服务同名约定，优先于前缀变量）
#      FLOW_CONFIG=flow.toml     ← 向后兼容的前缀形式
#   3. flow-server.toml          ← 内置默认（随工作目录）
#   4. defaults("flow")          ← 兜底：0.0.0.0:8080 / logs/flow.log / info / 10s
#
# 同理可用：FLOW_HOST / FLOW_LOG_DIR / FLOW_LOG_LEVEL / FLOW_GRACEFUL_SECS
```

---

## Features

无 `[features]`，本 crate 为通用骨架，不含可选编译特性。
