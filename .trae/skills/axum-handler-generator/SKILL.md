---
name: axum-handler-generator
description: 在生成axum rest接口handler的时候要求遵循的规范
---

# cmx-api Handler 开发规范

> 本文档用于指导 AI 在 cmx-container 项目中生成 Handler 代码时遵循的规范。
> 请在生成任何 `cmx-api/src/handlers/` 下的 handler 代码前，仔细阅读并严格遵守。

---

## 一、cmx-api 职责边界（最重要）

cmx-api 是**纯 HTTP 适配层**，只负责协议转换，不包含任何业务逻辑。

### 1.1 cmx-api 只保留

| 内容 | 说明 |
|------|------|
| Handler 薄层 | 接收 HTTP 请求 → 调用 Service → 返回 HTTP 响应 |
| 路由注册 | 通过 `ModuleRoutes` trait + 宏系统注册路由 |
| Request/Response DTO | API 层专用请求/响应结构体（带 `ToSchema`） |
| 中间件 | 认证、CORS、限流等 HTTP 中间件 |
| OpenAPI 文档 | utoipa 注解和文档生成 |

### 1.2 cmx-api 禁止包含

| 禁止内容 | 正确位置 |
|---------|---------|
| Entity（实体结构体） | `cmx-biz/src/{module}/entity.rs` |
| BMC（表映射） | `cmx-biz/src/{module}/bmc.rs` |
| Filter（过滤器） | `cmx-biz/src/{module}/filter.rs` |
| Service（业务逻辑） | `cmx-biz/src/{module}/service.rs` |
| modql 定义（Fields/FilterNodes） | `cmx-biz/src/{module}/` |

### 1.3 从 cmx-biz 引用业务类型

cmx-api 的 handler 模块通过 **re-export** 引用 cmx-biz 的业务类型：

```rust
// cmx-api/src/handlers/xxx/mod.rs

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::xxx::{
    XxxEntity, XxxBmc, XxxFilter, XxxForCreate, XxxForUpdate, XxxService,
};
```

> **规则**：如果业务类型不存在，应先在 `cmx-biz` 中定义，然后在 cmx-api 中 re-export。

---

## 二、核心架构：参数类型与文档类型的双轨制

cmx-container 采用**双层结构体**模式处理请求参数：

| 层级 | 模块 | 用途 | 能否用 utoipa 宏 |
|------|------|------|------------------|
| **运行时参数** | `cmx-core::PageParams<F>` 等 | axum handler 实际接收的参数类型 | 否（cmx-core 没有 utoipa 依赖） |
| **文档类型** | `cmx-api::rest::param_doc::PageParamsDoc<F>` 等 | 仅用于 `#[utoipa::path]` 宏的 `request_body` 注解 | 是（实现了 `ToSchema`） |

### 2.1 双轨类型对照表

| 操作 | 运行时类型（handler 参数） | 文档类型（utoipa 宏） |
|------|--------------------------|----------------------|
| 查询单条 | `cmx_core::GetParams` | `crate::rest::param_doc::GetParamsDoc` |
| 创建 | `Json<E>`（自定义实体） | `E`（需自身有 `ToSchema`） |
| 更新 | `Json<cmx_core::UpdatePayload<E>>` | `crate::rest::param_doc::UpdatePayloadDoc<E>` |
| 删除 | `Json<cmx_core::DeletePayload>` | `crate::rest::param_doc::DeletePayloadDoc` |
| 列表查询 | `Json<cmx_core::ListParams<F>>` | `crate::rest::param_doc::ListParamsDoc<F>` |
| 分页查询 | `Json<cmx_core::PageParams<F>>` | `crate::rest::param_doc::PageParamsDoc<F>` |

> **关键规则**：handler 函数签名使用**运行时类型**，`#[utoipa::path]` 宏的 `request_body` 使用**文档类型**。

---

## 三、HTTP 方法规范

