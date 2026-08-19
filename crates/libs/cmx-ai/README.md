# cmx-ai

> AI 生成能力中继层（一期薄代理）：前端与 [OpenCode](https://opencode.ai)（:4096）之间的纯转发层。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)]

## 模块定位

`cmx-ai` 作为 `cmx-container` workspace 的一个 crate，职责是把前端的 AI 生成请求转发给
独立部署的 OpenCode 服务，并把 OpenCode 的 SSE 事件流按 sessionID 分发回前端。

| 阶段 | 会话持久化 | 生成产物 | 行为 |
|------|-----------|----------|------|
| **一期（当前）** | ❌（OpenCode 内存） | ❌（仅返回前端展示） | 薄代理：转发 + SSE 翻译 |
| 二期（规划） | ✅ `ai_sessions` 表 | ✅ 调 portal API 保存 | 胖代理：会话列表/历史回看/恢复 |

本 crate **不含 HTTP 路由**：HTTP 皮肤在 [`cmx-ai-api`](../cmx-apis/cmx-ai-api)（`AiModule`，
挂 `/api/ai/*`，由 `cmx-platform-app` 合并进主路由）。

## 架构与上下游

```
前端 ──HTTP/SSE──→ cmx-ai-api (/api/ai/*) ──→ cmx-ai ──HTTP/SSE──→ OpenCode (:4096)
                     AiModule                    ↑                ↑
                 (cmx-apis 皮肤 crate)      全局单例            Bearer 鉴权
                                      (registry/client)   (OPENCODE_SERVER_PASSWORD)
```

- **上游**：OpenCode 服务（`GET /event` 全局单流 SSE + session/question/permission REST）。
- **下游消费方**：
  - `cmx-ai-api`：薄 axum handler，解析请求 → 委托 `cmx_ai::OpenCodeClient` → 包 `ApiResp` 信封；
  - `cmx-platform-app`：启动期调 [`init_ai_subsystem`] + 合并 `AiModule` 路由与 `AiApiDoc` 切片；
  - `cmx-common-api`：经 Cargo 依赖引入（门户 AI 对话走 portal handler，与 `cmx-ai-api` 的
    `/api/ai/*` 是两条不同链路）。
- **依赖**：`cmx-api-types`（AiError → `cmx_api_types::Error` 映射）、`cmx-utils`（ConfigManager）。

## crate 结构与模块职责

```
cmx-ai/
├── Cargo.toml
└── src/
    ├── lib.rs              # 模块入口 + 全局单例（REGISTRY/CLIENT）+ init_ai_subsystem + registry()/client() 访问器
    ├── config.rs           # OpenCodeConfig（TOML + env + 默认值）+ load_config
    ├── error.rs            # AiError（thiserror）+ From<AiError> for cmx_api_types::Error
    ├── opencode_client.rs  # reqwest 客户端：7 个转发方法 + stream_events（SSE 字节流）
    ├── session_registry.rs # 订阅路由表 + pending question/permission/context id + 活跃生成锁 + AiSseEvent
    ├── sse_relay.rs        # 全局 SSE 中继：事件解析/翻译/分发/指数退避重连 + JSON 流式切分状态机
    └── types.rs            # 请求/响应/SSE 事件 DTO（ToSchema，供 Swagger）
```

### `opencode_client` —— OpenCode HTTP 客户端

`OpenCodeClient` 复用一个 `reqwest::Client`（连接池，`Clone` 廉价），统一注入
`Authorization: Bearer <OPENCODE_SERVER_PASSWORD>`，并把上游错误映射为 [`AiError`]
（401 → Config 错误提示、超时 → Timeout、连接失败 → Upstream）：

| 方法 | 对应 OpenCode 接口 | 说明 |
|------|--------------------|------|
| `create_session(&self, body)` | `POST /session` | 返回完整 Session JSON |
| `prompt_async(&self, sid, body)` | `POST /session/{sid}/prompt_async` | 异步发消息（204，结果走 SSE） |
| `abort(&self, sid)` | `POST /session/{sid}/abort` | 中止当前生成 |
| `delete_session(&self, sid)` | `DELETE /session/{sid}` | 删除会话 |
| `reply_question(&self, rid, answers)` | `POST /question/{rid}/reply` | 回答询问（`answers: string[][]`） |
| `reject_question(&self, rid)` | `POST /question/{rid}/reject` | 拒绝询问 |
| `reply_permission(&self, rid, reply, msg)` | `POST /permission/{rid}/reply` | 审批回复（`once`/`always`/`reject`） |
| `stream_events(&self)` | `GET /event` | SSE 字节流（专用无超时 client，长连接不被 30s 切断） |

### `session_registry` —— 前端订阅路由表与状态管理

`SessionRegistry`（`DashMap` 线程安全，全局单例）维护四类内存状态：

| 状态 | 结构 | 用途 |
|------|------|------|
| 订阅路由表 | `{ses_* → Vec<mpsc sender>}` | sse_relay 按 sessionID 广播（支持多标签页） |
| pending id 表 | `{ses_* → que_*/per_*}` | answer/approval 接口据此转发到正确端点；无限等待（对齐 OpenCode） |
| 隐式上下文表 | `{ctx_* → oneshot sender}` | 插件工具 context-request 挂起 / context-response 解除 |
| 活跃生成锁 | `{ses_* → ()}` | 同一 session 仅一条活跃生成流，并发 `send_message` 返回 409 |

主要方法：`subscribe(sid)`（订阅事件流）/ `broadcast(sid, event)` / `clear_subscribers(sid)` /
`register_pending_question` / `take_pending_question` / `register_pending_permission` /
`take_pending_permission` / `register_context_request` / `resolve_context_request` /
`try_acquire_session` / `release_session` / `is_session_active` / `purge(sid)`（会话结束清理）。

`AiSseEvent`（`event_name` + 已序列化 `payload`）提供便捷构造器：`text_delta` /
`reasoning_delta` / `tool_call` / `tool_call_full` / `json_chunk` / `result` / `error` / `done`。

### `sse_relay` —— 全局 SSE 中继

`start_global_relay(client)` 幂等拉起后台 tokio task：维护**一条**到 OpenCode `GET /event`
的全局 SSE 长连接（单连接多路复用），断开后指数退避重连（初始 1s，翻倍封顶 30s）。
该 task 仅在 `init_ai_subsystem` 且 `enabled=true` 时拉起（重复调用幂等跳过）。

事件翻译职责（OpenCode 原生 → cmx-ai 简化事件）+ `JsonStreamState` 状态机
（`Inactive` / `InFencedJson` / `InBareJson`）：识别 ```` ```json ```` 围栏或裸 JSON 边界，
把 `message.part.delta` 切分为渐进 `json_chunk` 供前端实时拼装预览；`session.status`
idle 时从累积文本提取最终产物（含 HTML 标签识别为 `html_page_result`）下发 `result` + `done`。

### `types` —— 前端契约 DTO

- 请求：`CreateSessionReq` / `SendMessageReq`（`parts: Vec<TextPartInput>`）/ `AnswerReq`
  （二维 answers）/ `ApprovalReq`（`ApprovalDecision::Approve|Reject`）。
- 响应：`SessionInfo`（`session_id` 透传 `ses_*`；`title`/`created_at` 二期填充）。
- SSE 载荷：`TextDeltaEvent` / `ReasoningDeltaEvent` / `ToolCallEvent` / `AskUserEvent`
  （多问题 `questions` 数组）/ `RequireApprovalEvent`（含 diff）/ `ResultEvent`
  （`result_type: html_page_result|dct_result|doc_result`，`saveable` 一期 false）/
  `JsonChunkEvent` / `ErrorEvent` / `DoneEvent`。
- 隐式上下文：`ContextRequestReq` / `ContextRequestEvent` / `ContextResponseReq`。

全部派生 `utoipa::ToSchema`，经 `cmx-ai-api::AiApiDoc` 进 Swagger。

### `config` / `error`

`OpenCodeConfig { enabled, base_url, password, request_timeout_ms, sse_heartbeat_secs }`；
`load_config()` 优先级：环境变量 > TOML `[opencode]` 段 > 默认值；校验失败自动禁用子系统。

`AiError` 变体：`Config` / `Upstream` / `UpstreamStatus` / `Timeout` / `NotConfigured` /
`InvalidSession` / `NoPendingRequest` / `ChannelClosed` / `Serde` / `Http` / `Url` / `Internal`。
`From<AiError> for cmx_api_types::Error` 映射（handler 可直接 `?`）：

| AiError | HTTP | 说明 |
|---------|------|------|
| `InvalidSession` / `NoPendingRequest` | 404 | 会话失效 / 无待处理询问 |
| `Timeout` | 504 | 请求 OpenCode 超时 |
| `NotConfigured` | 200 + code 1 | 未配置（前端按 code 判断） |
| 其它 | 500 | 内部错误 |

## 使用示例

### 场景一：平台启动初始化（cmx-platform-app 的实际用法）

```rust
// cmx-platform-app/src/lib.rs（20 步 init 之一）：
cmx_ai::init_ai_subsystem().await;
// 幂等：加载配置 → enabled=false 直接返回（/api/ai/* 返回 503）；
// enabled=true 时构建全局 CLIENT/REGISTRY 单例并拉起后台 SSE relay task。
```

### 场景二：HTTP 皮肤转发（cmx-ai-api handler 的实际模式）

```rust
use cmx_ai::{get_client, get_registry};
use cmx_ai::types::*;

// 取全局客户端；未启用时返回 503（cmx-ai-api::handler::require_client 的实际实现）
fn require_client() -> Result<&'static cmx_ai::OpenCodeClient> {
    get_client().ok_or_else(|| Error::ServiceUnavailable("AI 功能未启用".into()))
}

