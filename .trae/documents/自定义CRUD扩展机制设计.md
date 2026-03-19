# 自定义 CRUD 扩展机制设计

## 1. 设计目标

当默认的通用 CRUD 不满足需求时，开发者需要能够：

1. 扩展现有的 CRUD 方法
2. 添加自定义的业务方法
3. 覆盖默认的 CRUD 行为
4. 组合多个 Service

## 2. 目录结构

### 2.1 模块目录结构

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

### 2.2 cmx-api 提供的扩展点

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

## 3. Entity 定义

### 3.1 完整实体定义

```rust
// models/domain/entity.rs

use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;

/// 领域实体（完整字段，用于查询返回）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
pub struct Domain {
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name = "type")]  // 字段名映射
    pub r#type: Option<String>,
    // ... 其他字段
}

/// 创建请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct DomainForCreate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // ... 其他可选字段
}

/// 更新请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct DomainForUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    // ... 所有字段均为可选
}
```

### 3.2 关键点

1. **Fields 派生宏**：`#[derive(modql::field::Fields)]` 自动实现 `HasSeaFields` trait
2. **字段映射**：使用 `#[field(name = "type")]` 处理 Rust 保留字
3. **跳过 None**：`#[serde(skip_serializing_if = "Option::is_none")]` 避免序列化空值

## 4. 自定义 Service

### 4.1 继承扩展模式

```rust
// models/domain/service.rs

use crate::crud::service::GenericCrudService;
use crate::error::{Error, Result};
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use modql::filter::{ListOptions, OpValString, OpValsString};
use tracing::{debug, info};

use super::{DomainBmc, DomainFilter, DomainForCreate};

/// Domain 自定义服务
///
/// 继承 GenericCrudService 并添加自定义业务方法
pub struct DomainService;

impl DomainService {
    /// 扩展方法：按名称查询
    pub async fn get_by_name(
        mm: &DatabaseManager,
        db_id: &str,
        name: &str,
    ) -> Result<DataSet> {
        info!("{:<12} - DomainService::get_by_name - name: {}", "SERVICE", name);

        let filter = DomainFilter {
            code: None,
            name: Some(OpValsString(vec![OpValString::Eq(name.to_string())])),
            r#type: None,
            status: None,
            archived: None,
        };

        // 调用 GenericCrudService 的 list 方法
        GenericCrudService::<DomainBmc, DomainFilter>::list(mm, db_id, Some(filter), None).await
    }

    /// 扩展方法：批量创建
    pub async fn batch_create(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<DomainForCreate>,
    ) -> Result<DataSet> {
        info!("{:<12} - DomainService::batch_create - count: {}", "SERVICE", items.len());

        // 调用 GenericCrudService 的 create_many 方法
        GenericCrudService::<DomainBmc>::create_many(mm, db_id, items).await
    }

    /// 覆盖方法：自定义创建逻辑
    ///
    /// 添加额外的验证和业务逻辑
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: DomainForCreate,
    ) -> Result<DataSet> {
        info!("{:<12} - DomainService::create", "SERVICE");

        // 自定义验证：名称长度
        if data.name.len() < 2 {
            return Err(Error::bad_request("域名长度不能小于2个字符"));
        }
        if data.name.len() > 100 {
            return Err(Error::bad_request("域名长度不能超过100个字符"));
        }

        // 调用 GenericCrudService 方法
        GenericCrudService::<DomainBmc>::create(mm, db_id, data).await
    }

    /// 扩展方法：按状态统计
    pub async fn count_by_status(mm: &DatabaseManager, db_id: &str) -> Result<DataSet> {
        debug!("{:<12} - DomainService::count_by_status", "SERVICE");

        let sql = r#"
            SELECT status, COUNT(*) as count 
            FROM cmx_domain 
            WHERE archived = 0
            GROUP BY status
        "#;

        mm.query_sql(db_id, None, sql, "count_by_status")
            .await
            .map_err(|e| Error::internal_error(format!("统计查询失败: {}", e)))
    }

    /// 扩展方法：搜索域名
    ///
    /// 支持模糊搜索和分页
    pub async fn search(
        mm: &DatabaseManager,
        db_id: &str,
        keyword: &str,
        page: i64,
        page_size: i64,
    ) -> Result<(DataSet, i64)> {
        debug!("{:<12} - DomainService::search - keyword: {}", "SERVICE", keyword);

        let filter = DomainFilter {
            code: None,
            name: Some(OpValsString(vec![OpValString::Contains(keyword.to_string())])),
            r#type: None,
            status: None,
            archived: None,
        };

        let list_options = ListOptions {
            limit: Some(page_size),
            offset: Some((page - 1) * page_size),
            order_bys: Some("name".into()),
        };

        // 调用 GenericCrudService 的 page 方法
        GenericCrudService::<DomainBmc, DomainFilter>::page(mm, db_id, Some(filter), list_options).await
    }
}
```

### 4.2 扩展模式说明

