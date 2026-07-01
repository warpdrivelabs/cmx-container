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
业务类型（Entity / BMC / Filter / Service）由**业务 crate 提供**，cmx-api 仅做引用与适配。
**跨 crate 共享的数据结构**（与 WASM、RPC 等其他模块共用）放在 `cmx-core`，详见 1.4 节。

### 1.1 cmx-api 只保留

| 内容 | 说明 |
|------|------|
| Handler 薄层 | 接收 HTTP 请求 → 调用业务 Service → 返回 HTTP 响应 |
| 路由注册 | 通过 `ModuleRoutes` trait + 宏系统注册路由 |
| Request/Response DTO（**仅本模块使用**） | API 层专用请求/响应结构体（带 `ToSchema`） |
| **跨 crate 共享 DTO（与 WASM/RPC 共用）** | **抽到 `cmx-core`，handler 直接 import** |
| 中间件 | 认证、CORS、限流等 HTTP 中间件 |
| OpenAPI 文档 | utoipa 注解和文档生成 |

### 1.2 cmx-api 禁止包含

| 禁止内容 | 正确位置（业务 crate 内部） |
|---------|---------------------------|
| Entity（实体结构体） | `<业务 crate>/src/{module}/entity.rs` |
| BMC（表映射） | `<业务 crate>/src/{module}/bmc.rs` |
| Filter（过滤器） | `<业务 crate>/src/{module}/filter.rs` |
| Service（业务逻辑） | `<业务 crate>/src/{module}/service.rs` |
| modql 定义（Fields/FilterNodes） | `<业务 crate>/src/{module}/` |

> **说明**：本项目内的「业务 crate」包括但不限于：
> `cmx-biz`（基础业务数据：domain/application/datasource/...）、
> `cmx-iam`（身份与权限：user/role/permission/...）、
> `cmx-auth`（认证服务）、`cmx-plugin`（插件管理）、
> `cmx-registry-config`（注册中心）等。
> **任何**业务类型都应归属到对应的业务 crate，而非 cmx-api。

### 1.3 从业务 crate 引用业务类型

cmx-api 的 handler 模块通过 **re-export 或直接 import** 引用业务 crate 的类型：

```rust
// 方式一：cmx-api handler 通过业务 crate 直接引用
use <业务 crate>::xxx::{XxxEntity, XxxBmc, XxxFilter, XxxForCreate, XxxForUpdate, XxxService};
// 例如：
use cmx_biz::domain::{Domain, DomainBmc, DomainFilter, ...};
use cmx_iam::user::{User, UserForCreate, UserService};
```

> **规则**：业务类型由业务 crate 负责定义与实现；cmx-api 只做引用，不在 cmx-api 内部重新定义。

### 1.4 跨 crate 共享结构体 → 抽到 `cmx-core`

> **版本变更（2026-06）**：cmx-core 现在可以使用 utoipa 依赖。

**判定流程**：
```
某个 DTO/Request/Response 需要使用吗？
  ├─ 仅 cmx-api handler 内部使用 ──> 放在 cmx-api/<handler>/request.rs 或 response.rs
  ├─ cmx-api + 其他业务 crate（如 cmx-iam）+ WASM 插件 ──> 抽到 cmx-core
  ├─ cmx-api + gRPC/RPC client ──> 抽到 cmx-core
  └─ 仅在 cmx-core 内部用于 axum handler 参数（PageParams/ListParams 等）
                                          ──> 留在 cmx-core
```

**典型共享结构体**（应放在 cmx-core）：
- 分页响应包装：`ApiPage<T>`、`Pagination`
- 通用查询参数：`PageParams<F>`、`ListParams<F>`、`GetParams`
- 通用结果包装：`ApiResp<T>`（如果多 crate 共用）
- 跨模块枚举：`Status`、`ResultCode` 等
- WASM 函数参数/返回值类型

> **设计原则**：`cmx-core` 作为"零业务基础层"，承载所有与具体业务无关、可被任意 crate 复用的数据结构。
> 只要某个类型被 ≥2 个不同 crate（含 WASM）依赖，就应上移到 `cmx-core`。

---

## 二、核心架构：cmx-core 参数类型直接携带 utoipa

> **版本变更（2026-06）**：cmx-core 已开放 utoipa 依赖。`cmx-core::PageParams<F>` / `ListParams<F>` 等通用参数类型**直接**实现 `ToSchema`，
> 不再需要 `cmx-api::rest::param_doc::*` 作为「文档类型」。

### 2.1 新架构：单一类型同时承担运行时 + 文档职责

| 层级 | 类型来源 | 用途 | 能否用 utoipa 宏 |
|------|---------|------|------------------|
| **参数类型（唯一层）** | `cmx_core::PageParams<F>` 等 | axum handler 实际接收 + utoipa `request_body` 注解 | **是**（cmx-core 已含 utoipa） |

> **关键规则**：
> - handler 函数签名使用 `cmx_core::PageParams<F>` / `ListParams<F>` 等
> - `#[utoipa::path]` 宏的 `request_body` **直接**使用同一个类型（无需 `*Doc` 包装）

### 2.2 类型与 OpenAPI 注解对照表

| 操作 | handler 参数类型 | utoipa `request_body` 注解 |
|------|------------------|--------------------------|
| 查询单条 | `Query<cmx_core::GetParams>` | （query 参数，无需 body） |
| 创建 | `Json<E>` | `E`（需自身有 `ToSchema`） |
| 批量创建 | `Json<Vec<E>>` | `Vec<E>`（需 `E` 有 `ToSchema`） |
| 更新 | `Json<cmx_core::UpdatePayload<E>>` | `cmx_core::UpdatePayload<E>` |
| 批量更新 | `Json<Vec<cmx_core::UpdatePayload<E>>>` | `Vec<cmx_core::UpdatePayload<E>>` |
| 删除 | `Json<cmx_core::DeletePayload>` | `cmx_core::DeletePayload` |
| 列表查询 | `Json<cmx_core::ListParams<F>>` | `cmx_core::ListParams<F>` |
| 分页查询 | `Json<cmx_core::PageParams<F>>` | `cmx_core::PageParams<F>` |

> **F 泛型**：是业务 crate 的 Filter 类型（`XxxFilter`），需要在业务 crate 中**派生 `ToSchema`** 或由 `cmx-core` 间接派生。
> 当 F 没派生 `ToSchema` 时，**回退方案**见 2.4 节。

### 2.3 完整 handler 示例

```rust
use cmx_core::{ListParams, PageParams};
use cmx_biz::domain::DomainFilter;  // 业务 crate 的 Filter

/// 列表查询
#[utoipa::path(
    post,
    path = "/api/domain/list",
    request_body = cmx_core::ListParams<serde_json::Value>,  // ✅ 文档用 serde_json::Value
    responses((status = 200, description = "查询成功", body = ApiResp<DataSet>)),
    tag = "Domain"
)]
pub async fn list_domains(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<DomainFilter>>,  // ✅ 函数签名用具体 Filter
) -> Result<Json<ApiResp<DataSet>>> {
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());
    // ... 继续业务逻辑
}
```

### 2.4 ⚠️ 强制规范：列表/分页查询的 `request_body` 必须使用 `serde_json::Value`

> **⚠️ 极其重要**：由于 modql 的 Filter 类型（`FilterNodes`）**不支持 `ToSchema`**，所有列表和分页查询接口的 **utoipa 注解中的 `request_body`** 必须使用 `serde_json::Value` 作为泛型参数，**函数签名可以使用具体 Filter 类型**！

#### 核心要求

1. **utoipa 注解的 `request_body`**：必须使用 `cmx_core::ListParams<serde_json::Value>` 或 `cmx_core::PageParams<serde_json::Value>`
2. **函数签名的参数类型**：可以使用具体 Filter 类型，如 `Json<cmx_core::ListParams<XxxFilter>>`

#### ✅ 正确写法（完整示例）

```rust
// ✅ 列表查询 - 完全符合规范
#[utoipa::path(
    post,
    path = "/api/xxx/list",
    request_body = cmx_core::ListParams<serde_json::Value>,  // ✅ 文档用 serde_json::Value
    responses((status = 200, description = "查询成功")),
    tag = "Xxx"
)]
pub async fn list_xxx(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<XxxFilter>>,  // ✅ 函数签名用具体 Filter
) -> Result<Json<ApiResp<DataSet>>> {
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());
    // ... 继续业务逻辑
}

// ✅ 分页查询 - 完全符合规范
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = cmx_core::PageParams<serde_json::Value>,  // ✅ 文档用 serde_json::Value
    responses((status = 200, description = "查询成功")),
    tag = "Xxx"
)]
pub async fn page_xxx(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<XxxFilter>>,  // ✅ 函数签名用具体 Filter
) -> Result<Json<ApiResp<Vec<XxxEntity>>>> {
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());
    // ... 继续业务逻辑
}
```

#### ❌ 错误写法（严格禁止）

