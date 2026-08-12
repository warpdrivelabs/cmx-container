# CRUD 宏使用说明

## 概述

cmx-api 提供了一套声明式宏，用于快速生成带 OpenAPI 文档注解的 CRUD Handler 函数和路由注册，避免为每个实体重复编写样板代码。

宏定义位于 [routes/macros.rs](src/routes/macros.rs)，通过三步完成从 Handler 声明到路由注册再到 OpenAPI 注册的全流程。

## 架构总览

```
宏层 (macros.rs)
  │
  ├─ declare_crud_handlers!     → 生成带 #[utoipa::path] 的 handler 模块
  ├─ register_crud_handlers_module! → 将 handler 注册到 axum 路由
  └─ setup_crud_api!            → 组合宏（声明 + 注册一步完成）
         │
         ▼
调用层 (crud_handlers.rs)
  └─ 为每个实体调用 declare_crud_handlers! 生成具体 handler 模块
         │
         ▼
路由层 (routes.rs)
  └─ 使用 register_crud_handlers_module! 注册路由
         │
         ▼
文档层 (openapi.rs)
  └─ 在 #[openapi(paths(...))] 中注册 handler 到 OpenAPI
```

## 宏详解

### 1. `declare_crud_handlers!`

为指定实体生成一个模块，包含 8 个带 `#[utoipa::path]` 注解的 CRUD handler 函数。每个 handler 内部调用通用的 `rest::handler` 泛型函数。

**函数签名：**

```rust
declare_crud_handlers!(
    $module_name,       // 生成的模块名（标识符）
    $entity,            // 实体类型
    $bmc,               // BMC 类型（实现 DbBmc trait）
    $entity_create,     // 创建 DTO 类型（实现 HasSeaFields + DeserializeOwned）
    $entity_update,     // 更新 DTO 类型（实现 HasSeaFields + DeserializeOwned）
    $filter,            // 过滤器类型（实现 Into<FilterGroups> + Clone）
    $tag,               // OpenAPI tag 名称（字符串字面量）
    $prefix             // 路由前缀（字符串字面量，如 "/domains"）
);
```

**生成的 8 个 Handler：**

| Handler | HTTP 方法 | 路径 | 说明 | OpenAPI request_body |
|---------|-----------|------|------|---------------------|
| `create` | POST | `$prefix/create` | 创建单个实体 | `$entity_create` |
| `create_many` | POST | `$prefix/create-many` | 批量创建实体 | `Vec<$entity_create>` |
| `get` | GET | `$prefix/get?id=xxx` | 根据主键查询实体 | 无（Query 参数） |
| `update` | POST | `$prefix/update` | 更新单个实体 | `serde_json::Value` |
| `update_many` | POST | `$prefix/update-many` | 批量更新实体 | `serde_json::Value` |
| `delete` | POST | `$prefix/delete` | 删除实体（支持批量） | `serde_json::Value` |
| `list` | POST | `$prefix/list` | 列表查询 | `serde_json::Value` |
| `page` | POST | `$prefix/page` | 分页查询 | `serde_json::Value` |

**关于 `serde_json::Value` 的说明：**

`update`、`update_many`、`delete`、`list`、`page` 这 5 个 handler 的函数参数类型为 `Json<serde_json::Value>`，OpenAPI `request_body` 也标注为 `serde_json::Value`。这是因为它们内部使用的泛型类型（`UpdatePayload<E>`、`DeletePayload`、`ListParams<F>`、`PageParams<F>`）来自 cmx-core，未实现 utoipa 的 `ToSchema` trait。

handler 内部会通过 `serde_json::from_value()` 将 `Value` 反序列化为实际的泛型类型后再转发给 `rest::handler`。因此运行时类型安全不受影响，只是 OpenAPI 文档中这些接口的请求体显示为通用 JSON 而非具体 schema。

> 如需在 OpenAPI 文档中展示这些接口的精确请求体结构，可在 cmx-core 中为 `UpdatePayload`、`DeletePayload`、`ListParams`、`PageParams` 手动实现 `ToSchema`。

### 2. `register_crud_handlers_module!`

将 `declare_crud_handlers!` 生成的 handler 模块注册到 Axum 路由器。

**函数签名：**

```rust
register_crud_handlers_module!(
    $router,            // Axum Router 表达式
    $handlers_mod,      // 模块名（标识符，需与 declare_crud_handlers! 的 $module_name 一致）
    $prefix             // 路由前缀（字符串字面量）
);
```

**注意：** 该宏内部硬编码了 `crate::routes::crud_handlers::$handlers_mod::` 路径，因此 handler 模块必须在 `routes/crud_handlers.rs` 中声明。

### 3. `setup_crud_api!`（组合宏）

一次性完成 handler 声明和路由注册，等价于依次调用 `declare_crud_handlers!` + `register_crud_handlers_module!`。

