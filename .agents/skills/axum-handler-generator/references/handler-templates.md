# Handler 模板与 HTTP 层规范

> 所有 import 与代码片段抄自 `crates/libs/cmx-apis/` 现有源码，可直接复用。
> 路径均相对于 `crates/libs/cmx-apis/`。

---

## 一、标准 import 清单（真实写法，勿改）

```rust
// axum
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Router;
use axum::routing::{get, post};

// HTTP 骨架（cmx-api-core）
use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::rest::header_parse::get_db_id_from_header;
use cmx_api_core::{ApiResp, Error, Result};

// cmx-core 运行时参数（真源：cmx-core/src/model/data/request/params.rs，经 lib.rs 顶层 re-export）
use cmx_core::{PageParams, ListParams, GetParams, UpdatePayload, DeletePayload};
use cmx_core::model::data::dataset::DataSet;

// 数据库（仅静态 Service 模式的 handler 需要）
use cmx_database::get_default_db_manager;
use cmx_database::crud::CustomQueryService;

// 业务 crate 类型（按实体归属选择，*-api 内禁止重新定义）
use cmx_biz::application::{ApplicationFilter, ApplicationForCreate, ApplicationForUpdate, ApplicationService};
use cmx_iam::user::{UserFilter, UserForCreate, UserForUpdate};

// modql（默认 filter 注入场景）
use modql::filter::OpValsString;

// 本模块专用 DTO
use super::request::*;
use super::response::*;
```

要点：

- `ApiResp` / `Error` / `Result` 定义在 cmx-api-types，但 `cmx_api_core` 已 re-export，
  域 crate 一律 `use cmx_api_core::{ApiResp, Error, Result};`。
- 宏导入：`use cmx_api_core::declare_crud_handlers;`（`#[macro_export]` 到 crate 根）。
- 路由注册宏用路径式调用：`cmx_api_core::register_crud_handlers_module!(router, xxx_crud, "/xxx");`。
- 日志一律 `tracing::debug!`，格式沿用现状：`debug!("{:<12} - handler::xxx_yyy", "HANDLER");`。

---

## 二、utoipa 注解规范（request_body / responses / params）

### 2.1 `request_body` 口径（与 AGENTS.md §七一致）

| 操作 | `request_body` 注解 | 函数签名 |
|------|--------------------|---------|
| create | `XxxForCreate`（具体类型，需派生 `ToSchema`） | `Json<XxxForCreate>` |
| create_many | `Vec<XxxForCreate>` | `Json<Vec<XxxForCreate>>` |
| update | `cmx_core::UpdatePayload<XxxForUpdate>` | `Json<UpdatePayload<XxxForUpdate>>` |
| update_many | `inline(Vec<cmx_core::UpdatePayload<XxxForUpdate>>)` | `Json<Vec<UpdatePayload<XxxForUpdate>>>` |
| delete | `cmx_core::DeletePayload` | `Json<DeletePayload>` |
| get | 无 body；`params(("id" = String, Query, ...))` | `Query<GetParams>` 或自定义 `IntoParams` 结构 |
| list | `cmx_core::ListParams<serde_json::Value>` | `Json<ListParams<XxxFilter>>` |
| page | `cmx_core::PageParams<serde_json::Value>` | `Json<PageParams<XxxFilter>>` |

**铁律**：list / page 的注解必须用 `serde_json::Value`（modql `FilterNodes` 不支持
`ToSchema`），签名必须用具体 Filter；`Value` 禁止扩散到签名。create / update /
delete 用具体类型（ForCreate / ForUpdate 由业务 crate 派生 `ToSchema`）。

### 2.2 注解骨架

```rust
#[utoipa::path(
    post,
    path = "/api/<域>/<模块>/<操作>",          // 以 /api 开头的完整路径
    request_body = <按上表>,
    params(UsernameQuery),                     // GET 用 IntoParams 结构
    responses(
        (status = 200, description = "成功", body = ApiResp<Xxx>),
        (status = 400, description = "参数错误"),
        (status = 404, description = "不存在")
    ),
    tag = "<域>-<模块>"                        // 如 "IAM-User"、"Application"
)]
```

---

## 三、Handler 模板

### 3.1 create（静态 Service 模式，cmx-biz-api 实例）