```rust
// ❌ 错误：request_body 使用了具体 Filter 类型
#[utoipa::path(
    post,
    path = "/api/xxx/list",
    request_body = cmx_core::ListParams<XxxFilter>,  // ❌ 绝对禁止！Filter 不支持 ToSchema
    // ...
)]
pub async fn list_xxx(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(params): Json<cmx_core::ListParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    // ...
}
```

#### 原因说明

1. **modql 限制**：modql 的 `FilterNodes` 类型未实现 `ToSchema` trait
2. **OpenAPI 文档生成**：utoipa 生成 OpenAPI 文档时要求 `request_body` 的类型必须实现 `ToSchema`
3. **运行时反序列化**：函数签名使用具体 Filter 类型在运行时反序列化没有问题
4. **解决方案**：`request_body` 用 `serde_json::Value` 生成文档，函数签名用具体 Filter 处理业务逻辑

#### 迁移检查清单

- [ ] 所有列表查询接口的 `request_body` 使用 `ListParams<serde_json::Value>`
- [ ] 所有分页查询接口的 `request_body` 使用 `PageParams<serde_json::Value>`
- [ ] 函数签名可以使用具体 Filter 类型

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

### 4.1 自定义 Handler（调用业务 crate Service）

当业务逻辑不能使用通用 CRUD 宏时，在 handler 中调用业务 crate 的 Service：

```rust
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_database::get_default_db_manager;
use tracing::debug;

// 业务 crate 类型（视实际归属的 crate 而定）
use <业务 crate>::xxx::{XxxService, XxxFilter};
// 例如：
// use cmx_biz::domain::{DomainService, DomainFilter};
// use cmx_iam::user::{UserService, UserFilter};
// use cmx_plugin::install::{PluginInstallService};

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

    // 调用业务 crate 的 Service
    let tree = XxxService::get_tree(mm, &db_id).await?;

    Ok(Json(ApiResp::ok(tree)))
}
```

### 4.2 列表查询 Handler（**最佳实践：filters + list_options 直传**）

```rust
use cmx_core::ListParams;

/// 列表查询 Handler
#[utoipa::path(
    post,
    path = "/api/xxx/list",
    request_body = ListParams<super::request::ApiXxxFilter>,  // ✅ 直接用 cmx-core 类型
    responses(
        (status = 200, description = "查询成功", body = ApiResp<DataSet>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<ListParams<super::request::ApiXxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::xxx_list", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 1. 从 ListParams 提取 list_options（含分页/排序）
    let list_options = params.to_list_options();
    // 2. 提取 filters；为空数组视为 None，便于 Service 走「无过滤」分支
    let filters = params.filters.clone().filter(|v| !v.is_empty());
    // 3. 若需要 API 层 DTO → 业务 crate Filter 转换：
    //    let biz_filters = filters
    //        .map(|v| v.into_iter().map(Into::into).collect());
    //    此处 handler 不做转换，由 Service 内部 From 实现

    let dataset = XxxService::list(mm, &db_id, filters, Some(list_options)).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
```

**关键约定**：
- `ListParams` 实际类型是 `ListParams<F>`，反序列化后 `params.filters: Option<Vec<F>>` 是**已转换好的业务 crate 类型**
- `params.to_list_options()` 直接得到 `modql::filter::ListOptions`，含 `limit/offset/order_bys`
- `filters.filter(|v| !v.is_empty())` 把 `Some(vec![])` 规范化为 `None`，让 Service 实现更简洁
- **`request_body` 直接写 `ListParams<F>`**，不需要 `*Doc` 包装（cmx-core 已含 utoipa）

### 4.3 分页查询 Handler（**最佳实践：filters + list_options 直传**）

```rust
use cmx_core::PageParams;

/// 分页查询 Handler（单表）
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = PageParams<super::request::ApiXxxFilter>,  // ✅ 直接用 cmx-core 类型
    responses(
        (status = 200, description = "查询成功", body = ApiResp<DataSet>)
    ),
    tag = "Xxx"
)]
pub async fn xxx_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<super::request::ApiXxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::xxx_page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 提取分页元信息用于响应
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    // 提取 list_options（order_bys / limit / offset）
    let list_options = params.to_list_options();
    // 提取 filters
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = XxxService::page(mm, &db_id, filters, list_options).await?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset, page_number, page_size, total as u64,
    )))
}
```

### 4.4 多表 JOIN 自定义分页 Handler（`CustomQueryService::page_custom`）

> 参考 `cmx-api/src/handlers/application/handler.rs` —— 这是**最贴近真实业务的最佳实践**：
> Service 接收 `filters` + `list_options`，handler 不重复组装。

```rust
use cmx_core::PageParams;
use cmx_database::crud::CustomQueryService;

/// Application 自定义分页查询（关联 cmx_application + cmx_domain）
#[utoipa::path(
    post,
    path = "/api/applications/custom-page",
    request_body = PageParams<ApplicationFilter>,  // ✅ 直接用 cmx-core 类型
    responses((status = 200, description = "查询成功")),
    tag = "Application"
)]
pub async fn application_custom_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<ApplicationFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::application_custom_page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 关键三步（缺一不可）：
    let list_options = params.to_list_options();
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    // 自定义 SQL（多表 JOIN 时使用，Filter 字段需在 #[modql(rel = "a")] 指定表别名）
    let sql = r#"
        SELECT a.*, d.name as domain_name
        FROM cmx_application a
        LEFT JOIN cmx_domain d ON a.domain_code = d.code
    "#;

    let (dataset, total) = CustomQueryService::page_custom(
        mm, &db_id, None, filters, list_options, sql, "cmx-application",
    )
    .await
    .map_err(|e| crate::Error::InternalError(format!("自定义分页查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset, page_number, page_size, total as u64,
    )))
}
```

### 4.5 默认过滤条件注入模式（多租户/应用隔离场景）

> 当某个 handler **必须**带额外过滤条件（如 `app_id`），而前端可能不传，
> 应在 handler 注入默认值；**不要**在 Service 内部硬编码租户条件，
> 保持 Service 的可复用性。

```rust
let mut filters = params.filters.clone().filter(|v| !v.is_empty());
let app_id = cmx_state.app_id();

// filters 为空时构造默认 filter；非空时给所有 filter 追加 app_id
if let Some(filters_vec) = filters.as_mut() {
    for filter in filters_vec.iter_mut() {
        filter.app_id.get_or_insert(OpValsString::from(app_id.clone()));
    }
} else {
    let default_filter = XxxFilter {
        app_id: Some(OpValsString::from(app_id)),
        ..Default::default()
    };
    filters = Some(vec![default_filter]);
}
```

### 4.6 调用其他业务 crate 的 Handler

对于没有遵循「静态 Service」或「注入式 Service」模式的业务 crate（如 `cmx-plugin` 提供全局单例），
直接调用对应 crate 的全局单例：

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
    crate::handlers::domain::Domain,          // 实体类型（从业务 crate re-export）
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

当需要自定义分页查询时，API 层定义过滤条件结构体并实现到业务 crate Filter 的转换：

```rust
use utoipa::ToSchema;

/// API 层过滤条件
#[derive(Debug, Deserialize, ToSchema, Default, Clone)]
pub struct ApiXxxFilter {
    pub status: Option<String>,
    pub name: Option<String>,
}

/// 实现 From 转换到业务 crate 层过滤类型
impl From<ApiXxxFilter> for <业务 crate>::xxx::XxxFilter {
    fn from(api: ApiXxxFilter) -> Self {
        Self {
            status: api.status.and_then(|s| s.parse().ok()),
            name: api.name,
        }
    }
}
```

> 例如 `impl From<ApiXxxFilter> for cmx_biz::domain::DomainFilter` 或 `for cmx_iam::user::UserFilter`。

### 6.3 注意事项

- API 层请求/过滤字段使用 `Option<String>`，枚举字段在 `From` 转换时 parse
- `#[serde(rename_all = "camelCase")]` 如果需要前端驼峰命名
- 业务 crate 层的枚举类型需要实现 `std::str::FromStr` 以支持字符串解析
- 使用 `#[serde(tag = "type")]` 实现带类型的枚举请求

### 6.4 跨 crate 共享结构体 → 抽到 `cmx-core`

> **核心原则**：当一个 DTO/Request/Response 同时被 cmx-api + 业务 crate（cmx-iam / cmx-plugin 等）+
> WASM 插件 / gRPC client 任意**两类以上**模块使用时，应抽到 `cmx-core` 统一管理。

#### 6.4.1 何时抽到 cmx-core（决策矩阵）

| 使用范围 | 归属 | 示例 |
|---------|------|------|
| 仅 cmx-api handler 内部 | `cmx-api/<handler>/request.rs` | 安装请求、复杂业务查询 DTO |
| 仅业务 crate 内部 | `<业务 crate>/src/<module>/` | 业务 ForCreate/ForUpdate |
| cmx-api + 业务 crate + WASM | **`cmx-core/src/dto/`** | 通用分页响应、状态枚举 |
| cmx-api + 业务 crate + gRPC | **`cmx-core/src/dto/`** | 跨服务调用结果包装 |
| 纯 axum 参数类型 | `cmx-core/src/params.rs` | `PageParams<F>` / `ListParams<F>` / `GetParams` |

