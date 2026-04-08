# 模块化路由注册方案

## 1. 设计目标

将各模块的路由定义下放到 `handlers` 目录下的各模块中，通过 trait 统一注册。路径前缀由各模块自己管理。

## 2. 方案设计

### 2.1 定义 ModuleRoutes Trait

```rust
/// 模块路由 Trait
///
/// 各 handler 模块实现此 trait 来定义自己的路由
pub trait ModuleRoutes {
    /// 注册该模块的路由
    fn routes(self) -> Router<CmxAppState>;

    /// 获取模块前缀路径
    fn prefix() -> &'static str;

    /// 获取模块名称（用于日志/调试）
    fn module_name(&self) -> &'static str;
}
```

### 2.2 Handler 模块实现示例

**handlers/domain/mod.rs**:

```rust
//! Domain 模块
//!
//! 提供领域实体的 CRUD 操作

mod bmc;
mod entity;
mod filter;
pub mod handler;
mod service;

pub use bmc::DomainBmc;
pub use entity::{Domain, DomainForCreate, DomainForUpdate, DomainTreeNodeData};
pub use filter::DomainFilter;
pub use service::DomainService;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// Domain 模块路由
pub struct DomainModule;

impl ModuleRoutes for DomainModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Domain CRUD 路由
        let router = crate::register_crud_handlers_module!(router, domain_crud, "/domains");
        // 注册 Domain 自定义路由
        router.route("/domains/tree", post(handler::get_tree))
    }

    fn prefix() -> &'static str {
        "domains"
    }

    fn module_name(&self) -> &'static str {
        "domain"
    }
}
```

**handlers/application/mod.rs**:

```rust
//! Application 模块
//!
//! 提供应用实体的 CRUD 操作

mod bmc;
mod entity;
mod filter;
pub mod handler;

pub use bmc::ApplicationBmc;
pub use entity::{Application, ApplicationForCreate, ApplicationForUpdate};
pub use filter::ApplicationFilter;
pub use handler::application_custom_page;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// Application 模块路由
pub struct ApplicationModule;

impl ModuleRoutes for ApplicationModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Application CRUD 路由
        let router = crate::register_crud_handlers_module!(router, application_crud, "/applications");
        // 注册 Application 自定义路由
        router.route("/applications/custom-page", post(application_custom_page))
    }

    fn prefix() -> &'static str {
        "applications"
    }

    fn module_name(&self) -> &'static str {
        "application"
    }
}
```

**handlers/table_metadata/mod.rs**（无 CRUD 的自定义模块）:

```rust
//! 表元数据查询 Handler
//!
//! 提供 cmx_meta_table_define 表的列表和分页查询接口

pub mod handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// TableMetadata 模块路由
pub struct TableMetadataModule;

impl ModuleRoutes for TableMetadataModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .nest("/table-metadata", inner_routes())
    }

    fn prefix() -> &'static str {
        "table-metadata"
    }

    fn module_name(&self) -> &'static str {
        "table_metadata"
    }
}

fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/get", get(handler::table_metadata_get_by_id))
        .route("/list", post(handler::table_metadata_list))
        .route("/page", post(handler::table_metadata_page))
}
```

## 3. routes.rs 简化

```rust
//! 集中路由注册模块
//!
//! 提供统一的路由注册入口，简化 web-server 的路由配置

use crate::handlers::application;
use crate::handlers::domain;
use crate::handlers::module;
use crate::handlers::plugin;
use crate::handlers::service;
use crate::handlers::sys_datasource;
use crate::handlers::table_metadata;
use crate::routes::traits::ModuleRoutes;
use crate::app_state::CmxAppState;
use crate::openapi::ApiDoc;
use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// 注册所有 API 路由
pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();

    // 注册 Domain 模块路由（使用 ModuleRoutes）
    let router = router.merge(domain::DomainModule.routes());

    // 注册 Application 模块路由（使用 ModuleRoutes）
    let router = router.merge(application::ApplicationModule.routes());

    // 注册 Module 模块路由（使用 ModuleRoutes）
    let router = router.merge(module::ModuleHandler.routes());

    // 注册 SysDatasource 模块路由（使用 ModuleRoutes）
    let router = router.merge(sys_datasource::SysDatasourceModule.routes());

    // 注册插件管理路由（使用 ModuleRoutes）
    let router = router.merge(plugin::PluginModule.routes());

    // 注册表元数据查询路由（使用 ModuleRoutes）
    let router = router.merge(table_metadata::TableMetadataModule.routes());

    // 注册服务调用路由（使用 ModuleRoutes）
    let router = router.merge(service::ServiceModule.routes());

    router
}

/// 注册带有 Swagger UI 的 API 路由
pub fn swagger_routes() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui")
            .url("/api-docs/openapi.json", ApiDoc::openapi()))
}
```

## 4. 目录结构