| 操作 | HTTP 方法 | Content-Type | 参数提取方式 |
|------|----------|-------------|-------------|
| 查询单条（get_by_id） | **GET** | N/A（URL 参数） | `Query<cmx_core::GetParams>` |
| 创建（create） | **POST** | `application/json` | `Json<E>` |
| 批量创建（create_many） | **POST** | `application/json` | `Json<Vec<E>>` |
| 更新（update） | **POST** | `application/json` | `Json<cmx_core::UpdatePayload<E>>` |
| 批量更新（update_many） | **POST** | `application/json` | `Json<Vec<cmx_core::UpdatePayload<E>>>` |
| 删除（delete） | **POST** | `application/json` | `Json<cmx_core::DeletePayload>` |
| 列表查询（list） | **POST** | `application/json` | `Json<cmx_core::ListParams<F>>` |
| 分页查询（page） | **POST** | `application/json` | `Json<cmx_core::PageParams<F>>` |

> **重要**：除 `get_by_id` 使用 **GET** 请求外，其他所有操作均使用 POST 请求以及 **application/json** 请求体。

---

## 四、Handler 代码模板

### 4.1 自定义 Handler（调用 cmx-biz Service）

当业务逻辑不能使用通用 CRUD 宏时，在 handler 中调用 cmx-biz 的 Service：

```rust
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_database::get_default_db_manager;
use tracing::debug;

use cmx_biz::xxx::{XxxService, XxxFilter};
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;

/// 自定义查询 Handler
#[utoipa::path(
    post,
    path = "/api/xxx/tree",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<XxxResponse>>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_tree(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Vec<XxxResponse>>>> {
    debug!("{:<12} - handler::xxx_tree", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 调用 cmx-biz 的 Service
    let tree = XxxService::get_tree(mm, &db_id).await?;

    Ok(Json(ApiResp::ok(tree)))
}
```

### 4.2 自定义分页查询 Handler

```rust
use crate::rest::PageParamsDoc;

/// 分页查询 Handler
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = PageParamsDoc<super::request::ApiXxxFilter>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<XxxResponse>>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<cmx_core::PageParams<super::request::ApiXxxFilter>>,
) -> Result<Json<ApiResp<Vec<XxxResponse>>>> {
    let page = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let skip = params.get_offset() as usize;

    let filter: XxxFilter = params.filter.unwrap_or_default().into();

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 调用 cmx-biz 的 Service
    let (data, total) = XxxService::page(mm, &db_id, vec![filter], page, page_size).await?;

    let items: Vec<XxxResponse> = data.into_iter().map(Into::into).collect();
    Ok(Json(ApiResp::ok_with_pagination(items, page, page_size, total)))
}
```

### 4.3 调用其他业务 crate 的 Handler

对于不在 cmx-biz 中的业务（如 cmx-plugin），直接调用对应 crate 的全局单例：

```rust
/// 插件安装 Handler
#[utoipa::path(
    post,
    path = "/api/plugin/install",
    request_body = PluginInstallRequest,
    responses(
        (status = 200, description = "安装成功", body = ApiResp<InstallResponse>)
    ),
    tag = "Plugin"
)]
pub async fn plugin_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginInstallRequest>,
) -> Result<Json<ApiResp<InstallResponse>>> {
    let manager = cmx_plugin::GlobalPluginManager::get();
    let result = manager.install(req.into()).await
        .map_err(|e| crate::Error::InternalError(format!("安装失败: {}", e)))?;
    Ok(Json(ApiResp::ok(result.into())))
}
```

---

## 五、宏系统：标准 CRUD Handler 生成

> 当实体已在 cmx-biz 中定义好 Entity/BMC/Filter 时，**必须**使用宏系统生成 CRUD handler，禁止手写。

### 5.1 注册宏调用（crud_handlers.rs）

在 `cmx-api/src/routes/crud_handlers.rs` 中调用 `declare_crud_handlers!` 宏：

```rust
use crate::declare_crud_handlers;

// 为每个实体声明 CRUD handler 模块
declare_crud_handlers!(
    domain_crud,                              // 模块名
    crate::handlers::domain::Domain,          // 实体类型（从 cmx-biz re-export）
    crate::handlers::domain::DomainBmc,       // BMC 类型
    crate::handlers::domain::DomainForCreate, // 创建 DTO
    crate::handlers::domain::DomainForUpdate, // 更新 DTO
    crate::handlers::domain::DomainFilter,    // 过滤器
    "Domain",                                  // OpenAPI tag
    "/domains"                                 // 路由前缀
);
```

宏会生成 8 个 handler 函数：`create`, `create_many`, `get`, `update`, `update_many`, `delete`, `list`, `page`。

### 5.2 在 mod.rs 中注册路由

