# cmx-flow-api

> 流程引擎的平台**反代薄壳**（proxy-only）：把门户 `/api/flow/*` 与流程拥有的前端页取页请求透明转发到独立流程微服务 `../cmx-flowengine` 的 `cmx-flow-server`，前端零改。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-flow-api` 是 cmx-container 平台中流程引擎域的 HTTP 反向代理壳。流程引擎核心 crate（`cmx-flow-model/-bpmn/-engine/-store-pg/-def/-app`）已整体迁至独立 workspace `../cmx-flowengine`，由那边的 `cmx-flow-server` 作为**独立流程微服务**承载。门户不再进程内嵌引擎，本 crate 因此**不依赖 `cmx-flow-app`**——这条曾经的跨 workspace path 依赖会把整个引擎源码拖进门户编译图（改前 `cargo run -p cmx-portal-server` 会连带编译 cmx-flowengine 的 `.rs`），现已彻底切断。

本 crate 只含反代，对外导出两件东西：

- `FlowProxyModule`：实现 `cmx-api-core` 的 `ModuleRoutes` 契约，把平台 `/api/flow/*` 透明转发到远程 flow-server。路径映射为 `/flow/{rest}` → `{flow_base}/api/flow/v1/{rest}`（**升级到 v1 正式契约**），query 原样透传，body 双向流式（SSE 逐块透传）。
- `with_flow_page_proxy`：页面反代中间件层，把**流程拥有的** native/html 单页取页请求（如 `/api/native-pages/portal.flow.todo`）转发到 flow-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。

是否挂流程路由只看 `[center_client]` 的服务定位配置（per-key：`services.flow` 配 url 静态基址或 discovery Nacos 选例）——配了才挂，**前端零改**（浏览器仍请求同源 `/api/flow/...`）。目标经 `UpstreamResolver` 按请求动态解析（静态基址 / Nacos 选例），无可用实例返回 503。

### 三层出站鉴权

转发时对齐平台既有 `remote_importers::apply_auth_headers` 三层注入：

| 头 | 来源 | 作用 |
|----|------|------|
| `X-API-Key` | `[service_auth].outgoing_api_key` | 平台服务身份 |
| `X-Delegated-User-Token: Bearer <JWT>` | `cmx_traits::auth::context_scope::current_original_token()` | 当前登录用户原始令牌（on-behalf-of，真实办理人） |
| `X-Request-Id` | `cmx_traits::auth::context_scope::current_request_id()` | 链路追踪 |

flow-server 的 S6 认证桥据此：API Key 验服务身份，委托令牌解真实用户 + 租户。

### 转发核行为（cmx-proxy-core，各域反代壳共用）

壳只负责路径重写与页面归属判定，转发行为统一在 `cmx-proxy-core` 一处定义（详见其 README）：

- **头卫生**：出站前剥除客户端可伪造的 `X-API-Key` / `X-Delegated-User-Token` / `X-Request-Id` 与 `Cookie`（防伪造服务身份/委托令牌打穿内部服务），随后从可信源重新注入上表三层；补齐 `X-Forwarded-For/Proto/Host`。
- **超时语义**：只设连接超时 5s + 读空闲超时 60s，**不设总超时**——SSE/长轮询等流式响应只要持续有数据就不被掐断。
- **响应头 append 语义**：多值头（`Set-Cookie` 等）全保留。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | API 共享骨架层（`CmxAppState` / `routes::traits::ModuleRoutes`），单向依赖避免环 |
| `cmx-proxy-core` | 反代转发核（头卫生/超时拆分/流式转发/三层出站鉴权，各域反代壳共用） |
| `axum` | Web 框架（Router / Request / Response） |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-flow-api = { workspace = true }` | 门户组装层 `merge_flow`：`flow_upstream()`（`[center_client.services].flow` per-key 定位）非空时 merge `FlowProxyModule::routes()` 并叠加 `with_flow_page_proxy` 页面反代层；未配置则不挂流程路由 |

被反代的微服务（本 crate 编译期不可见）：`../cmx-flowengine` 的 `cmx-flow-server`。

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| API 反代 | `/flow` 与 `/flow/{*rest}` 全方法（any）转发到 `{flow_base}/api/flow/v1/{rest}`，query 原样透传 |
| 页面反代 | 按 id 归属判定：`portal.flow.*`（native）与 `fi.cmxfico.gl.flow-*`（html）命中转发，其余 `next.run` 落回门户 handler |
| 页面恒等转发 | 页面反代**不升 /v1**（flow-server 页面挂在 `/api/native-pages`、`/api/html-pages`），转发 `{flow_base}/api{path}{query}` |
| 三层出站鉴权 | X-API-Key + X-Delegated-User-Token + X-Request-Id（见上表），注入前剥除客户端伪造值 |
| 双向流式转发 | 请求/响应体均 `Body::wrap_stream` / `Body::from_stream`，不整体缓冲；`text/event-stream` 逐块透传 |
| 逐跳头剥离 + X-Forwarded-* | 剥 RFC 7230 §6.1 逐跳头 + host + content-length，补 `X-Forwarded-For/Proto/Host`，其余请求头（含 Authorization）原样透传 |
| 502 错误信封 | 远端不可达时返回 `502` + `{ "code": 502, "msg": "流程服务不可达: ..." }` |
| 客户端复用 | 转发核内置 `reqwest::Client`（连接 5s + 读空闲 60s，不设总超时保 SSE 长流），构建失败退回默认客户端 |

---

## 模块结构

```text
cmx-flow-api
├── src
│   ├── lib.rs     # 模块声明与导出（pub use proxy::{FlowProxyModule, UpstreamResolver, with_flow_page_proxy}）
│   └── proxy.rs   # 反代实现：FlowProxyModule 路由 + flow_target 升 v1 重写 + 页面反代中间件（转发核在 cmx-proxy-core）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/proxy.rs —— 反代模块（持目标 resolver + 出站凭证 + 连接池）
pub struct FlowProxyModule { /* inner: Arc<ProxyCore> */ }
impl FlowProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self;
}

