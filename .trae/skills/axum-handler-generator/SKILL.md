---
name: axum-handler-generator
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

> **重要**：除 `get_by_id` 使用 **GET** 请求外，其他所有操作均使用 POST 请求以及 **application/json** 请求体。

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
        .route("/xxx/create", post(handler::create::<XxxBmc, XxxEntity>))
        .route("/xxx/list", post(handler::list::<XxxBmc, XxxFilter>))
        .route("/xxx/page", post(handler::page::<XxxBmc, XxxFilter>))
        .route("/xxx/get", get(handler::get_by_id::<XxxBmc>))
        .route("/xxx/update", post(handler::update::<XxxBmc, XxxEntity>))
        .route("/delete", post(handler::delete::<XxxBmc>))
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

## 四、request.rs 请求结构体规范

### 4.1 API 层请求结构体（必须带 `ToSchema`）

API 层需要定义自己的请求结构体（与 domain 层解耦），必须派生 `ToSchema`：

```rust
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// 插件安装请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginInstallRequest {
    /// 插件来源
    pub source: PluginSourceRequest,
    /// 目标数据库ID
    pub target_db_id: Option<String>,
}

/// 插件来源请求（使用 serde tag 枚举）
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginSourceRequest {
    /// 本地路径
    Local {
        /// 本地文件路径
        path: String,
    },
    /// 远程 URL
    Remote {
        /// 远程 URL
        url: String,
        /// 校验和
        checksum: Option<String>,
    },
    /// 注册表
    Registry {
        /// 注册表 URL
        registry_url: String,
        /// 包名
        package_name: String,
    },
}

/// 列表查询参数（使用 IntoParams 生成路径参数）
#[derive(Debug, Deserialize, IntoParams)]
pub struct XxxListQuery {
    /// 状态过滤
    pub status: Option<String>,
}
```

### 4.2 过滤条件结构体（必须带 `ToSchema`）

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

### 4.3 注意事项

- API 层请求/过滤字段使用 `Option<String>`，枚举字段在 `From` 转换时 parse
- `#[serde(rename_all = "camelCase")]` 如果需要前端驼峰命名
- domain 层的枚举类型需要实现 `std::str::FromStr` 以支持字符串解析
- 使用 `#[serde(tag = "type")]` 实现带类型的枚举请求

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

### 6.1 路由路径命名规范

**每个操作必须使用独立的路径**，严禁不同操作共享同一路径仅靠 HTTP 方法区分。

```rust
use axum::routing::{get, post};

pub fn routes() -> Router<CmxAppState> {
    Router::new()
        .route("/xxx/create", post(xxx_create))       // 创建 → POST
        .route("/xxx/list", post(xxx_list))            // 列表 → POST (json body)
        .route("/xxx/page", post(xxx_page))            // 分页 → POST (json body)
        .route("/xxx/get", get(xxx_get_by_id))         // 查询单条 → GET
        .route("/xxx/update", post(xxx_update))        // 更新 → POST
        .route("/xxx/delete", post(xxx_delete))        // 删除 → POST
}
```

> **重要**：路径必须语义明确，如 `/plugin/get`、`/plugin/publish`、`/plugin/update`。
> 禁止 `/plugin` 同时用于 GET(详情) 和 POST(发布)，这会导致路由冲突和语义混乱。

### 6.2 使用通用 CRUD Handler 的路由

当实体可以通过 `modql` + `GenericCrudService` 直接操作时：

