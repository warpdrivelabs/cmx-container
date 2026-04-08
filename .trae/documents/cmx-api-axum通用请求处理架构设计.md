# CMX-API 通用请求处理架构设计

## 1. 概述

### 1.1 设计目标

1. 提供强类型的 Entity 数据参数，解决 JSON 无法正确表达时间类型的问题
2. 提供通用的 CRUD 操作框架，支持批量操作
3. 支持扩展和自定义业务逻辑
4. 编译时类型检查，提高代码安全性
5. 自动生成 OpenAPI 文档

### 1.2 核心改进

| 改进项       | 改进前                 | 改进后                                    |
| --------- | ------------------- | -------------------------------------- |
| data 参数类型 | `serde_json::Value` | 强类型 Entity                             |
| 字段处理      | 手动遍历 Value          | `HasSeaFields::not_none_sea_fields()`  |
| 类型安全      | 运行时检查               | 编译时检查                                  |
| 创建/更新区分   | 无                   | ForCreate / ForUpdate                  |
| 批量操作      | 不支持                 | `create_many`, `update_many`, `delete` |
| 删除方法      | GET + Query         | POST + JSON Body                       |
| OpenAPI   | 手动编写                | 自动生成                                   |

## 2. 目录结构

```
crates/libs/cmx-api/src/
├── rest/                      # REST 协议层
│   ├── mod.rs
│   ├── param_doc.rs           # 参数文档类型
│   ├── handler.rs             # 通用 Handler
│   ├── header_parse.rs        # Header 解析
│   └── tree.rs                # 树形结构工具
│
├── routes/                    # 路由注册模块
│   ├── mod.rs
│   ├── routes.rs              # 统一注册入口
│   ├── traits.rs              # ModuleRoutes trait
│   ├── macros.rs              # 路由注册宏
│   └── crud_handlers.rs       # CRUD handlers 声明
│
├── handlers/                  # 业务模型层
│   └── domain/                # Domain 实体模块
│       ├── mod.rs             # 模块入口 + ModuleRoutes 实现
│       ├── bmc.rs             # DomainBmc
│       ├── entity.rs          # Domain, DomainForCreate, DomainForUpdate
│       ├── filter.rs          # DomainFilter
│       ├── service.rs         # DomainService（自定义服务）
│       └── handler.rs         # 自定义 Handler
│
├── middleware/                # 中间件
│   ├── mod.rs
│   ├── mw_context.rs          # 上下文中间件
│   ├── mw_cors.rs             # CORS 中间件
│   ├── mw_rate_limit.rs       # 限流中间件
│   ├── mw_security_headers.rs # 安全头中间件
│   └── mw_trace.rs            # 追踪中间件
│
├── api_response.rs            # API 响应封装
├── error.rs                   # 错误类型
├── app_state.rs               # 应用状态
├── openapi.rs                 # OpenAPI 文档
└── lib.rs                     # 模块入口
```

### 2.1 扩展点

```rust
// 1. GenericCrudService - 可继承扩展（来自 cmx-database）
pub struct GenericCrudService<MC, F = ()> { ... }

// 2. DbBmc trait - 可实现自定义表元信息（来自 cmx-database）
pub trait DbBmc { ... }

// 3. Handler 函数 - 可自定义
pub async fn create<MC, E>(...) { ... }

// 4. 宏 - 可组合使用
declare_crud_handlers!(...);
register_crud_handlers_module!(...);
```

## 3. Entity 定义规范

### 3.1 统一 Entity 文件

所有实体元数据定义放在一个文件中：

```rust
// handlers/domain/entity.rs

use crate::rest::TreeNodeData;
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 领域实体（完整字段，用于查询返回）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow, ToSchema)]
pub struct Domain {
    pub id: String,
    /// 唯一标识码
    pub code: String,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型（使用 #[field] 映射数据库字段名）
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name = "type")]
    pub r#type: Option<String>,
    /// 标签（JSON 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态（0: 禁用, 1: 启用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 是否归档（0: 否, 1: 是）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
    /// 创建时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
}

/// 创建请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct DomainForCreate {
    pub code: String,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name = "type")]
    pub r#type: Option<String>,
}

/// 更新请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct DomainForUpdate {
    /// 名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
}
```