使用 `register_crud_handlers_module!` 宏注册生成的 CRUD 路由，再叠加自定义路由：

```rust
use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

pub struct XxxModule;

impl ModuleRoutes for XxxModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册标准 CRUD 路由（8个）
        let router = crate::register_crud_handlers_module!(router, xxx_crud, "/xxx");
        // 注册自定义路由
        router.route("/xxx/tree", post(handler::xxx_tree))
    }

    fn prefix() -> &'static str { "xxx" }
    fn module_name(&self) -> &'static str { "xxx" }
}
```

### 5.3 宏生成的路由路径

| 操作 | 路径 |
|------|------|
| 创建 | `/{prefix}/create` |
| 批量创建 | `/{prefix}/create-many` |
| 查询单条 | `/{prefix}/get` |
| 更新 | `/{prefix}/update` |
| 批量更新 | `/{prefix}/update-many` |
| 删除 | `/{prefix}/delete` |
| 列表查询 | `/{prefix}/list` |
| 分页查询 | `/{prefix}/page` |

---

## 六、request.rs 请求结构体规范

### 6.1 API 层请求结构体（必须带 `ToSchema`）

API 层定义自己的请求结构体（与 domain 层解耦），必须派生 `ToSchema`：

```rust
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// 安装请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct XxxInstallRequest {
    /// 来源
    pub source: XxxSourceRequest,
    /// 目标ID
    pub target_id: Option<String>,
}

/// 来源请求（使用 serde tag 枚举）
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum XxxSourceRequest {
    /// 本地路径
    Local { path: String },
    /// 远程 URL
    Remote { url: String, checksum: Option<String> },
}

/// 列表查询参数（使用 IntoParams 生成路径参数）
#[derive(Debug, Deserialize, IntoParams)]
pub struct XxxListQuery {
    /// 状态过滤
    pub status: Option<String>,
}
```

### 6.2 API 层过滤条件结构体（必须带 `ToSchema`）

当需要自定义分页查询时，API 层定义过滤条件结构体并实现到 cmx-biz Filter 的转换：

```rust
use utoipa::ToSchema;

/// API 层过滤条件
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct ApiXxxFilter {
    pub status: Option<String>,
    pub name: Option<String>,
}

/// 实现 From 转换到 cmx-biz 层过滤类型
impl From<ApiXxxFilter> for cmx_biz::xxx::XxxFilter {
    fn from(api: ApiXxxFilter) -> Self {
        Self {
            status: api.status.and_then(|s| s.parse().ok()),
            name: api.name,
        }
    }
}
```

### 6.3 注意事项

- API 层请求/过滤字段使用 `Option<String>`，枚举字段在 `From` 转换时 parse
- `#[serde(rename_all = "camelCase")]` 如果需要前端驼峰命名
- cmx-biz 层的枚举类型需要实现 `std::str::FromStr` 以支持字符串解析
- 使用 `#[serde(tag = "type")]` 实现带类型的枚举请求

---

## 七、分页响应规范

使用 `ApiResp::ok_with_pagination` 返回分页数据：

```rust
ApiResp::ok_with_pagination(data, page, page_size, total)
```

返回的 JSON 结构：
```json
{
    "code": 0,
    "msg": "success",
    "data": [...],
    "pagination": {
        "page": 1,
        "pageSize": 20,
        "total": 100,
        "totalPages": 5
    }
}
```

---

## 八、路由注册规范

### 8.1 路由路径命名规范

**每个操作必须使用独立的路径**，严禁不同操作共享同一路径仅靠 HTTP 方法区分。

```
/xxx/create  → POST 创建
/xxx/list    → POST 列表查询
/xxx/page    → POST 分页查询
/xxx/get     → GET  查询单条
/xxx/update  → POST 更新
/xxx/delete  → POST 删除
```

> 禁止 `/xxx` 同时用于 GET(详情) 和 POST(发布)，这会导致路由冲突和语义混乱。

### 8.2 ModuleRoutes trait 模式

所有 handler 模块**必须**实现 `ModuleRoutes` trait：

```rust
use crate::routes::traits::ModuleRoutes;

pub struct XxxModule;

impl ModuleRoutes for XxxModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 标准 CRUD（如有）
        let router = crate::register_crud_handlers_module!(router, xxx_crud, "/xxx");
        // 自定义路由
        router
            .route("/xxx/custom", post(handler::xxx_custom))
    }

    fn prefix() -> &'static str { "xxx" }
    fn module_name(&self) -> &'static str { "xxx" }
}
```

