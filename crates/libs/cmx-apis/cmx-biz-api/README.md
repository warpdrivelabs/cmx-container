# cmx-biz-api

> cmx 平台业务基础模型域（domain / application / module / menu / sys_datasource / form）的 HTTP 皮肤层：写操作手写委托 cmx-biz Service（带 DAM 资产钩子），读操作复用 cmx-api-core 通用 CRUD。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-biz-api` 是 cmx-biz 业务领域层的 HTTP 适配层。cmx-biz 承载平台基础业务实体（域 / 应用 / 模块 / 菜单 / 数据源 / 表单）的 Entity / BMC / Filter / Service；本 crate 把这些能力暴露为 REST 端点，是「皮肤 vs 领域」分层在 biz 域的落地——Entity 等类型全部 `pub use cmx_biz::...` re-export，本 crate 零重定义。

### 读写分离的 handler 策略（本 crate 最大特色）

| 操作类别 | 实现方式 | 原因 |
|---------|---------|------|
| **写操作**（domain / application / module 的 create / update / delete） | 手写 handler 委托 `XxxService` | 必须触发 **DAM 资产文件副作用**：code 变更时级联搬移资源目录 + 重写关联表列；删除前做引用完整性校验。宏走 `GenericCrudService` 会绕过 Service 层钩子 |
| **读操作**（get / list / page） | 直接复用 `cmx_api_core::rest::handler` 泛型函数 | 无副作用，泛型函数按 `db_id` 头路由即可 |
| **menu 全部操作** | 手写委托 `MenuService` | 树形字段（leaf / depth / parent_code / id_path / code_path）组装与级联刷新必须走 Service |
| **sys_datasource / form 的标准 CRUD** | `declare_crud_handlers!` 宏生成 | 无文件副作用（sys_datasource 另有手写的 `-custom` 端点补连接池语义） |

### 与 cmx-plugin-api 的拆分约定

Module（模块）实体的能力拆成两半：CRUD 归本 crate（`ModuleCrudModule`，只依赖 cmx-biz）；迁移包导入导出归 cmx-plugin-api（`ModulePackageModule`，依赖 cmx-plugin）——避免 biz⇄plugin 循环依赖。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | `CmxAppState` / `CmxSvrContext` / `ApiResp` / `Result` / `ModuleRoutes` / `rest::handler` 泛型 CRUD / `rest::header_parse::get_db_id_from_header` / `declare_crud_handlers!` / `register_crud_handlers_module!` 宏 / `TreeNode` |
| `cmx-api-types` | 响应信封与文档类型源头 |
| `cmx-biz`（openapi feature） | 全部领域类型：`DomainService` / `ApplicationService` / `ModuleService` / `MenuService` / `SysDatasourceService` 及各 Entity / BMC / Filter / `DomainTreeNodeData` / `MenuTreeNodeData`；`CustomQueryService`（联表分页） |
| `cmx-core`（openapi feature） | `DataSet` / `UpdatePayload` / `DeletePayload` / `ListParams` / `PageParams` |
| `cmx-database` | `get_default_db_manager()` |
| `axum` / `serde` / `serde_json` / `tracing` / `utoipa` / `modql` | 常规 Web / 序列化 / 文档依赖 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-biz-api = { workspace = true }` | 平台总装配器：`routes()` 中依次 `.merge(DomainModule.routes())` / `ApplicationModule` / `MenuModule` / `SysDatasourceModule` / `FormModule` / `ModuleCrudModule`；`merged_openapi()` 中 `doc.merge(BizApiDoc::openapi())` |
| `cmx-portalservice`（跨 workspace） | path 引用 `cmx-platform-app` | 门户微服务 bin，间接获得 biz 域全部 HTTP 端点 |
| `cmx-flowengine`（跨 workspace） | 不直接依赖 | 流程微服务独立 workspace，仅引 cmx-core / cmx-database-pg 等基础设施 |

---

## 核心功能与特性（路由端点分组）

所有路由挂 `/api` 前缀下。

### DomainModule（`/api/domains`）

| 端点 | 方法 | 实现方式 |
|------|------|---------|
| `/domains/create` / `update` / `delete` | POST | 手写：委托 `DomainService`（update 触发 DAM 目录搬移；delete 校验域下无应用/模块） |
| `/domains/get` / `list` / `page` | GET / POST / POST | 复用 `rest::handler` 泛型函数 |
| `/domains/tree` | POST | 手写 `get_tree`：域→应用→模块三级树（递归 CTE） |

### ApplicationModule（`/api/applications`）

| 端点 | 方法 | 实现方式 |
|------|------|---------|
| `/applications/create` / `update` / `delete` | POST | 手写：委托 `ApplicationService`（写后确保应用级资源目录存在；update 搬移目录 + 重写 module 列；delete 校验无模块） |
| `/applications/get` / `list` / `page` | GET / POST / POST | 复用 `rest::handler` 泛型函数 |
| `/applications/custom-page` | POST | 手写联表分页（带 domain_name） |

