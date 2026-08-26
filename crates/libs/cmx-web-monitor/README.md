# cmx-web-monitor

> 通用**技术监控**（对标 cmx-web-chassis 的通用性）：任何服务与本 crate 组装即得统一技术盘——请求遥测中间件（环形缓冲）、sysinfo 后台系统采样、DB 连接池快照、服务依赖拓扑活体探测，经 `/_mon` 页 + `/_mon/tech-stats` 端点对外。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-web-monitor` 与「业务监控」正交：业务盘各服务自持（如 flow 的流程盘），技术盘一处实现处处可用。CMX 拆出多个独立微服务后，每个服务都需要回答同一组技术问题——「谁在调我（身份/IP/协议）、系统资源如何（CPU/内存/网络/磁盘）、DB 池够不够用、我依赖的远程服务活着吗」——本 crate 用零业务依赖的方式统一回答。

四个观测维度：

- **请求遥测**：`observe` 中间件采集每请求全维度（方法/路径/参数/协议/客户端 IP/UA/认证方式/租户/用户/角色/状态/耗时/响应字节）进进程级环形缓冲（cap 500）+ 原子计数，**并同步输出一行式访问日志**（target `cmx_access`，级别按状态分级：5xx=error / 4xx=warn / 其余=info；`[server].log_level = "info,cmx_access=off"` 或 RUST_LOG 同语法可关）；SSE 长连接经 `sse_connect` / `sse_disconnect` 单独计数。
- **系统指标**：`spawn_system_sampler` 后台任务（幂等，**3s 间隔**）用 sysinfo 刷新快照——**绝不在请求路径调 sysinfo::refresh**（刷新是重操作，会拖慢请求）；网络速率 = 相邻采样差 / 间隔。
- **DB 连接池**：读平台 PG 单例 `cmx_database_pg::get_default_pg_db_manager()` 各数据源 deadpool 池计数（零查询开销），`inUse = size - available` 派生。
- **依赖拓扑**：服务声明自己依赖的能力（`ServiceDep`：key/label/mode embedded|proxy/target），`spawn_topology_prober` 每 **10s** 对 proxy 目标打 `GET {target}/_mon/tech-stats`（3s 超时）判活并测往返延迟——CMX「一芯双壳」部署形态下，门户可据此展示 flow/report 等远程引擎的活体状态。

**依赖刻意轻**：不依赖 cmx-web-chassis（避免环——chassis 反过来依赖本 crate）、不依赖 cmx-api/平台业务；两个「服务差异点」经**零捕获自由函数指针注入**解耦：`set_identity_provider(fn() -> Option<Identity>)`（身份：各服务 scope 不同）与 `set_topology_provider(fn() -> Vec<ServiceDep>)`（拓扑：各服务依赖不同）。未注入则遥测身份记匿名、拓扑面板显「无依赖」。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-database-pg` | DB 池快照：`get_default_pg_db_manager().pool_statuses()` |
| `axum` | 中间件（from_fn）/ 路由 / handler |
| `sysinfo` | 系统/进程指标采样（后台任务） |
| `reqwest` | 拓扑探测器对 proxy 目标的活体 HTTP 探测 |
| `serde` / `serde_json` | 指标/快照 JSON（camelCase 序列化对齐前端） |
| `tokio` / `tracing` | 后台任务 / 日志 |

### 下游使用方（谁依赖本 crate）