```rust
#[utoipa::path(
    post,
    path = "/api/applications/create",
    request_body = ApplicationForCreate,
    responses((status = 200, description = "创建成功", body = ApiResp<serde_json::Value>)),
    tag = "Application"
)]
pub async fn create_application(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(data): Json<ApplicationForCreate>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::create_application", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = ApplicationService::create(mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}
```

### 3.2 update / delete（静态模式）

```rust
#[utoipa::path(
    post,
    path = "/api/applications/update",
    request_body = UpdatePayload<ApplicationForUpdate>,
    responses((status = 200, description = "更新成功", body = ApiResp<serde_json::Value>)),
    tag = "Application"
)]
pub async fn update_application(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<UpdatePayload<ApplicationForUpdate>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // ApplicationService::update 的 id 参数类型是 sea_query::Value，payload.id 直接透传
    let dataset = ApplicationService::update(mm, &db_id, payload.id, payload.data).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

#[utoipa::path(
    post,
    path = "/api/applications/delete",
    request_body = DeletePayload,
    responses((status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)),
    tag = "Application"
)]
pub async fn delete_application(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let dataset = ApplicationService::delete(mm, &db_id, payload.ids).await?;
    Ok(Json(ApiResp::ok(dataset)))
}
```

### 3.3 get（GET 查单条）

两种写法（真实代码并存）：

```rust
// 写法一：Query<GetParams>（id 走 query）
#[utoipa::path(
    get,
    path = "/api/table-metadata/get",
    params(("id" = String, Query, description = "表定义ID")),
    responses((status = 200, description = "查询成功")),
    tag = "TableMetadata"
)]
pub async fn table_metadata_get_by_id(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(params): Query<GetParams>,
) -> Result<Json<ApiResp<DataSet>>> { /* ... */ }

// 写法二：自定义 IntoParams（业务主键查询）
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UsernameQuery {
    /// 用户名。
    pub username: String,
}

#[utoipa::path(
    get,
    path = "/api/iam/users/get",
    params(UsernameQuery),
    responses((status = 200, description = "查询成功", body = ApiResp<User>)),
    tag = "IAM-User"
)]
pub async fn get_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<UsernameQuery>,
) -> Result<Json<ApiResp<User>>> { /* ... */ }
```

### 3.4 list（最佳实践：filters + list_options 透传）

```rust
#[utoipa::path(
    post,
    path = "/api/xxx/list",
    request_body = cmx_core::ListParams<serde_json::Value>,
    responses((status = 200, description = "查询成功", body = ApiResp<DataSet>)),
    tag = "Xxx"
)]
pub async fn xxx_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<ListParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::xxx_list", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let list_options = params.to_list_options();                              // 含 limit/offset/order_bys
    let filters = params.filters.clone().filter(|v| !v.is_empty());           // 空数组 → None

    let dataset = XxxService::list(mm, &db_id, filters, Some(list_options)).await?;
    Ok(Json(ApiResp::ok(dataset)))
}
```

### 3.5 page（三步提取 + ok_with_pagination）

```rust
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = cmx_core::PageParams<serde_json::Value>,
    responses((status = 200, description = "查询成功", body = ApiResp<DataSet>)),
    tag = "Xxx"
)]
pub async fn xxx_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::xxx_page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let list_options = params.to_list_options();
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = XxxService::page(mm, &db_id, filters, list_options).await?;
    Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
}
```

### 3.6 多表 JOIN 自定义分页（CustomQueryService::page_custom）

> 真实参考：`cmx-biz-api/src/handlers/application/handler.rs` 的 `application_custom_page`。

```rust
use cmx_database::crud::CustomQueryService;

#[utoipa::path(
    post,
    path = "/api/applications/custom-page",
    request_body = cmx_core::PageParams<serde_json::Value>,
    responses((status = 200, description = "查询成功")),
    tag = "Application"
)]
pub async fn application_custom_page(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<PageParams<ApplicationFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 自定义 SQL：只写 SELECT ... FROM ... JOIN，modql 自动追加 WHERE/ORDER BY/LIMIT/OFFSET
    let sql = r#"
        SELECT a.*, d.name as domain_name
        FROM cmx_application a
        LEFT JOIN cmx_domain d ON a.domain_code = d.code
    "#;

    let list_options = params.to_list_options();
    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (dataset, total) = CustomQueryService::page_custom(
        mm, &db_id, None, filters, list_options, sql, "cmx-application",
    )
    .await
    .map_err(|e| Error::InternalError(format!("自定义分页查询失败: {}", e)))?;

    Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
}
```

