# cmx-rpt-api

> 报表域的平台**反代薄壳**（proxy-only）：把门户 `/api/report-design/*`、`/api/report-source-bindings/*`、`/api/rpt/*`、`/api/consol/*`（合并报表）与报表拥有的前端页取页请求透明转发到独立报表微服务 `../cmx-report` 的 `cmx-rpt-server`，前端零改。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-rpt-api` 是 cmx-container 平台中报表域的 HTTP 反向代理壳。报表中立核 crate 已整体迁至独立 workspace `../cmx-report`，由那边的 `cmx-rpt-server` 作为**独立报表微服务**承载。门户不再进程内嵌报表引擎，本 crate 因此**不依赖 `cmx-rpt-app`**——这条依赖会把报表引擎源码拖进门户编译图，现已彻底切断（门户编译期不碰报表引擎源码），仅保留平台反代层。

「后端一芯双壳」在报表域的形态：`ReportModule`（进程内嵌，本进程 handler 处理）↔ `ReportProxyModule`（引擎在远程 `cmx-rpt-server`，透明转发）。二者对 web-server 是同一个 `ModuleRoutes` 契约、同一批报表前缀——**前端零改**，切换只看 `[center_client]` 的服务定位配置（per-key：`services.report` 配 url 静态基址或 discovery Nacos 选例）。目标经 `UpstreamResolver` 按请求动态解析，无可用实例返回 503。

与 flow 壳的关键差异：报表微服务对外 URL 与平台**完全一致**（无 `/v1` 升级），故转发是**恒等映射** `{report_base}/api{原path}{query}`，不重写任何路径段。本 crate 对外导出两件东西：

- `ReportProxyModule`：实现 `cmx-api-core` 的 `ModuleRoutes` 契约，覆盖报表四前缀（`/report-design`、`/report-source-bindings`、`/rpt`、`/consol`），全方法转发。
- `with_report_page_proxy`：页面反代中间件层，把**报表拥有的** native/html 单页取页请求转发到 cmx-rpt-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。

### 合并报表（consol）支持

合并报表工作台整体由独立报表微服务承载，本壳为其补齐门户侧入口：

- **转发路由**：`/consol` 与 `/consol/{*rest}` 恒等转发到 `{report_base}/api/consol/*`（cmx-rpt-server 已在 `/api` 下挂 consol_routes），页面取数不再 404。
- **页面白名单**：`portal.consol.*`（合并报表工作台 native 页）纳入 `is_report_owned_page` 归属判定；与 `cmx-common-api` 页面 handler 的属主路由表 `owner_service_of` 一一对应，新增前缀两处同步。
- **元数据定义**：合并报表模型定义见平台数据蓝本 `assets/model/data/meta/definitions/fi/cmxfico/consol/`（`cmxfico_consol_dct_meta_v1.json`）。

### 三层出站鉴权

转发时对齐平台既有 `remote_importers::apply_auth_headers` 三层注入（与 cmx-flow-api 一致）：

| 头 | 来源 | 作用 |
|----|------|------|
| `X-API-Key` | `[service_auth].outgoing_api_key` | 平台服务身份 |
| `X-Delegated-User-Token: Bearer <JWT>` | `cmx_traits::auth::context_scope::current_original_token()` | 当前登录用户原始令牌（on-behalf-of，真实操作人） |
| `X-Request-Id` | `cmx_traits::auth::context_scope::current_request_id()` | 链路追踪 |

出站前剥除客户端可伪造的同名头与 `Cookie`，再从可信源重新注入（见转发核 `cmx-proxy-core` 的头卫生）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | API 共享骨架层（`CmxAppState` / `routes::traits::ModuleRoutes`），单向依赖避免环 |
| `cmx-proxy-core` | 反代转发核（头卫生/超时拆分/流式转发/三层出站鉴权，各域反代壳共用） |
| `axum` | Web 框架（Router / Request / Response / 中间件） |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-rpt-api = { workspace = true }` | 门户组装层 `merge_report`：`report_upstream()`（`[center_client.services].report` per-key 定位）非空时 merge `ReportProxyModule::routes()` 并叠加 `with_report_page_proxy` 页面反代层；未配置则不挂报表路由 |

被反代的微服务（本 crate 编译期不可见）：`../cmx-report` 的 `cmx-rpt-server`。

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| API 反代（恒等映射） | `/report-design`、`/report-source-bindings`（根+子路径）、`/consol`（根+子路径）与 `/rpt/{*rest}` 全方法（any）转发到 `{report_base}/api{path}{query}`，query 原样透传 |
| 合并报表（consol） | `/api/consol/*` 恒等转发（cmx-rpt-server 已挂 consol_routes）+ `portal.consol.*` 页面白名单 + 合并报表元数据定义（见上节） |
| 页面反代（按 id 归属判定） | `portal.rpt.*`、`portal.consol.*`（native）与 `fi.cmxfico.gl.rpt-designer-*`、`fi.cmxfico.gl.rpt-spreadjs-designer-*`（html）命中转发；归属表与 `cmx-common-api` 的 `owner_service_of` 两处同步；batch/list 不拦截，未命中 `next.run` 落回门户 handler |
| 共用转发核 | `proxy_handler`（API 反代）与 `page_proxy_mw`（页面反代）共用同一 `ProxyCore`（基址 resolver/凭证/客户端一份）；头卫生/超时/流式转发在 `cmx-proxy-core` 一处定义 |
| 三层出站鉴权 | X-API-Key + X-Delegated-User-Token + X-Request-Id（见上表），注入前剥除客户端伪造值 |
| 双向流式转发 | 请求体 `reqwest::Body::wrap_stream`、响应体 `Body::from_stream`，不整体缓冲；`text/event-stream` 逐块透传 |
| 逐跳头剥离 + X-Forwarded-* | 剥 RFC 7230 §6.1 逐跳头 + host + content-length，补 `X-Forwarded-For/Proto/Host`，其余请求头（含 Authorization）原样透传 |
| 502 错误信封 | 远端不可达时返回 `502` + `{ "code": 502, "msg": "报表服务不可达: ..." }` |
| 客户端复用 | 转发核内置 `reqwest::Client`（连接 5s + 读空闲 60s，不设总超时保 SSE 长流），构建失败退回默认客户端 |

---

## 模块结构

```text
cmx-rpt-api
├── src
│   ├── lib.rs     # 模块声明与导出（pub use proxy::{ReportProxyModule, UpstreamResolver, with_report_page_proxy}）
│   └── proxy.rs   # 反代实现：ReportProxyModule 四前缀路由 + report_target 恒等重写 + 页面反代中间件（转发核在 cmx-proxy-core）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/proxy.rs —— 反代模块（持目标 resolver + 出站凭证 + 连接池）
pub struct ReportProxyModule { /* inner: Arc<ProxyCore> */ }
impl ReportProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self;
}