```rust
use crate::rest::handler;

pub fn routes() -> Router<CmxAppState> {
    Router::new()
        .route("/xxx/create", post(handler::create::<XxxBmc, XxxForCreate>))
        .route("/xxx/list", post(handler::list::<XxxBmc, XxxFilter>))
        .route("/xxx/page", post(handler::page::<XxxBmc, XxxFilter>))
        .route("/xxx/get", get(handler::get_by_id::<XxxBmc>))
        .route("/xxx/update", post(handler::update::<XxxBmc, XxxForUpdate>))
        .route("/xxx/delete", post(handler::delete::<XxxBmc>))
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
| `cmx-api/src/handlers/plugin/` | 实际 Handler 代码参考（plugin handler 实现） |
| `cmx-api/src/handlers/plugin/request.rs` | 实际请求结构体参考（带 ToSchema 的 request 定义） |
| `cmx-api/src/handlers/service/handler.rs` | 服务调用 Handler 参考（带详细文档注释） |

---

## 九、Handler 文件组织结构

每个 handler 模块应包含以下文件：

```
handlers/
  ├── xxx/
  │   ├── mod.rs       # 模块导出和路由注册
  │   ├── handler.rs   # Handler 函数实现
  │   ├── request.rs   # 请求结构体定义（派生 ToSchema）
  │   └── response.rs  # 响应结构体定义
  └── mod.rs           # 所有 handler 模块的汇总导出
```

### mod.rs 示例

```rust
//! XXX 模块
//!
//! 提供 XXX 相关的 HTTP API

pub mod handler;
pub mod request;
pub mod response;

pub use handler::*;
pub use request::*;
pub use response::*;

use crate::app_state::CmxAppState;
use axum::Router;

pub fn routes() -> Router<CmxAppState> {
    Router::new()
        .route("/xxx/create", post(xxx_create))
        .route("/xxx/page", post(xxx_page))
        .route("/xxx/get", get(xxx_get_by_id))
}
```

---

## 十、modql + GenericCrudService 使用规范

> 当实体不涉及多表 JOIN 操作时，**必须**使用 `modql` + `GenericCrudService` 实现 CRUD，
> 禁止手写 SQL。只有涉及多表 JOIN 或复杂聚合查询时才允许自定义 SQL。

### 10.1 实体结构体（Entity）

使用 `#[derive(Fields)]` 让 modql 自动生成字段元数据。每种操作定义独立的结构体：

```rust
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

### 10.2 过滤器结构体（Filter）

使用 `#[derive(FilterNodes)]` 让 modql 自动实现 `IntoFilterNodes` trait：

```rust
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

### 10.3 Bmc 结构体（表映射）

实现 `DbBmc` trait 告诉 GenericCrudService 表名和主键列：

```rust
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

### 10.4 Service 层使用 GenericCrudService

```rust
use cmx_database::crud::GenericCrudService;
use cmx_database::get_default_db_manager;

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

### 10.5 Service 方法参数规范

**严禁**将多个参数平铺到函数签名中。**必须**使用结构体传递：

```rust
// ❌ 错误：参数平铺，臃肿且难以维护
pub async fn publish_plugin(
    &self,
    plugin_id: String,
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    // ... 20+ 个参数
) -> PluginResult<MarketplacePlugin>

// ✅ 正确：使用结构体参数
pub async fn publish_plugin(
    &self,
    req: PublishPluginRequest,
) -> PluginResult<MarketplacePlugin>
```

### 10.6 何时使用 GenericCrudService vs 自定义 SQL

| 场景 | 推荐方式 |
|------|---------|
| 单表 CRUD（增删改查） | `GenericCrudService` |
| 单表分页/列表查询 | `GenericCrudService::page/list` + `FilterNodes` |
| 多表 JOIN 查询 | `CustomQueryService::page_custom` + `FilterNodes` |
| INSERT ... ON CONFLICT（UPSERT） | 自定义 SQL |
| 聚合统计（GROUP BY / SUM / AVG） | 自定义 SQL |
| 跨表事务操作 | 自定义 SQL + 事务 |

### 10.7 新增实体的完整步骤

1. **定义 Entity**：创建 `ForCreate` / `ForUpdate` 结构体，derive `Fields`
2. **定义 Filter**：derive `FilterNodes`，使用 `OpValsString`/`OpValsInt64` 等类型
3. **定义 Bmc**：实现 `DbBmc` trait，指定 `TABLE`、`PK_COLUMN`
4. **注册路由**：使用通用 `handler::*` 函数 + 泛型参数
5. **（可选）自定义 Service**：包装 `GenericCrudService` 并添加自定义业务逻辑