`page_custom` 参数依次为：`mm, db_id, txn_id(None), filters, list_options, sql, 表名标识`。
JOIN 时 Filter 字段需在业务 crate 加 `#[modql(rel = "a")]` 表别名（详见 modql 技能）。

### 3.7 默认过滤注入（多租户 app_id）

> 真实参考：`cmx-plugin-api/src/handlers/table_metadata/handler.rs`。
> handler 注入默认值，**不在 Service 硬编码**，保持 Service 可复用。

```rust
use modql::filter::OpValsString;

let mut filters = params.filters.clone().filter(|v| !v.is_empty());
let app_id = cmx_state.app_id();

match filters.as_mut() {
    // filters 非空：给每个 filter 补默认 app_id（未设置才补）
    Some(filters_vec) => {
        for filter in filters_vec.iter_mut() {
            filter
                .app_id
                .get_or_insert(OpValsString::from(app_id.clone()));
        }
    }
    // filters 为空：构造只含 app_id 的默认 filter
    None => {
        filters = Some(vec![XxxFilter {
            app_id: Some(OpValsString::from(app_id)),
            ..Default::default()
        }]);
    }
}
```

### 3.8 注入式 Service 模式（cmx-iam-api 实例）

> handler 不拿 mm / db_id；通过 `cmx_state.<业务>()` 取服务容器，Service 内部持库。

```rust
use cmx_core::model::iam::User;
use cmx_iam::user::{UserFilter, UserForCreate, UserForUpdate};

#[utoipa::path(
    post,
    path = "/api/iam/users/page",
    request_body = cmx_core::PageParams<serde_json::Value>,
    responses((status = 200, description = "查询成功", body = ApiResp<Vec<User>>)),
    tag = "IAM-User"
)]
pub async fn page_users(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(params): Json<cmx_core::PageParams<UserFilter>>,
) -> Result<Json<ApiResp<Vec<User>>>> {
    debug!("{:<12} - handler::page_users", "HANDLER");

    // cmx_state.iam() 返回 Option<&Arc<IamState>>；未初始化返回业务错误
    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let page_number = params.get_page() as u64;
    let page_size = params.get_size() as u64;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());

    let (users, total) = iam
        .user_service
        .page_users(filters, list_options)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok_with_pagination(users, page_number, page_size, total as u64)))
}
```

类型转换要点（注入式常见）：

| 场景 | 写法 |
|------|------|
| `payload.id: sea_query::Value` → String | `payload.id.as_str().ok_or_else(\|\| Error::business_error("无效的ID".into()))?.to_string()` |
| `payload.ids: Vec<Value>` → Vec<String> | `payload.ids.iter().filter_map(\|v\| v.as_str().map(\|s\| s.to_string())).collect()` |

### 3.9 外部服务 / 全局单例（cmx-plugin-api marketplace 实例）

```rust
use super::request::*;
use super::response::*;
use cmx_plugin::marketplace::model::{MarketplacePluginFilter, MarketplacePluginForCreate};

// 服务不在 CmxAppState 注入时，从库管理器现场构造
async fn get_marketplace_service() -> cmx_plugin::MarketplaceService {
    let db_manager = get_default_db_manager();
    let default_db_id = db_manager.get_default_db_id().await;
    let repo = Arc::new(cmx_plugin::MarketplaceRepository::new(
        db_manager.clone(),
        default_db_id.clone(),
    ));
    let stats_service = Arc::new(cmx_plugin::StatsService::new(repo.clone()));
    cmx_plugin::MarketplaceService::new(repo, stats_service, db_manager.clone(), default_db_id)
}

#[utoipa::path(
    post,
    path = "/api/marketplace/publish",
    request_body = PublishRequest,          // 本模块 request.rs 定义的 API 层 DTO
    responses((status = 200, description = "发布成功", body = ApiResp<PublishResponse>)),
    tag = "Marketplace"
)]
pub async fn publish(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Json(req): Json<PublishRequest>,
) -> Result<Json<ApiResp<PublishResponse>>> {
    let service = get_marketplace_service().await;
    let result = service.publish(req.into()).await
        .map_err(|e| Error::business_error(e.to_string()))?;
    Ok(Json(ApiResp::ok(result.into())))
}
```