### ModuleCrudModule（`/api/module`）

| 端点 | 方法 | 实现方式 |
|------|------|---------|
| `/module/create` / `update` / `delete` | POST | 手写：委托 `ModuleService`（写后确保模块资源目录存在；update 搬移目录） |
| `/module/get` / `list` / `page` | GET / POST / POST | 复用 `rest::handler` 泛型函数 |
| `/module/custom-page` | POST | 手写联表分页（带 application_name + domain_name） |

### MenuModule（`/api/menu`，全部手写委托 MenuService）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/menu/create` / `update` / `delete` | POST | 组装树形字段（leaf / depth / id_path / code_path）；update 级联刷新子节点路径；delete 校验子节点 |
| `/menu/get` | GET | 查询单个菜单节点 |
| `/menu/list` / `page` | POST | 全量 / 分页查询 |
| `/menu/tree` | GET | 取整棵菜单树（前端导航 / 权限装配用） |

### SysDatasourceModule（`/api/sys-datasource`）

| 端点 | 方法 | 实现方式 |
|------|------|---------|
| `/sys-datasource/create` / `create-many` / `get` / `update` / `update-many` / `delete` / `list` / `page` | 混合 | 宏生成（`sys_datasource_crud`） |
| `/sys-datasource/create-custom` / `update-custom` / `delete-custom` | POST | 手写：走 Service 完成 db_url 解析 / 加密 / 探活 / 连接池注册注销 / 级联清理 |
| `/sys-datasource/by-db-id` | POST | 按 db_id 反查数据源配置 |
| `/sys-datasource/test-connection` | GET | 建连探活（不持久化） |

### FormModule（`/api/form`）

| 端点 | 方法 | 实现方式 |
|------|------|---------|
| `/form/create` / `create-many` / `get` / `update` / `update-many` / `delete` / `list` / `page` | 混合 | 宏生成（`form_crud`），预留自定义路由追加位 |

---

## 模块结构

```text
cmx-biz-api
├── src
│   ├── lib.rs                          # 模块导出（BizApiDoc + 六个 Module）
│   ├── crud_handlers.rs                # declare_crud_handlers! 宏调用（sys_datasource_crud / form_crud）
│   ├── openapi.rs                      # BizApiDoc：本域 OpenApi 切片（手写 + 宏生成 paths）
│   └── handlers
│       ├── mod.rs                      #   子模块声明 + 「写手写 / 读复用」策略注释
│       ├── domain
│       │   ├── mod.rs                  #   DomainModule 路由 + cmx_biz::domain 类型 re-export
│       │   └── handler.rs              #   get_tree / create / update / delete（委托 DomainService）
│       ├── application
│       │   ├── mod.rs                  #   ApplicationModule 路由 + 类型 re-export
│       │   └── handler.rs              #   create / update / delete / custom-page（联表分页）
│       ├── module
│       │   ├── mod.rs                  #   ModuleCrudModule 路由 + 类型 re-export
│       │   └── handler.rs              #   create / update / delete / custom-page
│       ├── menu
│       │   ├── mod.rs                  #   MenuModule 路由（全手写）+ 类型 re-export
│       │   └── handler.rs              #   CRUD + tree（全委托 MenuService）
│       ├── sys_datasource
│       │   ├── mod.rs                  #   SysDatasourceModule 路由（宏 8 条 + 手写 5 条）
│       │   └── handler.rs              #   create/update/delete-custom、by-db-id、test-connection
│       └── form
│           ├── mod.rs                  #   FormModule 路由（宏 8 条）
│           └── handler.rs              #   （预留自定义 handler）
└── Cargo.toml
```

---

## 关键类型 / API

### 模块路由注册器（lib.rs 顶层导出）

```rust
pub struct DomainModule;         // prefix() = "domains"
pub struct ApplicationModule;    // prefix() = "applications"
pub struct MenuModule;           // prefix() = "menu"
pub struct SysDatasourceModule;  // prefix() = "sys-datasource"
pub struct FormModule;           // prefix() = "form"
pub struct ModuleCrudModule;     // prefix() = "module"；module_name() = "module-crud"

#[derive(OpenApi)]
pub struct BizApiDoc;            // 本域 OpenApi 切片，platform-app 用 OpenApi::merge() 聚合
```

### 领域类型 re-export（各 handlers/*/mod.rs）

```rust
// 例：handlers/domain/mod.rs —— 类型定义在 cmx-biz，此处 re-export 保持兼容
pub use cmx_biz::domain::{
    Domain, DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate,
    DomainService, DomainTreeNodeData,
};
// menu / sys_datasource / form / application / module 同构
```

### 典型 handler 签名（静态 Service 模式）

```rust
// 静态调用：XxxService::create(mm, &db_id, data)——规范 8.2 的「静态 Service 模式」
pub async fn create_domain(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<DomainForCreate>,
) -> Result<Json<ApiResp<DataSet>>>;

pub async fn get_tree(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Vec<TreeNode<DomainTreeNodeData>>>>>;
```