**函数签名：**

```rust
setup_crud_api!(
    $router,
    $module_name,
    $entity,
    $bmc,
    $entity_create,
    $entity_update,
    $filter,
    $tag,
    $prefix
);
```

### 4. `register_crud_routes!`（旧版）

直接将泛型 `rest::handler` 函数注册到路由，**不生成 OpenAPI 文档**。仅建议在不需 OpenAPI 的场景使用。

## 使用步骤

### 第一步：定义实体的类型

在 `src/handlers/<entity>/` 目录下准备以下类型：

- **Entity** - 实体结构体（需 derive `Serialize, Deserialize, Clone, Debug, utoipa::ToSchema`）
- **BMC** - 业务模型控制器（实现 `DbBmc` trait）
- **ForCreate** - 创建 DTO（需 derive `Serialize, Deserialize, HasSeaFields, utoipa::ToSchema`）
- **ForUpdate** - 更新 DTO（需 derive `Serialize, Deserialize, HasSeaFields, utoipa::ToSchema`）
- **Filter** - 过滤器类型（需实现 `Into<FilterGroups>`）

### 第二步：声明 CRUD Handler 模块

在 `src/routes/crud_handlers.rs` 中调用宏：

```rust
use crate::declare_crud_handlers;

declare_crud_handlers!(
    my_entity_crud,                              // 模块名
    crate::handlers::my_entity::MyEntity,        // 实体类型
    crate::handlers::my_entity::MyEntityBmc,     // BMC 类型
    crate::handlers::my_entity::MyEntityForCreate, // 创建 DTO
    crate::handlers::my_entity::MyEntityForUpdate, // 更新 DTO
    crate::handlers::my_entity::MyEntityFilter,  // 过滤器类型
    "MyEntity",                                  // OpenAPI tag
    "/my-entity"                                 // 路由前缀
);
```

### 第三步：注册路由

在 `src/routes/routes.rs` 中使用 `register_crud_handlers_module!` 或 `setup_crud_api!` 注册路由：

```rust
use crate::register_crud_handlers_module;
use crate::routes::crud_handlers::my_entity_crud;

pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();
    let router = register_crud_handlers_module!(router, my_entity_crud, "/my-entity");
    router
}
```

### 第四步：注册 OpenAPI 文档

在 `src/openapi.rs` 的 `#[openapi(paths(...))]` 中添加生成的 handler 路径：

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::crud_handlers::my_entity_crud::create,
        crate::routes::crud_handlers::my_entity_crud::create_many,
        crate::routes::crud_handlers::my_entity_crud::get,
        crate::routes::crud_handlers::my_entity_crud::update,
        crate::routes::crud_handlers::my_entity_crud::update_many,
        crate::routes::crud_handlers::my_entity_crud::delete,
        crate::routes::crud_handlers::my_entity_crud::list,
        crate::routes::crud_handlers::my_entity_crud::page,
    ),
    components(schemas(
        crate::handlers::my_entity::MyEntity,
        crate::handlers::my_entity::MyEntityForCreate,
        crate::handlers::my_entity::MyEntityForUpdate,
    ))
)]
pub struct ApiDoc;
```

## 现有实体配置

| 实体 | 模块名 | BMC | 路由前缀 | OpenAPI Tag |
|------|--------|-----|----------|-------------|
| Domain | `domain_crud` | `DomainBmc` | `/domains` | `"Domain"` |
| Application | `application_crud` | `ApplicationBmc` | `/applications` | `"Application"` |
| Module | `module_crud` | `ModuleBmc` | `/module` | `"Module"` |
| SysDatasource | `sys_datasource_crud` | `SysDatasourceBmc` | `/sys-datasource` | `"SysDatasource"` |

## 关键类型约束

| 类型 | 约束 | 说明 |
|------|------|------|
| `$bmc` (BMC) | 实现 `DbBmc` trait | 定义数据库表和操作 |
| `$entity_create` | 实现 `HasSeaFields + DeserializeOwned + ToSchema` | 创建接口的请求体 |
| `$entity_update` | 实现 `HasSeaFields + DeserializeOwned` | 更新接口的请求体 |
| `$filter` | 实现 `Into<FilterGroups> + DeserializeOwned + Clone` | list/page 的过滤条件 |

## 相关文件

| 文件 | 说明 |
|------|------|
| [src/routes/macros.rs](src/routes/macros.rs) | 宏定义 |
| [src/routes/crud_handlers.rs](src/routes/crud_handlers.rs) | 各实体的 handler 模块声明 |
| [src/routes/routes.rs](src/routes/routes.rs) | 路由注册 |
| [src/openapi.rs](src/openapi.rs) | OpenAPI 文档配置 |
| [src/rest/handler.rs](src/rest/handler.rs) | 通用 CRUD handler 泛型函数 |