---

## 四、CRUD 宏系统

### 4.1 宏 vs 手写决策

| 条件 | 用宏（`declare_crud_handlers!`） | 手写 handler |
|------|--------------------------------|-------------|
| 标准单表 CRUD，纯 GenericCrudService，无副作用 | ✅ | |
| 写操作需触发业务副作用（DAM 资产目录搬移、树形字段 code_path/id_path 维护、删除前引用校验） | | ✅ 委托 Service |
| Entity 的 ForCreate / ForUpdate 不派生 `Fields`（如 cmx-iam User） | | ✅ 委托 Service |
| 自定义查询（JOIN / 聚合 / 特殊过滤） | | ✅ |

> 真实例证（`cmx-biz-api/src/crud_handlers.rs` 注释）：domain / application / menu /
> module 的写操作已改手写（DAM 副作用、树形字段），sys_datasource / form 仍走宏。

### 4.2 声明宏（cmx-biz-api/src/crud_handlers.rs 集中调用）

```rust
use cmx_api_core::declare_crud_handlers;

// 模式二：无鉴权（8 参数）
declare_crud_handlers!(
    form_crud,                                              // 生成的模块名
    crate::handlers::form::Form,                            // 实体（业务 crate re-export 到本模块）
    crate::handlers::form::FormBmc,                         // BMC
    crate::handlers::form::FormForCreate,                   // 创建 DTO（需 ToSchema）
    crate::handlers::form::FormForUpdate,                   // 更新 DTO（需 ToSchema）
    crate::handlers::form::FormFilter,                      // 过滤器
    "Form",                                                 // OpenAPI tag
    "/form"                                                 // 路由前缀
);

// 模式一：统一权限资源名（第 9 参数）——自动拼接 :create/:read/:update/:delete 后缀
declare_crud_handlers!(
    user_crud, User, UserBmc, UserForCreate, UserForUpdate, UserFilter,
    "User", "/users",
    "user"      // 权限码: user:create, user:read, user:update, user:delete
);

// 模式三：精细化权限配置
declare_crud_handlers!(
    user_crud, User, UserBmc, UserForCreate, UserForUpdate, UserFilter,
    "User", "/users",
    perms(create="user", read="user", update="user_admin", delete="user_admin")
);
```

宏生成 8 个 handler：`create` / `create_many` / `get`(GET) / `update` / `update_many` /
`delete` / `list` / `page`。带权限时通过 `svr_ctx.require_permission(...)` 注入校验。

### 4.3 注册路由（handler 模块 mod.rs）

```rust
// cmx-biz-api/src/handlers/form/mod.rs（真实实例）
pub mod handler;

// 从业务 crate re-export 业务类型（供宏的类型路径引用）
pub use cmx_biz::form::{Form, FormBmc, FormFilter, FormForCreate, FormForUpdate, FormService};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
// 宏展开后以相对路径引用 form_crud 模块，必须显式导入
use crate::crud_handlers::form_crud;
use axum::Router;

pub struct FormModule;

impl ModuleRoutes for FormModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        let router = cmx_api_core::register_crud_handlers_module!(router, form_crud, "/form");
        // 自定义路由在此追加：router.route("/form/custom", post(handler::xxx))
        router
    }

    fn prefix() -> &'static str { "form" }
    fn module_name(&self) -> &'static str { "form" }
}
```

宏生成的 8 条路由：`/{prefix}/create`、`/create-many`、`/get`(GET)、`/update`、
`/update-many`、`/delete`、`/list`、`/page`。

另有组合宏 `setup_crud_api!`（声明 + 注册一步完成）与旧版 `register_crud_routes!`
（不生成 OpenAPI，勿用于新代码）。

---

## 五、路由注册与模块组织

### 5.1 手写模块的 mod.rs（cmx-iam-api 实例）

