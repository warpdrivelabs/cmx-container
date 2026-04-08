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
├── api_response.rs            # API 响应封装
├── error.rs                   # 错误类型
├── app_state.rs               # 应用状态
└── openapi.rs                 # OpenAPI 文档
```

### 2.2 cmx-api 提供的扩展点

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

## 3. Entity 定义

### 3.1 完整实体定义

```rust
// handlers/domain/entity.rs

use crate::rest::TreeNodeData;
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// 领域实体（完整字段，用于查询返回）
///
/// 表示系统中的一个领域/域对象
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
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
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
    // ... 其他审计字段
}

/// 创建请求 DTO
///
/// 用于创建 Domain 的请求数据
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
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 标签（JSON 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// 更新请求 DTO
///
/// 用于更新 Domain 的请求数据，所有字段均为可选
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct DomainForUpdate {
    /// 名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
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
}
```

### 3.2 关键点

1. **Fields 派生宏**：`#[derive(modql::field::Fields)]` 自动实现 `HasSeaFields` trait
2. **字段映射**：使用 `#[field(name = "type")]` 处理 Rust 保留字
3. **跳过 None**：`#[serde(skip_serializing_if = "Option::is_none")]` 避免序列化空值
4. **ToSchema**：`#[derive(utoipa::ToSchema)]` 支持 OpenAPI 文档生成

## 4. DbBmc 实现

```rust
// handlers/domain/bmc.rs

use cmx_database::crud::DbBmc;

/// Domain 实体的 Bmc
///
/// 定义了 cmx_domain 表的元信息
pub struct DomainBmc;

impl DbBmc for DomainBmc {
    /// 表名
    const TABLE: &'static str = "cmx_domain";
    
    /// 主键列名
    const PK_COLUMN: &'static str = "id";
    
    /// 是否有时间戳字段
    fn has_timestamps() -> bool {
        true
    }
    
    /// 是否有 owner_id 字段
    fn has_owner_id() -> bool {
        false
    }
}
```

## 5. Filter 定义

```rust
// handlers/domain/filter.rs

use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

/// Domain 查询过滤器
///
/// 支持按 code、name、type、status 等字段进行过滤
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct DomainFilter {
    /// 编码过滤
    pub code: Option<OpValsString>,
    /// 名称过滤
    pub name: Option<OpValsString>,
    /// 类型过滤
    pub r#type: Option<OpValsString>,
    /// 状态过滤
    pub status: Option<OpValsInt64>,
    /// 归档标志过滤
    pub archived: Option<OpValsInt64>,
}
```

## 6. 自定义 Service

### 6.1 继承扩展模式

```rust
// handlers/domain/service.rs

use super::{DomainBmc, DomainFilter, DomainTreeNodeData};
use crate::error::{Error, Result};
use crate::rest::TreeNode;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use cmx_database::crud::GenericCrudService;
use modql::filter::{ListOptions, OpValString, OpValsString};
use tracing::debug;

/// Domain 自定义服务
///
/// 继承 GenericCrudService 并添加自定义业务方法
pub struct DomainService;

impl DomainService {
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

        GenericCrudService::<DomainBmc, DomainFilter>::page(
            mm,
            db_id,
            None,
            Some(vec![filter]),
            list_options,
        )
        .await
        .map_err(Error::from)
    }

    /// 查询域-应用-模块树形数据
    ///
    /// 执行 tree.sql 查询获取扁平数据，然后构建为树形结构。
    pub async fn get_tree(
        mm: &DatabaseManager,
        db_id: &str,
    ) -> Result<Vec<TreeNode<DomainTreeNodeData>>> {
        debug!("{:<12} - DomainService::get_tree", "SERVICE");

        let sql = include_str!("tree.sql");
        let dataset = mm
            .query_sql(db_id, None, sql, "domain_tree")
            .await
            .map_err(|e| Error::internal_error(format!("查询域树形数据失败: {}", e)))?;

        let items: Vec<DomainTreeNodeData> = dataset
            .iter()
            .map(|row| Self::row_to_tree_node(row, &dataset.schema))
            .collect::<Result<Vec<_>>>()?;

        Ok(TreeNode::from_list(items))
    }
}
```

### 6.2 扩展模式说明

| 模式    | 说明                         | 示例                            |
| ----- | -------------------------- | ----------------------------- |
| 继承扩展  | 直接调用 GenericCrudService 方法 | `search`                      |
| 完全自定义 | 直接执行 SQL                   | `get_tree`                    |
| 组合模式  | 组合多个操作                     | `search`（过滤 + 分页）             |

## 7. 自定义 Handler

### 7.1 Handler 定义

```rust
// handlers/domain/handler.rs

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_database::get_default_db_manager;
use tracing::debug;

use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::handlers::domain::{DomainService, DomainTreeNodeData};
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::rest::header_parse::get_db_id_from_header;
use crate::rest::TreeNode;

/// 查询域-应用-模块树形结构 Handler
///
/// 查询所有启用且未归档的域、应用、模块数据，
/// 按 域→应用→模块 三级层级组织，同级按 sort_order 排序。
#[utoipa::path(
    post,
    path = "/api/domains/tree",
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<TreeNode<DomainTreeNodeData>>>)
    ),
    tag = "Domain"
)]
pub async fn get_tree(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Vec<TreeNode<DomainTreeNodeData>>>>> {
    debug!("{:<12} - handler::get_tree", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let tree = DomainService::get_tree(mm, &db_id).await?;

    Ok(Json(ApiResp::ok(tree)))
}
```

### 7.2 Handler 设计原则

1. **参数结构体**：为每个接口定义专用的参数结构体
2. **db_id 处理**：通过 `get_db_id_from_header` 从请求头获取数据库 ID
3. **调用 Service**：Handler 只负责参数解析和响应封装，业务逻辑放在 Service
4. **统一响应**：使用 `ApiResp::ok()` 和 `ApiResp::ok_with_pagination()` 封装响应
5. **OpenAPI 注解**：使用 `#[utoipa::path]` 宏添加 API 文档

## 8. 模块路由注册

### 8.1 实现 ModuleRoutes Trait

```rust
// handlers/domain/mod.rs

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

### 8.2 CRUD Handlers 声明

在 `routes/crud_handlers.rs` 中声明：

```rust
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
```

## 9. 完整接口列表

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

### 9.2 自定义接口

| 方法   | 路径               | 说明    | 请求体 |
| ---- | --------------- | ----- | --- |
| POST | `/domains/tree` | 查询树形结构 | -   |

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