#### 6.4.2 cmx-core 中 DTO 的标准写法

```rust
// crates/libs/cmx-core/src/dto/page_response.rs
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 通用分页响应包装（与 WASM、RPC、API 端共用）
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiPage<T> {
    pub data: Vec<T>,
    pub pagination: Pagination,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pagination {
    pub page: u64,
    pub size: u64,
    pub total: i64,
    pub total_pages: i64,
}
```

#### 6.4.3 cmx-api handler 引用 cmx-core 共享 DTO

```rust
// crates/libs/cmx-api/src/handlers/<module>/handler.rs
use cmx_core::dto::page_response::{ApiPage, Pagination};

#[utoipa::path(
    post,
    path = "/api/<module>/page",
    request_body = cmx_core::PageParams<<业务 crate>::<Module>Filter>,
    responses(
        (status = 200, description = "分页查询", body = ApiPage<<业务 crate>::<Module>>)
    ),
    tag = "<Module>"
)]
pub async fn page_<module>(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<<业务 crate>::<Module>Filter>>,
) -> Result<Json<ApiPage<<业务 crate>::<Module>>>> {
    // ...
    Ok(Json(ApiPage { data, pagination: Pagination { /* ... */ } }))
}
```

#### 6.4.4 cmx-core 共享类型的"瘦身"原则

> **警告**：`cmx-core` 的依赖会被所有上层 crate（含 WASM）传递性引入，**必须保持轻量**。

| 允许的依赖 | 禁止的依赖 |
|-----------|----------|
| `serde` / `serde_json` | `sea-query` / `modql`（业务查询相关） |
| `utoipa` / `utoipa-axum`（已开放） | `cmx-database`（数据库相关） |
| `chrono` / `time` | 任何业务 crate（如 cmx-iam / cmx-biz） |
| `uuid` | `axum`（应让 cmx-api 依赖） |
| `thiserror` | 任何重量级二进制依赖 |

#### 6.4.5 实战：把现有 request.rs 抽到 cmx-core 的步骤

> 当发现某个 request.rs 结构体被 cmx-api handler 之外的其他模块（如 WASM 插件、cmx-iam Service）也需要时：

1. **在 cmx-core 新建模块**：`crates/libs/cmx-core/src/dto/<module>.rs`
2. **移动结构体**：从 `cmx-api/.../request.rs` 剪切到 cmx-core
3. **加 `ToSchema` 派生**（如果还没有）
4. **更新 cmx-core `lib.rs`**：`pub mod dto;`
5. **更新引用方**：
   - cmx-api handler: `use cmx_core::dto::<module>::XxxRequest;`
   - WASM 插件: `use cmx_core::dto::<module>::XxxRequest;`
   - 业务 crate Service: `use cmx_core::dto::<module>::XxxRequest;`
6. **删除 cmx-api 中的原文件**
7. **检查依赖方向**：`cmx-core` 不能反向依赖任何业务 crate 或 cmx-api

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

### 9.1 标准模块结构（Entity 在业务 crate 中）

当 Entity/BMC/Filter/Service 已在业务 crate（如 cmx-biz、cmx-iam 等）中定义时，cmx-api 中只有薄层：

```
cmx-api/src/handlers/xxx/
  ├── mod.rs       # 模块导出（re-export 业务 crate 类型）+ ModuleRoutes 实现
  └── handler.rs   # 仅自定义 Handler 函数（标准 CRUD 由宏生成）
```

**mod.rs 示例**：

```rust
//! Xxx 模块
//!
//! 提供 Xxx 相关的 HTTP API
//! Entity/BMC/Filter/Service 已在业务 crate 中定义

pub mod handler;

// 从业务 crate 引用类型（视实际归属 crate 而定）
// 静态 Service 模式（cmx-biz 等）使用 re-export 供宏系统使用
pub use <业务 crate>::xxx::{
    XxxEntity, XxxBmc, XxxFilter, XxxForCreate, XxxForUpdate, XxxService,
};
// 例如：
// pub use cmx_biz::domain::{Domain, DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, DomainService};

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

> **注入式 Service 模式**（如 cmx-iam）：无 re-export、无宏系统调用，handler 直接 `use` 业务 crate 类型，
> 通过 `cmx_state.<业务>()` 获取 Service。详见第十三章。

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

> 以下是**通用流程**，适用于任何业务 crate。具体 crate 选择参考 1.2 节。

1. **在业务 crate 中定义业务类型**：Entity/BMC/Filter/Service（参见第十章）
2. **在 cmx-api 中创建 handler 模块**：`handlers/xxx/mod.rs`
3. **引用业务 crate 类型**：
   - 静态 Service 模式：`pub use <业务 crate>::xxx::*;`
   - 注入式 Service 模式：handler 内 `use <业务 crate>::xxx::*;`
4. **注册宏（仅静态 Service 模式）**：在 `routes/crud_handlers.rs` 中调用 `declare_crud_handlers!`
5. **实现 ModuleRoutes**：在 mod.rs 中注册路由
6. **注册到总路由**：在 `routes/routes_impl.rs` 中添加模块

### 9.4 含 `cmx-core` 共享 DTO 的模块结构

> 适用于 handler 既要引用业务 crate 的 Entity/Filter，又要引用 cmx-core 共享 DTO（如 `ApiPage<T>`、通用枚举）的场景。

```
cmx-api/src/handlers/xxx/
  ├── mod.rs              # 模块导出 + ModuleRoutes 实现
  ├── handler.rs          # Handler 函数实现
  ├── request.rs          # 仅本模块使用的 API 层 DTO（若需要）
  └── response.rs         # 仅本模块使用的 API 层 Response DTO（若需要）
```

> **cmx-core 共享 DTO 不放在 cmx-api 内**。在 cmx-core 中定义后，handler 用 `use cmx_core::dto::*;` 引用。

### 9.5 新增 cmx-core 共享 DTO 的步骤

1. **在 cmx-core 新建文件**：`crates/libs/cmx-core/src/dto/<module>.rs`
2. **派生必要 trait**：`#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]`
3. **注册模块**：`crates/libs/cmx-core/src/dto/mod.rs` 中 `pub mod <module>;`
4. **在 cmx-core lib.rs 暴露**：`pub mod dto;`
5. **在所有使用方引用**：
   - cmx-api handler: `use cmx_core::dto::<module>::XxxDto;`
   - WASM 插件: `use cmx_core::dto::<module>::XxxDto;`
   - 业务 crate Service: `use cmx_core::dto::<module>::XxxDto;`
6. **运行 cargo check 确认依赖方向正确**（cmx-core 不能依赖 cmx-api 或业务 crate）

---

## 十、业务层类型定义规范（Entity/BMC/Filter/Service）

> 本章节是**通用规范**，适用于**任何业务 crate**（cmx-biz、cmx-iam、cmx-auth、cmx-plugin、cmx-registry-config 等）。
> 当 cmx-api 的 handler 需要操作数据库实体时，**必须**先在对应的业务 crate 中定义好以下内容，
> 然后在 cmx-api 中引用。

### 10.1 业务 crate 目录结构

每个业务实体在业务 crate 中独立一个模块：

```
<业务 crate>/src/
  ├── xxx/
  │   ├── mod.rs       # 模块导出
  │   ├── entity.rs    # Entity / ForCreate / ForUpdate（derive Fields）
  │   ├── bmc.rs       # Bmc 结构体（impl DbBmc）
  │   ├── filter.rs    # Filter 结构体（derive FilterNodes）
  │   └── service.rs   # Service 层（包装 GenericCrudService 或自定义 SQL）
  └── lib.rs
```

例如 cmx-biz 的实际目录：
```
cmx-biz/src/
  ├── domain/{entity,bmc,filter,service}.rs
  ├── application/{entity,bmc,filter,service}.rs
  ├── datasource/{entity,bmc,filter,service}.rs
  └── ...
```

### 10.2 实体结构体（Entity）

使用 `#[derive(Fields)]` 让 modql 自动生成字段元数据。每种操作定义独立的结构体：

```rust
// <业务 crate>/src/xxx/entity.rs
use modql::field::Fields;
use serde::{Deserialize, Serialize};
// ToSchema 可选：仅当业务 crate 需要给 cmx-api 提供 OpenAPI 类型时才加
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
- `ToSchema` 是**可选**的：若该结构体直接作为 cmx-api 的 OpenAPI 输出类型则需要；
  若 cmx-api 走「API 层 DTO 转换」模式，业务 crate 可不依赖 utoipa

### 10.3 过滤器结构体（Filter）

使用 `#[derive(FilterNodes)]` 让 modql 自动实现 `IntoFilterNodes` trait：

