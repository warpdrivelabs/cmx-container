# cmx-ai

> AI 生成能力中继层（一期薄代理）：前端与 [OpenCode](https://opencode.ai)（:4096）之间的纯转发层。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 模块定位

`cmx-ai` 作为 `cmx-container` workspace 的一个 crate，职责是把前端的 AI 生成请求转发给
独立部署的 OpenCode 服务，并把 OpenCode 的 SSE 事件流按 sessionID 分发回前端。

| 阶段 | 会话持久化 | 生成产物 | 行为 |
|------|-----------|----------|------|
| **一期（当前）** | ❌（OpenCode 内存） | ❌（仅返回前端展示） | 薄代理：转发 + SSE 翻译 |
| 二期（规划） | ✅ `ai_sessions` 表 | ✅ 调 portal API 保存 | 胖代理：会话列表/历史回看/恢复 |

## 架构

```
前端 ──HTTP/SSE──→ cmx-api (/api/ai/*) ──→ cmx-ai ──HTTP/SSE──→ OpenCode (:4096)
                                      ↑                ↑
                              CmxAppState           Bearer 鉴权
                          (query token 校验 SSE)
```

cmx-ai 内部三个核心组件：

- **`opencode_client`**：reqwest 客户端，封装 OpenCode 的 `session`/`prompt_async`/
  `question`/`permission` 接口，统一携带 `OPENCODE_SERVER_PASSWORD`。
- **`sse_relay`**：维护**一条**到 OpenCode `GET /event` 的全局 SSE 长连接（单连接多路复用），
  按事件载荷的 `sessionID` 分发到各前端订阅，并把 OpenCode 原生事件翻译为简化的 cmx-ai 事件。
- **`session_registry`**：前端订阅路由表（`{ses_* → Vec<前端 sender>}`）+ 待处理
  question/permission id 管理。

## 对外 API（前端契约）

路由经 `cmx-api::handlers::ai` 注册，挂在 `/api/ai/*`。完整 OpenAPI 文档见 Swagger UI
（`/swagger-ui`，tag = `AI`）。

| 方法 | 路径 | 作用 |
|------|------|------|
| POST | `/api/ai/sessions` | 创建会话（转发 `POST /session`） |
| POST | `/api/ai/sessions/{sid}/messages` | 异步发消息（转发 `prompt_async`，返回 202） |
| POST | `/api/ai/sessions/{sid}/answer` | 回答询问（转发 `/question/{rid}/reply`） |
| POST | `/api/ai/sessions/{sid}/approval` | 审批决策（转发 `/permission/{rid}/reply`） |
| POST | `/api/ai/sessions/{sid}/abort` | 中止生成（转发 `/session/{sid}/abort`） |
| DELETE | `/api/ai/sessions/{sid}` | 删除会话（转发 `DELETE /session/{sid}`） |
| GET | `/api/ai/events` | SSE 事件流（query token 鉴权，按 sessionID 分发） |

### SSE 鉴权（关键）

`GET /api/ai/events` 因 EventSource 无法发送 Authorization header，采用 **query token 鉴权**：

```
GET /api/ai/events?session_id=ses_xxx&access_token=eyJ...
```

- 该端点需加入 `[auth].whitelist`（`/api/ai/events`）让 `mw_auth` 放行；
- handler 内部用全局 `AuthService::validate_token` 校验 `access_token`；
- 其它 6 个 POST/DELETE 接口走正常 Bearer 认证，**不要**加入白名单。

### SSE 事件协议

cmx-ai 把 OpenCode 原生事件翻译为以下简化事件（SSE 帧的 `event:` 字段即类型名，`data:` 是 JSON）：

| cmx-ai 事件 | 说明 | 来源 OpenCode 事件 |
|------|------|------|
| `text_delta` | AI 回复的流式文本片段 | `message.part.delta`（`field:"text"`） |
| `reasoning_delta` | 推理过程片段 | `message.part.delta`（`field:"reasoning"`） |
| `tool_call` | 工具调用进度 | `message.part.updated`（`part.type:"tool"`） |
| `json_chunk` | 渐进 JSON 片段（DCT/DOC 等结构化产物，前端实时拼装预览） | cmx-ai 从 `message.part.delta` 识别 ```json 围栏或裸 JSON 边界后切分 |
| `ask_user` | 弹出询问卡片 | `question.v2.asked` |
| `require_approval` | 审批窗口 | `permission.v2.asked` |
| `result` | 最终完整结果 | `session.status`（`status.type:"idle"`） |
| `error` | 异常信息 | `session.error` / `session.status`（`status.type:"error"`） |
| `done` | 本轮流结束标志 | result/abort 后下发 |

前端示例：
```js
const es = new EventSource('/api/ai/events?session_id=ses_xxx&access_token=' + token);
es.addEventListener('text_delta', e => appendText(JSON.parse(e.data).content));
es.addEventListener('json_chunk', e => appendJsonPreview(JSON.parse(e.data))); // DCT/DOC 渐进拼装
es.addEventListener('ask_user', e => showAskCard(JSON.parse(e.data)));
es.addEventListener('result', e => showResult(JSON.parse(e.data)));
es.addEventListener('done', () => finishTurn());
```

## 配置

优先级：环境变量 > `config.toml [opencode]` 段 > 默认值。

| 配置项 | 环境变量 | 默认值 | 说明 |
|--------|----------|--------|------|
| `opencode.base_url` | `OPENCODE_BASE_URL` | `http://127.0.0.1:4096` | OpenCode 服务地址 |
| `opencode.password` | `OPENCODE_SERVER_PASSWORD` | 空 | 访问凭证（生产必配） |
| `opencode.request_timeout_ms` | — | `30000` | 普通 HTTP 请求超时 |
| `opencode.sse_heartbeat_secs` | — | `30` | SSE 心跳周期（日志参考） |

详见 [CONFIG_MANUAL.md](../../../config/CONFIG_MANUAL.md) 与 [ENV_MANUAL.md](../../../config/ENV_MANUAL.md)。

## 启动初始化

`web-server` 启动期调用 `cmx_ai::init_ai_subsystem()`（幂等）：

1. 加载 `OpenCodeConfig`；
2. 构建全局 `OpenCodeClient`；
3. 初始化全局 `SessionRegistry`；
4. `tokio::spawn` 后台 SSE relay task（连接 OpenCode `GET /event`，断开指数退避重连）。

## crate 结构

```
cmx-ai/
├── Cargo.toml
└── src/
    ├── lib.rs              # 模块入口 + 全局单例（registry/client）+ init_ai_subsystem
    ├── config.rs           # OpenCodeConfig（TOML + env + 默认值）
    ├── error.rs            # AiError（thiserror）+ From<AiError> for cmx_api_types::Error
    ├── opencode_client.rs  # reqwest 客户端：8 个转发方法 + SSE 字节流
    ├── session_registry.rs # 订阅路由表 + pending question/permission id + AiSseEvent
    ├── sse_relay.rs        # 全局 SSE 中继：事件解析/翻译/分发/重连
    └── types.rs            # 请求/响应/SSE 事件 DTO（ToSchema，供 Swagger）
```

## 一期为二期预留的接缝

| 预留项 | 一期 | 二期只需填充 |
|--------|------|------------|
| 会话 ID 双轨 | `sid` 直接透传 OpenCode `ses_*` | 引入 cmx-ai 自有 id，内部映射 `opencode_session_id` |
| `SessionInfo` 结构 | 已含 `title`/`created_at`（空） | 二期填充 |
| `ResultEvent` | `saveable:false`、`product_type:"html"` | 二期置 `saveable:true` 触发保存按钮 |
| `config.session_store` | 未启用（薄代理） | 新增 `session_store.rs`（BMC/Service）+ 迁移 SQL |
| handler 上下文 | 已带 `CmxSvrContext` + `CmxAppState` | 二期直接取 `user_id`/`app_id` 写 `ai_sessions` |
