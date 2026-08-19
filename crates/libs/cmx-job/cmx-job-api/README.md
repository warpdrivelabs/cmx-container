# cmx-job-api

> 异步任务中心的 HTTP 协议皮肤：提交 / 列表 / 详情 / 控制的薄 axum handler + SSE 实时进度端点（单作业流 + 全库汇总流）+ `JobModule` 路由聚合。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-job-api` 是 CMX 异步任务中心的 HTTP 层：handler 只做「提取参数 → 调 `cmx_job_core::JobManager` → `ApiResp` 信封 / SSE 流」的协议适配，业务逻辑全部在内核层。`JobModule` 实现 `cmx-api-core` 的 `ModuleRoutes` trait，聚合任务中心 9 组路由（含 2 条 SSE），由组装层 `cmx-platform-app` 合并 `JobModule.routes()`——cmx-api 不反向依赖本 crate，避免依赖环。

端点覆盖任务全生命周期（`/api` 前缀由 web 层 nest 加）：

- **提交与查询**：`POST /jobs`（提交，返回 id）、`GET /jobs`（列表，含已注册种类元数据）、`GET /jobs/{id}`（详情）
- **控制**：`POST /jobs/{id}/pause` / `resume` / `cancel` / `restart`（Fresh 重启派生新作业）；`DELETE /jobs/{id}`（归档，仅终态，RU/HI 分离转移到历史表）
- **历史**：`GET /jobs/history`、`GET /jobs/history/{id}`（归档作业查询）
- **SSE**：`GET /jobs/{id}/events`（单作业实时进度）、`GET /jobs/events`（汇总流，列表页实时刷新）

分布式部署下（默认开启），作业可能由其它节点执行。此时单作业 SSE 自动降级为 **DB 轮询合成流**（每 1s 轮询、`rev` 前进才推帧，无跨节点事件总线时的务实方案）；本节点属主则走实时 hub 流（首帧 `snapshot` + `state`/`progress`/`item`/`log`/`result`/`error`/`done` 增量）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-job-core` | 任务中心内核：`JobManager` 单例 + `JobEvent`/`JobEventHub` + 模型（`Job`/`JobStatus`/`SubmitRequest`/`ControlOutcome`） |
| `cmx-api-core` | API 框架：`CmxAppState` / `middleware::CmxSvrContext` / `routes::traits::ModuleRoutes` / `ApiResp`/`Result`/`Error` |
| `axum` | handler 提取器 + `Router` + SSE（`Event`/`KeepAlive`/`Sse`） |
| `futures` / `tokio` | mpsc receiver → `stream::unfold` SSE 流；跨节点轮询 `tokio::time::sleep` |
| `serde` / `serde_json` / `tracing` | 请求 DTO 反序列化、JSON 载荷、日志 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-job-api = { workspace = true }` | 组装层 `routes.rs` 中 `.merge(JobModule.routes())` 合并任务中心路由 |

---

## 核心功能与特性

| 功能 | 端点 | 说明 |
|------|------|------|
| 提交作业 | `POST /api/jobs` | body `{ kind, params?, title?, priority? }` → `{ id }`；单例约束冲突返回 409 |
| 作业列表 | `GET /api/jobs?kind=&status=&limit=` | 合并内存热态 + 持久化；返回 `items` + `kinds` + `kindsMeta`（kindClass/singleton/pausable，前端区分批处理 vs 常驻消费者） |
| 作业详情 | `GET /api/jobs/{id}` | 完整快照；内存未命中回落持久化（id 序列化为 string 防 JS 大整数精度丢失） |
| 暂停/恢复/停止 | `POST /api/jobs/{id}/pause·resume·cancel` | `ControlOutcome` → Accepted=200 / NotFound=404 / Rejected=409 |
| 重启 | `POST /api/jobs/{id}/restart` | Fresh 模式派生新作业，返回新 id |
| 归档 | `DELETE /api/jobs/{id}` | 仅终态；事务转移到 `cmx_job_hi` / `cmx_job_hi_log` 历史表（RU/HI 分离，非真删） |
| 历史查询 | `GET /api/jobs/history`、`/api/jobs/history/{id}` | 按 kind/status 过滤，`archived_at` 倒序分页 |
| 单作业 SSE | `GET /api/jobs/{id}/events` | 本地属主：首帧 snapshot + 实时增量；他节点：DB 轮询合成流；鉴权复用 mw_auth 的 query `access_token` 兜底（EventSource 无法发 header） |
| 汇总 SSE | `GET /api/jobs/events` | 订阅全部作业状态变化（`job` 事件），前端 upsert 列表行 |

---

## 模块结构

```text
cmx-job-api
├── src
│   ├── lib.rs        # JobModule 路由聚合（impl ModuleRoutes，prefix="job"；静态段路由先于 /jobs/{id} 声明防歧义）
│   └── handlers.rs   # 11 个 handler + SSE 双端点 + ControlOutcome→HTTP 映射 + job_json 序列化（539 行）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/lib.rs
pub struct JobModule;
impl cmx_api_core::routes::traits::ModuleRoutes for JobModule {
    fn routes(self) -> axum::Router<cmx_api_core::CmxAppState>; // 9 组路由（含 2 条 SSE）
    fn prefix() -> &'static str;        // "job"
    fn module_name(&self) -> &'static str; // "job"
}