```rust
// <业务 crate>/src/xxx/filter.rs
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
// <业务 crate>/src/xxx/bmc.rs
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

> **核心原则**：`list` / `page` 方法**必须**接收 `filters: Option<Vec<F>>` 和 `list_options: ListOptions` 两个参数，
> 由 handler 端从 `ListParams` / `PageParams` 透传过来。
> 详见 **第十四章「Service 列表/分页查询接口规范（最佳实践）」**。

> **重要**：Service 写法按集成方式分为**两种模式**：
>
> | 模式 | 调用方式 | 典型 crate | Service 定义 |
> |------|---------|-----------|------------|
> | 静态 Service 模式 | `XxxService::create(mm, db_id, ...)` | cmx-biz 等 | `impl XxxService { pub async fn create(...) }` |
> | 注入式 Service 模式 | `iam.user_service.create_user(...)` | cmx-iam 等 | `impl IamUserService for IamUserServiceImpl { async fn create_user(&self, ...) }` |
>
> 两种模式**都遵循**本节的 `list` / `page` 签名约定，区别仅在「是否用 `&self`」和「是否带 mm/db_id 参数」。
> 详见 14.8 / 14.9 对照示例。

```rust
// <业务 crate>/src/xxx/service.rs（静态 Service 模式示例）
use cmx_database::crud::CustomQueryService;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_core::model::data::dataset::DataSet;
use modql::filter::ListOptions;
use sea_query::Value;

use crate::xxx::{XxxBmc, XxxForCreate, XxxForUpdate, XxxFilter};
use crate::error::Result;

pub struct XxxService;

impl XxxService {
    // ====== 单条 CRUD（保持现有规范）======
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

    // ====== 列表查询（最佳实践：filters + list_options）======
    /// 列表查询
    ///
    /// - `filters`：多组过滤器，组与组之间是 OR 关系，组内字段是 AND 关系
    /// - `list_options`：分页与排序（None 表示使用默认 limit=20）
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<XxxBmc, XxxFilter>::list(
            mm, db_id, None, filters, list_options,
        ).await
    }

    // ====== 分页查询（最佳实践：filters + list_options）======
    /// 分页查询
    ///
    /// 返回 `(DataSet, total)`，`total` 用于前端分页器
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<XxxBmc, XxxFilter>::page(
            mm, db_id, None, filters, list_options,
        ).await
    }

    // ====== 多表 JOIN 自定义分页（最佳实践：filters + list_options + sql）======
    /// 多表 JOIN 自定义分页查询
    ///
    /// - `sql` 必须是 `SELECT ... FROM xxx` 且只引用主表别名（modql 会自动加 WHERE/ORDER BY/LIMIT/OFFSET）
    /// - `XxxFilter` 的字段需要 `#[modql(rel = "主表别名")]` 指定表
    pub async fn page_custom(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        let sql = r#"
            SELECT a.*, d.name as rel_name
            FROM cmx_xxx a
            LEFT JOIN cmx_xxx_rel d ON a.rel_code = d.code
        "#;
        CustomQueryService::page_custom(
            mm, db_id, None, filters, list_options, sql, "cmx-xxx",
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

> 例外：`list` / `page` 接收的 `filters + list_options` 是 modql 提供的**通用结构体参数**，
> 本身就是结构化参数，符合本节要求。

### 10.7 mod.rs 导出

```rust
// <业务 crate>/src/xxx/mod.rs
pub mod entity;
pub mod bmc;
pub mod filter;
pub mod service;

pub use entity::{XxxEntity, XxxForCreate, XxxForUpdate};
pub use bmc::XxxBmc;
pub use filter::XxxFilter;
pub use service::XxxService;
```

并在业务 crate 的 `lib.rs` 中导出模块：

```rust
// <业务 crate>/src/lib.rs
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

// utoipa 文档类型导入（**新代码不再需要**，直接用 cmx_core 类型即可）
// 旧代码可能引用：use crate::rest::PageParamsDoc;
// 新代码统一改为：use cmx_core::PageParams;

// cmx-core 运行时参数 + 共享 DTO（**直接使用，cmx-core 已含 utoipa**）
use cmx_core::{
    PageParams, ListParams, GetParams, UpdatePayload, DeletePayload,
};
// 共享 DTO（按需）
// use cmx_core::dto::page_response::{ApiPage, Pagination};
// use cmx_core::dto::common_status::Status;

// 业务 crate 类型（从对应 crate 引用，cmx-api 不在内部定义）
// 静态 Service 模式：通过业务 crate 直接引用具体类型
use <业务 crate>::xxx::{XxxService, XxxFilter};
// 例如：
// use cmx_biz::domain::{DomainService, DomainFilter};
// use cmx_iam::user::{UserService, UserFilter};
// use cmx_plugin::install::{PluginInstallService};

// 请求/响应类型（**仅本模块专用**的 DTO；共享 DTO 应抽到 cmx-core）
use super::request::*;
use super::response::*;
```

> **注意**：
> 1. 业务类型从**对应业务 crate** 引用，不要写死 `cmx_biz::xxx`。
>    选择哪个业务 crate 取决于该实体的归属（参考 1.2 节业务 crate 列表）。
> 2. 跨 crate 共享的 DTO 抽到 `cmx-core`，**不放在 cmx-api 内**。
>    通过 `use cmx_core::dto::*;` 引用。
> 3. `use cmx_core::PageParams<F>` 既能作为 handler 参数类型，也能作为 `request_body` 注解类型——**统一使用**。

---

## 十二、关键源文件参考

> 以下是**真实参考文件**。skill 内的「静态 Service 模式」对应 cmx-biz 等；
> 「注入式 Service 模式」对应 cmx-iam 等；选择参考时按当前要实现的模式找对应示例。

### 12.1 cmx-api 内部基础设施

| 文件 | 说明 |
|------|------|
| `cmx-api/src/routes/macros.rs` | CRUD handler 生成宏和路由注册宏 |
| `cmx-api/src/routes/crud_handlers.rs` | 各实体的宏调用集中管理（**静态 Service 模式专用**） |
| `cmx-api/src/routes/traits.rs` | ModuleRoutes trait 定义 |
| `cmx-api/src/rest/handler.rs` | 通用 CRUD Handler 函数（宏内部调用） |
| `cmx-api/src/rest/param_doc.rs` | **遗留**：旧 `*Doc` 文档类型，新代码应直接用 `cmx_core::PageParams<F>` 等 |
| `cmx-api/src/api_response.rs` | 统一响应结构 ApiResp 和 Pagination |
| `cmx-api/src/handlers/domain/mod.rs` | **静态 Service 模式**标准模块参考（re-export + ModuleRoutes） |
| `cmx-api/src/handlers/plugin/mod.rs` | 复杂模块参考（嵌套路由 + Request/Response） |

### 12.2 Handler 范例（按模式分类）

| 模式 | 场景 | 参考文件 |
|------|------|---------|
| 静态 Service | 单表 page（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `page_users` |
| 注入式 Service | 单表 page（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `page_users` |
| 静态 Service | 单表 list（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `list_users` |
| 注入式 Service | 单表 list（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `list_users` |
| 静态 Service | 多表 JOIN 自定义分页 | `cmx-api/src/handlers/application/handler.rs` 的 `application_custom_page` |
| 静态 Service | 默认 filter 注入（多租户 app_id） | `cmx-api/src/handlers/table_metadata/handler.rs` 的 `table_metadata_page` |
| 外部 Service | 非业务 crate 提供的 page / list | `cmx-api/src/handlers/marketplace/handler.rs` |
| 注入式 Service | role / role_group / permission | `cmx-api/src/handlers/iam/role/handler.rs`、`role_group/handler.rs`、`permission/handler.rs` |

### 12.3 业务 crate Service 范例

| 业务 crate | 适用模式 | 参考文件 |
|-----------|---------|---------|
| `cmx-biz` | 静态 Service | `cmx-biz/src/datasource/service.rs`、`cmx-biz/src/domain/service.rs` |
| `cmx-biz` | 静态 Service + 多表 JOIN | `cmx-biz/src/application/service.rs`（`CustomQueryService::page_custom`） |
| `cmx-iam` | 注入式 Service | `cmx-iam/src/user/service.rs`（trait + impl） |

### 12.4 cmx-core 共享类型参考

| 文件 | 说明 |
|------|------|
| `crates/libs/cmx-core/src/params.rs` | 通用 axum 参数类型（`PageParams<F>` / `ListParams<F>` / `GetParams` 等） |
| `crates/libs/cmx-core/src/dto/page_response.rs` | **示例**：通用分页响应 `ApiPage<T>` / `Pagination`（**已被多个 crate 共用时应上移到这里**） |
| `crates/libs/cmx-core/src/dto/` | **共享 DTO 目录**：与 WASM、RPC、API 端共用的结构体集中地 |
| `crates/libs/cmx-core/src/dto/common_enums.rs` | **示例**：跨模块枚举（`Status`、`ResultCode` 等） |

---

## 十三、注入式 Service Handler 模式（独立业务 crate）

> 适用于业务逻辑在**独立业务 crate**（如 `cmx-iam`、`cmx-auth`）中实现、
> 通过 `Arc<dyn Trait>` 注入 `CmxAppState` 的场景。
> 第十章描述的「静态 Service 模式」是另一条路径（典型如 cmx-biz），所有业务类型静态调用。
> 本章描述的「注入式 Service 模式」**不使用宏系统**，所有 handler 均手写。
> 两种模式都遵循第十四章的 list / page 最佳实践。

### 13.1 适用场景

- 业务 crate 与 cmx-biz 平行存在（如 `cmx-iam`、`cmx-auth`），**不依赖** cmx-biz
- Service 通过 `Arc<dyn Trait>` 注入 `CmxAppState`，而非静态方法调用
- Entity/Filter 在业务 crate 中定义，cmx-api 通过依赖引用
- 不需要 `request.rs` / `response.rs`（直接使用业务 crate 的类型）

### 13.2 目录结构

```
cmx-api/src/handlers/<业务>/
  ├── mod.rs           # <业务>Module（ModuleRoutes）聚合各子模块
  ├── <子模块1>/
  │   ├── mod.rs       # <子模块1>Module（ModuleRoutes）路由注册
  │   └── handler.rs   # <子模块1> handler 函数
  ├── <子模块2>/
  │   ├── mod.rs
  │   └── handler.rs
  └── ...
```

**关键区别**（与第九章「静态 Service 模式」对比）：
- 无 `request.rs` / `response.rs` — 直接使用业务 crate 的 Entity/Filter 类型
- 无宏系统调用 — 所有路由手写注册
- Entity 从业务 crate 直接导入（非 re-export）

### 13.3 Handler 实现模式

Handler 通过 `cmx_state.<业务>()` 获取 Service 容器，调用对应的 service trait 方法：

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
    path = "/api/<业务>/<子模块>/create",
    request_body = <业务 crate>::<子模块>::<Entity>ForCreate,
    responses((status = 200, description = "创建成功", body = ApiResp<<Entity>>)),
    tag = "<业务>-<子模块>"
)]
pub async fn create_<子模块>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<<业务 crate>::<子模块>::<Entity>ForCreate>,
) -> Result<Json<ApiResp<<业务 crate>::<子模块>::<Entity>>>> {
    let svc = cmx_state.<业务>().ok_or(crate::error::Error::InternalError(
        "<业务> 服务未初始化".to_string(),
    ))?;
    let result = svc.<子模块>_service.create_<子模块>(&svr_ctx, data).await?;
    Ok(Json(ApiResp::ok(result)))
}
```

> 例如 `cmx-iam` 的 `user` 子模块：`<业务>=iam`、`<子模块>=user`、
> `<Entity>=User`、`<Entity>ForCreate=UserForCreate`。

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

pub struct <子模块>Module;

impl ModuleRoutes for <子模块>Module {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .route("/<业务>/<子模块>/create", post(handler::create_<子模块>))
            .route("/<业务>/<子模块>/get", get(handler::get_<子模块>))
            .route("/<业务>/<子模块>/update", post(handler::update_<子模块>))
            .route("/<业务>/<子模块>/delete", post(handler::delete_<子模块>))
            .route("/<业务>/<子模块>/page", post(handler::page_<子模块>))
            .route("/<业务>/<子模块>/list", post(handler::list_<子模块>))
            // 业务自定义路由...
    }
    fn prefix() -> &'static str { "<业务>" }
    fn module_name(&self) -> &'static str { "<业务>-<子模块>" }
}
```

