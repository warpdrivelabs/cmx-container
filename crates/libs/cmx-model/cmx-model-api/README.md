# cmx-model-api

> 模型中心 HTTP 协议皮肤：定义中心（DCT/DOC/BASE）+ 弹性组合 + 数据库初始化与模块部署的薄 axum handler 与路由聚合（`ModelModule`）。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-model-api` 是 CMX 模型中心的 HTTP 层，对标 `cmx-doc-api` / `cmx-dct-api` 的分层模式：元数据读写委托 `cmx-model-meta`（JSON 文件存储），数据库初始化与部署委托 `cmx-model-deploy`（真实落库），本 crate 只做「参数提取 → 调用 → `ApiResp` 信封 / SSE 流」的协议适配，不含任何业务逻辑与手写 SQL。

模型中心是 CMX 平台「表定义 JSON → DDL 生成 → 数据库初始化 → 模块部署台账」链路的入口。设计器维护的五类定义（BASE-DCT / BASE-DOC / DCT / DOC / FC 弹性组合）先以 JSON 文件形式存放于 `data/meta/**`，再由部署流程编译为 `TableDefine` 建到目标库，并记录 5 张台账系统表（`cmx_model_meta` 等）。

本 crate 由组装层（`cmx-platform-app`）通过 `.merge(ModelModule.routes())` 合并进主路由——而非 `cmx-api` 反向依赖本 crate，保证依赖方向单向无环。`ModuleRoutes::prefix()` 返回 `"model"`，`/api` 前缀由 web 层 nest 统一添加。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | API 框架：`ModuleRoutes` trait + `ApiResp` + `CmxAppState` + `CmxSvrContext` |
| `cmx-model-meta` | 元数据层：`definitions`（定义中心）/ `flexible_combination`（弹性组合）读写 |
| `cmx-model-deploy` | 部署层：`db_state` / `init_db` / `deploy` 及三个 SSE 流式变体 |
| `cmx-core` | `SVRContext`（handler 中取操作人） |
| `cmx-api-types` | `Error` 构造（`bad_request` 等）与 `Result` |
| `axum` / `serde` / `serde_json` / `tokio` / `futures` | Web 框架、序列化、异步运行时、SSE 流处理 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-model-api = { workspace = true }` | 组装层 `routes.rs` 中 `.merge(ModelModule.routes())` 合并模型中心全部路由 |

---

## 核心功能与特性

| 功能 | 路由 | 说明 |
|------|------|------|
| 定义列表 | `GET /api/definitions/list` | 按 kind/domain/application/module 递归扫描定义文件并汇总摘要 |
| 定义读/存/删 | `GET·POST·DELETE /api/definitions/config` | 支持文件名定位与业务编码（dictCode/moduleCode）反查定位 |
| 定义批量读 | `POST /api/definitions/batch` | 批量读 + 附带各定义引用的 base 字段集文件（去重） |
| 设默认版本 | `POST /api/definitions/default` | 同 stem 多版本间切换 isDefault |
| 弹性组合档案 CRUD | `GET·POST·DELETE /api/flexible-combination/config` | 按 DAM + scenario 四段定位，保存前强制 schema 校验 |
| 弹性组合解析/规则 | `GET /api/flexible-combination/resolve`、`/rule` | 锚点评分合并规则、产出列模型 |
| 弹性组合校验/预览 | `POST /api/flexible-combination/validate`、`/preview` | domain-neutral 校验 + 决策表预览 |
| 库状态矩阵 | `GET /api/model/db-state?db_id=` | 库门闸 + 每模块每 kind 的 scenario（create/upgrade/current/retry/drift…） |
| 数据库初始化 | `POST /api/model/init` | 建 5 张台账系统表 + 写 meta + 历史（另有 init-plan-stream / init-stream 两个 SSE 变体） |
| 模块部署 | `POST /api/model/deploy` | 编译 DCT/DOC/RPT/SEED/MENU 定义并落库（另有 deploy-plan-stream / deploy-stream SSE 变体） |

---

## 模块结构

```text
cmx-model-api
├── src
│   ├── lib.rs                            # ModelModule 路由聚合（impl ModuleRoutes，prefix="model"）
│   └── handlers/
│       ├── mod.rs                        # 模块导出
│       ├── definitions.rs                # 定义中心 handler（list/config/batch/default，业务编码定位分流）
│       ├── flexible_combination.rs       # 弹性组合 handler（list/config/resolve/rule/validate/preview/default）
│       └── deploy.rs                     # 初始化与部署 handler（db-state/init/deploy × 普通 + SSE 流）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/lib.rs —— 路由模块聚合
pub struct ModelModule;
impl cmx_api_core::routes::traits::ModuleRoutes for ModelModule {
    fn routes(self) -> axum::Router<cmx_api_core::app_state::CmxAppState>; // 18 条路由
    fn prefix() -> &'static str;   // "model"
    fn module_name(&self) -> &'static str; // "model"
}

// src/handlers/definitions.rs —— 定义中心（节选）
pub async fn definitions_list(State, CmxSvrContext, Query<DefQuery>)
    -> Result<Json<ApiResp<serde_json::Value>>>;
pub async fn definitions_get(..);      // 文件名直读 / 业务编码按 kind 分流反查
pub async fn definitions_save(..);
pub async fn definitions_delete(..);
pub async fn definitions_batch(..);
pub async fn definitions_set_default(..);

// src/handlers/flexible_combination.rs —— 弹性组合（节选）
pub async fn fc_list(..); pub async fn fc_get_config(..); pub async fn fc_save_config(..);
pub async fn fc_resolve(..); pub async fn fc_rule(..);
pub async fn fc_validate(..); pub async fn fc_preview(..); pub async fn fc_set_default(..);

// src/handlers/deploy.rs —— 初始化与部署（节选）
pub async fn model_db_state(..);       // GET  /api/model/db-state
pub async fn model_init(..);           // POST /api/model/init
pub async fn model_deploy(..);         // POST /api/model/deploy
pub async fn model_init_plan_stream(..);   // POST /api/model/init-plan-stream（SSE）
pub async fn model_init_stream(..);        // POST /api/model/init-stream（SSE）
pub async fn model_deploy_plan_stream(..); // POST /api/model/deploy-plan-stream（SSE）
pub async fn model_deploy_stream(..);      // POST /api/model/deploy-stream（SSE）

// 查询参数 DTO
pub struct DefQuery  { kind, domain, application, module, file, id: Option<String> }
pub struct FcQuery   { domain, app, module, scenario: Option<String>,
                       rest: HashMap<String, String> /* 锚点维度键（flatten）*/ }