### 8.3 嵌套路由模式（复杂模块）

复杂模块（如 plugin）使用 `nest` 嵌套路由：

```rust
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/install", post(plugin_install))
        .route("/uninstall", post(plugin_uninstall))
        .route("/list", post(plugin_list))
}

pub struct PluginModule;

impl ModuleRoutes for PluginModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/plugin", inner_routes())
    }
    fn prefix() -> &'static str { "plugin" }
    fn module_name(&self) -> &'static str { "plugin" }
}
```

---

## 九、Handler 文件组织结构

### 9.1 标准模块结构（Entity 在 cmx-biz 中）

当 Entity/BMC/Filter/Service 已在 cmx-biz 中定义时，cmx-api 中只有薄层：

```
cmx-api/src/handlers/xxx/
  ├── mod.rs       # 模块导出（re-export cmx-biz 类型）+ ModuleRoutes 实现
  └── handler.rs   # 仅自定义 Handler 函数（标准 CRUD 由宏生成）
```

**mod.rs 示例**：

```rust
//! Xxx 模块
//!
//! 提供 Xxx 相关的 HTTP API
//! Entity/BMC/Filter/Service 已在 cmx-biz crate 中定义

pub mod handler;

// 从 cmx-biz re-export 业务层类型（供宏系统使用）
pub use cmx_biz::xxx::{
    XxxEntity, XxxBmc, XxxFilter, XxxForCreate, XxxForUpdate, XxxService,
};

// 导出自定义 handler
pub use handler::xxx_tree;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

pub struct XxxModule;

impl ModuleRoutes for XxxModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        let router = crate::register_crud_handlers_module!(router, xxx_crud, "/xxx");
        router.route("/xxx/tree", post(handler::xxx_tree))
    }
    fn prefix() -> &'static str { "xxx" }
    fn module_name(&self) -> &'static str { "xxx" }
}
```

### 9.2 带 Request/Response DTO 的模块结构

当 handler 需要 API 层专用请求/响应结构体时：

```
cmx-api/src/handlers/xxx/
  ├── mod.rs       # 模块导出 + ModuleRoutes 实现
  ├── handler.rs   # Handler 函数实现
  ├── request.rs   # API 层请求结构体（派生 ToSchema）
  └── response.rs  # API 层响应结构体
```

### 9.3 新增实体的完整步骤

1. **在 cmx-biz 中定义业务类型**：Entity/BMC/Filter/Service（参见第十章）
2. **在 cmx-api 中创建 handler 模块**：`handlers/xxx/mod.rs`
3. **re-export cmx-biz 类型**：`pub use cmx_biz::xxx::*;`
4. **注册宏**：在 `routes/crud_handlers.rs` 中调用 `declare_crud_handlers!`
5. **实现 ModuleRoutes**：在 mod.rs 中注册标准 CRUD + 自定义路由
6. **注册到总路由**：在 `routes/routes_impl.rs` 中添加模块

---

## 十、cmx-biz 业务类型定义规范（Entity/BMC/Filter/Service）

> 本章节说明 **cmx-api 依赖的业务类型** 如何在 cmx-biz 中定义。
> 当 cmx-api 的 handler 需要操作数据库实体时，必须先在 cmx-biz 中定义好以下内容，
> 然后在 cmx-api 中通过 re-export 引用。

### 10.1 cmx-biz 目录结构

每个业务实体在 cmx-biz 中独立一个模块：

```
cmx-biz/src/
  ├── xxx/
  │   ├── mod.rs       # 模块导出
  │   ├── entity.rs    # Entity / ForCreate / ForUpdate（derive Fields）
  │   ├── bmc.rs       # Bmc 结构体（impl DbBmc）
  │   ├── filter.rs    # Filter 结构体（derive FilterNodes）
  │   └── service.rs   # Service 层（包装 GenericCrudService 或自定义 SQL）
  └── lib.rs
```

### 10.2 实体结构体（Entity）

使用 `#[derive(Fields)]` 让 modql 自动生成字段元数据。每种操作定义独立的结构体：