### 13.6 聚合模块

顶层 `<业务>/mod.rs` 聚合子模块路由：

```rust
use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;

pub mod <子模块1>;
pub mod <子模块2>;
pub mod <子模块3>;

pub struct <业务>Module;

impl ModuleRoutes for <业务>Module {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        let router = router.merge(<子模块1>Module.routes());
        let router = router.merge(<子模块2>Module.routes());
        let router = router.merge(<子模块3>Module.routes());
        router
    }
    fn prefix() -> &'static str { "<业务>" }
    fn module_name(&self) -> &'static str { "<业务>" }
}
```

### 13.7 两种 Service 模式对比

| 维度 | 静态 Service 模式（第九、十章） | 注入式 Service 模式（本章） |
|------|----------------------|---------------------|
| Service 调用 | 静态方法 `XxxService::create(mm, db_id, ...)` | trait 对象 `cmx_state.<业务>().<子模块>_service.create_<子模块>(...)` |
| Entity 来源 | 业务 crate re-export 到 cmx-api | 直接 `use <业务 crate>::<子模块>::*;` |
| CRUD 生成 | `declare_crud_handlers!` 宏 | 手写所有 handler |
| db_id 获取 | handler 内 `get_db_id_from_header(&headers)` | Service 内部持有 db_id |
| 状态注入 | 无需 `CmxAppState`（静态方法） | 通过 `cmx_state.<业务>()` 获取 |
| `request.rs` | 可选（API 层 DTO） | 不需要（直接用业务 crate 类型） |
| 典型 crate | cmx-biz 等基础业务数据 | cmx-iam、cmx-auth 等独立业务模块 |
| list / page 最佳实践 | 同样适用（第十四章） | 同样适用（第十四章） |

---

## 十四、Service 列表/分页查询接口规范（**最佳实践**）

> 本章节是**整个技能文档的核心约定**，生成任何 list / page 相关代码时**必须**遵守。

### 14.1 核心原则（一句话总结）

> **Service 的 `list` / `page` 方法必须接收 `filters: Option<Vec<F>>` 和 `list_options: ListOptions` 两个结构化参数，**
> **handler 端只做「提取 + 透传」，不重新组装 page/page_size/order_bys。**
> 这两个参数是 modql 提供的「一等公民」，能同时支撑**条件过滤、排序、分页**三类需求，是项目内的 best practice。

### 14.2 为什么这是最佳实践

| 反模式 | 带来的问题 | 最佳实践 | 优势 |
|--------|----------|---------|------|
| Service 接收 `page: u64, page_size: u64, keyword: &str, ...` 平铺 | 加新条件要改 Service 签名、调用方全部要改 | Service 接收 `filters + list_options` | 加条件只改 Filter 结构体，零侵入 |
| Handler 里 `XxxService::page(mm, db_id, page, size, keyword)` 调用 | handler 越长越像业务层 | Handler `XxxService::page(mm, db_id, filters, list_options)` | handler 维持「薄」层身份 |
| Service 内部硬编码默认过滤（如 `app_id`） | 复用 Service 时要改源码 | handler 注入默认 filter | Service 可被不同上下文复用 |
| `ListOptions` 各处手搓 `{ limit, offset, order_bys }` | 字段名拼写错误百出 | 统一用 `modql::filter::ListOptions` | 与 modql/sea-query 生态对齐 |
| `filters: XxxFilter`（单组） | 无法表达 `(A=1 AND B=2) OR (C=3)` | `filters: Option<Vec<XxxFilter>>`（多组 OR） | 自然支持多组组合查询 |

### 14.3 标准参数签名

| Service 方法 | filters 类型 | list_options 类型 | 返回值 | 适用场景 |
|------------|-------------|------------------|--------|---------|
| `list` | `Option<Vec<XxxFilter>>` | `Option<ListOptions>` | `DataSet` | 全量/限量的列表（不带 total） |
| `page` | `Option<Vec<XxxFilter>>` | `ListOptions` | `(DataSet, i64)` | 标准分页（带 total） |
| `page_custom` | `Option<Vec<XxxFilter>>` | `ListOptions` | `(DataSet, i64)` | 多表 JOIN / 自定义 SQL 分页 |
| `list_custom` | `Option<Vec<XxxFilter>>` | `Option<ListOptions>` | `DataSet` | 多表 JOIN / 自定义 SQL 列表（不带 total） |

> **禁止**出现 `(page, page_size, filter1, filter2, ...)` 这种平铺签名。

### 14.4 Handler 端的标准三步提取模式

任何 list / page handler 都必须包含以下三行（顺序固定）：

```rust
// 1. 提取 ListOptions（含分页/排序）—— from ListParams 或 PageParams
let list_options = params.to_list_options();
// 2. 提取分页元信息（仅 page 时需要，用于 ok_with_pagination 响应）
let page_number = params.get_page() as u64;
let page_size = params.get_size() as u64;
// 3. 提取 filters；空数组 → None，便于 Service 走「无过滤」分支
let filters = params.filters.clone().filter(|v| !v.is_empty());
```

随后**直接透传**给 Service：

```rust
// list
let dataset = XxxService::list(mm, &db_id, filters, Some(list_options)).await?;
Ok(Json(ApiResp::ok(dataset)))

// page
let (dataset, total) = XxxService::page(mm, &db_id, filters, list_options).await?;
Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))

// page_custom（多表 JOIN）
let sql = "SELECT a.*, d.name FROM cmx_xxx a LEFT JOIN cmx_yyy d ON ...";
let (dataset, total) = XxxService::page_custom(mm, &db_id, filters, list_options).await?;
Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
```

### 14.5 ListOptions 的 JSON 格式（前端传参约定）

前端在 `ListParams` / `PageParams` 的 JSON body 中通过以下字段控制分页和排序：