// src/handlers.rs —— handler（均为薄转发）
pub async fn submit_job(State, CmxSvrContext, Json<SubmitRequest>)
    -> Result<Json<ApiResp<Value>>>;          // POST /api/jobs → { "id": "..." }
pub async fn list_jobs(State, CmxSvrContext, Query<ListQuery>)
    -> Result<Json<ApiResp<Value>>>;          // GET  /api/jobs
pub async fn get_job(State, CmxSvrContext, Path<String>)
    -> Result<Json<ApiResp<Value>>>;          // GET  /api/jobs/{id}
pub async fn pause_job / resume_job / cancel_job(..) -> Response;   // ControlOutcome 映射
pub async fn restart_job(..) -> Response;     // 成功返回新作业 id
pub async fn delete_job(..) -> Response;      // 归档（仅终态）
pub async fn list_history(State, CmxSvrContext, Query<HistoryQuery>)
    -> Result<Json<ApiResp<Value>>>;          // GET /api/jobs/history
pub async fn get_history(..);                 // GET /api/jobs/history/{id}
pub async fn subscribe_events(..) -> Response;   // SSE 单作业流
pub async fn subscribe_summary(..) -> Response;  // SSE 汇总流

// 查询参数 DTO
pub struct ListQuery    { pub kind: Option<String>, pub status: Option<String>, pub limit: Option<usize> }
pub struct HistoryQuery { /* kind / status / offset / limit（分页） */ }
pub struct EventsQuery  { pub access_token: Option<String> }
```

---

## 使用示例

### 一、组装层合并路由（参考 cmx-platform-app/src/routes.rs）

```rust
use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_job_api::JobModule;

fn build_router() -> axum::Router<cmx_api_core::CmxAppState> {
    Router::new()
        .merge(JobModule.routes()) // 任务中心 9 组路由
    // web 层 .nest("/api", ..) 后路径形如 /api/jobs、/api/jobs/{id}/events
}
```

### 二、HTTP 提交与控制（前端 / curl 视角）

```bash
# 1) 提交演示作业（内置 kind：job.demo / job.consumer；业务 kind 如 rpt.compute）
curl -X POST "http://localhost:8080/api/jobs" -H "Content-Type: application/json" -d '{
  "kind": "job.demo", "params": { "steps": 10, "stepMs": 500 }
}'
# → { "code": 200, "data": { "id": "7359230048614400" } }

# 2) 列表（kindsMeta 告知每个 kind 的 kindClass/singleton/pausable，前端据此渲染按钮）
curl "http://localhost:8080/api/jobs?status=running&limit=50"

# 3) 控制三连：暂停 → 恢复 → 停止（200 受理 / 404 不存在 / 409 状态不允许或单例冲突）
curl -X POST "http://localhost:8080/api/jobs/7359230048614400/pause"
curl -X POST "http://localhost:8080/api/jobs/7359230048614400/resume"
curl -X POST "http://localhost:8080/api/jobs/7359230048614400/cancel"

# 4) 归档终态作业到历史表（DELETE /jobs/{id}，RU/HI 分离）
curl -X DELETE "http://localhost:8080/api/jobs/7359230048614400"
```

### 三、SSE 订阅实时进度（前端 EventSource）

```javascript
// 单作业流：首帧 snapshot（完整快照）→ 之后增量 state/progress/item/log/result/error/done。
// 鉴权：EventSource 无法发 header，token 走 query 参数（mw_auth 兜底校验）。
const es = new EventSource(
  `/api/jobs/7359230048614400/events?access_token=${token}`);
es.addEventListener("snapshot", e => render(JSON.parse(e.data))); // status + progress(items)
es.addEventListener("item",     e => appendItem(JSON.parse(e.data))); // 明细项状态
es.addEventListener("done",     () => { es.close(); refreshDetail(); });

// 汇总流（列表页）：任何作业提交/跃迁/进度去抖点广播 job 事件，前端 upsert 行
const sum = new EventSource(`/api/jobs/events?access_token=${token}`);
sum.addEventListener("job", e => upsertRow(JSON.parse(e.data)));
```

> 作业在他节点执行时，单作业流自动切换为 DB 轮询合成流：首帧 snapshot 后每 1s 轮询，仅 `progress.rev` 前进才推帧（终态帧为全量 snapshot + state，确保明细补齐）。

---

## 设计说明

- **静态段先于动态段**：`/jobs/events`、`/jobs/history` 等静态路由必须声明在 `/jobs/{id}` 之前，避免 axum 路由歧义。
- **「先订阅再判定」防丢帧**：`subscribe_events` 先 `hub().subscribe(id)` 再判断属主，并在 claim 窗口期（≤1s 一拍）有界等待约 2.4s——否则刚提交的作业会误落轮询兜底分支，整个运行期收不到 item/log 明细。
- **id 全部字符串化**：`JobId` 为 52 位安全 bigint，JSON 序列化统一转 string，避免前端 JS 精度丢失。
