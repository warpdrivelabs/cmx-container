---
name: axum Handler生成
description: 在生成axum rest接口handler的时候要求遵循的规范
---

# cmx-container Handler 开发规范

> 本文档用于指导 AI 在 cmx-container 项目中生成 Handler 代码时遵循的规范。
> 请在生成任何 `cmx-api/src/handlers/` 下的 handler 代码前，仔细阅读并严格遵守。

---

## 一、核心架构：参数类型与文档类型的双轨制

cmx-container 采用**双层结构体**模式处理请求参数：

| 层级 | 模块 | 用途 | 能否用 utoipa 宏 |
|------|------|------|------------------|
| **运行时参数** | `cmx-core::PageParams<F>` 等 | axum handler 实际接收的参数类型 | ❌ 不能（cmx-core 没有 utoipa 依赖） |
| **文档类型** | `cmx-api::rest::param_doc::PageParamsDoc<F>` 等 | 仅用于 `#[utoipa::path]` 宏的 `request_body` 注解 | ✅ 可以（实现了 `ToSchema`） |

### 1.1 为什么需要两套结构体？

`cmx-core` 是核心库，不依赖 `utoipa`（OpenAPI 文档框架），因此 `PageParams<F>` 等泛型结构体无法派生 `ToSchema`。
而 utoipa 宏（`#[utoipa::path]`）在编译期需要 `ToSchema` 来生成 OpenAPI spec，所以需要一个镜像的文档类型。

### 1.2 双轨类型对照表

| 操作 | 运行时类型（handler 参数） | 文档类型（utoipa 宏） |
|------|--------------------------|----------------------|
| 查询单条 | `cmx_core::GetParams` | `crate::rest::param_doc::GetParamsDoc` |
| 创建 | `Json<E>`（自定义实体） | `E`（需自身有 `ToSchema`） |
| 更新 | `Json<cmx_core::UpdatePayload<E>>` | `crate::rest::param_doc::UpdatePayloadDoc<E>` |
| 删除 | `Json<cmx_core::DeletePayload>` | `crate::rest::param_doc::DeletePayloadDoc` |
| 列表查询 | `Json<cmx_core::ListParams<F>>` | `crate::rest::param_doc::ListParamsDoc<F>` |
| 分页查询 | `Json<cmx_core::PageParams<F>>` | `crate::rest::param_doc::PageParamsDoc<F>` |

> **关键规则**：handler 函数签名使用**运行时类型**，`#[utoipa::path]` 宏的 `request_body` 使用**文档类型**。两者结构一致但不是同一类型。

---

## 二、HTTP 方法规范

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

> **重要**：除 `get_by_id` 使用 **GET** 请求外，其他所有操作均使用post请求以及 **application/json** 请求体。

---

## 三、Handler 代码模板

### 3.1 分页查询 Handler（自定义业务逻辑）

当业务逻辑不能使用通用 `crate::rest::handler::page` 时，需要自定义 handler：

```rust
use axum::extract::{State, Query};
use axum::Json;
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::PageParamsDoc;

/// 分页查询 Handler（函数级注释描述业务功能）
#[utoipa::path(
    post,
    path = "/xxx/page",
    request_body = PageParamsDoc<super::request::ApiXxxFilter>,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<XxxResponse>>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<super::request::ApiXxxFilter>>,
) -> Result<Json<ApiResp<Vec<XxxResponse>>>> {
    let page = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let skip = params.get_offset() as usize;

    let filter: DomainFilter = params.filter.unwrap_or_default().into();

    // 业务逻辑：获取全部数据后内存分页
    let all_items = some_service.list(&filter).await?;
    let total = all_items.len() as u64;

    let paginated: Vec<XxxResponse> = all_items
        .into_iter()
        .skip(skip)
        .take(page_size as usize)
        .map(convert_to_response)
        .collect();

    Ok(Json(ApiResp::ok_with_pagination(paginated, page, page_size, total)))
}
```

### 3.2 使用通用 CRUD Handler

当实体可以通过 `modql` + `GenericCrudService` 直接操作时，可直接使用 `crate::rest::handler` 中的通用函数：

```rust
use crate::rest::handler;

pub fn routes() -> Router<CmxAppState> {
    Router::new()
        .route("/xxx", post(handler::create::<XxxBmc, XxxEntity>))
        .route("/xxx/list", post(handler::list::<XxxBmc, XxxFilter>))
        .route("/xxx/page", post(handler::page::<XxxBmc, XxxFilter>))
        .route("/xxx", get(handler::get_by_id::<XxxBmc>))
        .route("/xxx", post(handler::update::<XxxBmc, XxxEntity>))
        .route("/xxx", post(handler::delete::<XxxBmc>))
}
```