async fn create_session(body: CreateSessionReq) -> Result<ApiResp<SessionInfo>> {
    let client = require_client()?;
    let session = client.create_session(&serde_json::to_value(body)?).await?;
    Ok(ApiResp::ok(SessionInfo { session_id: session["id"].as_str().unwrap_or_default().into(), ..Default::default() }))
}
```

### 场景三：前端订阅 SSE 事件流

```rust
// handler 侧（cmx-ai-api::subscribe_events 的核心逻辑）：
let rx = get_registry().unwrap().subscribe(&sid);   // 每个前端连接独立 receiver
// rx.recv() 循环产出 AiSseEvent → axum::response::sse::Event
```

```js
// 前侧（EventSource 无法发 header，走 query token 鉴权）：
const es = new EventSource('/api/ai/events?session_id=ses_xxx&access_token=' + token);
es.addEventListener('text_delta', e => appendText(JSON.parse(e.data).content));
es.addEventListener('json_chunk', e => appendJsonPreview(JSON.parse(e.data))); // DCT/DOC 渐进拼装
es.addEventListener('ask_user', e => showAskCard(JSON.parse(e.data)));        // 可含多问题
es.addEventListener('require_approval', e => showApproval(JSON.parse(e.data)));
es.addEventListener('context_request', e => autoCollectAndPost(JSON.parse(e.data)));
es.addEventListener('result', e => showResult(JSON.parse(e.data)));
es.addEventListener('done', () => finishTurn());
```

## 对外 API（前端契约）

路由经 [`cmx-ai-api`](../cmx-apis/cmx-ai-api) 的 `AiModule` 注册，挂 `/api/ai/*`，由
cmx-platform-app 合并。完整 OpenAPI 文档见 Swagger UI（`/swagger-ui`，`AiApiDoc` 切片）。

| 方法 | 路径 | 作用 |
|------|------|------|
| POST | `/api/ai/sessions` | 创建会话（转发 `POST /session`） |
| POST | `/api/ai/sessions/{sid}/messages` | 异步发消息（转发 `prompt_async`，返回 202） |
| POST | `/api/ai/sessions/{sid}/answer` | 回答询问（转发 `/question/{rid}/reply`） |
| POST | `/api/ai/sessions/{sid}/approval` | 审批决策（转发 `/permission/{rid}/reply`） |
| POST | `/api/ai/sessions/{sid}/abort` | 中止生成（转发 `/session/{sid}/abort`） |
| POST | `/api/ai/sessions/{sid}/context-request` | 隐式上下文请求（插件工具发起，挂起等待） |
| POST | `/api/ai/sessions/{sid}/context-response` | 隐式上下文回传（前端收集后解除挂起） |
| DELETE | `/api/ai/sessions/{sid}` | 删除会话（转发 `DELETE /session/{sid}`） |
| GET | `/api/ai/events` | SSE 事件流（query token 鉴权，按 sessionID 分发） |

隐式上下文回传链路（对用户透明，无询问框）：

```
插件工具 → POST context-request（oneshot 挂起）→ 后端广播 SSE context_request
        → 前端自动收集页面信息 → POST context-response → 后端 resolve → 工具解除挂起
```

### SSE 鉴权（关键）

`GET /api/ai/events` 因 EventSource 无法发送 Authorization header，采用 **query token 鉴权**：

```
GET /api/ai/events?session_id=ses_xxx&access_token=eyJ...
```

- `mw_auth` 中间件支持 query `access_token` 兜底校验（无需加入白名单，中间件注入
  AuthContext 后放行）；
- handler 侧保留 `access_token` 查询字段仅为兼容旧客户端，不再二次校验；
- 其它 POST/DELETE 接口走正常 Bearer 认证。

### SSE 事件协议

cmx-ai 把 OpenCode 原生事件翻译为以下简化事件（SSE 帧的 `event:` 字段即类型名，`data:` 是 JSON）：

| cmx-ai 事件 | 说明 | 来源 OpenCode 事件 |
|------|------|------|
| `text_delta` | AI 回复的流式文本片段 | `message.part.delta`（`field:"text"`） |
| `reasoning_delta` | 推理过程片段 | `message.part.delta`（`field:"reasoning"`） |
| `tool_call` | 工具调用进度（含 input/output/metadata） | `message.part.updated`（`part.type:"tool"`） |
| `json_chunk` | 渐进 JSON 片段（DCT/DOC 结构化产物） | cmx-ai 从 `message.part.delta` 识别 ```json 围栏或裸 JSON 边界后切分 |
| `ask_user` | 弹出询问卡片（可携带多问题） | `question.v2.asked` |
| `require_approval` | 审批窗口 | `permission.v2.asked`（兼容 `permission.asked`） |
| `context_request` | 隐式上下文收集请求 | cmx-ai 内部（context-request 端点触发） |
| `result` | 最终完整结果 | `session.status`（`status.type:"idle"`）后提取产物 |
| `error` | 异常信息（含人话化错误文案） | `session.error` / `session.status`（`status.type:"error"`） |
| `done` | 本轮流结束标志 | result/abort 后下发 |

## 配置

优先级：环境变量 > `config.toml [opencode]` 段 > 默认值。

| 配置项 | 环境变量 | 默认值 | 说明 |
|--------|----------|--------|------|
| `opencode.enabled` | `OPENCODE_ENABLED` | `false` | **总开关（默认关闭）**：false 时 `/api/ai/*` 统一 503 |
| `opencode.base_url` | `OPENCODE_BASE_URL` | `http://127.0.0.1:4096` | OpenCode 服务地址 |
| `opencode.password` | `OPENCODE_SERVER_PASSWORD` | 空 | 访问凭证（生产必配） |
| `opencode.request_timeout_ms` | — | `30000` | 普通 HTTP 请求超时（下限 1000ms） |
| `opencode.sse_heartbeat_secs` | — | `30` | SSE 心跳周期（日志参考） |

详见 [CONFIG_MANUAL.md](../../../config/CONFIG_MANUAL.md) 与 [ENV_MANUAL.md](../../../config/ENV_MANUAL.md)。

## 启动初始化

`cmx-platform-app` 启动期调用 [`init_ai_subsystem()`](lib.rs)（幂等）：

1. `load_config()` 加载 `OpenCodeConfig`（校验失败自动禁用）；
2. `enabled=false` 直接返回——不建客户端、不拉 relay task（避免后台日志永久刷「无法连接
   OpenCode」），此时 `client()`/`registry()` 返回 `None`，handler 统一 503；
3. `enabled=true`：构建全局 `OpenCodeClient` → 初始化全局 `SessionRegistry` 单例 →
   `tokio::spawn` 后台 SSE relay task（连接 OpenCode `GET /event`，断开指数退避重连）。

## 测试

- 单元测试（`#[cfg(test)]` 内联）：config 的 URL 拼接/校验、session_registry 的订阅广播 /
  pending 生命周期 / 活跃锁获取释放 / purge、opencode_client 的错误体解析等。
- 集成测试（`tests/`）：`opencode_integration.rs`（直连真实 OpenCode 验证转发链路）、
  `sse_relay_integration.rs`（SSE 事件翻译与分发链路）。两者默认 `#[ignore]`（依赖外部
  OpenCode 服务，不阻塞 CI），手动运行：
  `cargo test -p cmx-ai --test opencode_integration -- --ignored --nocapture`；集成测试
  需先初始化 ConfigManager（等价 web-server 启动前置）。

## 一期为二期预留的接缝

| 预留项 | 一期 | 二期只需填充 |
|--------|------|------------|
| 会话 ID 双轨 | `sid` 直接透传 OpenCode `ses_*` | 引入 cmx-ai 自有 id，内部映射 `opencode_session_id` |
| `SessionInfo` 结构 | 已含 `title`/`created_at`（空） | 二期填充 |
| `ResultEvent` | `saveable:false`、`product_type:"html"` | 二期置 `saveable:true` 触发保存按钮 |
| `config.session_store` | 未启用（薄代理） | 新增 `session_store.rs`（BMC/Service）+ 迁移 SQL |
| handler 上下文 | 已带 `CmxSvrContext` + `CmxAppState` | 二期直接取 `user_id`/`app_id` 写 `ai_sessions` |
