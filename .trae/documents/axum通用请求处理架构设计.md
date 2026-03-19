# CMX-API 通用请求处理架构设计

## 1. 概述

### 1.1 设计目标

1. 提供强类型的 Entity 数据参数，解决 JSON 无法正确表达时间类型的问题
2. 提供通用的 CRUD 操作框架，支持批量操作
3. 支持扩展和自定义业务逻辑
4. 编译时类型检查，提高代码安全性

### 1.2 核心改进

| 改进项       | 改进前                 | 改进后                                    |
| --------- | ------------------- | -------------------------------------- |
| data 参数类型 | `serde_json::Value` | 强类型 Entity                             |
| 字段处理      | 手动遍历 Value          | `HasSeaFields::not_none_sea_fields()`  |
| 时间类型      | JSON 字符串            | `time::OffsetDateTime`                 |
| 类型安全      | 运行时检查               | 编译时检查                                  |
| 创建/更新区分   | 无                   | ForCreate / ForUpdate                  |
| 批量操作      | 不支持                 | `create_many`, `update_many`, `delete` |
| 删除方法      | GET + Query         | POST + JSON Body                       |

## 2. 目录结构

```
crates/libs/cmx-api/src/
├── crud/                      # 通用 CRUD 框架
│   ├── mod.rs
│   ├── traits.rs              # DbBmc trait
│   ├── macros.rs              # 路由注册宏
│   ├── utils.rs               # prep_fields_for_create/update
│   └── service.rs             # GenericCrudService
│
├── rest/                      # REST 协议层
│   ├── mod.rs
│   ├── params.rs              # 参数定义
│   └── handler.rs             # 通用 Handler
│
├── models/                    # 业务模型层
│   └── domain/                # Domain 实体模块
│       ├── mod.rs
│       ├── bmc.rs             # DomainBmc
│       ├── entity.rs          # Domain, DomainForCreate, DomainForUpdate
│       ├── filter.rs          # DomainFilter
│       ├── service.rs         # DomainService（自定义服务）
│       └── handler.rs         # 自定义 Handler
│
├── routes.rs                  # 路由注册入口
└── state.rs                   # CmxAppState
```

### 2.1 扩展点

```rust
// 1. GenericCrudService - 可继承扩展
pub struct GenericCrudService<MC, F = ()> { ... }

// 2. DbBmc trait - 可实现自定义表元信息
pub trait DbBmc { ... }

// 3. Handler 函数 - 可自定义
pub async fn create<MC, E>(...) { ... }

// 4. 宏 - 可组合使用
register_crud_routes!(router, DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, "/domains");
```

## 3. Entity 定义规范

### 3.1 统一 Entity 文件

所有实体元数据定义放在一个文件中：

```rust
// models/domain/entity.rs

use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;

/// 领域实体（完整字段，用于查询返回）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
pub struct Domain {
    /// 唯一标识码（主键）
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
    pub create_time: Option<OffsetDateTime>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<OffsetDateTime>,
    /// 创建者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_by: Option<String>,
    /// 创建者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_name: Option<String>,
    /// 更新者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_by: Option<String>,
    /// 更新者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_name: Option<String>,
}

/// 创建请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct DomainForCreate {
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name = "type")]
    pub r#type: Option<String>,
    /// 标签
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// 更新请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct DomainForUpdate {
    /// 名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name = "type")]
    pub r#type: Option<String>,
    /// 标签
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 是否归档
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
}
```

### 3.2 关键点说明

1. **Fields 派生宏**：`#[derive(modql::field::Fields)]` 自动实现 `HasSeaFields` trait
2. **字段映射**：使用 `#[field(name = "type")]` 将 Rust 保留字 `r#type` 映射到数据库字段 `type`
3. **跳过 None 值**：`#[serde(skip_serializing_if = "Option::is_none")]` 确保可选字段为 None 时不序列化
4. **时间类型**：使用 `time::OffsetDateTime` 作为时间类型，与 sqlx 兼容

## 4. GenericCrudService 实现

### 4.1 核心方法

