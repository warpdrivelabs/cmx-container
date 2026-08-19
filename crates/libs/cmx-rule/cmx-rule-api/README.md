# cmx-rule-api

> 决策规则引擎的平台**反代薄壳**（proxy-only，无内嵌）：把门户 `/api/rules/*` 与规则拥有的前端页取页请求透明转发到独立规则微服务 `../cmx-rulesengine` 的 `cmx-rule-server`，前端零改。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-rule-api` 是 cmx-container 平台中决策规则引擎域的 HTTP 反向代理壳。规则引擎整体位于独立 workspace `../cmx-rulesengine`，由那边的 `cmx-rule-server` 作为**独立规则微服务**承载。与 flow/report 不同，规则引擎**没有进程内嵌壳**（始终独立微服务）：`[center_client.urls].rules` 配了才挂本反代，`/api/rules/*` 透明转发到远程 cmx-rule-server；不配则门户无规则路由（规则页无法加载）。

规则微服务对外 URL 与平台一致（`/api/rules/v1/*`，无路径重写），故转发是**恒等映射** `{rules_base}/api{原path}{query}`——与 cmx-rpt-api 同构，不重写路径段。本 crate 对外导出两件东西：

- `RulesProxyModule`：实现 `cmx-api-core` 的 `ModuleRoutes` 契约，覆盖 `/rules` 与 `/rules/{*rest}`，全方法转发。
- `with_rules_page_proxy`：页面反代中间件层，把**规则拥有的** native 单页取页请求（`portal.rules.*`）转发到 cmx-rule-server，其余页请求落回门户内嵌 handler。

### 三层出站鉴权

转发时对齐平台既有三层注入（与 cmx-flow-api / cmx-rpt-api 完全一致）：

| 头 | 来源 | 作用 |
|----|------|------|
| `X-API-Key` | `[service_auth].outgoing_api_key` | 平台服务身份 |
| `X-Delegated-User-Token: Bearer <JWT>` | `cmx_traits::auth::context_scope::current_original_token()` | 当前登录用户原始令牌（on-behalf-of，真实操作人） |
| `X-Request-Id` | `cmx_traits::auth::context_scope::current_request_id()` | 链路追踪 |

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | API 共享骨架层（`CmxAppState` / `routes::traits::ModuleRoutes`），单向依赖避免环 |
| `cmx-traits` | 三层出站鉴权上下文（`context_scope` 的 original_token / request_id） |
| `axum` | Web 框架（Router / Request / Response / 中间件） |
| `reqwest` | 出站 HTTP（转发到远程 cmx-rule-server，流式请求/响应体，SSE 透传） |
| `serde_json` / `tracing` | 502 错误信封 JSON / 转发失败日志 |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-rule-api = { workspace = true }` | 门户组装层 `merge_rules`：`rules_remote_base()` 非空时 merge `RulesProxyModule::routes()` 并叠加 `with_rules_page_proxy` 页面反代层 |

被反代的微服务（本 crate 编译期不可见）：`../cmx-rulesengine` 的 `cmx-rule-server`。

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| API 反代（恒等映射） | `/rules` 与 `/rules/{*rest}` 全方法（any）转发到 `{rules_base}/api{path}{query}`，query 原样透传 |
| 页面反代（仅 native） | `portal.rules.*` 命中转发；`page_id_of` 只匹配 `/native-pages/` 前缀（规则域无 html 页），batch/list 不拦截，未命中 `next.run` 落回门户 handler |
| 共用转发核 | `proxy_handler`（API 反代）与 `page_proxy_mw`（页面反代）共用同一 `forward()` 转发核 |
| 三层出站鉴权 | X-API-Key + X-Delegated-User-Token + X-Request-Id（见上表） |
| 双向流式转发 | 请求体 `reqwest::Body::wrap_stream`、响应体 `Body::from_stream`，不整体缓冲；SSE 逐块透传 |
| 逐跳头剥离 | 剥 RFC 7230 §6.1 逐跳头 + host + content-length，其余请求头原样透传 |
| 502 错误信封 | 远端不可达时返回 `502` + `{ "code": 502, "msg": "规则服务不可达: ..." }` |
| 客户端复用 | 内置 `reqwest::Client`（30s 超时），构建失败退回默认客户端 |

---

## 模块结构

```text
cmx-rule-api
├── src
│   ├── lib.rs     # 模块声明与导出（pub use proxy::{with_rules_page_proxy, RulesProxyModule}）
│   └── proxy.rs   # 反代实现：RulesProxyModule 路由 + forward 转发核 + 逐跳头过滤 + 页面反代中间件
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/proxy.rs —— 反代模块（持远程基址 + 出站凭证 + 复用 HTTP 客户端）
pub struct RulesProxyModule { /* inner: Arc<ProxyState> */ }
impl RulesProxyModule {
    /// 用远程基址 + 出站 API Key 构建；基址末尾多余 `/` 会去掉。
    pub fn new(rules_base: impl Into<String>, api_key: Option<String>) -> Self;
}