```rust
pub mod handler;

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

pub struct UserModule;

impl ModuleRoutes for UserModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .route("/iam/users/create", post(handler::create_user))
            .route("/iam/users/update", post(handler::update_user))
            .route("/iam/users/delete", post(handler::delete_user))
            .route("/iam/users/get", get(handler::get_user))
            .route("/iam/users/page", post(handler::page_users))
            .route("/iam/users/list", post(handler::list_users))
        // 业务自定义路由继续追加
    }

    fn prefix() -> &'static str { "iam/users" }
    fn module_name(&self) -> &'static str { "iam/user" }
}
```

### 5.2 域内聚合（cmx-iam-api/src/handlers/iam/mod.rs 实例）

```rust
pub mod permission;
pub mod role;
pub mod user;
// ...

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use axum::Router;

pub struct IamModule;

impl ModuleRoutes for IamModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        let router = router.merge(user::UserModule.routes());
        let router = router.merge(role::RoleModule.routes());
        // ...
        router.merge(rule::RuleModule.routes())
    }

    fn prefix() -> &'static str { "iam" }
    fn module_name(&self) -> &'static str { "iam" }
}
```

复杂模块可用 `Router::new().nest("/xxx", inner_routes())` 嵌套（见 cmx-plugin-api）。

### 5.3 总装配

各域 crate 的顶层 Module（如 `cmx_biz_api::ApplicationModule` / `FormModule`、
`cmx_iam_api::{AuthModule, IamModule}`）由 **cmx-platform-app**
（`crates/libs/cmx-platform-app/src/routes.rs`，`.merge(XxxModule.routes())` 链）
合并进主 Router；`cmx-common-api/src/routes/routes_impl.rs` 保留 portal / service /
debug / dev 的聚合。新增域模块后需在 platform-app 的 routes.rs 装配点登记。

### 5.4 目录结构模板

```
<域 crate>/src/
  ├── crud_handlers.rs        # 宏集中声明（仅用宏的实体，静态 Service 模式）
  ├── openapi.rs              # 本域 OpenApi 切片（XxxApiDoc）
  └── handlers/
      ├── mod.rs
      └── <module>/
          ├── mod.rs          # re-export 业务类型 + ModuleRoutes 实现
          ├── handler.rs      # handler 函数
          ├── request.rs      # 仅本模块用的 API 层请求 DTO（可选）
          └── response.rs     # 仅本模块用的响应 DTO（可选）
```

注入式 Service 模块的 mod.rs 无 re-export、handler 直接 `use cmx_iam::user::...`。

---

## 六、request.rs / response.rs（API 层 DTO）

仅本模块使用的请求/响应结构体放 `<handler>/request.rs` / `response.rs`，必须派生 `ToSchema`：

```rust
use serde::Deserialize;
use utoipa::ToSchema;

/// 安装请求。
#[derive(Debug, Deserialize, ToSchema)]
pub struct XxxInstallRequest {
    /// 来源。
    pub source: XxxSourceRequest,
    /// 目标 ID。
    pub target_id: Option<String>,
}

/// 来源请求（serde tag 枚举）。
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum XxxSourceRequest {
    /// 本地路径。
    Local { path: String },
    /// 远程 URL。
    Remote { url: String, checksum: Option<String> },
}
```

约定：

- 字段用 `Option<String>`，枚举在 `From` 转换时 parse（业务 crate 枚举需实现 `FromStr`）。
- 需要前端驼峰时加 `#[serde(rename_all = "camelCase")]`。
- 实现 `From<XxxRequest> for <业务类型>` 转换后透传给 Service。
- 跨 crate 共享（≥2 crate 或 WASM 使用）→ 下沉 `cmx-core/src/model/`，不放本模块。

---

## 七、OpenAPI 注册（openapi.rs）

每个域 crate 一个切片，platform-app 用 `OpenApi::merge()` 聚合：

```rust
// cmx-biz-api/src/openapi.rs（节选自真实文件）
use utoipa::OpenApi;

/// 业务模型模块 OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::application::handler::create_application,
        crate::handlers::application::handler::application_custom_page,
        crate::crud_handlers::form_crud::create,
        crate::crud_handlers::form_crud::page,
        // ...
    ),
    components(
        schemas(
            crate::handlers::domain::Domain,
            crate::handlers::domain::DomainForCreate,
            crate::handlers::domain::DomainForUpdate,
            // ...
        )
    )
)]
pub struct BizApiDoc;
```

新增 handler 后必须同步：paths 加 `#[utoipa::path]` 函数、components 加新 DTO schema。