| 使用方 | 仓库 | 实际用途 |
|--------|------|---------|
| `cmx-web-chassis` | 本仓库 | `run()` 默认启用监控：merge `monitor_routes` + 起采样器/探测器（chassis → monitor 单向） |
| `cmx-platform-app` | 本仓库 | 平台服务接线 |
| `cmx-flow-app` / `cmx-flow-server` | `../cmx-flowengine` | `cmx-flow-app/src/observe.rs` re-export `observe`/`client_stats`/`sse_connect`/`sse_disconnect`；flow-server 注入身份/拓扑 provider |
| `cmx-rule-app` / `cmx-rule-server` | `../cmx-rulesengine` | 规则引擎服务同款接线 |
| `cmx-rpt-server` | `../cmx-report` | 报表微服务同款接线 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| observe 中间件 | 每请求采集进 `CallRecord`（15+ 字段）环形缓冲，同时输出一行式访问日志（target `cmx_access`，按状态分级，可经日志过滤关闭）；**建议夹在认证中间件内层**（`.layer(observe).layer(auth)`）使身份 scope 已建立 |
| 聚合快照 | `requests_snapshot()`：overview（总量/错误率/平均与最大延迟/QPS/运行时长/SSE 活跃）+ byProtocol/byAuth/byClient(top10)/byUser(top10)/byEndpoint(top12) + recent(倒序 100) |
| SSE 计数 | `sse_connect()` / `sse_disconnect()` 由服务 SSE handler 入口/流 Drop 时调，活跃与累计分开计 |
| 系统采样 | `SystemMetrics`（camelCase）：procMem/procCpu/procUptime + hostMem/hostCpu/负载/网络速率/磁盘列表；3s 后台采样，请求只读快照 |
| DB 池快照 | `pool_snapshot()`：`[{dbId,maxSize,size,available,waiting,inUse}]`，复用已注册数据源，零查询开销 |
| 拓扑声明 | `ServiceDep { key, label, mode: "embedded"/"proxy", target, proxiable }`——同一能力的内嵌/反代两种形态统一描述 |
| 活体探测 | `spawn_topology_prober()`（幂等，10s 周期）：GET proxy 目标 `/_mon/tech-stats`（reqwest 3s 超时），解析远端 uptime 与服务名；embedded 能力不探（恒可达） |
| 探测结果 | `ProbeResult { reachable, latency_ms, remote_service, remote_uptime_secs, error, checked_at_ms }` |
| 身份钩子 | `set_identity_provider` + `current_identity()`（未注入/不在 scope → None → 匿名） |
| 监控路由 | `monitor_routes()`：`GET /_mon`（自包含 HTML 大盘）+ `GET /_mon/tech-stats`（合并 JSON）+ `GET /_mon/deps`（轻量拓扑，供门户状态页轮询） |
| 统一信封 | `ApiResp<T> { code(0=成功), msg, data }`（camelCase，与 cmx-flow-app::resp 逐字节对齐，前端解析一致） |

---

## 模块结构

```text
cmx-web-monitor
├── src
│   ├── lib.rs        # monitor_routes()（/_mon、/_mon/tech-stats、/_mon/deps）+ 全部顶层再导出
│   ├── middleware.rs # CallRecord / observe / sse_connect / sse_disconnect / requests_snapshot / client_stats（环形缓冲 cap 500 + cmx_access 访问日志）
│   ├── system.rs     # SystemMetrics / DiskInfo / system_snapshot / spawn_system_sampler（3s 后台采样）
│   ├── topology.rs   # ServiceDep / ProbeResult / set_topology_provider / spawn_topology_prober（10s 探测）/ topology_snapshot
│   ├── identity.rs   # Identity / set_identity_provider / current_identity（零捕获 fn 指针注入）
│   ├── db.rs         # pool_snapshot（deadpool 池计数）
│   ├── handlers.rs   # tech_stats / deps_stats / tech_dashboard（HTML）/ set_service_name（OnceLock）
│   └── resp.rs       # ApiResp<T> 统一信封
├── assets
│   └── tech-dashboard.html  # 自包含大盘页（编译期内嵌，替换 __SVC_TITLE__ 占位）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/lib.rs —— 路由（挂根级、免认证；整组 nest 到别处也能工作）
pub fn monitor_routes<S>() -> Router<S> where S: Clone + Send + Sync + 'static;

// src/middleware.rs —— 请求遥测
pub struct CallRecord {
    pub seq: u64, pub at_ms: u64, pub method: String, pub path: String, pub query: String,
    pub protocol: String,            // HTTP/1.1、HTTP/2、SSE
    pub client_ip: String,           // X-Forwarded-For 链首 / X-Real-IP
    pub user_agent: String, pub auth: String,   // apikey|jwt|delegated|header|anon
    pub tenant: String, pub user: Option<String>, pub roles: Vec<String>,
    pub via_proxy: bool,             // 有 X-Request-Id + X-Delegated-User-Token
    pub request_id: Option<String>, pub status: u16,
    pub latency_ms: u64, pub resp_bytes: u64,
}
pub async fn observe(req: Request, next: Next) -> Response;   // 夹在认证内层
pub fn sse_connect(); pub fn sse_disconnect();
pub fn requests_snapshot() -> Value;
pub async fn client_stats() -> Json<ApiResp<Value>>;          // 兼容旧 flow /clients 端点

// src/system.rs
pub struct SystemMetrics { /* camelCase：procMemBytes / hostCpuPct / netRxPerSec / disks … */ }
pub struct DiskInfo { pub mount: String, pub total: u64, pub available: u64 }
pub fn system_snapshot() -> SystemMetrics;   // 后台未起/首刷未完 → 默认值
pub fn spawn_system_sampler();               // 幂等（AtomicBool），3s 周期

// src/topology.rs
pub struct ServiceDep { pub key: String, pub label: String,
    pub mode: String, pub target: Option<String>, pub proxiable: bool }
pub struct ProbeResult { pub reachable: bool, pub latency_ms: u64,
    pub remote_service: String, pub remote_uptime_secs: u64,
    pub error: Option<String>, pub checked_at_ms: i64 }
pub fn set_topology_provider(f: fn() -> Vec<ServiceDep>);
pub fn spawn_topology_prober();              // 幂等，10s 周期
pub async fn topology_snapshot() -> Value;   // deps + probe 合成 { services, nowMs }

// src/identity.rs
pub struct Identity { pub tenant: String, pub user: Option<String>, pub roles: Vec<String> }
pub fn set_identity_provider(f: fn() -> Option<Identity>);
pub fn current_identity() -> Option<Identity>;

// src/handlers.rs
pub fn set_service_name(name: impl Into<String>);   // OnceLock，大盘页标题
```