```rust
// cmx-biz/src/xxx/entity.rs
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 完整实体（查询返回用）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct XxxEntity {
    pub id: String,
    pub code: String,
    pub name: Option<String>,
    pub status: Option<String>,
    pub archived: Option<i32>,
    pub create_time: Option<String>,
    pub update_time: Option<String>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 创建请求 DTO（不含自动生成字段）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct XxxForCreate {
    pub code: String,
    pub name: Option<String>,
}

/// 更新请求 DTO（所有字段可选，仅更新提供的字段）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct XxxForUpdate {
    pub name: Option<String>,
    pub status: Option<String>,
}
```

**要点**：
- `ForCreate` 不包含 `id`、`create_time`、`update_time` 等自动生成字段（GenericCrudService 自动处理）
- `ForUpdate` 所有字段都是 `Option`，GenericCrudService 只更新非 None 的字段
- `Fields` derive 宏让结构体实现 `HasSeaFields`，GenericCrudService 通过它构建 INSERT/UPDATE SQL
- 需要 `ToSchema` 以支持 cmx-api 的 OpenAPI 文档（cmx-biz 依赖 utoipa）

### 10.3 过滤器结构体（Filter）

使用 `#[derive(FilterNodes)]` 让 modql 自动实现 `IntoFilterNodes` trait：

```rust
// cmx-biz/src/xxx/filter.rs
use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct XxxFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub status: Option<OpValsString>,
    pub archived: Option<OpValsInt64>,
}
```

**字段类型映射**：
| 数据库类型 | Filter 字段类型 | 支持的操作符 |
|-----------|----------------|-------------|
| varchar/text | `Option<OpValsString>` | Eq, Not, In, NotIn, Contains, StartsWith, EndsWith, ContainsCi, Empty, Null 等 |
| int4/int8 | `Option<OpValsInt64>` | Eq, Not, In, NotIn, Lt, Lte, Gt, Gte, Null |
| bool | `Option<OpValsBool>` | Eq, Not, Null |
| float/double | `Option<OpValsFloat64>` | Eq, Not, In, NotIn, Lt, Lte, Gt, Gte, Null |

**前端 JSON 调用示例**：
```json
{
    "filter": {
        "name": {"$contains": "test"},
        "status": {"$eq": "published"},
        "archived": {"$eq": 0}
    },
    "page": 1,
    "size": 20
}
```

**多表 JOIN 时指定表别名**：
```rust
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct XxxFilter {
    #[modql(rel = "a")]
    pub code: Option<OpValsString>,
    #[modql(rel = "a")]
    pub name: Option<OpValsString>,
}
```

### 10.4 Bmc 结构体（表映射）

实现 `DbBmc` trait 告诉 GenericCrudService 表名和主键列：

```rust
// cmx-biz/src/xxx/bmc.rs
use cmx_database::crud::DbBmc;

pub struct XxxBmc;

impl DbBmc for XxxBmc {
    const TABLE: &'static str = "cmx_xxx";
    const PK_COLUMN: &'static str = "id";
}
```

**DbBmc 可配置项**：
| 方法 | 默认值 | 说明 |
|------|--------|------|
| `TABLE` | （必须指定） | 表名 |
| `PK_COLUMN` | `"code"` | 主键列名 |
| `has_timestamps()` | `true` | 是否自动填充 create_time/update_time |
| `has_owner_id()` | `false` | 是否自动填充 owner_id |
| `encrypted_fields()` | `&[]` | 需要加解密的字段列表 |

### 10.5 Service 层（GenericCrudService）

```rust
// cmx-biz/src/xxx/service.rs
use cmx_database::crud::GenericCrudService;
use cmx_database::get_default_db_manager;
use cmx_core::model::data::dataset::DataSet;
use sea_query::Value;
use crate::xxx::{XxxBmc, XxxForCreate, XxxForUpdate, XxxFilter};
use crate::error::Result;

pub struct XxxService;

impl XxxService {
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: XxxForCreate) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::create(mm, db_id, None, data).await
    }

    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::get(mm, db_id, None, id.into()).await
    }

    pub async fn update(mm: &DatabaseManager, db_id: &str, id: Value, data: XxxForUpdate) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::update(mm, db_id, None, id, data).await
    }

    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::delete(mm, db_id, None, ids).await
    }

    pub async fn page(
        mm: &DatabaseManager, db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<XxxBmc, XxxFilter>::page(
            mm, db_id, None, filters, list_options,
        ).await
    }
}
```

