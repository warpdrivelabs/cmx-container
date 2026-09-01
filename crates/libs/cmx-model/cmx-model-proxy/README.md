# cmx-model-proxy

> 模型中心的平台**反代薄壳**（proxy-only，无内嵌）：把门户模型中心七前缀 `/api/{dct,dict,doc,model,definitions,flexible-combination,code}/*` 与模型中心拥有的 `portal.model.*` native/html 页取页请求透明转发到独立模型中心微服务 `../cmx-model` 的 `cmx-model-server`（:8093），前端零改。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-model-proxy` 是 cmx-container 平台中模型中心域的 HTTP 反向代理壳。模型中心域库（dct/doc/code/master-slave 各域 crate + 中立核 `cmx-model-app`）已整体物理迁至独立 workspace `../cmx-model`，由那边的 `cmx-model-server` 作为**独立模型中心微服务**（:8093）承载，同时承载 dct/doc/model/code 四能力。门户不再进程内嵌模型中心，本 crate **不依赖任何模型中心引擎 crate**——门户编译期不碰引擎源码，仅保留平台反代层。

本 crate 只含反代，对外导出两件东西：

- `ModelProxyModule`：实现 `cmx-api-core` 的 `ModuleRoutes` 契约，覆盖模型中心七前缀 `/dct`、`/dict`、`/doc`、`/model`、`/definitions`、`/flexible-combination`、`/code`（各自根 + `{*rest}`），全方法转发。与报表一致：模型中心微服务对外 URL 与平台**完全一致**（无 `/v1` 升级），故转发是**恒等映射** `{model_base}/api{原path}{query}`——不重写任何路径段，与 cmx-rpt-api 同构。
- `with_model_page_proxy`：页面反代中间件层，把**模型中心拥有的** `portal.model.*` native/html 单页取页请求（如 `/api/native-pages/portal.model.definition.base-dct`、`/api/html-pages/portal.model.gl.dct-data-editor-html`）转发到 cmx-model-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。

是否挂模型中心路由只看 `[service_rpc.services].model` 的服务定位配置（per-key：`url` 静态基址或 `discovery` Nacos 选例，见 `cmx_service_rpc::locator`）——配了才挂反代；**没配 = 门户不挂模型中心路由**（统一语义，与 mdm/flow/report/rules 一致，无进程内嵌兜底）。目标经 `UpstreamResolver` 按请求动态解析（静态基址固化 / Nacos 实例缓存选例），无可用实例返回 503（区别于下游不可达的 502）。

### 三层出站鉴权

转发时对齐平台既有 `remote_importers::apply_auth_headers` 三层注入（各反代壳一致）：

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
| `cmx-proxy-core` | 反代转发核（头卫生/超时拆分/流式转发/三层出站鉴权，各域反代壳共用），见其 README |
| `axum` | Web 框架（Router / Request / Response / 中间件） |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-model-proxy = { workspace = true }` | 门户组装层 `merge_model`：`model_upstream()`（`[service_rpc.services].model`）非空时 merge `ModelProxyModule::routes()` 并叠加 `with_model_page_proxy` 页面反代层；未配置则不挂模型中心路由 |

被反代的微服务（本 crate 编译期不可见）：`../cmx-model` 的 `cmx-model-server`（:8093）。

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| API 反代（恒等映射） | 七前缀（`/dct`、`/dict`、`/doc`、`/model`、`/definitions`、`/flexible-combination`、`/code`，各根 + `{*rest}`）全方法（any）转发到 `{model_base}/api{path}{query}`，query 原样透传 |
| 页面反代（按 id 归属判定） | `portal.model.*`（native + html 统一前缀，与 `assets/model/web` 清单一致）命中转发；`/native-pages/batch` 等混合 id 端点不拦截，未命中 `next.run` 落回门户 handler。**不含** `portal.mdm.*`（已独立 `cmx-mdm-proxy`）与门户自有页 |
| 共用转发核 | `proxy_handler`（API 反代）与 `page_proxy_mw`（页面反代）共用同一 `ProxyCore`（目标 resolver/凭证/客户端一份）；头卫生/超时/流式转发在 `cmx-proxy-core` 一处定义 |
| 三层出站鉴权 | X-API-Key + X-Delegated-User-Token + X-Request-Id（见上表），注入前剥除客户端伪造值 |
| 双向流式转发 | 请求体 `reqwest::Body::wrap_stream`、响应体 `Body::from_stream`，不整体缓冲；`text/event-stream` 逐块透传 |
| 逐跳头剥离 + X-Forwarded-* | 剥 RFC 7230 §6.1 逐跳头 + host + content-length，补 `X-Forwarded-For/Proto/Host`，其余请求头（含 Authorization）原样透传 |
| 502 错误信封 | 远端不可达时返回 `502` + `{ "code": 502, "msg": "模型中心服务不可达: ..." }` |
| 客户端复用 | 转发核内置 `reqwest::Client`（连接 5s + 读空闲 60s，不设总超时保 SSE 长流），构建失败退回默认客户端 |

