# cmx-doc-api

> 业务单据（DOC）模块的 HTTP 协议皮肤：薄 axum handler（提取参数 → 解析 DocMetaView（带缓存）→ 调 `cmx-doc-store-pg` 装载/回存 → `ApiResp`/msgpack 信封）+ `DocModule` 路由聚合，端点路径与迁移前完全一致（`/doc/*`）。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-doc-api` 是 DOC 域三件套中的 **HTTP 协议层**。它不承载业务逻辑，只做三件事：从请求中提取参数（query / body / header）、解析带缓存的 `DocMetaView` 单据定义、把结果封装成协议响应（`ApiResp<T>` JSON 信封或列式 msgpack 二进制信封）。真正的装载、回存、版本化逻辑全部位于下游 `cmx-doc-store-pg`。

### 在三件套中的分工

| 层 | crate | 职责 |
|----|-------|------|
| 协议皮肤 | `cmx-doc-api`（本 crate） | axum handler、参数提取、响应封装、OpenAPI 切片 |
| 领域模型 | `cmx-doc-model` | `DocMetaView` 强类型定义投影、`DocQuery` 富查询、公式/规则、SQL 生成 |
| 持久化 | `cmx-doc-store-pg` | `DocLoader`/`DocSaver`/`DocRevision`、缓存、事务编排 |

### 核心端点设计：「驱动 × 内存模式 × 传输」三维度组合

数据装载端点命名为 `/api/doc/data/<驱动>-<内存模式>-<传输>`，一眼可辨三个维度：

- **驱动**：`sqlx`（PG/MySQL/SQLite）| `tokio`（tokio-postgres）
- **内存模式**：`dataset`（老 DataSet，全拷贝）| `zmc`（ZmcDataSet，持原始行零拷贝）
- **传输**：`json`（ApiResp JSON 信封）| `msgpack`（列式二进制信封）

每个装载端点支持 **GET 便捷路径**（URL query：坐标 + `filter=列:值` 简单等值 + `limit` + `depth`）与 **POST 富查询**（body 为 `DocQuery` JSON：每层 filter/orderBy/分页/游标）。POST 时 URL 上的 `depth`/`limit` 作为兜底默认值，body 显式给定的字段优先。

本 crate 由 web-server（`cmx-platform-app`）合并 `DocModule.routes()` 进主路由，`cmx-api-core` 不反向依赖本 crate（避免环）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | API 框架：`CmxAppState` / `middleware::CmxSvrContext` / `ModuleRoutes` / `ApiResp` / `Result` / `Error` / `actor` / `db_id` / `msgpack` |
| `cmx-doc-store-pg` | 装载/回存/版本化服务层：`DocLoader` / `ZmcDocLoader` / `ZmcDocLoaderSqlx` / `DocSaver` / `DocRevision` / `saver` / `resolve_doc_meta` |
| `cmx-doc-model` | DOC 中立模型：`Filter` / `ColumnView` 等 handler 直接引用的类型 |
| `cmx-model-meta` | 模型中心 `definitions::store`：读单据定义 JSON + base 字段集 |
| `cmx-api-types` | `ApiResp<T>` 响应信封（OpenAPI schema 引用） |
| `cmx-biz` | `BizError` → `cmx_api_types::Error` 桥接（供 `?` 传播）+ `errcode::validation_fail_resp` |
| `cmx-core` | `ColumnarCodec`：老 DataSet 列式编码 |
| `cmx-database` / `cmx-database-pg` | 数据库门面：`get_default_db_manager` / `get_default_pg_db_manager` |
| `axum` / `serde` / `serde_json` / `tokio` / `bytes` / `futures` / `rmp` / `utoipa` / `tracing` | Web 框架、序列化、流式传输、msgpack 手写编码、OpenAPI、日志 |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-doc-api = { workspace = true }` | web-server 组装层：合并 `DocModule.routes()` 进主 Router（`/api` 前缀由 web-server nest 加），并用 `DocApiDoc` 聚合 OpenAPI 文档 |
| `cmx-portalservice` / `cmx-flowengine`（跨 workspace） | **不直接依赖** | 经 HTTP 接口（`/api/doc/*`）消费本 crate 暴露的端点 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 五组合装载端点 | `sqlx-dataset-json`（老链路）/ `tokio-zmc-msgpack` / `sqlx-zmc-msgpack` / `tokio-zmc-json` / `sqlx-zmc-json`，GET 便捷 + POST 富查询共用入口 |
| 懒下钻装载 | `POST /doc/data/children`：装载某层在给定父 id 下的子树，前端 grid 展开时调用 |
| 真·流式装载 | `GET|POST /doc/data/tokio-zmc-stream`：超大扁平单层结果长度分帧 chunked 传输，峰值内存 O(单行) |
| 显示元数据 | `GET /doc/meta`：层序 L1..LN + 各层列 caption/类型 + 父子关系，通用单据前端页动态建表用 |
| 单据回存 | `POST /doc/save`：merge/replace 双模式，自动审计填充、版本快照、乐观锁；列级校验失败返回结构化 violations |
| 批量回存 | `POST /doc/save/batch`：一次请求混多种单据，`atomic=true`（缺省）一个大事务全成全败 |
| 版本化 | `GET /doc/revisions`（版本时间线）/ `GET /doc/revision`（历史快照）/ `POST /doc/restore`（回滚，replace 模式写回） |
| 后端二次校验 | 保存前对 changeset 各行跑 `validationRules`，error 级违规阻断保存（前端校验不可信原则） |
| 铸号规则覆盖 | MDM cr-form 场景：`body.codeRuleOverrides` 覆盖单据字段铸号规则 |
| OpenAPI 切片 | `DocApiDoc` 聚合全部 handler 的 `#[utoipa::path]` 注解 + `DocChildrenReq` schema |

