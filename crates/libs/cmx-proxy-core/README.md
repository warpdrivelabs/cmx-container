# cmx-proxy-core

> 反向代理**转发核**——cmx-flow-api / cmx-rpt-api / cmx-rule-api 三个反代壳的公共实现：出站头卫生、三层出站鉴权、超时语义、流式转发、502/503 兜底一处定义一处修复。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

平台「后端一芯双壳」架构里，独立微服务域（流程/报表/规则）在门户侧各有一个反代薄壳，把平台同源 API 透明转发到远程微服务。三壳的转发逻辑原本各持一份复制（拼 URL 后的流式转发 + 头处理 + 出站鉴权 + 错误兜底），**超时、头卫生等行为修复需同步改三遍、漏一处即三服务行为分叉**——本 crate 将其收敛为单一转发核。

壳与核的分工：

| 层 | 职责 | 所在 |
|----|------|------|
| 壳 | `ModuleRoutes` 路由挂载 + **路径重写规则**（flow 升 `/v1`、rpt/rule 恒等映射）+ 页面归属判定（`portal.flow.*` 等按 id 反代单页） | cmx-flow-api / cmx-rpt-api / cmx-rule-api |
| 核 | 目标解析（`UpstreamResolver`）→ 出站头卫生 → 三层出站鉴权 → 流式转发 → 响应构建 → 502/503 兜底 | 本 crate |

本 crate 刻意保持**薄依赖**（axum + reqwest + cmx-traits + 日志/JSON），不依赖 cmx-api-core / cmx-plugin——转发核是 handler 级原语，与路由装配、服务定位配置解耦（目标经 `UpstreamResolver` 闭包注入：静态基址或 Nacos 服务发现由装配层 cmx-platform-app 决定）。

### 核心行为（P0 修复后的语义）

- **超时拆分**：只设连接超时 5s + 读空闲超时 60s，**不设总超时**——总超时是含响应体读完的硬期限，会掐断 SSE/长轮询等长流（原 30s 总超时对 `/events` 会在 30s 处硬切流）。读空闲超时只约束"两次数据之间的间隔"，流持续有数据就不受影响。
- **出站头卫生**：剥除客户端可伪造的平台注入型头（`X-API-Key` / `X-Delegated-User-Token` / `X-Request-Id`）与 `Cookie`（门户会话不下发内部服务），随后从可信源（配置 / task-local）重新注入——防止外部请求伪造服务身份/委托令牌打穿到内部微服务。
- **X-Forwarded-\* 补齐**：`For`（append 直连客户端 IP，取不到则保留原值）、`Proto`（缺省 http）、`Host`（从入站 Host 补），供下游获取真实客户端信息。
- **响应头 append 语义**：多值头（`Set-Cookie` 等）全保留，不因 `insert` 覆盖丢值。
- **双向流式转发**：请求体 `reqwest::Body::wrap_stream`、响应体 `Body::from_stream`，不整体缓冲；`text/event-stream` 逐块透传。
- **错误兜底**：无可用实例（服务发现未就绪/实例全下线）→ `503`；下游不可达 → `502`，均带 `{ "code": ..., "msg": ... }` JSON 信封。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `axum` | Web 框架（Request / Response / HeaderMap / 流式 body） |
| `reqwest` | 出站 HTTP 客户端（connect/read 超时拆分 + 流式转发） |
| `cmx-traits` | 三层出站鉴权上下文（`context_scope` 的 original_token / request_id） |
| `serde_json` / `tracing` | 502/503 错误信封 JSON / 转发失败日志 |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-flow-api` | `cmx-proxy-core = { workspace = true }` | FlowProxyModule 的转发核（API + 页面反代共用） |
| `cmx-rpt-api` | 同上 | ReportProxyModule 的转发核 |
| `cmx-rule-api` | 同上 | RulesProxyModule 的转发核 |

目标 resolver 由 `cmx-platform-app`（装配层）从 `cmx_service_rpc::Locator::resolver_fn` 构造后传入壳（`UpstreamResolver` 是结构类型别名，cmx-plugin 无需依赖本 crate）。

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| `ProxyCore::new` | 构建（resolver + 出站 API Key + 复用客户端）；连接 5s + 读空闲 60s，不设总超时 |
| `ProxyCore::forward` | 完整转发：解析目标 → 壳的 `rewrite` 闭包拼目标 URL → 头卫生 + 鉴权注入 → 流式转发 |
| `ProxyCore::no_upstream` | 无可用实例的 503 响应（静态方法，供壳复用） |
| `UpstreamResolver` | 目标 resolver 类型：`Arc<dyn Fn() -> Option<String> + Send + Sync>`，每请求动态解析 |
| 三层出站鉴权 | X-API-Key（服务身份）+ X-Delegated-User-Token（on-behalf-of 用户令牌）+ X-Request-Id（链路追踪），对齐 `remote_importers::apply_auth_headers` |
| 头卫生 | 剥逐跳/host/content-length + 注入型头/Cookie，补 X-Forwarded-*（见上"核心行为"） |
| 响应构建 | 状态 + 头（append 多值保留）+ 流式体，SSE 逐块透传 |

---

## 使用示例

### 壳侧：构建 + 转发（以 flow 的 /v1 升级重写为例）

```rust
use std::sync::Arc;
use cmx_proxy_core::{ProxyCore, UpstreamResolver};

// 装配层（cmx-platform-app）构造 resolver：静态基址或 Nacos 服务发现。
let resolver: UpstreamResolver = Arc::new(|| Some("http://127.0.0.1:8091".to_string()));
let core = Arc::new(ProxyCore::new(resolver, Some("outgoing-api-key".into())));

// handler 内：壳只提供路径重写闭包（域差异全在这一处）。
// async fn handler(State(px): State<Arc<ProxyCore>>, req: Request) -> Response {
//     px.forward("流程服务", req, |base, uri| {
//         format!("{base}/api/flow/v1{}", uri.path().strip_prefix("/flow").unwrap_or(""))
//     })
//     .await
// }
# let _ = core;
```

### 页面反代中间件（恒等映射）

```rust
// 页面不升 /v1：恒等转发 {base}/api{原path}{query}，同样走转发核。
// px.forward("流程服务", req, |base, uri| {
//     format!("{base}/api{}{}", uri.path(), uri.query().map(|q| format!("?{q}")).unwrap_or_default())
// })
# ()
```

---

## 模块结构

```text
src/
├── lib.rs       # crate 文档 + 导出（ProxyCore / UpstreamResolver）
├── core.rs      # 转发核：目标解析、出站构建、流式转发、502/503 兜底、超时语义
└── headers.rs   # 头卫生纯函数：出站剥除名单、X-Forwarded-* 补齐、响应头 append（6 个单测锁定行为）
```

---

## 测试

```bash
cargo test -p cmx-proxy-core
```

6 个单测锁定头处理行为：注入型/会话/逐跳头剥除、X-Forwarded-For append 语义（含无 peer 退化）、Proto/Host 补齐、响应多值头 append 保留。转发路径（`forward`）依赖真实网络不做单测，由三壳的静态冒烟覆盖。

---

## Features

无 `[features]`，本 crate 为纯转发核，不含可选编译特性。