```
crates/libs/cmx-api/src/
├── routes/
│   ├── mod.rs
│   ├── routes.rs           # 统一注册入口
│   ├── traits.rs           # ModuleRoutes trait
│   ├── macros.rs           # 路由注册宏
│   └── crud_handlers.rs    # CRUD handlers 声明
│
└── handlers/
    ├── mod.rs
    ├── domain/
    │   ├── mod.rs          # 实现 ModuleRoutes，定义完整路径
    │   ├── bmc.rs          # DomainBmc
    │   ├── entity.rs       # Domain, DomainForCreate, DomainForUpdate
    │   ├── filter.rs       # DomainFilter
    │   ├── service.rs      # DomainService（自定义服务）
    │   └── handler.rs      # 自定义 Handler
    │
    ├── application/
    │   ├── mod.rs          # 实现 ModuleRoutes
    │   ├── bmc.rs
    │   ├── entity.rs
    │   ├── filter.rs
    │   └── handler.rs
    │
    ├── module/
    │   ├── mod.rs          # 实现 ModuleRoutes
    │   ├── bmc.rs
    │   ├── entity.rs
    │   ├── filter.rs
    │   └── handler.rs
    │
    ├── sys_datasource/
    │   ├── mod.rs          # 实现 ModuleRoutes
    │   ├── bmc.rs
    │   ├── entity.rs
    │   ├── filter.rs
    │   ├── service.rs
    │   └── handler.rs
    │
    ├── table_metadata/
    │   ├── mod.rs          # 实现 ModuleRoutes（无 CRUD）
    │   └── handler.rs
    │
    ├── plugin/
    │   ├── mod.rs          # 实现 ModuleRoutes
    │   ├── handler.rs
    │   ├── request.rs
    │   └── response.rs
    │
    └── service/
        ├── mod.rs          # 实现 ModuleRoutes
        └── handler.rs
```

## 5. CRUD Handlers 声明

### 5.1 crud_handlers.rs

CRUD handlers 通过 `declare_crud_handlers!` 宏在 `crud_handlers.rs` 中集中声明：

```rust
//! CRUD Handlers 模块
//!
//! 为各实体生成带 OpenAPI 文档的 CRUD handler 模块

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

declare_crud_handlers!(
    application_crud,
    crate::handlers::application::Application,
    crate::handlers::application::ApplicationBmc,
    crate::handlers::application::ApplicationForCreate,
    crate::handlers::application::ApplicationForUpdate,
    crate::handlers::application::ApplicationFilter,
    "Application",
    "/applications"
);

declare_crud_handlers!(
    module_crud,
    crate::handlers::module::Module,
    crate::handlers::module::ModuleBmc,
    crate::handlers::module::ModuleForCreate,
    crate::handlers::module::ModuleForUpdate,
    crate::handlers::module::ModuleFilter,
    "Module",
    "/module"
);

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
```

### 5.2 宏定义

#### declare_crud_handlers! 宏

为指定实体生成包含 8 个 CRUD handler 函数的模块：

```rust
/// 声明 CRUD Handlers 模块
///
/// # 参数
/// * `$module_name` - 生成的模块名
/// * `$entity` - 实体类型
/// * `$bmc` - BMC 类型
/// * `$entity_create` - 创建 DTO 类型（需实现 ToSchema）
/// * `$entity_update` - 更新 DTO 类型（需实现 ToSchema）
/// * `$filter` - 过滤器类型
/// * `$tag` - OpenAPI tag
/// * `$prefix` - 路由前缀
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
        }
    };
}
```

#### register_crud_handlers_module! 宏

将已生成的 CRUD handlers 模块注册到路由：

```rust
/// 注册已生成的 CRUD Handlers 模块到路由
///
/// # 参数
/// * `$router` - Axum 路由器
/// * `$handlers_mod` - 生成的 handlers 模块 (如 `domain_crud`)
/// * `$prefix` - 路由前缀
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

#### setup_crud_api! 宏（组合宏）

一次性完成 handler 声明和路由注册：

```rust
/// 设置 CRUD API（组合宏）
///
/// 一次性完成 handler 声明和路由注册
#[macro_export]
macro_rules! setup_crud_api {
    (
        $router:expr,
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr
    ) => {
        $crate::declare_crud_handlers!(
            $module_name, $entity, $bmc, $entity_create, $entity_update, $filter, $tag, $prefix
        );
        $crate::register_crud_handlers_module!($router, $module_name, $prefix)
    };
}
```

## 6. 优点

1. **分散管理**：各模块的路由和路径前缀在本模块管理，便于维护
2. **统一注册**：routes.rs 中只需 `.merge()` 即可，无需关心路径
3. **类型安全**：编译时检查路由完整性
4. **易于扩展**：新增模块只需实现 trait
5. **自包含**：模块完全自包含，外部只需调用
6. **OpenAPI 支持**：自动生成 API 文档

## 7. 模块类型

### 7.1 标准 CRUD 模块

需要 Entity、Bmc、Filter、Create DTO、Update DTO，使用 `register_crud_handlers_module!` 注册：

- domain
- application
- module
- sys_datasource

### 7.2 自定义模块

只有自定义 Handler，不使用 CRUD 宏：

- table_metadata
- plugin
- service

## 8. 标准 CRUD 接口

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