---

## 模块结构

```text
cmx-model-proxy
├── src
│   ├── lib.rs     # 模块声明与导出（pub use proxy::{ModelProxyModule, UpstreamResolver, with_model_page_proxy}）
│   └── proxy.rs   # 反代实现：ModelProxyModule 七前缀路由 + model_target 恒等重写 + 页面反代中间件
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/proxy.rs —— 反代模块（持目标 resolver + 出站凭证 + 连接池）
pub struct ModelProxyModule { /* inner: Arc<ProxyCore> */ }
impl ModelProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self;
}

impl ModuleRoutes for ModelProxyModule {
    fn routes(self) -> Router<CmxAppState>;   // 七前缀（各根 + {*rest}），自持 State
    fn prefix() -> &'static str;              // "model"
    fn module_name(&self) -> &'static str;    // "model-proxy"
}

/// 给 api 路由叠加模型中心页面反代层：模型中心拥有的 native/html 单页请求转发 cmx-model-server。
pub fn with_model_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState>;
```

---

## 使用示例

### 场景一：门户组装层按配置挂载（真实用法，参考 `cmx-platform-app/src/routes.rs` 的 `merge_model`）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_model_proxy::{ModelProxyModule, with_model_page_proxy};

/// 目标来自 `[service_rpc.services].model` 服务定位配置（per-key）；未配置（None）则不挂模型中心路由。
fn merge_model(router: Router<CmxAppState>, upstream: Option<cmx_service_rpc::Locator>) -> Router<CmxAppState> {
    match upstream {
        Some(upstream) => {
            // 出站服务凭证：[service_auth].outgoing_api_key（可空）
            let api_key = load_outgoing_credential();
            // resolver：Static 固化基址；Discovery 每请求查实例缓存选例（捕获启动期配置快照）
            let resolver = upstream.resolver_fn();
            // ① merge 反代模块：/api/dct/* 等七前缀 → {base}/api/dct/*（恒等）
            let router = router.merge(ModelProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes());
            // ② 叠加页面反代层：portal.model.* 单页请求也转发过去
            with_model_page_proxy(router, resolver, api_key)
        }
        None => router, // 未配置 → 门户不挂模型中心路由（需启动独立 cmx-model-server 并配置其地址）
    }
}
```

### 场景二：手工构造并验证转发行为

```rust
use cmx_model_proxy::ModelProxyModule;

// 静态基址包成 resolver；api_key 为 None 时出站不带 X-API-Key 头
let resolver: cmx_model_proxy::UpstreamResolver = std::sync::Arc::new(|| Some("http://model-server:8093".into()));
let module = ModelProxyModule::with_resolver(resolver, Some("svc-key-001".into()));

// ModuleRoutes 契约元信息（对 web-server 与独立微服务同构）
assert_eq!(module.module_name(), "model-proxy");
assert_eq!(<ModelProxyModule as cmx_api_core::routes::traits::ModuleRoutes>::prefix(), "model");

// 浏览器请求 GET /api/model/db-state（本进程已剥 /api，path=/model/db-state）
// → 转发 GET {model_base}/api/model/db-state，恒等映射不重写路径段
```

### 场景三：共享页面端点按 id 归属判定（混合页面不整前缀反代）

```rust
// /api/native-pages、/api/html-pages 是共享端点：一部分页属模型中心，其余是门户自有页或
// MDM 页（portal.mdm.* 归 cmx-mdm-proxy 另案）。page_proxy_mw 只拦截「单页取页且
// id 命中 portal.model. 前缀」的请求：
//   GET /api/native-pages/portal.model.definition.base-dct   → 命中 → 转发 model-server
//   GET /api/html-pages/portal.model.gl.dct-data-editor-html → 命中 → 转发 model-server
//   GET /api/native-pages/portal.workspace.home              → 未命中 → next.run 落回门户
//   POST /api/native-pages/batch                             → 不拦截（含混合 id，留门户聚合）
// model-server 返回逐字节一致的页面源（rev 一致，ETag/缓存不错位），shell 零感知。
```

---

## Features

无 `[features]`，本 crate 为纯反代薄壳，不含可选编译特性。