---

## 使用示例

### 一、cmx-platform-app 合并 biz 域路由（组装场景）

```rust
use cmx_biz_api::{
    ApplicationModule, BizApiDoc, DomainModule, FormModule, MenuModule,
    ModuleCrudModule, SysDatasourceModule,
};
use utoipa::OpenApi;

// 路由合并：六个 Module 一次挂全
let router = Router::new()
    .merge(DomainModule.routes())
    .merge(ApplicationModule.routes())
    .merge(MenuModule.routes())
    .merge(SysDatasourceModule.routes())
    .merge(FormModule.routes())
    .merge(ModuleCrudModule.routes());

// OpenAPI 聚合
let mut doc = ApiDoc::openapi();
doc.merge(BizApiDoc::openapi());
```

### 二、读写混搭路由注册（domain/mod.rs 原样模式）

```rust
use cmx_api_core::rest::handler as rest_handler;
use cmx_api_core::{CmxAppState, ModuleRoutes};
use axum::routing::{get, post};

impl ModuleRoutes for DomainModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 写操作：手写，走 DomainService（带 DAM 资产钩子）
            .route("/domains/create", post(handler::create_domain))
            .route("/domains/update", post(handler::update_domain))
            .route("/domains/delete", post(handler::delete_domain))
            // 读操作：复用 rest::handler 泛型函数（无副作用），零 handler 代码
            .route("/domains/get", get(rest_handler::get_by_id::<DomainBmc>))
            .route("/domains/list", post(rest_handler::list::<DomainBmc, DomainFilter>))
            .route("/domains/page", post(rest_handler::page::<DomainBmc, DomainFilter>))
            // 自定义：域-应用-模块三级树
            .route("/domains/tree", post(handler::get_tree))
    }
    fn prefix() -> &'static str { "domains" }
    fn module_name(&self) -> &'static str { "domain" }
}
```

### 三、手写写操作 handler（静态 Service + DAM 钩子，摘自 domain/handler.rs）

```rust
use axum::http::HeaderMap;
use cmx_core::{DataSet, UpdatePayload};
use cmx_database::get_default_db_manager;
use cmx_api_core::rest::header_parse::get_db_id_from_header;
use cmx_api_core::{ApiResp, Result};
use cmx_biz::domain::DomainService;

pub async fn update_domain(
    headers: HeaderMap,
    Json(payload): Json<UpdatePayload<DomainForUpdate>>,
) -> Result<Json<ApiResp<DataSet>>> {
    // 1. 按请求头路由数据库（db_id 缺失回退默认库）
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 2. 委托 Service：若 code 变更，内部触发 DAM 资产目录搬移 + application 表列重写
    //    （宏走 GenericCrudService 会绕过该钩子，故必须手写）
    let dataset = DomainService::update(mm, &db_id, payload.id, payload.data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
```

### 四、宏生成标准 CRUD（sys_datasource，crud_handlers.rs 原样）

```rust
use cmx_api_core::declare_crud_handlers;

// 无文件副作用的实体用宏一键生成 8 个 handler（create/create-many/get/update/
// update-many/delete/list/page，含 OpenAPI 注解；未传权限参数 = 无鉴权）
declare_crud_handlers!(
    sys_datasource_crud,
    crate::handlers::sys_datasource::SysDatasource,
    crate::handlers::sys_datasource::SysDatasourceBmc,
    crate::handlers::sys_datasource::SysDatasourceForCreate,
    crate::handlers::sys_datasource::SysDatasourceForUpdate,
    crate::handlers::sys_datasource::SysDatasourceFilter,
    "SysDatasource",
    "/sys-datasource"
);

// 路由注册（sys_datasource/mod.rs 内）：
// let router = cmx_api_core::register_crud_handlers_module!(router, sys_datasource_crud, "/sys-datasource");
// 之后可继续 .route("/sys-datasource/by-db-id", post(handler::get_by_db_id)) 追加自定义端点
```

---

## 设计要点

1. **为什么 domain/application/module 不用宏**：见 `crud_handlers.rs` 头注释——写操作需触发 DAM 资产文件副作用（目录搬移 / 引用校验），宏走 `GenericCrudService` 直接写库会绕过 Service 层钩子；菜单则是树形字段组装同理。这是「皮肤层复用泛型」与「领域钩子不可绕过」的权衡样板。
2. **类型 re-export 兼容层**：各 `handlers/*/mod.rs` 把 `cmx_biz::xxx` 的 Entity/BMC/Filter/Service `pub use` 出来，供宏泛型路由（`rest_handler::list::<DomainBmc, DomainFilter>`）与 OpenAPI schema 引用，迁移自 cmx-api 后调用路径不变。
3. **custom-page 联表分页**：application / module 的列表需带跨层名称（domain_name / application_name），走 `CustomQueryService::page_custom` 手写 SQL 联表，不复用泛型 page。