### 3.2 关键点说明

1. **Fields 派生宏**：`#[derive(modql::field::Fields)]` 自动实现 `HasSeaFields` trait
2. **字段映射**：使用 `#[field(name = "type")]` 将 Rust 保留字 `r#type` 映射到数据库字段 `type`
3. **跳过 None 值**：`#[serde(skip_serializing_if = "Option::is_none")]` 确保可选字段为 None 时不序列化
4. **ToSchema**：`#[derive(utoipa::ToSchema)]` 支持 OpenAPI 文档生成

## 4. 通用 REST Handler

### 4.1 核心方法

```rust
// rest/handler.rs

use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::{DeletePayload, GetParams, ListParams, PageParams, UpdatePayload};
use cmx_database::crud::{DbBmc, GenericCrudService};
use cmx_database::get_default_db_manager;
use modql::field::HasSeaFields;
use serde::de::DeserializeOwned;

/// 创建单个实体 Handler
pub async fn create<MC, E>(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<E>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = GenericCrudService::<MC>::create(mm, &db_id, None, data).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

/// 根据主键获取单条实体的 Handler
pub async fn get_by_id<MC>(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<GetParams>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let id = params.id.clone();
    let dataset = GenericCrudService::<MC>::get(mm, &db_id, None, id.into()).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新单个实体 Handler
pub async fn update<MC, E>(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<UpdatePayload<E>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = GenericCrudService::<MC>::update(mm, &db_id, None, payload.id, payload.data).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除实体 Handler（支持单个和批量）
pub async fn delete<MC>(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let dataset = GenericCrudService::<MC>::delete(mm, &db_id, None, payload.ids).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

/// 分页查询的 Handler
pub async fn page<MC, F>(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<F>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    F: DeserializeOwned + Into<modql::filter::FilterGroups> + modql::filter::IntoFilterNodes + Clone,
{
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let list_options = params.to_list_options();

    let mut filters = params.filters.clone();
    if params.filters.is_none() || params.filters.unwrap().is_empty() {
        filters = None;
    }
    if let Some(filter) = params.filter.clone() {
        filters = Some(vec![filter]);
    }

    let (dataset, total) = GenericCrudService::<MC, F>::page(mm, &db_id, None, filters, list_options).await?;
    Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
}
```

## 5. 参数类型

### 5.1 运行时参数类型（来自 cmx-core）

```rust
// cmx-core 中的参数定义

/// 获取单条记录的查询参数
pub struct GetParams {
    pub id: String,
    pub db_id: Option<String>,
}

/// 更新请求 Payload
pub struct UpdatePayload<E> {
    pub id: Value,
    pub data: E,
}

/// 删除请求 Payload
pub struct DeletePayload {
    pub ids: Vec<Value>,
}

/// 列表查询参数
pub struct ListParams<F> {
    pub filter: Option<F>,
    pub filters: Option<Vec<F>>,
    pub order_bys: Option<String>,
}

/// 分页查询参数
pub struct PageParams<F> {
    pub filter: Option<F>,
    pub filters: Option<Vec<F>>,
    pub current: Option<i64>,
    pub size: Option<i64>,
}
```

### 5.2 文档参数类型（用于 OpenAPI）

```rust
// rest/param_doc.rs

use modql::filter::ListOptions;
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;

/// 分页默认每页条数
pub const PAGE_SIZE_DEFAULT: i64 = 20;

/// 分页最大每页条数
pub const PAGE_SIZE_MAX: i64 = 500;

/// 获取单条记录的查询参数
#[derive(Debug, Deserialize, Clone)]
pub struct GetParamsDoc {
    pub id: String,
}

/// 更新请求 Payload
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdatePayloadDoc<E> {
    pub id: Value,
    pub data: E,
}

/// 删除请求 Payload
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeletePayloadDoc {
    pub ids: Vec<Value>,
}

/// 列表查询参数
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct ListParamsDoc<F> {
    pub filter: Option<F>,
    pub filters: Option<Vec<F>>,
    pub order_bys: Option<String>,
}

/// 分页查询参数
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct PageParamsDoc<F> {
    pub filter: Option<F>,
    pub filters: Option<Vec<F>>,
    pub current: Option<i64>,
    pub size: Option<i64>,
}
```