---

## 模块结构

```text
cmx-doc-api
├── src
│   ├── lib.rs        # DocModule 路由聚合（impl ModuleRoutes）+ pub use DocApiDoc
│   ├── handlers.rs   # 全部 HTTP handler：装载/懒下钻/流式/meta/save/save-batch/版本化
│   └── openapi.rs    # DocApiDoc：utoipa OpenApi 切片（paths + schemas）
└── Cargo.toml
```

---

## 关键类型与 API

### 路由聚合（[lib.rs](src/lib.rs)）

```rust
pub struct DocModule;

impl ModuleRoutes for DocModule {
    fn routes(self) -> Router<CmxAppState> { /* ... */ }
    fn prefix() -> &'static str { "doc" }
    fn module_name(&self) -> &'static str { "doc" }
}
```

端点一览（`/api` 前缀由 web-server nest 加）：

| 方法 | 路径 | handler | 说明 |
|------|------|---------|------|
| GET/POST | `/doc/data/sqlx-dataset-json` | `doc_data_sqlx_dataset_json` | sqlx + 老 DataSet 全拷贝 + JSON |
| GET/POST | `/doc/data/tokio-zmc-msgpack` | `doc_data_tokio_zmc_msgpack` | tokio-postgres + Zmc 零拷贝 + msgpack |
| GET/POST | `/doc/data/sqlx-zmc-msgpack` | `doc_data_sqlx_zmc_msgpack` | sqlx + Zmc 零拷贝 + msgpack |
| GET/POST | `/doc/data/tokio-zmc-json` | `doc_data_tokio_zmc_json` | tokio-postgres + Zmc 零拷贝 + JSON |
| GET/POST | `/doc/data/sqlx-zmc-json` | `doc_data_sqlx_zmc_json` | sqlx + Zmc 零拷贝 + JSON |
| POST | `/doc/data/children` | `doc_children` | 懒下钻装载子树 |
| GET/POST | `/doc/data/tokio-zmc-stream` | `doc_data_stream` | 长度分帧二进制流 |
| GET | `/doc/meta` | `doc_meta` | 单据显示元数据 |
| POST | `/doc/save` | `doc_save` | changeset 回存（merge/replace） |
| POST | `/doc/save/batch` | `doc_save_batch` | 批量回存多张单据 |
| GET | `/doc/revisions` | `doc_revisions` | 版本时间线 |
| GET | `/doc/revision` | `doc_revision` | 历史版本快照 |
| POST | `/doc/restore` | `doc_restore` | 回滚到历史版本 |

### 请求 DTO（[handlers.rs](src/handlers.rs)）

```rust
/// 装载端点共用查询参数（GET 便捷路径）。
pub struct DocDataQuery {
    pub domain: Option<String>,        // 域；缺失时按 doc/file 全局反查
    pub application: Option<String>,   // 应用
    pub module: Option<String>,        // 模块
    pub file: Option<String>,          // 单据定义文件名（如 cmxfico_doc_meta_v1.json）
    pub doc: Option<String>,           // 单据模块编码（moduleMeta.moduleCode）
    pub filter: Option<String>,        // GET 便捷：根层过滤 "col:value"（简单等值）
    pub limit: Option<u64>,            // GET 便捷：根层限制行数
    pub depth: Option<usize>,          // 可选装载深度（懒下钻）
}

/// 懒下钻请求体（POST /doc/data/children）。
pub struct DocChildrenReq {
    pub domain/application/module/file/doc: Option<String>,  // 单据坐标（同上）
    pub layer: String,                 // 要下钻装载的层 id
    pub parent_ids: Vec<Value>,        // 上层选中的父 id 列表（该层 childKey 匹配）
    pub query: Option<Value>,          // 该层查询（filter/orderBy/limit/offset/cursor）
    pub depth: Option<usize>,          // 从该层继续下钻几层
    pub exit: Option<String>,          // 出口通道（缺省 tokio-zmc-json；可选 "sqlx-zmc-json"）
}

/// 保存/恢复共用坐标参数。
pub struct DocSaveQuery { /* domain/application/module/file/doc 同上 */ }

/// 版本查询参数。
pub struct RevisionsQuery {
    pub doc_file: String,   // 单据定义文件名
    pub root_id: String,    // 单据根行 id
    pub rev: Option<i32>,   // 目标版本号；缺省取当前版（is_current=1）
}
```