```json
{
  "filters": [
    {
      "name":    { "$contains": "财务" },
      "status":  { "$eq": 1 }
    },
    {
      "type":   { "$eq": "platform" }
    }
  ],
  "page": 1,
  "size": 20,
  "order_bys": "!create_time,code"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `filters` | `Vec<Filter>` 或 `null` | 多组过滤器，**组与组之间是 OR**，**组内字段是 AND** |
| `page` | `u64` | 页码（从 1 开始），仅 PageParams 使用 |
| `size` | `u64` | 每页条数 |
| `order_bys` | `String` | 排序字段，逗号分隔，前缀 `!` 表示降序；例：`!create_time,code` |

### 14.6 API 层 DTO → 业务 crate Filter 转换的两种方式

**方式一：handler 内做转换**（适合「每个 handler 独立过滤逻辑」）

```rust
Json(params): Json<cmx_core::ListParams<ApiXxxFilter>>,
// ...
let biz_filters: Option<Vec<XxxFilter>> = params.filters
    .map(|v| v.into_iter().map(XxxFilter::from).collect())
    .filter(|v: &Vec<XxxFilter>| !v.is_empty());
let dataset = XxxService::list(mm, &db_id, biz_filters, Some(list_options)).await?;
```

**方式二：cmx-core 自动反序列化**（推荐）

> `cmx_core::ListParams<F>` 的泛型 `F` 就是业务 crate 的 Filter 类型，框架已经处理反序列化；
> handler **不需要**额外 `From` 转换，直接 `params.filters: Option<Vec<XxxFilter>>` 即可用。
> 只有当需要「API 层独立 DTO」时，才用方式一。

### 14.7 真实代码参考

| 模式 | 场景 | 参考文件 |
|------|------|---------|
| 静态 Service | 单表 page（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `page_users` |
| 注入式 Service | 单表 page（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `page_users` |
| 静态 Service | 单表 list（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `list_users` |
| 注入式 Service | 单表 list（最佳实践标准） | `cmx-api/src/handlers/iam/user/handler.rs` 的 `list_users` |
| 静态 Service | 多表 JOIN page_custom | `cmx-api/src/handlers/application/handler.rs` 的 `application_custom_page` |
| 静态 Service | 默认 filter 注入（多租户） | `cmx-api/src/handlers/table_metadata/handler.rs` 的 `table_metadata_page` |
| 静态 Service | Service 层 page / list 标准实现 | `cmx-biz/src/datasource/service.rs`、`cmx-biz/src/domain/service.rs` |
| 静态 Service | Service 层 page_custom 实现 | `cmx-biz/src/application/service.rs`（`CustomQueryService::page_custom`） |
| 注入式 Service | Service trait page / list 实现 | `cmx-iam/src/user/service.rs` |

### 14.8 完整对照示例（**静态 Service 模式**，典型如 cmx-biz）

**业务 crate Service 层**：
```rust
impl <业务 crate>::xxx::XxxService {
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<XxxBmc, XxxFilter>::page(
            mm, db_id, None, filters, list_options,
        ).await.map_err(Into::into)
    }

    pub async fn page_custom(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        let sql = r#"
            SELECT a.*, d.name as rel_name
            FROM cmx_xxx a
            LEFT JOIN cmx_xxx_rel d ON a.rel_code = d.code
        "#;
        CustomQueryService::page_custom(
            mm, db_id, None, filters, list_options, sql, "cmx-xxx",
        ).await.map_err(Into::into)
    }
}
```

**cmx-api Handler 层**：
```rust
pub async fn xxx_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<cmx_core::PageParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = XxxService::page(mm, &db_id, filters, list_options).await?;
    Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
}
```

### 14.9 完整对照示例（**注入式 Service 模式**，典型如 cmx-iam）

```rust
// 业务 crate Service trait + impl
#[async_trait]
impl <业务 crate>::<子模块>::<子模块>Service for <子模块>ServiceImpl {
    async fn page_<子模块>(
        &self,
        filters: Option<Vec<<子模块>Filter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<<子模块>Bmc, <子模块>Filter>::page(
            &self.mm, &self.db_id, None, filters, list_options,
        ).await
    }

    async fn list_<子模块>(
        &self,
        filters: Option<Vec<<子模块>Filter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<<子模块>Bmc, <子模块>Filter>::list(
            &self.mm, &self.db_id, None, filters, list_options,
        ).await
    }
}

// cmx-api handler
pub async fn page_<子模块>(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<<子模块>Filter>>,
) -> Result<Json<ApiResp<Vec<<子模块>>>>> {
    let svc = cmx_state.<业务>().ok_or_else(|| Error::business_error("<业务> 服务未初始化".into()))?;
    let current = params.get_page() as u64;
    let size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (items, total) = svc.<子模块>_service.page_<子模块>(filters, list_options).await?;
    Ok(Json(ApiResp::ok_with_pagination(items, current, size, total as u64)))
}
```

### 14.10 检查清单

生成或审查 list / page 相关代码时，按以下清单逐条核对：

- [ ] Service 方法是否使用 `(filters: Option<Vec<F>>, list_options: ListOptions)` 签名？
- [ ] Service 方法是否避免了 `(page, page_size, keyword, ...)` 平铺签名？
- [ ] Handler 是否只做「提取 + 透传」，没有重新组装分页/排序？
- [ ] `filters.filter(|v| !v.is_empty())` 是否用于把空数组规范化为 `None`？
- [ ] 多表 JOIN 时 Filter 字段是否带 `#[modql(rel = "表别名")]`？
- [ ] 多租户/应用隔离场景是否在 handler 注入默认 filter（不放在 Service）？
- [ ] `page` 返回值是否包含 `total` 并通过 `ApiResp::ok_with_pagination` 返回？
- [ ] 排序字段是否通过 `order_bys`（前端 JSON）传入，而非 Service 内硬编码？

---

## 十五、与 modql 技能的协同（**必读**）

> 本章节是 axum-handler-generator 与 modql 两个技能的**桥梁**。
> **强烈建议**：在编写任何涉及「查询过滤 / 排序 / 分页」的代码前，**先调 modql 技能获取基础支持**，再回到本技能完成 handler / Service 集成。

### 15.1 为什么需要协同 modql

axum-handler-generator 关注 **HTTP 协议层**（axum handler / utoipa 注解 / 路由注册）。
modql 关注 **查询语义层**（Filter 结构定义 / 操作符 / sea-query 集成 / ListOptions）。

list / page 路径是两者**唯一强耦合**的地方：
- 业务 Filter 的字段类型由 modql 提供（`OpValsString` / `OpValsInt64` 等）
- 业务 Filter 的 `#[derive(FilterNodes)]` 由 modql 宏实现
- 业务 Entity 的 `#[derive(Fields)]` 由 modql 宏实现
- handler 的 `to_list_options()` 返回的 `ListOptions` 由 modql 定义
- 多表 JOIN 的表别名由 modql 的 `#[modql(rel = "x")]` 控制

**没有 modql，axum-handler-generator 的 list / page 模式无法落地**。

### 15.2 触发场景：什么时候必须先调 modql

遇到以下任意**一个**场景时，**必须**先 `Use Skill: modql`：

| 场景 | 关键词 | 调 modql 解决的问题 |
|------|--------|------------------|
| 设计一个**新业务实体的 Filter 结构体** | "新增 filter" "查询条件" "搜索" | OpValsXxx 字段类型选择、操作符支持、FilterNodes 宏使用 |
| 设计 Entity / ForCreate / ForUpdate | "新增实体" "表结构" "增改 DTO" | Fields 宏、HasSeaFields、sea-query 列映射 |
| 编写**多表 JOIN** 的 page_custom | "JOIN" "关联查询" "分页 SQL" | `#[modql(rel = "a")]` 表别名、Filter 字段指向哪张表 |
| 解析前端的 `order_bys` 字段 | "排序" "order by" "字段排序" | `ListOptions` 解析、`!field` 降序语法、多字段排序 |
| 解析前端的 `filters` JSON | "前端筛选" "条件过滤" "动态 where" | FilterGroups、AND/OR 语义、嵌套条件 |
| 实现自定义 sea-query 转换 | "特殊操作符" "自定义 SQL 条件" | `to_sea_condition_fn` / `to_sea_value_fn` |
| 编写复杂的统计查询 | "聚合" "GROUP BY" "SUM" | 与 sea-query 集成的最佳实践 |
| 写好 list / page handler 之后发现**不知道 ListOptions 怎么用** | "分页参数" "ListOptions" | `to_list_options()` 调用、order_bys 解析 |
| 审查别人写的 Filter 结构体 | "代码审查" "Filter 设计" | 是否正确使用 OpValsXxx、是否缺 ToSchema |

**反例：什么时候**不要**调 modql**：
- 仅写 create / get / update / delete handler（不涉及条件查询）
- 纯静态数据的展示（无 filter / 无分页）
- 已经写好且经过验证的 Filter / Entity 不需要重新查 modql

### 15.3 衔接点：modql 在 axum-handler 工作流中的位置

