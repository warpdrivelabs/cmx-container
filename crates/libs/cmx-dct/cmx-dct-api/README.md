# cmx-dct-api

> 数据字典（DCT）模块的 HTTP 协议皮肤：薄 axum handler（提取参数 → `resolve_dict` 解析字典视图 → 调 `cmx-dct-store-pg` 服务 → `ApiResp`/msgpack 信封）+ `DctModule`（impl `cmx_api_core::ModuleRoutes`）路由聚合，端点路径与迁移前完全一致（`/dct/*`）。

![version](https://img.shields.io/badge/version-0.1.12-blue) ![rust-edition](https://img.shields.io/badge/rust--edition-2024-orange)

## 项目简介

`cmx-dct-api` 是 DCT 域三件套中的 **HTTP 协议层**。它不承载业务逻辑，只做三件事：从请求中提取参数（query / body / header / multipart）、把坐标交给下游解析字典视图、把结果封装成协议响应（`ApiResp<T>` JSON 信封或列式 msgpack 二进制信封）。真正的解析、装载、回存、导入导出逻辑全部位于 `cmx-dct-store-pg`。

核心业务概念是**数据字典（dict）**：以 `dict`（dictCode，如 `currency` / `gl_account`）定位一张 `cf_*` 字典物理表，DAM（domain/application/module）/file 坐标可缺省——缺失时后端按 dictCode 全局反查补全，前端"只传 code"即可。字典分**平表**（flat，如币种）与**自分级**（self_hierarchy，如科目/部门树，带 `parentField`，装载时可按 `parentId` 取 children）。端点覆盖字典元数据、数据装载（JSON + 零拷贝 msgpack 两种出口）、回存（upsert 与 changeset 事务两种语义）、流式导入导出（NDJSON/CSV）。

路由由 `DctModule` 聚合并实现 `cmx_api_core` 的 `ModuleRoutes` trait，由 web-server（cmx-platform-app）合并进主路由——`cmx-api-core` 不反向依赖本 crate，避免环。`/api` 前缀由 web-server nest 追加。

### 三件套分工

| 层 | crate | 职责 |
|----|-------|------|
| **协议皮肤** | **`cmx-dct-api`（本 crate）** | **axum handler、参数提取、ApiResp/msgpack 信封、OpenAPI 切片、multipart/流式出口** |
| 领域模型 | `cmx-dct-model` | `DictView`/`DictColumn` 强类型视图、`DctQuery` 坐标 DTO、DB-free SQL 构造 |
| 持久化 | `cmx-dct-store-pg` | `resolve_dict`、`cf_*` 表装载/回存执行、CSV/NDJSON 导入导出、事务 |

## 端点一览

| 方法 | 路径 | handler | 说明 |
|------|------|---------|------|
| GET | `/api/dct/meta` | `dct_meta` | 字典显示元数据（列 caption/类型/PK/是否自分级）；`?with_props=true` 附带字段扁平属性 |
| GET/POST | `/api/dct/data/search` | `dct_search` | 装载字典数据（flat / 自分级 children，分页；POST body 为 SearchQuery） |
| GET/POST | `/api/dct/data/tokio-zmc-msgpack` | `dct_search_zmc_msgpack` | 零拷贝装载：tokio-postgres + ZmcDataSet + 列式 msgpack 二进制 |
| POST | `/api/dct/entries` | `dct_upsert` | 批量新增/更新（upsert，merge 语义）；校验失败返回结构化 violations |
| DELETE | `/api/dct/entries/{id}` | `dct_delete` | 按主键删除一行（连号域字典删除前记录断号） |
| POST | `/api/dct/save` | `dct_save` | changeset 事务回存（对标 doc 的 DocSaver）；乐观锁冲突返回 409 |
| GET | `/api/dct/export` | `dct_export` | 流式导出全表（`format=json` NDJSON / `format=csv`），附件下载 |
| POST | `/api/dct/import` | `dct_import` | 流式导入（multipart：file + mode），自动识别 CSV/NDJSON |

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-dct-store-pg` | 全部场景服务：`dict_meta` / `dict_search` / `dict_search_zmc` / `dict_upsert` / `dict_delete` / `dict_save` / `export_stream` / `import_stream`，及 `DctQuery` / `SearchQuery` / `Txn` / `BatchConflictMode` re-export |
| `cmx-dct-model`（`features = ["openapi"]`） | `DctQuery` 坐标 DTO；openapi feature 开启 `IntoParams` 供 Swagger 查询参数描述 |
| `cmx-api-core` | `CmxAppState` / `middleware::CmxSvrContext` / `ModuleRoutes` / `ApiResp` / `Result` / `db_id` / `msgpack` |
| `cmx-api-types` / `cmx-biz` | 响应信封 schema / `Violation` 结构化校验响应 |
| axum / serde / serde_json / rmp / futures / bytes / tokio | Web 框架、DTO 反序列化、msgpack 手写编码、流式 Body、multipart 接收 |

### 下游使用者

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-dct-api = { workspace = true }` | web-server 合并 `DctModule.routes()` 到主路由 + `DctApiDoc` OpenApi merge |

> 跨 workspace 的 `cmx-portalservice` / `cmx-flowengine` 不直接依赖本 crate。

## 模块结构

```text
src/
├── lib.rs       # DctModule（impl ModuleRoutes）路由表 + pub use DctApiDoc
├── handlers.rs  # 8 个薄 handler + SafeReceiverStream（流式出口防 panic）+ Export/ImportParams + detect_format
└── openapi.rs   # DctApiDoc：utoipa OpenApi 切片（8 个 path 注解聚合，platform-app merge）
```

## 关键类型与 API

```rust
// lib.rs —— 路由聚合（由 web-server 合并；prefix()/module_name() 均为 "dct"）
pub struct DctModule;
impl cmx_api_core::routes::traits::ModuleRoutes for DctModule {
    fn routes(self) -> axum::Router<CmxAppState> { /* 上述 8 组端点 */ }
    fn prefix() -> &'static str { "dct" }
    fn module_name(&self) -> &'static str { "dct" }
}
pub use openapi::DctApiDoc;   // OpenApi 切片

// handlers.rs —— 8 个 pub handler（均挂 #[utoipa::path]，tag = "DCT字典接口"）
pub async fn dct_meta(...)              -> Result<Json<ApiResp<Value>>>;
pub async fn dct_search(...)            -> Result<Json<ApiResp<Value>>>;            // rows/total/page/pageSize
pub async fn dct_search_zmc_msgpack(...) -> Result<axum::response::Response>;        // application/x-msgpack
pub async fn dct_upsert(...)            -> Result<Json<ApiResp<Value>>>;            // {count, idMap}
pub async fn dct_delete(...)            -> Result<Json<ApiResp<Value>>>;            // {ok, deleted}
pub async fn dct_save(...)              -> Result<axum::response::Response>;        // 200 / 409 / 422
pub async fn dct_export(...)            -> Result<axum::response::Response>;        // 流式附件下载
pub async fn dct_import(...)            -> Result<Json<ApiResp<Value>>>;            // {total, affected, skipped, errors}

// 导出/导入的 query 参数 DTO（utoipa::IntoParams）
pub struct ExportParams { pub format: String }     // "json"（默认）| "csv"
pub struct ImportParams { pub mode: String }       // "upsert"（默认）| "replace" | "insert_only"

// 下游服务结果分支（自 cmx-dct-store-pg re-export 使用）
store::UpsertOutcome::{ Invalid(violations), Ok { affected, id_map } };
store::SaveOutcome::{ Invalid(violations), Conflict, Ok { affected, updated_at, id_map } };
store::BatchConflictMode::{ Upsert, InsertOnly, Replace };   // 导入冲突处理
store::ImportFormat::{ Json, Csv };                           // fmt.ext()/content_type()/as_str()
```

`SafeReceiverStream` 是本层值得注意的私有设施：把 `mpsc::Receiver<Bytes>` 包成 `Stream` 供 `Body::from_stream` 流式吐出。**不用 `futures::stream::unfold`**——unfold 在返回 `Ready(None)` 后再次 poll 会 panic，而 hyper 1.x 在客户端断开时可能再次 poll；这里用 `Option<Receiver>` 显式 take，channel 关闭后安全返回 `None`。

## 使用示例

### 场景 1：取字典元数据 + 装载数据（前端动态建表）

```bash
# 字典显示元数据：DAM/file 缺省，只传 dict（dictCode）即可；with_props=true 附带字段完整扁平属性
curl "http://host/api/dct/meta?dict=currency&with_props=true"

# 富查询装载（POST body 为 SearchQuery；filters 标量=等值/数组=IN/null=IS NULL，pageSize 上限 5000）
curl -X POST "http://host/api/dct/data/search?dict=gl_account" \
  -H 'Content-Type: application/json' \
  -d '{ "filters": { "status": "1", "id": ["101", "102"] },
        "q": "应收", "sort": { "field": "code", "order": "desc" },
        "page": 1, "pageSize": 500, "parentId": "1001" }'
# 响应：{ rows: [...], total, page, pageSize }；自分级字典传 parentId 时装载该父行的 children

# 大数据量走零拷贝 msgpack 出口（语义一致，仅出口不同）
curl -X POST "http://host/api/dct/data/tokio-zmc-msgpack?dict=gl_account" -d '{...}'
```

### 场景 2：回存（upsert 快路径 + changeset 事务路径）

```bash
# 快路径：整行数组 upsert——带真主键的行更新，临时 id/无主键行铸号插入，返回 {count, idMap}
curl -X POST "http://host/api/dct/entries?dict=currency" \
  -H 'Content-Type: application/json' \
  -d '[ { "id": "temp-1", "code": "CNY", "name": "人民币" }, { "code": "USD", "name": "美元" } ]'

# 事务路径：changeset 精确回存（对标 doc 的 ChangeSetCollector/DocSaver）
curl -X POST "http://host/api/dct/save?dict=gl_account" \
  -H 'Content-Type: application/json' \
  -d '{ "saveMode": "merge",
        "changes": { "cf_gl_account": {
            "inserted": [ { "id": "tmp-9", "fields": { "code": "1002", "name": "银行存款" } } ],
            "updated":  [ { "id": "88", "fields": { "name": "库存现金" },
                            "baseline": { "update_time": "2026-07-24T03:17:42.078808+00:00" } } ],
            "deleted":  [ "901" ] } } }'
# 响应 {ok, mode, affected, updatedAt, idMap}；baseline 不匹配（他人已改）→ HTTP 409 提示刷新重试
```

### 场景 3：流式导出 / 导入全表

```bash
# 导出：keyset 分页 + mpsc 流式吐出（服务端内存平稳）；文件名 {dictCode}_{tableName}.{ext}
curl -OJ "http://host/api/dct/export?dict=currency&format=csv"

# 导入：multipart 上传；mode 三种写语义；扩展名/Content-Type 自动识别 CSV / JSON(NDJSON)
curl -X POST "http://host/api/dct/import?dict=currency" \
  -F "file=@cf_currency.csv" -F "mode=replace"
# 响应：{ total, affected, skipped, errors }（分批 1000 行写入，replace 先 TRUNCATE）
```

### 场景 4：把 DCT 模块接入平台应用（Rust）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_dct_api::DctModule;

// web-server（cmx-platform-app）合并模块路由：/dct/* 端点注册进主 Router
let app: Router<CmxAppState> = Router::new().merge(DctModule.routes());
// OpenApi 文档聚合：DctApiDoc 的 8 个 path 注解 merge 进主文档
```
