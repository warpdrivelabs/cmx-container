# cmx-ai-api

> cmx 平台 AI 生成能力中继模块的 HTTP 皮肤层（一期薄代理）：把 AI 会话 / 消息 / 询问 / 审批端点转发到 OpenCode 服务，SSE 事件流按 sessionID 分发。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-ai-api` 是 AI 域的 HTTP 适配层。AI 能力本体在 `cmx-ai` crate（`OpenCodeClient` HTTP 客户端 + `AiSseEventRegistry` 会话事件注册表）；本 crate 只做薄代理：**解析请求 → 委托 `cmx_ai::OpenCodeClient` 转发到 OpenCode 服务（:4096）→ 包 `ApiResp` 信封返回**。AI 错误经 `From<AiError> for cmx_api_types::Error` 自动 `?` 传播为 HTTP 错误。

### 关键机制

- **会话标识透传**：一期 session id 直接使用 OpenCode 的 `ses_*`，路径参数 `{sid}` 即 OpenCode session。
- **异步生成 + SSE 推送**：`send_message` 转发 `prompt_async`（返回 202），生成过程经 `GET /ai/events` SSE 流推送；同一 session 仅允许一条活跃生成流（registry 锁），并发发送返回 409。
- **双向桥接**：`context-request` / `context-response` 端点实现「插件工具 ↔ 前端」隐式上下文回传（oneshot channel + 30s 超时兜底），全程无询问框。
- **SSE 鉴权特例**：EventSource 无法发 Authorization 头，mw_auth 支持 query `access_token` 兜底校验（该端点需加入认证白名单）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | `CmxAppState` / `CmxSvrContext` / `ApiResp` / `Result` / `ModuleRoutes` |
| `cmx-api-types` | 响应信封与错误类型源头 |
| `cmx-ai` | `OpenCodeClient`（create_session / prompt_async / reply_question / reply_permission / abort / delete_session）、`get_client()` / `get_registry()` 全局获取、`AiSseEvent`、`types::*`（请求 / 响应 / SSE 事件 DTO，含 `ToSchema` 派生） |
| `axum` | Router / `Sse` / `Event` / `KeepAlive`（SSE 流式响应） |
| `futures` | mpsc receiver → SSE stream（`stream::unfold`） |
| `tokio` | 超时控制（context-request 30s 兜底） |
| `serde` / `serde_json` / `tracing` / `utoipa` | 常规序列化 / 日志 / 文档依赖 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-ai-api = { workspace = true }` | 平台总装配器：`routes()` 中 `.merge(AiModule.routes())`；`merged_openapi()` 中 `doc.merge(AiApiDoc::openapi())` |
| `cmx-portalservice`（跨 workspace） | path 引用 `cmx-platform-app` | 门户微服务 bin，间接获得 AI 中继全部端点 |
| `cmx-flowengine`（跨 workspace） | 不直接依赖 | 流程微服务独立 workspace，与 AI 域无依赖关系 |

---

## 核心功能与特性（路由端点）

所有路由挂 `/api/ai` 前缀下（AiModule 路径内建 `/ai/...`）。

| 端点 | 方法 | 说明 |
|------|------|------|
| `/ai/sessions` | POST | 创建会话（转发 OpenCode `POST /session`），返回 `SessionInfo` |
| `/ai/sessions/{sid}/messages` | POST | 异步发送消息（`prompt_async`，202 已接受；同 session 活跃流冲突 409） |
| `/ai/sessions/{sid}/answer` | POST | 回答 AI 询问（转发 `POST /question/{requestID}/reply`） |
| `/ai/sessions/{sid}/approval` | POST | 审批决策（approve → `reply:"once"`；reject → `reply:"reject"`） |
| `/ai/sessions/{sid}/abort` | POST | 中止当前生成（并立即释放活跃锁） |
| `/ai/sessions/{sid}/context-request` | POST | 隐式上下文请求（插件工具发起，广播 SSE 后挂起等待前端回传，30s 超时） |
| `/ai/sessions/{sid}/context-response` | POST | 前端回传当前页面信息，解除挂起 |
| `/ai/sessions/{sid}` | DELETE | 删除会话（清理订阅与 pending 状态） |
| `/ai/events` | GET | SSE 事件流（`session_id` + `access_token` query 订阅） |

所有端点在 AI 功能未启用（`opencode.enabled` 未配置 / 服务未部署）时返回 503 业务提示。

---

## 模块结构