pub struct ModelQuery { db_id: String }
```

---

## 使用示例

### 一、组装层合并路由（参考 cmx-platform-app/src/routes.rs）

```rust
use axum::Router;
use cmx_api_core::app_state::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_model_api::ModelModule;

fn build_router() -> Router<CmxAppState> {
    Router::new()
        // 合并模型中心 18 条路由（definitions / flexible-combination / model 三组）
        .merge(ModelModule.routes())
    // web 层随后统一 .nest("/api", router)，最终路径形如 /api/definitions/list
}
```

### 二、HTTP 端点调用（前端 / curl 视角）

```bash
# 1) 列出 fi 域 cmxfico 应用 gl 模块的全部 DCT 定义
curl "http://localhost:8080/api/definitions/list?kind=DCT&domain=fi&application=cmxfico&module=gl"

# 2) 按业务编码（dictCode）反查字典定义（id 非 .json 时走 resolve_dict_file 反查）
curl "http://localhost:8080/api/definitions/config?kind=DCT&id=cf_client&domain=fi&application=cmxfico&module=gl"

# 3) 查询目标库部署状态矩阵（库门闸 + scenario 统计）
curl "http://localhost:8080/api/model/db-state?db_id=primary"

# 4) 部署一批定义到目标库（items 按 DCT→DOC→RPT→SEED→MENU 优先级稳定排序执行）
curl -X POST "http://localhost:8080/api/model/deploy" -H "Content-Type: application/json" -d '{
  "db_id": "primary",
  "items": [
    { "kind": "DCT", "domain": "fi", "application": "cmxfico", "module": "gl", "file": "cmxfico_dct_meta_v1.json" },
    { "kind": "MENU", "domain": "fi", "application": "cmxfico", "module": "gl", "file": "" }
  ]
}'
```

### 三、SSE 流式部署（前端 EventSource 消费 InitEvent）

```javascript
// 订阅 deploy-stream：后端把每个阶段（connect/step/progress/done/error）实时推送
const es = new EventSourcePolyfill(
  "http://localhost:8080/api/model/deploy-stream",
  { method: "POST", headers: { "Content-Type": "application/json" },
    payload: JSON.stringify({ db_id: "primary", items: [/* 同上 */] }) }
);
es.addEventListener("step", e => console.log("步骤", JSON.parse(e.data)));
es.addEventListener("done", e => {
  // done 事件携带 results / batch_id / db_state，可立即刷新工作台
  console.log("部署完成", JSON.parse(e.data));
  es.close();
});
es.addEventListener("error", e => console.error("部署失败", JSON.parse(e.data)));
```

> 注：`InitEvent`（`{ kind: "connect"|"step"|"progress"|"done"|"error", data: Value }`）由 `cmx-model-deploy` 定义，本 crate 仅做 mpsc → SSE `Event` 的转发，事件语义见 cmx-model-deploy README。

---

## 设计说明

- **薄皮肤原则**：所有 handler 一行委托（如 `model_db_state` 直接 `cmx_model_deploy::db_state(&q.db_id).await?`），无手写 SQL、无业务判断；复杂分流逻辑（如 `definitions_get` 的文件名 vs 业务编码定位）仅做参数归一后转调 `cmx-model-meta`。
- **与 cmx-doc-api / cmx-dct-api 的对称性**：三者都是「-api 皮肤 + meta/model 能力层」结构，共享 `definitions::resolve` 的编码定位逻辑（实现在 `cmx-model-meta`，避免三方互相依赖成环）。
- **SSE 转发模式**：三个 `*-stream` 端点均为「spawn 任务 + unbounded mpsc 接收 `InitEvent` + 转为 axum SSE `Event`」，与 cmx-job-api 的 SSE 模式同构。