impl ModuleRoutes for ReportProxyModule {
    fn routes(self) -> Router<CmxAppState>;   // /report-design、/report-source-bindings、/rpt、/consol 四前缀，自持 State
    fn prefix() -> &'static str;              // "report"
    fn module_name(&self) -> &'static str;    // "report-proxy"
}

/// 给 api 路由叠加报表页面反代层：报表拥有的 native/html 单页请求转发 cmx-rpt-server。
pub fn with_report_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState>;
```

---

## 使用示例

### 场景一：门户组装层按配置挂载（真实用法，参考 `cmx-platform-app/src/routes.rs` 的 `merge_report`）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_rpt_api::{ReportProxyModule, with_report_page_proxy};

/// 目标来自 `[center_client]` 服务定位配置（per-key）；未配置（None）则不挂报表路由。
fn merge_report(router: Router<CmxAppState>, upstream: Option<cmx_plugin::center_client::ProxyUpstream>) -> Router<CmxAppState> {
    match upstream {
        Some(upstream) => {
            // 出站服务凭证：[service_auth].outgoing_api_key（可空）
            let api_key = load_outgoing_credential();
            // resolver：Static 固化基址；Discovery 每请求查实例缓存选例（捕获启动期配置快照）
            let resolver = upstream.resolver_fn();
            // ① merge 反代模块：/api/report-design/* 等四前缀 → {base}/api/report-design/*（恒等）
            let router = router.merge(ReportProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes());
            // ② 叠加页面反代层：portal.rpt.* / fi.cmxfico.gl.rpt-designer-* 单页请求也转发过去
            with_report_page_proxy(router, resolver, api_key)
        }
        None => router, // 未配置 → 门户无 /api/report-design/* 路由
    }
}
```

### 场景二：手工构造并验证转发行为

```rust
use cmx_rpt_api::ReportProxyModule;

// 静态基址包成 resolver；api_key 为 None 时出站不带 X-API-Key 头
let resolver: cmx_rpt_api::UpstreamResolver = std::sync::Arc::new(|| Some("http://report-server:8092".into()));
let module = ReportProxyModule::with_resolver(resolver, Some("svc-key-001".into()));

// ModuleRoutes 契约元信息（与内嵌壳对 web-server 同构）
assert_eq!(module.module_name(), "report-proxy");
assert_eq!(
    <ReportProxyModule as cmx_api_core::routes::traits::ModuleRoutes>::prefix(),
    "report"
);

// 浏览器请求 GET /api/report-design/overview（本进程已剥 /api，path=/report-design/overview）
// → 转发 GET {report_base}/api/report-design/overview，恒等映射不重写路径段；
//   合并报表取数同理：GET /api/consol/{rest} → {report_base}/api/consol/{rest}
```

### 场景三：混合 id 的页面请求不拦截（共享端点按 id 归属判定）

```rust
// /api/native-pages 是共享端点：一部分页属门户、一部分属报表。
// page_proxy_mw 只拦截「单页取页且 id 命中报表前缀」的请求：
//   GET /api/native-pages/portal.rpt.designer        → 命中 is_report_owned_page → 转发 report-server
//   GET /api/native-pages/portal.consol.workbench    → 命中（合并报表工作台）→ 转发 report-server
//   GET /api/native-pages/portal.workspace.home  → 未命中 → next.run 落回门户内嵌 handler
//   POST /api/native-pages/batch                 → 不拦截（含混合 id，留门户聚合）
// report-server 返回逐字节一致的页面源（rev 一致，ETag/缓存不错位），shell 零感知。
```

---

## Features

无 `[features]`，本 crate 为纯反代薄壳，不含可选编译特性。