### 3.3 自定义创建/更新/删除 Handler

```rust
/// 创建 Handler
#[utoipa::path(
    post,
    path = "/xxx",
    request_body = XxxCreateRequest,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<XxxResponse>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_create(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<XxxCreateRequest>,
) -> Result<Json<ApiResp<XxxResponse>>> {
    // 业务逻辑
    let result = some_service.create(req).await?;
    Ok(Json(ApiResp::ok(result)))
}

/// 更新 Handler
#[utoipa::path(
    post,
    path = "/xxx",
    request_body = crate::rest::param_doc::UpdatePayloadDoc<XxxUpdateRequest>,
    responses(
        (status = 200, description = "更新成功", body = ApiResp<XxxResponse>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_update(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::UpdatePayload<XxxUpdateRequest>>,
) -> Result<Json<ApiResp<XxxResponse>>> {
    // payload.id 是主键，payload.data 是更新数据
    let result = some_service.update(payload.id, payload.data).await?;
    Ok(Json(ApiResp::ok(result)))
}

/// 删除 Handler
#[utoipa::path(
    delete,
    path = "/xxx",
    request_body = crate::rest::param_doc::DeletePayloadDoc,
    responses(
        (status = 200, description = "删除成功", body = ApiResp<DataSet>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_delete(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(payload): Json<cmx_core::DeletePayload>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    // payload.ids 是主键列表
    some_service.delete(payload.ids).await?;
    Ok(Json(ApiResp::ok_no_data()))
}
```

---

## 四、request.rs 过滤条件结构体规范

### 4.1 API 层过滤结构体（必须带 `ToSchema`）

API 层需要定义自己的过滤结构体（与 domain 层解耦），必须派生 `ToSchema`：

```rust
use utoipa::ToSchema;

/// API 层过滤条件
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct ApiXxxFilter {
    pub status: Option<String>,
    pub name: Option<String>,
    // ... 其他过滤字段
}

/// 实现 From 转换到 domain 层过滤类型
impl From<ApiXxxFilter> for domain_module::XxxFilter {
    fn from(api: ApiXxxFilter) -> Self {
        Self {
            status: api.status.and_then(|s| s.parse().ok()),
            name: api.name,
            // ... 其他字段映射
        }
    }
}
```

### 4.2 注意事项

- API 层过滤字段使用 `Option<String>`，枚举字段在 `From` 转换时 parse
- `#[serde(rename_all = "camelCase")]` 如果需要前端驼峰命名
- domain 层的枚举类型需要实现 `std::str::FromStr` 以支持字符串解析

---

## 五、分页响应规范

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

## 六、路由注册规范

在 `mod.rs` 中注册路由时，注意 HTTP 方法与操作的对应关系：

```rust
use axum::routing::{get, post};

pub fn routes() -> Router<CmxAppState> {
    Router::new()
        .route("/xxx", post(xxx_create))        // 创建 → POST
        .route("/xxx/list", post(xxx_list))      // 列表 → POST (json body)
        .route("/xxx/page", post(xxx_page))      // 分页 → POST (json body)
        .route("/xxx", get(xxx_get_by_id))       // 查询单条 → GET
        .route("/xxx", post(xxx_update))         // 更新 → POST
        .route("/xxx", post(xxx_delete))         // 删除 → POST
}
```

---

## 七、import 规范

```rust
// 标准导入
use axum::extract::{State, Query};
use axum::Json;
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;

// utoipa 文档类型导入
use crate::rest::PageParamsDoc;

// cmx-core 运行时参数（通过 cmx_core:: 直接使用）
// PageParams, GetParams, ListParams, UpdatePayload, DeletePayload
// 无需额外 import，cmx-core 通过 pub use model::data::request::params::* 导出

// 请求/响应类型
use super::request::*;
use super::response::*;
```

---

## 八、关键源文件参考

| 文件 | 说明 |
|------|------|
| `cmx-core/src/model/data/request/params.rs` | 运行时参数定义（GetParams, PageParams, ListParams, UpdatePayload, DeletePayload） |
| `cmx-api/src/rest/param_doc.rs` | utoipa 文档类型定义（GetParamsDoc, PageParamsDoc, ListParamsDoc, UpdatePayloadDoc, DeletePayloadDoc） |
| `cmx-api/src/rest/handler.rs` | 通用 CRUD Handler 函数 |
| `cmx-api/src/api_response.rs` | 统一响应结构 ApiResp 和 Pagination |
| `cmx-api/src/rest/mod.rs` | REST 模块导出 |