## 6. API 响应封装

```rust
// api_response.rs

use serde::Serialize;
use utoipa::ToSchema;

/// API 统一响应结构
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiResp<T> {
    pub code: u16,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
}

impl<T> ApiResp<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            msg: "success".to_string(),
            data: Some(data),
            pagination: None,
        }
    }

    pub fn ok_with_pagination(data: T, page: u64, page_size: u64, total: u64) -> Self {
        let total_pages = (total as f64 / page_size as f64).ceil() as u64;
        Self {
            code: 0,
            msg: "success".to_string(),
            data: Some(data),
            pagination: Some(Pagination {
                page,
                page_size,
                total,
                total_pages,
            }),
        }
    }

    pub fn fail(code: u16, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
            data: None,
            pagination: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub page: u64,
    pub page_size: u64,
    pub total: u64,
    pub total_pages: u64,
}
```

## 7. 错误处理

```rust
// error.rs

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

/// 错误码枚举
#[derive(Debug, Clone, Copy, Serialize)]
pub enum ErrCode {
    Success = 0,
    /// 业务错误（HTTP 200，json code 1）
    BusinessError = 1,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    BadRequest = 400,
    InternalError = 500,
}

/// 结果类型
pub type Result<T> = core::result::Result<T, Error>;

/// Web 层错误类型
#[derive(Debug, Error)]
pub enum Error {
    /// 业务错误（HTTP 200，json code 1）
    #[error("{0}")]
    BusinessError(String),

    #[error("未授权: {0}")]
    Unauthorized(String),

    #[error("请求错误: {0}")]
    BadRequest(String),

    #[error("{0}")]
    InternalError(String),

    // ... 其他错误类型
}

impl Error {
    pub fn code(&self) -> ErrCode {
        match self {
            Self::BusinessError(_) => ErrCode::BusinessError,
            Self::Unauthorized(_) => ErrCode::Unauthorized,
            Self::BadRequest(_) => ErrCode::BadRequest,
            Self::InternalError(_) => ErrCode::InternalError,
            _ => ErrCode::InternalError,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BusinessError(_) => StatusCode::OK,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn business_error(msg: impl Into<String>) -> Self {
        Self::BusinessError(msg.into())
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status_code = self.status_code();
        let body = json!({
            "code": self.code() as u16,
            "msg": self.to_string(),
        });
        (status_code, axum::Json(body)).into_response()
    }
}
```

## 8. 路由注册

### 8.1 路由注册宏

```rust
/// 声明 CRUD Handlers 模块
#[macro_export]
macro_rules! declare_crud_handlers {
    (
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr
    ) => {
        pub mod $module_name {
            // 生成 8 个 handler 函数：
            // - create, create_many
            // - get
            // - update, update_many
            // - delete
            // - list, page
            // 每个 handler 都带有 #[utoipa::path] 注解
        }
    };
}

/// 注册已生成的 CRUD Handlers 模块到路由
#[macro_export]
macro_rules! register_crud_handlers_module {
    ($router:expr, $handlers_mod:ident, $prefix:expr) => {
        $router
            .route(concat!($prefix, "/create"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::create))
            .route(concat!($prefix, "/create-many"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::create_many))
            .route(concat!($prefix, "/get"), axum::routing::get(crate::routes::crud_handlers::$handlers_mod::get))
            .route(concat!($prefix, "/update"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::update))
            .route(concat!($prefix, "/update-many"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::update_many))
            .route(concat!($prefix, "/delete"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::delete))
            .route(concat!($prefix, "/list"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::list))
            .route(concat!($prefix, "/page"), axum::routing::post(crate::routes::crud_handlers::$handlers_mod::page))
    };
}
```

### 8.2 使用示例

```rust
// routes/crud_handlers.rs

use crate::declare_crud_handlers;

declare_crud_handlers!(
    domain_crud,
    crate::handlers::domain::Domain,
    crate::handlers::domain::DomainBmc,
    crate::handlers::domain::DomainForCreate,
    crate::handlers::domain::DomainForUpdate,
    crate::handlers::domain::DomainFilter,
    "Domain",
    "/domains"
);

// handlers/domain/mod.rs

impl ModuleRoutes for DomainModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 CRUD 路由
        let router = crate::register_crud_handlers_module!(router, domain_crud, "/domains");
        // 注册自定义路由
        router.route("/domains/tree", post(handler::get_tree))
    }
}
```