impl ModuleRoutes for RulesProxyModule {
    fn routes(self) -> Router<CmxAppState>;   // /rules 与 /rules/{*rest}，自持 State
    fn prefix() -> &'static str;              // "rules"
    fn module_name(&self) -> &'static str;    // "rules-proxy"
}

/// 给 api 路由叠加规则页面反代层：规则拥有的 native 单页请求转发 cmx-rule-server。
pub fn with_rules_page_proxy(
    router: Router<CmxAppState>,
    rules_base: impl Into<String>,
    api_key: Option<String>,
) -> Router<CmxAppState>;
```

---

## 使用示例

### 场景一：门户组装层按配置挂载（真实用法，参考 `cmx-platform-app/src/routes.rs` 的 `merge_rules`）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_rule_api::{RulesProxyModule, with_rules_page_proxy};

/// 远程基址来自 `[center_client.urls].rules`；未配置（None）则不挂规则路由。
fn merge_rules(router: Router<CmxAppState>, rules_base: Option<String>) -> Router<CmxAppState> {
    match rules_base {
        Some(base) => {
            // 出站服务凭证：[service_auth].outgoing_api_key（可空）
            let api_key = load_outgoing_credential();
            // ① merge 反代模块：/api/rules/v1/* → {base}/api/rules/v1/*（恒等）
            let router = router.merge(RulesProxyModule::new(base.clone(), api_key.clone()).routes());
            // ② 叠加页面反代层：portal.rules.* native 单页请求也转发过去
            with_rules_page_proxy(router, base, api_key)
        }
        None => router, // 规则引擎无内嵌壳：不配则门户无 /api/rules/* 路由（规则页无法加载）
    }
}
```

### 场景二：手工构造并验证转发行为

```rust
use cmx_rule_api::RulesProxyModule;

// 基址末尾多余 `/` 会被 trim；api_key 为 None 时出站不带 X-API-Key 头
let module = RulesProxyModule::new("http://rule-server:8094", None);

// ModuleRoutes 契约元信息
assert_eq!(module.module_name(), "rules-proxy");
assert_eq!(
    <RulesProxyModule as cmx_api_core::routes::traits::ModuleRoutes>::prefix(),
    "rules"
);

// 浏览器请求 GET /api/rules/v1/definitions（本进程已剥 /api，path=/rules/v1/definitions）
// → 转发 GET {rules_base}/api/rules/v1/definitions，恒等映射不重写路径段
```

### 场景三：页面归属判定与 edition 2024 let-chains

```rust
// page_proxy_mw 的核心判定（源码真实写法，edition 2024 的 let-chains 特性）：
//   if let Some(id) = page_id_of(req.uri().path())
//       && is_rules_owned_page(id)
//   {
//       return forward(&px, req).await;   // portal.rules.* → 转发 rules-server
//   }
//   next.run(req).await                    // 其余 → 落回门户内嵌 handler
//
// 归属清单只有一条前缀（与 cmx-rulesengine/web 的清单一致）：
//   portal.rules.*  → 规则引擎拥有的 native 页（如规则设计器）
// batch（/native-pages/batch）与列表端点不拦截——含混合 id，留门户聚合。
```

---

## Features

无 `[features]`，本 crate 为纯反代薄壳，不含可选编译特性。