impl ModuleRoutes for FlowProxyModule {
    fn routes(self) -> Router<CmxAppState>;   // /flow 与 /flow/{*rest}，自持 State
    fn prefix() -> &'static str;              // "flow"
    fn module_name(&self) -> &'static str;    // "flow-proxy"
}

/// 页面反代层：给 api 路由叠加中间件，流程拥有的 native/html 单页请求转发 flow-server。
pub fn with_flow_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState>;
```

---

## 使用示例

### 场景一：门户组装层按配置挂载（真实用法，参考 `cmx-platform-app/src/routes.rs` 的 `merge_flow`）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_flow_api::{FlowProxyModule, with_flow_page_proxy};

/// 目标来自 `[center_client]` 服务定位配置（per-key）；未配置（None）则不挂流程路由。
fn merge_flow(router: Router<CmxAppState>, upstream: Option<cmx_plugin::center_client::ProxyUpstream>) -> Router<CmxAppState> {
    match upstream {
        Some(upstream) => {
            // 出站服务凭证：[service_auth].outgoing_api_key（可空）
            let api_key = load_outgoing_credential();
            // resolver：Static 固化基址；Discovery 每请求查实例缓存选例（捕获启动期配置快照）
            let resolver = upstream.resolver_fn();
            // ① merge 反代模块：/api/flow/* → {base}/api/flow/v1/*
            let router = router.merge(FlowProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes());
            // ② 叠加页面反代层：portal.flow.* / fi.cmxfico.gl.flow-* 单页请求也转发过去
            with_flow_page_proxy(router, resolver, api_key)
        }
        None => router, // 未配置 → 门户无 /api/flow/* 路由（需启动独立 flow-server 并配置地址）
    }
}
```

### 场景二：手工构造并验证转发行为

```rust
use cmx_flow_api::FlowProxyModule;

// 静态基址包成 resolver（基址末尾多余 `/` 会被 trim）；api_key 为 None 时出站不带 X-API-Key 头
let resolver: cmx_flow_api::UpstreamResolver = std::sync::Arc::new(|| Some("http://flow-server:8091".into()));
let module = FlowProxyModule::with_resolver(resolver, Some("svc-key-001".into()));

// ModuleRoutes 契约元信息（对 web-server 与内嵌壳同构）
assert_eq!(module.module_name(), "flow-proxy");
assert_eq!(<FlowProxyModule as cmx_api_core::routes::traits::ModuleRoutes>::prefix(), "flow");

// 浏览器请求 POST /api/flow/definitions?draft=true（本进程已剥 /api，path=/flow/definitions）
// → flow_target 剥 /flow/ 前缀得 rest=「definitions」，升到 v1 正式契约：
//   转发 POST {flow_base}/api/flow/v1/definitions?draft=true（/v1 升级由壳完成，浏览器无感）
```

### 场景三：SSE 长连接透传

```rust
// 前端订阅流程事件流：GET /api/flow/v1/instances/{id}/events（SSE）
// FlowProxyModule 的转发核对 body 做流式透传：
// - 请求侧：reqwest::Body::wrap_stream(req.into_body().into_data_stream())
// - 响应侧：axum::Body::from_stream(resp.bytes_stream())
//   且 content-type: text/event-stream 原样回传，逐块透传不缓冲，
//   配合转发核的超时语义（只设连接 5s + 读空闲 60s、不设总超时），
//   事件流只要持续有数据就不被掐断，长轮询/事件流在反代下行为与直连一致。
```

---

## Features

无 `[features]`，本 crate 为纯反代薄壳，不含可选编译特性。