### 10.6 Service 方法参数规范

**严禁**将多个参数平铺到函数签名中。**必须**使用结构体传递：

```rust
// ❌ 错误：参数平铺，臃肿且难以维护
pub async fn publish(
    &self,
    id: String,
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    // ... 20+ 个参数
) -> Result<Xxx>

// ✅ 正确：使用结构体参数
pub async fn publish(
    &self,
    req: PublishRequest,
) -> Result<Xxx>
```

### 10.7 mod.rs 导出

```rust
// cmx-biz/src/xxx/mod.rs
pub mod entity;
pub mod bmc;
pub mod filter;
pub mod service;

pub use entity::{XxxEntity, XxxForCreate, XxxForUpdate};
pub use bmc::XxxBmc;
pub use filter::XxxFilter;
pub use service::XxxService;
```

并在 `cmx-biz/src/lib.rs` 中导出模块：

```rust
// cmx-biz/src/lib.rs
pub mod xxx;
```

### 10.8 何时使用 GenericCrudService vs 自定义 SQL

| 场景 | 推荐方式 |
|------|---------|
| 单表 CRUD（增删改查） | `GenericCrudService` |
| 单表分页/列表查询 | `GenericCrudService::page/list` + `FilterNodes` |
| 多表 JOIN 查询 | `CustomQueryService::page_custom` + `FilterNodes` |
| INSERT ... ON CONFLICT（UPSERT） | 自定义 SQL |
| 聚合统计（GROUP BY / SUM / AVG） | 自定义 SQL |
| 跨表事务操作 | 自定义 SQL + 事务 |

---

## 十一、import 规范

```rust
// axum 标准导入
use axum::extract::{State, Query};
use axum::http::HeaderMap;
use axum::Json;

// cmx-api 内部
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;

// utoipa 文档类型导入（按需）
use crate::rest::PageParamsDoc;

// cmx-core 运行时参数（通过 cmx_core:: 直接使用）
// PageParams, GetParams, ListParams, UpdatePayload, DeletePayload

// cmx-biz 业务类型（从 cmx-biz 引用，不在 cmx-api 中定义）
use cmx_biz::xxx::{XxxService, XxxFilter};

// 请求/响应类型
use super::request::*;
use super::response::*;
```

---

## 十二、关键源文件参考

| 文件 | 说明 |
|------|------|
| `cmx-api/src/routes/macros.rs` | CRUD handler 生成宏和路由注册宏 |
| `cmx-api/src/routes/crud_handlers.rs` | 各实体的宏调用集中管理 |
| `cmx-api/src/routes/traits.rs` | ModuleRoutes trait 定义 |
| `cmx-api/src/rest/handler.rs` | 通用 CRUD Handler 函数（宏内部调用） |
| `cmx-api/src/rest/param_doc.rs` | utoipa 文档类型定义（PageParamsDoc 等） |
| `cmx-api/src/api_response.rs` | 统一响应结构 ApiResp 和 Pagination |
| `cmx-api/src/handlers/domain/mod.rs` | 标准模块参考（re-export + ModuleRoutes） |
| `cmx-api/src/handlers/plugin/mod.rs` | 复杂模块参考（嵌套路由 + Request/Response） |
| `cmx-biz/src/domain/` | Entity/BMC/Filter/Service 定义参考 |

---

## 十三、新架构薄层 Handler 模式（cmx-iam 等）

> 适用于业务逻辑在独立 crate（如 cmx-iam）中实现、通过 trait 对象注入 CmxAppState 的场景。
> 与第九章的 cmx-biz 模式不同，此模式**不使用宏系统**，所有 handler 均手写。

### 13.1 适用场景

- 业务 crate 独立于 cmx-biz（如 cmx-iam、cmx-auth）
- Service 通过 `Arc<dyn Trait>` 注入 CmxAppState，而非静态方法调用
- Entity/Filter 在业务 crate 中定义，cmx-api 通过依赖引用
- 不需要 request.rs/response.rs（直接使用业务 crate 的类型）

### 13.2 目录结构