```rust
/// 创建单个实体
pub async fn create<E>(mm: &DatabaseManager, db_id: &str, data: E) -> Result<DataSet>
where E: HasSeaFields

/// 批量创建多个实体
pub async fn create_many<E>(mm: &DatabaseManager, db_id: &str, data: Vec<E>) -> Result<DataSet>
where E: HasSeaFields

/// 根据主键获取单条实体
pub async fn get(mm: &DatabaseManager, db_id: &str, id: Value) -> Result<DataSet>

/// 更新单个实体
pub async fn update<E>(mm: &DatabaseManager, db_id: &str, id: Value, data: E) -> Result<DataSet>
where E: HasSeaFields

/// 批量更新多个实体
pub async fn update_many<E>(mm: &DatabaseManager, db_id: &str, data: Vec<UpdateItem<E>>) -> Result<DataSet>
where E: HasSeaFields

/// 删除实体（支持单个和批量）
pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet>

/// 列表查询（带过滤和排序）
pub async fn list(mm: &DatabaseManager, db_id: &str, filter: Option<F>, list_options: Option<ListOptions>) -> Result<DataSet>

/// 分页查询（带过滤和排序）
pub async fn page(mm: &DatabaseManager, db_id: &str, filter: Option<F>, list_options: ListOptions) -> Result<(DataSet, i64)>
```

### 4.2 实现示例

```rust
/// 创建单个实体
pub async fn create<E>(
    mm: &DatabaseManager,
    db_id: &str,
    data: E,
) -> Result<DataSet>
where
    E: HasSeaFields,
{
    // 获取非 None 字段
    let mut fields = data.not_none_sea_fields();
    
    // 预处理字段（添加主键等）
    prep_fields_for_create::<MC>(&mut fields, None);

    // 构建 INSERT 语句
    let (columns, sea_values) = fields.for_sea_insert();
    let mut query = Query::insert();
    query
        .into_table(MC::table_ref())
        .columns(columns)
        .values(sea_values)?;

    let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
    mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values).await?;

    Ok(empty_dataset())
}
```

## 5. 自定义 Service 扩展

### 5.1 扩展模式

| 模式    | 说明                         | 示例                            |
| ----- | -------------------------- | ----------------------------- |
| 继承扩展  | 直接调用 GenericCrudService 方法 | `get_by_name`, `batch_create` |
| 覆盖方法  | 添加验证逻辑后调用父类方法              | `create`（验证名称长度）              |
| 完全自定义 | 直接执行 SQL                   | `count_by_status`             |
| 组合模式  | 组合多个操作                     | `search`（过滤 + 分页）             |

### 5.2 实现示例

```rust
// models/domain/service.rs

use crate::crud::service::GenericCrudService;
use crate::error::{Error, Result};
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use modql::filter::{ListOptions, OpValString, OpValsString};

use super::{DomainBmc, DomainFilter, DomainForCreate};

/// Domain 自定义服务
pub struct DomainService;

impl DomainService {
    /// 扩展方法：按名称查询
    pub async fn get_by_name(
        mm: &DatabaseManager,
        db_id: &str,
        name: &str,
    ) -> Result<DataSet> {
        let filter = DomainFilter {
            name: Some(OpValsString(vec![OpValString::Eq(name.to_string())])),
            ..Default::default()
        };
        GenericCrudService::<DomainBmc, DomainFilter>::list(mm, db_id, Some(filter), None).await
    }

    /// 覆盖方法：自定义创建逻辑
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: DomainForCreate,
    ) -> Result<DataSet> {
        // 自定义验证
        if data.name.len() < 2 {
            return Err(Error::bad_request("域名长度不能小于2个字符"));
        }
        GenericCrudService::<DomainBmc>::create(mm, db_id, data).await
    }

    /// 完全自定义：按状态统计
    pub async fn count_by_status(mm: &DatabaseManager, db_id: &str) -> Result<DataSet> {
        let sql = r#"
            SELECT status, COUNT(*) as count 
            FROM cmx_domain 
            WHERE archived = 0
            GROUP BY status
        "#;
        mm.query_sql(db_id, None, sql, "count_by_status").await
            .map_err(|e| Error::internal_error(format!("统计查询失败: {}", e)))
    }

    /// 组合模式：搜索域名
    pub async fn search(
        mm: &DatabaseManager,
        db_id: &str,
        keyword: &str,
        page: i64,
        page_size: i64,
    ) -> Result<(DataSet, i64)> {
        let filter = DomainFilter {
            name: Some(OpValsString(vec![OpValString::Contains(keyword.to_string())])),
            ..Default::default()
        };
        let list_options = ListOptions {
            limit: Some(page_size),
            offset: Some((page - 1) * page_size),
            order_bys: Some("name".into()),
        };
        GenericCrudService::<DomainBmc, DomainFilter>::page(mm, db_id, Some(filter), list_options).await
    }
}
```