---

## 使用示例

### 场景一：独立服务完整接线（真实用法，参考 `../cmx-flowengine` 的 flow-server）

```rust
use cmx_web_monitor::{set_identity_provider, set_service_name, topology::ServiceDep};

// ① 服务名（监控页标题）
set_service_name("cmx-flow 流程引擎");
// ② 身份钩子：把本服务的鉴权 scope 映射为 Identity（零捕获函数指针）
set_identity_provider(cmx_flow_app::identity_snapshot);
// ③ 拓扑声明：flow 引擎在本进程为 embedded（不探，恒可达）
set_topology_provider(|| vec![ServiceDep {
    key: "flow".into(), label: "流程引擎".into(),
    mode: "embedded".into(), target: None, proxiable: true,
}]);
// ④ observe 夹在认证内层（先 auth 建 scope，再 observe 读身份）
let api = api_routes()
    .layer(axum::middleware::from_fn(cmx_web_monitor::observe))
    .layer(axum::middleware::from_fn(auth_mw));
// ⑤ 交给 cmx-web-chassis::run —— 它会 merge monitor_routes 并起采样器/探测器
```

### 场景二：门户声明 proxy 形态依赖（拓扑活体探测）

```rust
use cmx_web_monitor::topology::ServiceDep;

// 门户进程不内嵌流程/报表引擎时，声明为 proxy 形态并给出目标基址：
set_topology_provider(|| {
    let mut deps = vec![ServiceDep {
        key: "mdm".into(), label: "主数据".into(),
        mode: "embedded".into(), target: None, proxiable: false,
    }];
    if let Some(flow_base) = flow_remote_base() {
        deps.push(ServiceDep {
            key: "flow".into(), label: "流程引擎".into(),
            mode: "proxy".into(), target: Some(flow_base), proxiable: true,
        });
    }
    deps
});
// spawn_topology_prober 每 10s 对 proxy 目标 GET {target}/_mon/tech-stats：
//   可达 → ProbeResult { reachable: true, latency_ms, remote_service, remote_uptime_secs, .. }
//   不可达 → error: "timeout" / "connect refused" / "unreachable"
// 门户状态页轮询 GET /_mon/deps 即可画出各引擎红绿灯。
```

### 场景三：SSE 长连接遥测（服务侧两行接线）

```rust
use axum::response::sse::{Event, Sse};
use futures_util::Stream;

async fn events() -> Sse<impl Stream<Item = Result<Event, std::io::Error>>> {
    cmx_web_monitor::sse_connect();          // 入口计数：sse_active + 1、sse_total + 1
    let stream = my_event_stream().inspect_drop(|| {
        cmx_web_monitor::sse_disconnect();   // 流 Drop 时：sse_active - 1
    });
    Sse::new(stream)
}
// 大盘 overview 面板由此显示 sseActive（当前在线流）/ sseTotal（累计建立数）/
// distinctClients，区分「普通请求」与「常驻事件流」两类负载。
```

---

## Features

无 `[features]`，本 crate 为通用观测件，不含可选编译特性。