```
cmx-api/src/handlers/iam/
  ├── mod.rs           # IamModule（ModuleRoutes）聚合 user/role/permission 子模块
  ├── user/
  │   ├── mod.rs       # UserModule（ModuleRoutes）路由注册
  │   └── handler.rs   # 用户 handler 函数
  ├── role/
  │   ├── mod.rs       # RoleModule（ModuleRoutes）路由注册
  │   └── handler.rs   # 角色 handler 函数
  └── permission/
      ├── mod.rs       # PermissionModule（ModuleRoutes）路由注册
      └── handler.rs   # 权限 handler 函数
```

**关键区别**：
- 无 `request.rs` / `response.rs` — 直接使用业务 crate 的 Entity/Filter 类型
- 无宏系统调用 — 所有路由手写注册
- Entity 从业务 crate 直接导入（非 re-export from cmx-biz）

### 13.3 Handler 实现模式

Handler 通过 `cmx_state.iam()` 获取 `IamState`，调用对应的 service trait 方法：

```rust
use axum::extract::State;
use axum::Json;
use cmx_core::SVRContext;
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;

/// 创建用户
#[utoipa::path(
    post,
    path = "/api/iam/user/create",
    request_body = UserForCreate,
    responses((status = 200, description = "创建成功", body = ApiResp<User>)),
    tag = "IAM-User"
)]
pub async fn create_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<cmx_iam::user::UserForCreate>,
) -> Result<Json<ApiResp<cmx_core::model::iam::User>>> {
    let iam = cmx_state.iam().ok_or(crate::error::Error::InternalError(
        "IAM 服务未初始化".to_string(),
    ))?;
    let user = iam.user_service.create_user(&svr_ctx, data).await?;
    Ok(Json(ApiResp::ok(user)))
}
```

### 13.4 类型转换要点

| 场景 | 转换方式 |
|------|---------|
| `UpdatePayload.id: Value` → `&str` | `payload.id.as_str().ok_or(...)?` |
| `DeletePayload.ids: Vec<Value>` → `Vec<String>` | `ids.into_iter().filter_map(\|v\| v.as_str().map(\|s\| s.to_string())).collect()` |
| `PageParams.filters: Option<Vec<F>>` → 单个 `Filter` | `params.filters.and_then(\|v\| v.into_iter().next()).unwrap_or_default()` |
| 分页响应 | `ApiResp::ok_with_pagination(data, page, size, total)` |

### 13.5 mod.rs 路由注册

```rust
use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

pub struct UserModule;

impl ModuleRoutes for UserModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .route("/iam/user/create", post(handler::create_user))
            .route("/iam/user/get", get(handler::get_user))
            .route("/iam/user/update", post(handler::update_user))
            .route("/iam/user/delete", post(handler::delete_user))
            .route("/iam/user/page", post(handler::page_users))
            .route("/iam/user/list", post(handler::list_users))
            .route("/iam/user/assign-roles", post(handler::assign_roles))
            .route("/iam/user/roles", get(handler::get_user_roles))
    }
    fn prefix() -> &'static str { "iam" }
    fn module_name(&self) -> &'static str { "iam-user" }
}
```

### 13.6 聚合模块

顶层 `iam/mod.rs` 聚合子模块路由：

```rust
use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;

pub mod user;
pub mod role;
pub mod permission;

pub struct IamModule;

impl ModuleRoutes for IamModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        let router = router.merge(user::UserModule.routes());
        let router = router.merge(role::RoleModule.routes());
        let router = router.merge(permission::PermissionModule.routes());
        router
    }
    fn prefix() -> &'static str { "iam" }
    fn module_name(&self) -> &'static str { "iam" }
}
```

### 13.7 与 cmx-biz 模式的对比

| 维度 | cmx-biz 模式（第九章） | 新架构薄层模式（本章） |
|------|----------------------|---------------------|
| Service 调用 | 静态方法 `XxxService::create(mm, db_id, ...)` | trait 对象 `iam.user_service.create_user(...)` |
| Entity 来源 | `cmx-biz` re-export | 直接 `use cmx_iam::user::UserForCreate` |
| CRUD 生成 | `declare_crud_handlers!` 宏 | 手写所有 handler |
| db_id 获取 | `get_db_id_from_header(&headers)` | Service 内部持有 db_id |
| 状态注入 | 无需 CmxAppState（静态方法） | 通过 `CmxAppState.iam()` 获取 |
| request.rs | 可选（API 层 DTO） | 不需要（直接用业务 crate 类型） |