```
[Step 1: 调 modql] 设计业务 Filter
    │
    ├─ modql 输出：XxxFilter 结构体（#[derive(FilterNodes, Deserialize, Default)]）
    │              字段类型 OpValsString / OpValsInt64 / OpValsBool
    │              （可选）#[modql(rel = "a")] 表别名
    │
    ▼
[Step 2: 调 axum-handler] 写 Service 签名
    │
    ├─ axum-handler 输出：Service::page(mm, db_id, filters: Option<Vec<XxxFilter>>, list_options: ListOptions)
    │
    ▼
[Step 3: 调 axum-handler] 写 Handler 三步提取
    │
    ├─ axum-handler 输出：list_options = params.to_list_options()  ← modql API
    │                    filters = params.filters.clone().filter(|v| !v.is_empty())
    │
    ▼
[Step 4: 调 modql（可选）] 自定义转换
    │
    └─ 复杂场景：使用 to_sea_condition_fn 处理特殊字段
```

### 15.4 5 个核心协同点（按代码出现顺序）

| 协同点 | 出现位置 | 调 modql 的目的 | 关键 API |
|--------|---------|---------------|---------|
| **1. Entity 定义** | cmx-biz/<业务>/entity.rs | 派生 `Fields` 宏，让 GenericCrudService 自动构建 INSERT/UPDATE | `#[derive(Fields)]` |
| **2. Filter 定义** | cmx-biz/<业务>/filter.rs | 派生 `FilterNodes` 宏，让 Service 支持动态 WHERE | `#[derive(FilterNodes)]` + `OpValsXxx` |
| **3. 多表 JOIN 表别名** | Filter 字段 | 告诉 modql 字段属于哪张表 | `#[modql(rel = "a")]` |
| **4. ListOptions 提取** | handler 内 | 把 PageParams 转成 modql 通用结构 | `params.to_list_options()` |
| **5. ListOptions 构造**（罕见） | 自定义场景 | 直接构造 ListOptions 传给 Service | `ListOptions { limit, offset, order_bys }` |

### 15.5 标准调用流程（端到端示例）

> 场景：新增一个 `XxxEntity`，需要支持条件查询 + 排序 + 分页。

#### Step 1：先调 modql 设计 Filter

```
Use Skill: modql
```

**关键提问**：
- "我想设计一个 XxxFilter，字段包括 status: String, name: String, archived: i32，应该用什么 OpValsXxx 类型？"
- "FilterNodes 宏怎么用？"
- "archived 是 int4 类型，OpValsInt64 包含哪些操作符？"

**modql 输出**：
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

#### Step 2：回 axum-handler-generator 写 Service 签名

**遵循本技能第十四章**：
```rust
impl XxxService {
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<XxxBmc, XxxFilter>::page(
            mm, db_id, None, filters, list_options,
        ).await
    }
}
```

#### Step 3：写 Handler（modql 关键 API：`to_list_options`）

```rust
use cmx_core::PageParams;
use <业务 crate>::xxx::{XxxFilter, XxxService};

#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = PageParams<XxxFilter>,
    responses((status = 200, description = "分页查询", body = ApiResp<DataSet>)),
    tag = "Xxx"
)]
pub async fn xxx_page(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;

    // ✅ modql API：PageParams 提供 to_list_options() 方法
    let list_options = params.to_list_options();
    // ✅ axum-handler 规范：空数组 → None
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = XxxService::page(mm, &db_id, filters, list_options).await?;
    Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
}
```

#### Step 4（多表 JOIN）：再调 modql 获取表别名

> 涉及多表 JOIN 时（Service 用 `CustomQueryService::page_custom`）：
>
> ```
> Use Skill: modql
> ```
>
> 关键提问：
> - "Filter 字段怎么指定表别名？多表 JOIN 时 modql 怎么知道字段属于哪张表？"
> - "to_sea_condition_fn 怎么用？"
>
> modql 输出：在 Filter 字段上加 `#[modql(rel = "a")]`

### 15.6 反例：什么时候**不要**先调 modql

| 场景 | 原因 |
|------|------|
| 仅写 create / update / delete handler | 这些方法**不涉及** Filter / ListOptions，纯 modql 无关 |
| 已有标准 Filter 直接复用 | 直接抄已有 Filter 结构体，无需重新设计 |
| 写非分页的简单查询（如 get_by_id） | 仅用 `cmx_core::GetParams`，不涉及 modql |
| 写 OpenAPI 文档注解 | utoipa 相关，与 modql 无关 |
| 写业务 Service 内部的事务逻辑 | 业务逻辑层，不涉及 query 构造 |

### 15.7 完整协同示例：多表 JOIN

**Step 1：调 modql**（咨询"多表 JOIN 时 Filter 字段怎么写"）：

```rust
// modql 答案：使用 #[modql(rel = "a")] 指定表别名
use modql::filter::{FilterNodes, OpValsString, OpValsInt64};

#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct ApplicationFilter {
    #[modql(rel = "a")]  // a = cmx_application 表
    pub code: Option<OpValsString>,
    #[modql(rel = "a")]
    pub name: Option<OpValsString>,
    #[modql(rel = "a")]
    pub status: Option<OpValsString>,
    #[modql(rel = "d")]  // d = cmx_domain 表（JOIN 的另一张表）
    pub domain_name: Option<OpValsString>,
}
```

**Step 2：回 axum-handler-generator 写 Service（page_custom）**：

```rust
impl ApplicationService {
    pub async fn page_custom(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<ApplicationFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        let sql = r#"
            SELECT a.*, d.name as domain_name
            FROM cmx_application a
            LEFT JOIN cmx_domain d ON a.domain_code = d.code
        "#;
        // modql 自动根据 #[modql(rel = ...)] 给 WHERE 加正确的表别名
        CustomQueryService::page_custom(
            mm, db_id, None, filters, list_options, sql, "cmx-application",
        ).await
    }
}
```

**Step 3：写 Handler**：

```rust
// 与 Step 2 14.4 章节一致，此处省略
```

### 15.8 协同检查清单

在编写 list / page 相关代码前，**按顺序自检**：

- [ ] **Step 1**：是否需要设计新 Filter？若是 → `Use Skill: modql`
- [ ] **Step 2**：Filter 字段类型是否使用 `OpValsXxx`（而非原始 `String` / `i64`）？
- [ ] **Step 3**：多表 JOIN 时 Filter 字段是否带 `#[modql(rel = "表别名")]`？
- [ ] **Step 4**：Service 签名是否使用 `(filters: Option<Vec<F>>, list_options: ListOptions)`？
- [ ] **Step 5**：handler 是否使用 `params.to_list_options()` 而非手搓？
- [ ] **Step 6**：是否用了 `filters.filter(|v| !v.is_empty())` 规范化？

### 15.9 两个技能的关系总结

| 维度 | modql 技能 | axum-handler-generator 技能 |
|------|-----------|----------------------------|
| 关注点 | **查询语义层**：Filter / OpVals / Fields / ListOptions | **HTTP 协议层**：axum handler / utoipa / 路由 |
| 解决的问题 | 怎么定义查询条件、怎么转 SQL | 怎么接收 HTTP 请求、怎么返回响应 |
| 落地代码 | Entity / Filter 结构体定义 | Service 调用 + Handler 路由 |
| 是否需要对方 | 总是被 axum-handler 调用 | 调用 modql 设计 Filter |
| 触发关键词 | "Filter" "OpVals" "Fields" "ListOptions" "分页" | "handler" "axum" "utoipa" "路由" |

---

## 十六、与 cmx-sql-execution 技能的协同（**必读**）

> 本章节是 axum-handler-generator 与 cmx-sql-execution 两个技能的**桥梁**。
> **强烈建议**：在编写任何涉及「手写 SQL 执行 / DataValue 参数构造 / 动态 UPDATE」的 Service 层代码前，**先调 cmx-sql-execution 技能获取参数构造与 API 选择指导**，再回到本技能完成 handler 集成。

### 16.1 为什么需要协同 cmx-sql-execution

axum-handler-generator 关注 **HTTP 协议层**（axum handler / utoipa 注解 / 路由注册）。
cmx-sql-execution 关注 **SQL 执行层**（DatabaseManager API 选择 / DataValue 参数构造 / ParamsBuilder / 带类型 NULL / 事务）。

Service 层是两者**唯一强耦合**的地方：
- handler 调用 Service 方法，Service 内部执行 SQL
- Service 的 SQL 参数构造方式（`Vec<DataValue>` / `dv!` 宏 / `From<Option<T>>` / `ParamsBuilder`）由 cmx-sql-execution 规范
- Service 调用的 DatabaseManager 方法（`execute_sql_with_datavalues` / `query_sql_with_datavalues` 等）由 cmx-sql-execution 指导选择
- NULL 类型处理（`NullTyped` vs `Null`）由 cmx-sql-execution 规范

**没有 cmx-sql-execution，axum-handler-generator 的 Service 层 SQL 代码容易写出 NULL 类型丢失、占位符漂移、参数构造冗长等问题**。

### 16.2 触发场景：什么时候必须先调 cmx-sql-execution

遇到以下任意**一个**场景时，**必须**先 `Use Skill: cmx-sql-execution`：