| 模式    | 说明                         | 示例                            |
| ----- | -------------------------- | ----------------------------- |
| 继承扩展  | 直接调用 GenericCrudService 方法 | `get_by_name`, `batch_create` |
| 覆盖方法  | 添加验证逻辑后调用父类方法              | `create`（验证名称长度）              |
| 完全自定义 | 直接执行 SQL                   | `count_by_status`             |
| 组合模式  | 组合多个操作                     | `search`（过滤 + 分页）             |

## 5. 自定义 Handler

### 5.1 Handler 定义

```rust
// models/domain/handler.rs

use axum::{extract::Query, Json};
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use serde::Deserialize;
use tracing::debug;

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
///
/// POST /api/domains/by-name
pub async fn get_by_name(
    Json(params): Json<GetByNameParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::get_by_name", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let name = params.name.clone();
    let dataset = DomainService::get_by_name(&mm, &db_id, &name).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 批量创建的请求参数
#[derive(Debug, Deserialize, Clone)]
pub struct BatchCreateParams {
    pub items: Vec<DomainForCreate>,
    #[serde(default)]
    pub db_id: Option<String>,
}

/// 批量创建 Handler
///
/// POST /api/domains/batch-create
pub async fn batch_create(
    Json(params): Json<BatchCreateParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::batch_create", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let items = params.items.clone();
    let results = DomainService::batch_create(&mm, &db_id, items).await?;

    Ok(Json(ApiResp::ok(results)))
}

/// 搜索 Handler
///
/// POST /api/domains/search
pub async fn search(
    Json(params): Json<SearchParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    debug!("{:<12} - handler::search", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let keyword = params.keyword.clone();
    let page = params.get_page();
    let page_size = params.get_page_size();

    let (dataset, total) = DomainService::search(&mm, &db_id, &keyword, page, page_size).await?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page as u64,
        page_size as u64,
        total as u64,
    )))
}
```

### 5.2 Handler 设计原则

1. **参数结构体**：为每个接口定义专用的参数结构体
2. **db\_id 处理**：提供 `get_db_id()` 方法，支持可选的数据库 ID
3. **调用 Service**：Handler 只负责参数解析和响应封装，业务逻辑放在 Service
4. **统一响应**：使用 `ApiResp::ok()` 和 `ApiResp::ok_with_pagination()` 封装响应

## 6. 路由注册

### 6.1 标准路由注册

```rust
// routes.rs

use axum::Router;
use crate::register_crud_routes;
use crate::state::CmxAppState;
use crate::models::domain::{DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate};

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

    router
}
```

### 6.2 自定义路由注册

```rust
// routes.rs

use axum::routing::post;
use crate::models::domain::handler as domain_handler;

pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();

    // 注册标准 CRUD 路由
    let router = register_crud_routes!(
        router, 
        DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, "/domains"
    );

    // 注册自定义路由
    let router = router
        .route("/domains/by-name", post(domain_handler::get_by_name))
        .route("/domains/batch-create", post(domain_handler::batch_create))
        .route("/domains/search", post(domain_handler::search))
        .route("/domains/count-by-status", post(domain_handler::count_by_status));

    router
}
```

## 7. 完整接口列表

### 7.1 标准 CRUD 接口

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

### 7.2 自定义接口

| 方法   | 路径                         | 说明    | 请求体                                                |
| ---- | -------------------------- | ----- | -------------------------------------------------- |
| POST | `/domains/by-name`         | 按名称查询 | `{ "name": "xxx" }`                                |
| POST | `/domains/batch-create`    | 批量创建  | `{ "items": [{ ... }] }`                           |
| POST | `/domains/search`          | 搜索    | `{ "keyword": "xxx", "page": 1, "page_size": 20 }` |
| GET  | `/domains/count-by-status` | 按状态统计 | -                                                  |

## 8. 最佳实践

### 8.1 分层架构

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

### 8.2 命名约定

| 组件      | 命名             | 示例                            |
| ------- | -------------- | ----------------------------- |
| 实体      | 名词             | `Domain`                      |
| 创建 DTO  | 实体 + ForCreate | `DomainForCreate`             |
| 更新 DTO  | 实体 + ForUpdate | `DomainForUpdate`             |
| DbBmc   | 实体 + Bmc       | `DomainBmc`                   |
| Filter  | 实体 + Filter    | `DomainFilter`                |
| Service | 实体 + Service   | `DomainService`               |
| Handler | 动作/操作          | `get_by_name`, `batch_create` |

### 8.3 错误处理

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

## 9. 总结

| 扩展方式                    | 适用场景       | 示例                           |
| ----------------------- | ---------- | ---------------------------- |
| 直接使用 GenericCrudService | 标准 CRUD 操作 | `create`, `update`, `delete` |
| Service 扩展方法            | 添加业务逻辑     | `get_by_name`, `search`      |
| Service 覆盖方法            | 自定义验证逻辑    | `create`（验证名称长度）             |
| 完全自定义 SQL               | 复杂查询       | `count_by_status`            |
| 自定义 Handler             | 自定义接口      | `/domains/by-name`           |