---

## 使用示例

### 场景一：web-server 组装层合并 DOC 路由

```rust
use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_doc_api::{DocApiDoc, DocModule};

// cmx-platform-app（web-server）中：把 DOC 模块路由合并进主 Router。
// cmx-api-core 不反向依赖 cmx-doc-api，依赖方向单向无环。
let app = axum::Router::new()
    .merge(DocModule.routes())      // /doc/data/*、/doc/meta、/doc/save、/doc/revisions 等
    .with_state(state);

// OpenAPI 文档聚合：用 utoipa 的 OpenApi::merge 把 DOC 切片并入主文档
let doc = utoipa::openapi::OpenApiBuilder::new()
    .build();
let doc = doc.merge(utoipa::OpenApi::openapi(&DocApiDoc));
```

### 场景二：前端 GET 便捷装载（简单等值 + 限深）

```text
GET /api/doc/data/tokio-zmc-json?doc=cmxfico&filter=period_code:2026&limit=20&depth=2
```

handler 内部流程（`doc_load_entry`）：`resolve_db_id_from_headers` 取库 → `resolve_doc_meta` 解析带缓存的 `DocMetaView` → 无 body 时用 `DocQuery::simple(&root_id, limit, depth)` 组根层等值过滤 → `run_doc_load` 按驱动+出口跑装载器 → `ApiResp::ok(zmc.encode_columnar_json())` 返回列式 JSON 包。

### 场景三：POST 富查询（每层条件/排序/游标分页）

```text
POST /api/doc/data/sqlx-zmc-msgpack?doc=cmxfico
Content-Type: application/json

{
  "depth": 2,
  "countTotal": true,
  "layers": {
    "cv_batch": {
      "filter": { "status": "posted", "amount": { "$gt": 0 },
                  "$or": [ { "type": "1" }, { "type": "2" } ] },
      "orderBy": ["!posting_date", "code"],
      "limit": 50,
      "cursor": "<base64 游标>"
    }
  }
}
```

filter 语义：键值对隐式 AND；标量值 = 等值简写；支持算子 `$eq` `$ne` `$gt` `$gte` `$lt` `$lte` `$like` `$ilike` `$contains` `$startsWith` `$endsWith` `$null` 及 `$or` / `$and` 组合；`orderBy` 元素前缀 `!` 表示降序。msgpack 出口返回 `{code, msg, data}` 二进制信封（`content-type: application/x-msgpack`）。

### 场景四：懒下钻（前端 grid 展开父行时装子树）

```text
POST /api/doc/data/children
{
  "doc": "cmxfico",
  "layer": "cv_line",            // 要下钻的层 id
  "parentIds": [1001, 1002],     // 上层选中的父行 id
  "query": { "filter": { "local_dr": { "$gt": 0 } }, "orderBy": ["line_no"] },
  "depth": 1                     // 只装该层（不继续装孙层）
}
```

handler 把 `query` 塞进 `DocQuery.layers[layer]` 后调用 `ZmcDocLoader::load_subtree(mm, &db_id, &meta, &req.layer, &req.parent_ids, &dq)`，返回子树列式 JSON 包，前端回填父行 `_children`。

### 场景五：changeset 回存 + 后端二次校验失败的结构化响应

```text
POST /api/doc/save?doc=cmxfico
{
  "saveMode": "merge",
  "changes": {
    "cv_line": {
      "inserted": [ { "id": "tmp-1", "upper_id": 1001, "fields": { "amount": "100.00" } } ],
      "updated":  [ { "id": 2001, "fields": { "amount": "-5" }, "baseline": { "update_time": "..." } } ],
      "deleted":  [3001, 3002]
    }
  }
}
```

`doc_save` handler 流程：`saver::parse_save_body` 解析模式与 changeset → `saver::parse_code_rule_overrides` 取铸号覆盖 → `run_validation` 对 inserted/updated 行跑 `validationRules`（error 违规直接返回 `{ok:false, errorCode:"DOC_VALIDATION_FAILED", violations:[...]}`）→ `save_ctx` 构造审计上下文（actor_id/actor_name 兜底 0/「系统」）→ `DocSaver::save`；若落库列级校验失败，`e.violations()` 提取结构化 violations 经 `validation_fail_resp` 返回（HTTP 200 错误信封），前端逐行逐列高亮。

---

## Features 说明

本 crate 无 `[features]` 配置。所有 handler 的 OpenAPI 注解（`#[utoipa::path]`）随默认构建一起编译，无需 feature 开关。