```text
cmx-ai-api
├── src
│   ├── lib.rs        # AiModule 路由聚合（实现 ModuleRoutes）+ AiApiDoc 导出
│   ├── handler.rs    # 全部 9 个 handler + EventsQuery + require_client 辅助
│   └── openapi.rs    # AiApiDoc：OpenApi 切片（7 个 path + cmx_ai types 全套 schema）
└── Cargo.toml
```

---

## 关键类型 / API

### AiModule（lib.rs）

```rust
pub struct AiModule;   // impl ModuleRoutes：路径内建 /ai/...；prefix() = "ai"；module_name() = "ai"

#[derive(OpenApi)]
pub struct AiApiDoc;   // paths：create_session / send_message / answer_question / approve /
                       //        abort_session / delete_session / subscribe_events
                       // schemas：cmx_ai::types::{CreateSessionReq, SendMessageReq, AnswerReq,
                       //          ApprovalReq, SessionInfo, TextDeltaEvent, ToolCallEvent, ...}
```

### 典型 handler 签名（handler.rs）

```rust
// 会话类：委托 OpenCodeClient，Result 传播
pub async fn create_session(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    body: Option<Json<CreateSessionReq>>,
) -> Result<Json<ApiResp<SessionInfo>>>;

// 消息类：含活跃锁语义，返回裸 Response（202 / 409 / 503 分级）
pub async fn send_message(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(sid): Path<String>,
    Json(req): Json<SendMessageReq>,
) -> Response;

// SSE 订阅：mpsc receiver → Sse<impl Stream<Item = Result<Event, Infallible>>>
pub async fn subscribe_events(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<EventsQuery>,      // { session_id, access_token? }
) -> Response;
```

---

## 使用示例

### 一、cmx-platform-app 合并 AI 路由与文档（组装场景）

```rust
use cmx_ai_api::{AiApiDoc, AiModule};
use utoipa::OpenApi;

let router = Router::new().merge(AiModule.routes());   // /api/ai/* 全端点
let mut doc = ApiDoc::openapi();
doc.merge(AiApiDoc::openapi());                        // OpenAPI 切片聚合
```

### 二、薄代理 handler（摘自 handler.rs 模式）

```rust
use cmx_ai::{get_client, get_registry};
use cmx_api_core::{ApiResp, Result};

pub async fn delete_session(Path(sid): Path<String>) -> Result<Json<ApiResp<serde_json::Value>>> {
    // 1. 取全局客户端；未启用返回 503 业务错误
    let client = require_client()?;

    // 2. 委托转发到 OpenCode，AiError 经 From 自动转 HTTP 错误
    client.delete_session(&sid).await?;

    // 3. 清理本 session 的前端订阅与 pending 状态（询问 / 审批 / 上下文回传）
    if let Some(reg) = get_registry() {
        reg.purge(&sid);
    }
    Ok(Json(ApiResp::ok(serde_json::json!({ "deleted": true }))))
}
```

### 三、SSE 事件流订阅（registry 协作）

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use cmx_ai::{get_registry, AiSseEvent};

// 1. 按 session_id 订阅（mpsc receiver；每个 AiSseEvent 含 event_name + payload JSON）
let registry = get_registry().expect("AI 未启用已在前面拦截");
let rx = registry.subscribe(&q.session_id);

// 2. receiver → SSE 流：event 字段 = 事件类型，data = JSON 载荷
let stream = futures::stream::unfold(rx, |mut rx| async move {
    rx.recv().await.map(|ev: AiSseEvent| {
        let event = Event::default().event(ev.event_name).data(ev.payload);
        (Ok::<Event, std::convert::Infallible>(event), rx)
    })
});

// 3. 带保活的 SSE 响应（前端用 EventSource + query access_token 订阅）
Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
```

---

## 设计要点

1. **鉴权职责归中间件**：`subscribe_events` 不再自行校验 query token——mw_auth 已支持 query `access_token` 兜底（EventSource 场景），handler 只保留 `EventsQuery.access_token` 字段兼容旧客户端。
2. **活跃锁的释放路径全覆盖**：转发失败、主动 abort、会话删除均显式 `release_session`，避免 SSE 流不再推送 idle 时锁悬挂导致该 session 永久无法发消息。
3. **隐式上下文回传的降级语义**：前端 30s 未回传时返回 `timed_out: true` 的成功响应（非错误），让插件工具拿空信息优雅降级而非整条链路失败。