| 场景 | 关键词 | 调 cmx-sql-execution 解决的问题 |
|------|--------|------------------------------|
| 在 Service 层**手写 SQL 并执行** | "execute_sql" "query_sql" "raw sql" | API 选择（datavalues vs json vs typed）、参数构造、结果提取 |
| 构造 `Vec<DataValue>` 参数 | "DataValue" "params" "参数构造" | dv! 宏、From<Option<T>> 糖、NullTyped |
| 构建动态 UPDATE SET 子句 | "动态 UPDATE" "set 子句" "条件更新" | ParamsBuilder 自动管理占位符编号 |
| 处理 NULL 绑定到非字符串列 | "NULL 类型" "NullTyped" "prepare 失败" | SqlTypeMarker + NullTyped 正确绑定 |
| 在事务中执行多条 SQL | "事务" "txn_id" "transaction" | 事务 API、txn_id 传递规范 |
| 从 DataSet 提取查询结果 | "DataSet" "提取结果" "反序列化" | row 遍历、to_json_value、get_by_name_as |
| 编写涉及 Option 字段的 INSERT/UPDATE | "Option" "可空字段" "None" | From<Option<T>> 自动产生 NullTyped(带类型) |
| WASM plugin 传带类型参数 | "data_values" "wasm sql" "DbRequest" | data_values 优先于 params JSON |

**反例：什么时候**不要**调 cmx-sql-execution**：
- 仅写 handler 薄层（接收 HTTP → 调 Service → 返回响应），不涉及 SQL
- 使用 GenericCrudService 标准 CRUD（已封装好 SQL，无需手写）
- 使用 modql + sea-query 构建查询（调 modql 技能即可）
- 编写 DDL / migrations SQL 文件（调 sql-guide 技能）

### 16.3 衔接点：cmx-sql-execution 在 axum-handler 工作流中的位置

```
[Step 1: 调 axum-handler] 设计 handler 签名 + 路由
    │
    ├─ axum-handler 输出：handler 函数 + utoipa 注解 + 路由注册
    │
    ▼
[Step 2: 调 axum-handler] 设计 Service trait 签名
    │
    ├─ axum-handler 输出：XxxService::create/update/delete 方法签名
    │
    ▼
[Step 3: 调 cmx-sql-execution] 实现 Service 内部 SQL 逻辑  ★
    │
    ├─ cmx-sql-execution 输出：
    │   ├─ API 选择：execute_sql_with_datavalues / query_sql_with_datavalues
    │   ├─ 参数构造：dv! 宏 / .into() 糖 / ParamsBuilder
    │   ├─ NULL 处理：From<Option<T>> 自动 NullTyped
    │   └─ 事务模式：txn_id 传递
    │
    ▼
[Step 4: 回 axum-handler] handler 调用 Service
    │
    └─ axum-handler 输出：handler 内 XxxService::create(mm, db_id, data).await
```

### 16.4 核心协同点（按代码出现顺序）

| 协同点 | 出现位置 | 调 cmx-sql-execution 的目的 | 关键 API |
|--------|---------|---------------------------|---------|
| **1. Service 内 SQL 执行** | `<业务 crate>/src/<module>/service.rs` | 选择正确的 DatabaseManager 方法 | `execute_sql_with_datavalues` / `query_sql_with_datavalues` |
| **2. 参数构造** | Service 内 | 用 dv! 宏或 .into() 糖简化 params 构造 | `dv![...]` / `From<Option<T>>` |
| **3. 动态 UPDATE** | Service 内 update 方法 | 用 ParamsBuilder 替换手动占位符管理 | `ParamsBuilder::new(0).set_opt(...).build()` |
| **4. NULL 类型处理** | Service 内 | 确保非字符串列的 NULL 带类型 | `NullTyped(SqlTypeMarker::Int)` |
| **5. 事务内执行** | Service 内多步操作 | 正确传递 txn_id | `txn_id: Some(&txn_id)` |

### 16.5 标准调用流程（端到端示例）

> 场景：为一个新实体 `XxxEntity` 实现 create + update handler，Service 层需要手写 SQL。

#### Step 1：调 axum-handler-generator 设计 handler

```
Use Skill: axum-handler-generator
```

**输出**：handler 函数 + 路由注册
```rust
#[utoipa::path(post, path = "/api/xxx/create", ...)]
pub async fn xxx_create(
    State(state): State<CmxAppState>,
    Json(data): Json<XxxForCreate>,
) -> Result<Json<ApiResp<XxxEntity>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let result = XxxService::create(mm, &db_id, data).await?;
    Ok(Json(ApiResp::ok(result)))
}
```

#### Step 2：调 axum-handler-generator 设计 Service 签名

```rust
impl XxxService {
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: XxxForCreate) -> Result<XxxEntity, TraitError> {
        // 内部实现需要手写 SQL → 进入 Step 3
    }
}
```

#### Step 3：调 cmx-sql-execution 实现 SQL 逻辑 ★

```
Use Skill: cmx-sql-execution
```

**关键提问**：
- "我要实现一个 INSERT，参数含 Option<String> 和 Option<i64>，应该用什么 API？"
- "DataValue 参数怎么构造？Option 字段怎么处理？"
- "如何在事务中执行？"

**cmx-sql-execution 输出**：
```rust
use cmx_core::model::cell::DataValue;

impl XxxService {
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: XxxForCreate) -> Result<XxxEntity, TraitError> {
        let id = cmx_utils::id::snowflake_id_str();
        let sql = "INSERT INTO cmx_xxx (id, name, sort_order, description) VALUES ($1, $2, $3, $4)";

        // ★ cmx-sql-execution 规范：.into() 糖 + execute_sql_with_datavalues
        let params: Vec<DataValue> = vec![
            DataValue::String(id.clone()),
            DataValue::String(data.name),
            data.sort_order.unwrap_or(0).into(),  // 保留 None→0 语义
            data.description.into(),               // Option<String> → Null
        ];

        mm.execute_sql_with_datavalues(db_id, None, sql, params).await?;

        // 查询返回
        let sql = "SELECT * FROM cmx_xxx WHERE id = $1";
        let ds = mm.query_sql_with_datavalues(db_id, None, sql, vec![DataValue::String(id)], "xxx").await?;
        // ... 提取结果
    }
}
```

#### Step 4（动态 UPDATE）：再调 cmx-sql-execution 获取 ParamsBuilder 指导

> 涉及动态 UPDATE 时：
>
> ```
> Use Skill: cmx-sql-execution
> ```
>
> 关键提问：
> - "update 方法有多个 Option 字段，怎么构建动态 SET 子句？"
> - "ParamsBuilder 怎么用？占位符怎么编号？"
>
> cmx-sql-execution 输出：使用 ParamsBuilder::new(0).set_opt(...).build()

### 16.6 反例：什么时候**不要**先调 cmx-sql-execution

| 场景 | 原因 |
|------|------|
| 仅写 handler 薄层（不涉及 SQL） | handler 只做协议转换，SQL 在 Service 层 |
| 使用 GenericCrudService 标准 CRUD | 已封装好 SQL，调 axum-handler-generator 即可 |
| 使用 modql + sea-query 构建查询 | 调 modql 技能，不走 raw SQL 路径 |
| 编写 DDL / migrations SQL 文件 | 调 sql-guide 技能 |
| 写 OpenAPI 文档注解 | utoipa 相关，与 SQL 执行无关 |

### 16.7 协同检查清单

在编写 Service 层 SQL 代码前，**按顺序自检**：

- [ ] **Step 1**：是否需要手写 SQL？若是 GenericCrudService 能覆盖的 → 不调 cmx-sql-execution
- [ ] **Step 2**：是否选择了 `execute_sql_with_datavalues`（而非旧的 `with_json`）？
- [ ] **Step 3**：参数中含 Option<T> 时是否用 `.into()` 糖（而非 `.map().unwrap_or()`）？
- [ ] **Step 4**：整型/时间/Uuid 的 None 是否走 NullTyped（自动，通过 From<Option<T>>）？
- [ ] **Step 5**：动态 UPDATE 是否用 ParamsBuilder（而非手动 idx 管理）？
- [ ] **Step 6**：None→0 vs None→NULL 语义是否逐处核对？
- [ ] **Step 7**：事务内执行是否正确传递 `txn_id: Some(&txn_id)`？

### 16.8 两个技能的关系总结

| 维度 | cmx-sql-execution 技能 | axum-handler-generator 技能 |
|------|----------------------|----------------------------|
| 关注点 | **SQL 执行层**：DatabaseManager API / DataValue / ParamsBuilder / NullTyped | **HTTP 协议层**：axum handler / utoipa / 路由 |
| 解决的问题 | 怎么执行 SQL、怎么构造参数、怎么处理 NULL 类型 | 怎么接收 HTTP 请求、怎么返回响应 |
| 落地代码 | Service 内部 SQL 执行逻辑 | handler 函数 + 路由注册 |
| 是否需要对方 | 被 axum-handler 的 Service 层调用 | 调用 cmx-sql-execution 实现 Service SQL |
| 触发关键词 | "execute_sql" "DataValue" "params" "ParamsBuilder" "NullTyped" | "handler" "axum" "utoipa" "路由" |