## 6. Handler 实现

### 6.1 通用 Handler

```rust
use crate::crud::service::{GenericCrudService, UpdateItem};
use crate::crud::traits::DbBmc;
use crate::error::Result;
use crate::response::ApiResp;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use modql::field::HasSeaFields;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// 创建单个实体 Handler
pub async fn create<MC, E>(Json(data): Json<E>) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = mm.get_default_db_id().await;
    let dataset = GenericCrudService::<MC>::create(&mm, &db_id, data).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除 Handler（支持批量）
pub async fn delete<MC>(Json(payload): Json<DeletePayload>) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    let mm = get_default_db_manager();
    let db_id = mm.get_default_db_id().await;
    let dataset = GenericCrudService::<MC>::delete(&mm, &db_id, payload.ids).await?;
    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新请求 Payload
#[derive(Debug, Deserialize)]
pub struct UpdatePayload<E> {
    pub id: Value,
    pub data: E,
}

/// 删除请求 Payload
#[derive(Debug, Deserialize)]
pub struct DeletePayload {
    pub ids: Vec<Value>,
}
```

### 6.2 自定义 Handler

```rust
// models/domain/handler.rs

use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use serde::Deserialize;

use crate::error::Result;
use crate::models::domain::{DomainForCreate, DomainService};
use crate::response::ApiResp;

/// 按名称查询的请求参数
#[derive(Debug, Deserialize)]
pub struct GetByNameParams {
    pub name: String,
    #[serde(default)]
    pub db_id: Option<String>,
}

impl GetByNameParams {
    pub async fn get_db_id(&self) -> String {
        self.db_id.clone()
            .unwrap_or(get_default_db_manager().get_default_db_id().await)
    }
}

/// 按名称查询 Handler
/// POST /api/domains/by-name
pub async fn get_by_name(
    Json(params): Json<GetByNameParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let dataset = DomainService::get_by_name(&mm, &db_id, &params.name).await?;
    Ok(Json(ApiResp::ok(dataset)))
}
```

### 6.3 Handler 设计原则

1. **参数结构体**：为每个接口定义专用的参数结构体
2. **db\_id 处理**：提供 `get_db_id()` 方法，支持可选的数据库 ID
3. **调用 Service**：Handler 只负责参数解析和响应封装，业务逻辑放在 Service
4. **统一响应**：使用 `ApiResp::ok()` 和 `ApiResp::ok_with_pagination()` 封装响应

## 7. 路由注册

### 7.1 路由注册宏

```rust
/// 注册 CRUD 路由的宏
#[macro_export]
macro_rules! register_crud_routes {
    ($router:expr, $bmc:ty, $filter:ty, $entity_create:ty, $entity_update:ty, $path:expr) => {
        $router
            // 创建操作（使用 ForCreate）
            .route(concat!($path, "/create"), axum::routing::post(
                $crate::rest::handler::create::<$bmc, $entity_create>
            ))
            .route(concat!($path, "/create-many"), axum::routing::post(
                $crate::rest::handler::create_many::<$bmc, $entity_create>
            ))
            // 查询操作
            .route(concat!($path, "/get"), axum::routing::get(
                $crate::rest::handler::get_by_id::<$bmc>
            ))
            // 更新操作（使用 ForUpdate）
            .route(concat!($path, "/update"), axum::routing::post(
                $crate::rest::handler::update::<$bmc, $entity_update>
            ))
            .route(concat!($path, "/update-many"), axum::routing::post(
                $crate::rest::handler::update_many::<$bmc, $entity_update>
            ))
            // 删除操作（支持批量）
            .route(concat!($path, "/delete"), axum::routing::post(
                $crate::rest::handler::delete::<$bmc>
            ))
            // 列表和分页
            .route(concat!($path, "/list"), axum::routing::post(
                $crate::rest::handler::list::<$bmc, $filter>
            ))
            .route(concat!($path, "/page"), axum::routing::post(
                $crate::rest::handler::page::<$bmc, $filter>
            ))
    };
}
```

### 7.2 使用示例

