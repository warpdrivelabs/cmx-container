# cmx-storage-api

> cmx 平台文件存储域的 HTTP 皮肤层（纯路由胶水）：把 cmx-storage::handler 的 13 个 HTTP 函数装配成 axum Router，本 crate 不写任何 handler。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-storage-api` 是六个 `cmx-*-api` 皮肤 crate 中最薄的一个：文件上传 / 下载 / 预签名 / 分片上传等 HTTP handler 函数**本就定义在 `cmx-storage::handler`**（含 `#[utoipa::path]` 注解），本 crate 只实现 `ModuleRoutes` trait 做路由注册与 OpenAPI 切片聚合——这是「HTTP 函数已随领域 crate 演进」场景下的最简皮肤形态（与 cmx-rpt-api / cmx-doc-api 同模式）。

### 关键设计

- **状态桥接**：`cmx_storage::handler` 的函数以自己的 `AppState { storage_service }` 为状态，而平台主 Router 状态是 `CmxAppState`。`FromRef<CmxAppState> for StorageAppState` 实现放在 **cmx-api-core**（孤儿规则要求与 `CmxAppState` 同 crate），本 crate 无需也不能再 impl。
- **服务注入**：`StorageService` 实例由组装层经 `CmxAppState::with_storage_service()` 注入，`StorageModule.routes()` 挂载后 handler 即可自动提取。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | `CmxAppState` / `ModuleRoutes` trait + `FromRef` 桥接（在 api-core 侧实现） |
| `cmx-storage` | `handler::*` 13 个 HTTP 函数（含 utoipa 注解）与 `types::*` schema |
| `axum` / `utoipa` | Router 装配 / OpenAPI 切片 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-storage-api = { workspace = true }` | 平台总装配器：`routes()` 中 `.merge(StorageModule.routes())`；`merged_openapi()` 中 `doc.merge(StorageApiDoc::openapi())` |
| `cmx-portalservice`（跨 workspace） | path 引用 `cmx-platform-app` | 门户微服务 bin，间接获得文件存储全部端点 |
| `cmx-flowengine`（跨 workspace） | 不直接依赖 | 流程微服务独立 workspace，仅引 cmx-core / cmx-database-pg 等基础设施 |

---

## 核心功能与特性（路由端点）

所有路由挂 `/api/storage` 前缀下（StorageModule nest `/storage`）。

| 端点 | 方法 | 说明 |
|------|------|------|
| `/storage/upload` | POST | 简单上传（单次提交，适合小文件） |
| `/storage/download` | GET | 下载单个文件 |
| `/storage/batch-download` | POST | 批量打包下载（多文件合并为 zip） |
| `/storage/info` | GET | 查询文件元信息（大小 / mime / 摘要，不返回内容） |
| `/storage/delete` | POST | 删除文件（既有接口，已按新规范改用 POST） |
| `/storage/page` | POST | 文件分页查询（按 owner / 业务维度筛选） |
| `/storage/presign-download` | POST | 预签名下载 URL（前端直链，绕过后端流量） |
| `/storage/presign-upload` | POST | 预签名上传 URL（前端直传对象存储） |
| `/storage/multipart/init` | POST | 分片上传：初始化（返回 upload_id） |
| `/storage/multipart/part` | POST | 分片上传：上传单个分片 |
| `/storage/multipart/complete` | POST | 分片上传：合并所有分片完成上传 |
| `/storage/multipart/abort` | POST | 分片上传：中止并清理已上传分片 |

---

## 模块结构

```text
cmx-storage-api
├── src
│   ├── lib.rs        # StorageModule：ModuleRoutes 实现（nest "/storage"）+ 13 条路由注册
│   └── openapi.rs    # StorageApiDoc：OpenApi 切片（cmx_storage::handler 的 12 个 path + 全套 schema）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// lib.rs —— 模块路由注册器
pub struct StorageModule;
// impl ModuleRoutes for StorageModule：
//   fn routes(self) -> Router<CmxAppState>          // nest "/storage" + 13 条路由
//   fn prefix() -> &'static str                     // "storage"
//   fn module_name(&self) -> &'static str           // "storage"

// openapi.rs —— 本域 OpenApi 切片（paths 来自 cmx_storage::handler::* 的 #[utoipa::path]，
// schemas 来自 cmx_storage::types::{FileInfo, FileQuery, FilePage, MultipartSession,
// PartInfo, PresignUploadResult, ...} 与 handler 内的请求 DTO）
#[derive(OpenApi)]
pub struct StorageApiDoc;
```

---

## 使用示例

### 一、cmx-platform-app 合并存储路由与文档（组装场景）

```rust
use cmx_storage_api::{StorageApiDoc, StorageModule};
use utoipa::OpenApi;

// 路由合并：/api/storage/* 全端点一次挂载
let router = Router::new().merge(StorageModule.routes());

// OpenAPI 聚合：以主文档为基底 merge 本域切片
let mut doc = ApiDoc::openapi();
doc.merge(StorageApiDoc::openapi());
```

### 二、组装层注入 StorageService（状态准备，见 cmx-api-core 文档）

```rust
use std::sync::Arc;
use cmx_api_core::CmxAppState;

// 组装层（如 cmx-platform-app 20 步 init 的 storage 步）构造 StorageService 实现并注入；
// FromRef<CmxAppState> for cmx_storage::handler::AppState 由 cmx-api-core 实现，
// axum 会在调用 handler 时自动拆解出 storage_service，本 crate 侧零胶水代码。
let state = CmxAppState::new()
    .with_storage_service(storage_service_impl);   // Arc<dyn StorageService>

// 之后 StorageModule.routes() 挂载的 handler 即可正常提取状态
```

---

## 设计要点

1. **零 handler 皮肤**：本 crate 只 import `cmx_storage::handler::*` 的 13 个函数注册路由，是「HTTP 函数已在领域 crate」时的最小皮肤实现；新端点直接在 cmx-storage 加 handler + 在本 crate 注册一条路由即可。
2. **孤儿规则下的桥接位置**：`FromRef` impl 必须与 `CmxAppState` 或 `StorageAppState` 之一同 crate，故放在 cmx-api-core（`CmxAppState` 的定义处），避免本 crate 违反孤儿规则。
3. **`/storage/delete` 用 POST**：注释明确标注「既有接口，已按新规范改用 POST」——对齐 AGENTS.md 第八章「除 get_by_id 外 CRUD 一律 POST + application/json」的硬约束。