## 9. 接口设计

### 9.1 标准 CRUD 接口

| 方法   | 路径                     | 说明   | 请求体                                  |
| ---- | ---------------------- | ---- | ----------------------------------- |
| POST | `/domains/create`      | 创建单个 | `{ "name": "xxx", ... }`            |
| POST | `/domains/create-many` | 批量创建 | `[{ ... }, { ... }]`                |
| GET  | `/domains/get?id=xxx`  | 获取单条 | -                                   |
| POST | `/domains/update`      | 更新单个 | `{ "id": "xxx", "data": { ... } }`  |
| POST | `/domains/update-many` | 批量更新 | `[{ "id": "xxx", "data": { ... } }]`|
| POST | `/domains/delete`      | 删除   | `{ "ids": ["xxx", "yyy"] }`         |
| POST | `/domains/list`        | 列表查询 | `{ "filter": { ... } }`             |
| POST | `/domains/page`        | 分页查询 | `{ "filter": { ... }, "current": 1, "size": 20 }` |

### 9.2 响应格式

**成功响应**:
```json
{
    "code": 0,
    "msg": "success",
    "data": { ... }
}
```

**分页响应**:
```json
{
    "code": 0,
    "msg": "success",
    "data": [ ... ],
    "pagination": {
        "page": 1,
        "pageSize": 20,
        "total": 100,
        "totalPages": 5
    }
}
```

**业务错误响应**（HTTP 200）:
```json
{
    "code": 1,
    "msg": "参数错误"
}
```

**系统错误响应**（HTTP 500）:
```json
{
    "code": 500,
    "msg": "内部错误"
}
```

## 10. 最佳实践

### 10.1 分层架构

```
┌─────────────────────────────────────┐
│           Handler 层                 │  ← 处理 HTTP 请求/响应
│   (handlers/*/handler.rs)            │
├─────────────────────────────────────┤
│           Service 层                 │  ← 业务逻辑
│   (handlers/*/service.rs)            │
├─────────────────────────────────────┤
│           Model 层                   │  ← 数据模型
│   (handlers/*/entity.rs, bmc.rs)     │
├─────────────────────────────────────┤
│           cmx-database               │  ← 通用 CRUD 框架
│   (GenericCrudService)               │
└─────────────────────────────────────┘
```

### 10.2 命名约定

| 组件      | 命名             | 示例                            |
| ------- | -------------- | ----------------------------- |
| 实体      | 名词             | `Domain`                      |
| 创建 DTO  | 实体 + ForCreate | `DomainForCreate`             |
| 更新 DTO  | 实体 + ForUpdate | `DomainForUpdate`             |
| DbBmc   | 实体 + Bmc       | `DomainBmc`                   |
| Filter  | 实体 + Filter    | `DomainFilter`                |
| Service | 实体 + Service   | `DomainService`               |
| Handler | 动作/操作          | `get_tree`, `search`          |
| Module  | 实体 + Module    | `DomainModule`                |

### 10.3 错误处理

```rust
use crate::error::{Error, Result};

pub async fn custom_method() -> Result<()> {
    // 参数验证错误（业务错误，HTTP 200，json code 1）
    if invalid_input {
        return Err(Error::business_error("参数错误"));
    }
    
    // 内部错误（HTTP 500）
    database_operation()
        .map_err(|e| Error::internal_error(format!("操作失败: {}", e)))?;
    
    Ok(())
}
```

## 11. 总结

| 扩展方式                    | 适用场景       | 示例                           |
| ----------------------- | ---------- | ---------------------------- |
| 直接使用 GenericCrudService | 标准 CRUD 操作 | `create`, `update`, `delete` |
| Service 扩展方法            | 添加业务逻辑     | `search`, `get_tree`         |
| 完全自定义 SQL               | 复杂查询       | `get_tree`                   |
| 自定义 Handler             | 自定义接口      | `/domains/tree`              |