```rust
// routes.rs

use axum::Router;
use crate::register_crud_routes;
use crate::state::CmxAppState;
use crate::models::domain::{DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate};
use crate::models::domain::handler as domain_handler;

pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();

    // 注册标准 CRUD 路由
    let router = register_crud_routes!(
        router, 
        DomainBmc,        // Bmc 类型
        DomainFilter,     // Filter 类型
        DomainForCreate,  // 创建 Entity 类型
        DomainForUpdate,  // 更新 Entity 类型
        "/domains"
    );

    // 注册自定义路由
    let router = router
        .route("/domains/by-name", axum::routing::post(domain_handler::get_by_name));

    router
}
```

## 8. 接口设计

### 8.1 标准 CRUD 接口

| 方法   | 路径                     | 说明   | 请求体                                               |
| ---- | ---------------------- | ---- | ------------------------------------------------- |
| POST | `/domains/create`      | 创建单个 | `{ "name": "xxx", ... }`                          |
| POST | `/domains/create-many` | 批量创建 | `[{ ... }, { ... }]`                              |
| GET  | `/domains/get?id=xxx`  | 获取单条 | -                                                 |
| POST | `/domains/update`      | 更新单个 | `{ "id": "xxx", "data": { ... } }`                |
| POST | `/domains/update-many` | 批量更新 | `[{ "id": "xxx", "data": { ... } }]`              |
| POST | `/domains/delete`      | 删除   | `{ "ids": ["xxx", "yyy"] }`                       |
| POST | `/domains/list`        | 列表查询 | `{ "filter": { ... } }`                           |
| POST | `/domains/page`        | 分页查询 | `{ "filter": { ... }, "offset": 0, "limit": 10 }` |

### 8.2 自定义接口示例

| 方法   | 路径                 | 说明    | 请求体                                                |
| ---- | ------------------ | ----- | -------------------------------------------------- |
| POST | `/domains/by-name` | 按名称查询 | `{ "name": "xxx" }`                                |
| POST | `/domains/search`  | 搜索    | `{ "keyword": "xxx", "page": 1, "page_size": 20 }` |

## 9. 最佳实践

### 9.1 分层架构

```
┌─────────────────────────────────────┐
│           Handler 层                 │  ← 处理 HTTP 请求/响应
│   (models/*/handler.rs)              │
├─────────────────────────────────────┤
│           Service 层                 │  ← 业务逻辑
│   (models/*/service.rs)              │
├─────────────────────────────────────┤
│           Model 层                   │  ← 数据模型
│   (models/*/entity.rs, bmc.rs)       │
├─────────────────────────────────────┤
│           cmx-api                    │  ← 通用 CRUD 框架
│   (crud/service.rs)                  │
└─────────────────────────────────────┘
```

### 9.2 命名约定

| 组件      | 命名             | 示例                            |
| ------- | -------------- | ----------------------------- |
| 实体      | 名词             | `Domain`                      |
| 创建 DTO  | 实体 + ForCreate | `DomainForCreate`             |
| 更新 DTO  | 实体 + ForUpdate | `DomainForUpdate`             |
| DbBmc   | 实体 + Bmc       | `DomainBmc`                   |
| Filter  | 实体 + Filter    | `DomainFilter`                |
| Service | 实体 + Service   | `DomainService`               |
| Handler | 动作/操作          | `get_by_name`, `batch_create` |

### 9.3 错误处理

```rust
use crate::error::{Error, Result};

pub async fn custom_method() -> Result<()> {
    // 参数验证错误
    if invalid_input {
        return Err(Error::bad_request("参数错误"));
    }
    
    // 内部错误
    database_operation()
        .map_err(|e| Error::internal_error(format!("操作失败: {}", e)))?;
    
    Ok(())
}
```

## 10. 总结

| 扩展方式                    | 适用场景       | 示例                           |
| ----------------------- | ---------- | ---------------------------- |
| 直接使用 GenericCrudService | 标准 CRUD 操作 | `create`, `update`, `delete` |
| Service 扩展方法            | 添加业务逻辑     | `get_by_name`, `search`      |
| Service 覆盖方法            | 自定义验证逻辑    | `create`（验证名称长度）             |
| 完全自定义 SQL               | 复杂查询       | `count_by_status`            |
| 自定义 Handler             | 自定义接口      | `/domains/by-name`           |

