# cmx-api 使用指南

本指南详细介绍 cmx-api 模块的使用方法，包括基础用法、高级功能和扩展机制。

## 目录

1. [核心概念](#1-核心概念)
2. [快速开始](#2-快速开始)
3. [DbBmc Trait](#3-dbmc-trait)
4. [过滤器定义](#4-过滤器定义)
5. [CRUD 服务](#5-crud-服务)
6. [Handler 函数](#6-handler-函数)
7. [路由注册](#7-路由注册)
8. [多数据库支持](#8-多数据库支持)
9. [自定义扩展](#9-自定义扩展)
10. [最佳实践](#10-最佳实践)

---

## 1. 核心概念

### 1.1 架构概览

```
┌─────────────────────────────────────┐
│           Handler 层                 │  ← 处理 HTTP 请求/响应
│   (cmx-api/src/rest/handler.rs)     │
├─────────────────────────────────────┤
│           Service 层                 │  ← 业务逻辑
│   (cmx-api/src/crud/service.rs)     │
├─────────────────────────────────────┤
│           Model 层                   │  ← 数据模型
│   (cmx-api/src/crud/traits.rs)      │
├─────────────────────────────────────┤
│           cmx-database               │  ← 数据库操作
│   (cmx-database crate)               │
└─────────────────────────────────────┘
```

### 1.2 核心组件

| 组件 | 说明 |
|-----|------|
| `DbBmc` | 表元信息 trait，定义表名、主键等 |
| `GenericCrudService` | 通用 CRUD 服务 |
| `FilterNodes` | 过滤器 derive 宏（来自 modql） |
| `register_crud_routes!` | 路由注册宏 |

---

## 2. 快速开始

### 2.1 添加依赖

```toml
[dependencies]
cmx-api = { path = "../../crates/libs/cmx-api" }
cmx-database = { path = "../../crates/libs/cmx-infra/cmx-database" }
cmx-core = { path = "../../crates/libs/cmx-core" }
modql = { workspace = true, features = ["with-sea-query"] }
sea-query = { workspace = true }
sea-query-binder = { workspace = true }
axum = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

### 2.2 最小示例

```rust
use axum::Router;
use cmx_api::{DbBmc, register_crud_routes};
use cmx_database::DatabaseManager;
use modql::filter::FilterNodes;
use serde::{Deserialize, Serialize};

// 1. 定义过滤器
#[derive(Debug, Clone, FilterNodes, Serialize, Deserialize)]
pub struct UserFilter {
    pub name: Option<modql::filter::OpValsString>,
}

// 2. 定义 DbBmc
pub struct UserBmc;
impl DbBmc for UserBmc {
    const TABLE: &'static str = "users";
}

// 3. 注册路由
#[tokio::main]
async fn main() {
    let mm = DatabaseManager::new();
    let router = Router::new().with_state(mm);
    let router = register_crud_routes!(router, UserBmc, UserFilter, "/api/users");
    
    // 启动服务器...
}
```

---

## 3. DbBmc Trait

### 3.1 基本定义

```rust
pub trait DbBmc {
    /// 表名（必须实现）
    const TABLE: &'static str;
    
    /// 主键列名（默认 "code"）
    const PK_COLUMN: &'static str = "code";
    
    /// 获取表引用
    fn table_ref() -> TableRef {
        TableRef::Table(SIden(Self::TABLE).into_iden())
    }
    
    /// 是否有时间戳字段（默认 true）
    fn has_timestamps() -> bool { true }
    
    /// 是否有 owner_id 字段（默认 false）
    fn has_owner_id() -> bool { false }
}
```

### 3.2 实现示例

```rust
use cmx_api::DbBmc;

/// 用户表
pub struct UserBmc;
impl DbBmc for UserBmc {
    const TABLE: &'static str = "users";
    const PK_COLUMN: &'static str = "id";  // 使用 id 作为主键
}

/// 订单表（有 owner_id）
pub struct OrderBmc;
impl DbBmc for OrderBmc {
    const TABLE: &'static str = "orders";
    const PK_COLUMN: &'static str = "order_no";
    
    fn has_owner_id() -> bool {
        true  // 订单有 owner_id 字段
    }
}

/// 日志表（无时间戳）
pub struct LogBmc;
impl DbBmc for LogBmc {
    const TABLE: &'static str = "logs";
    
    fn has_timestamps() -> bool {
        false  // 日志表没有 updated_at 字段
    }
}
```

---

## 4. 过滤器定义

### 4.1 基本用法

```rust
use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, FilterNodes, Serialize, Deserialize)]
pub struct UserFilter {
    /// 字符串字段过滤
    pub name: Option<OpValsString>,
    pub email: Option<OpValsString>,
    
    /// 整数字段过滤
    pub age: Option<OpValsInt64>,
    pub status: Option<OpValsInt64>,
}
```

### 4.2 过滤操作符

#### 字符串OpValString运算符
| 操作符 | 含义 | 示例 |
|--------|------|------|
| `$eq` | 与一个值精确匹配 | `{ "name": { "$eq": "Jon Doe" } }` 等同于 `{ "name": "Jon Doe" }` |
| `$in` | 与值列表中的任意一项完全匹配（逻辑 OR） | `{ "name": { "$in": ["Alice", "Jon Doe"] } }` |
| `$not` | 排除精确匹配的值 | `{ "name": { "$not": "Jon Doe" } }` |
| `$notIn` | 排除列表中的任意一项 | `{ "name": { "$notIn": ["Jon Doe"] } }` |
| `$contains` | 字符串包含子串（区分大小写） | `{ "name": { "$contains": "Doe" } }` |
| `$containsAny` | 字符串包含列表中任意子串 | `{ "name": { "$containsAny": ["Doe", "Ali"] } }` |
| `$containsAll` | 字符串包含列表中所有子串 | `{ "name": { "$containsAll": ["Hello", "World"] } }` |
| `$notContains` | 字符串不包含子串 | `{ "name": { "$notContains": "Doe" } }` |
| `$notContainsAny` | 字符串不包含列表中任意子串 | `{ "name": { "$notContainsAny": ["Doe", "Ali"] } }` |
| `$startsWith` | 字符串以指定前缀开头（区分大小写） | `{ "name": { "$startsWith": "Jon" } }` |
| `$startsWithAny` | 字符串以列表中任意前缀开头 | `{ "name": { "$startsWithAny": ["Jon", "Al"] } }` |
| `$notStartsWith` | 字符串不以指定前缀开头 | `{ "name": { "$notStartsWith": "Jon" } }` |
| `$notStartsWithAny` | 字符串不以列表中任意前缀开头 | `{ "name": { "$notStartsWithAny": ["Jon", "Al"] } }` |
| `$endsWith` | 字符串以指定后缀结尾（区分大小写） | `{ "name": { "$endsWith": "Doe" } }` |
| `$endsWithAny` | 字符串以列表中任意后缀结尾 | `{ "name": { "$endsWithAny": ["Doe", "ice"] } }` |
| `$notEndsWith` | 字符串不以指定后缀结尾 | `{ "name": { "$notEndsWith": "Doe" } }` |
| `$notEndsWithAny` | 字符串不以列表中任意后缀结尾 | `{ "name": { "$notEndsWithAny": ["Doe", "ice"] } }` |
| `$lt` | 字典序小于 | `{ "name": { "$lt": "C" } }` |
| `$lte` | 字典序小于或等于 | `{ "name": { "$lte": "C" } }` |
| `$gt` | 字典序大于 | `{ "name": { "$gt": "J" } }` |
| `$gte` | 字典序大于或等于 | `{ "name": { "$gte": "J" } }` |
| `$null` | 值为 `null` | `{ "name": { "$null": true } }` |
| `$containsCi` | 字符串包含子串（不区分大小写） | `{ "name": { "$containsCi": "doe" } }` |
| `$notContainsCi` | 字符串不包含子串（不区分大小写） | `{ "name": { "$notContainsCi": "doe" } }` |
| `$startsWithCi` | 字符串以指定前缀开头（不区分大小写） | `{ "name": { "$startsWithCi": "jon" } }` |
| `$notStartsWithCi` | 字符串不以指定前缀开头（不区分大小写） | `{ "name": { "$notStartsWithCi": "jon" } }` |
| `$endsWithCi` | 字符串以指定后缀结尾（不区分大小写） | `{ "name": { "$endsWithCi": "doe" } }` |
| `$notEndsWithCi` | 字符串不以指定后缀结尾（不区分大小写） | `{ "name": { "$notEndsWithCi": "doe" } }` |
| `$ilike` | 类似 SQL `ILIKE`，不区分大小写模糊匹配（需启用 `with-ilike` feature） | `{ "name": { "$ilike": "DoE" } }` |

💡 注意：$ilike 通常需要在 Cargo.toml 中启用对应特性，例如：
```toml
[dependencies]
your-orm = { version = "...", features = ["with-ilike"] }
```

#### 数字操作符(OpValInt32, OpValInt64, OpValFloat64）

| 操作符 | 含义 | 示例 |
|--------|------|------|
| `$eq` | 与一个值精确匹配 | `{ "age": { "$eq": 24 } }` 等同于 `{ "age": 24 }` |
| `$in` | 与值列表中的任意一项完全匹配 | `{ "age": { "$in": [23, 24] } }` |
| `$not` | 排除精确匹配的值 | `{ "age": { "$not": 24 } }` |
| `$notIn` | 排除列表中的任意一项 | `{ "age": { "$notIn": [24] } }` |
| `$lt` | 小于 | `{ "age": { "$lt": 30 } }` |
| `$lte` | 小于或等于 | `{ "age": { "$lte": 30 } }` |
| `$gt` | 大于 | `{ "age": { "$gt": 30 } }` |
| `$gte` | 大于或等于 | `{ "age": { "$gte": 30 } }` |
| `$null` | 值为 `null` | `{ "name": { "$null": true } }` |

#### 布尔操作符（OpValBool）

| 操作符 | 含义 | 示例 |
|--------|------|------|
| `$eq` | 与一个值精确匹配 | `{ "dev": { "$eq": true } }` 等同于 `{ "dev": true }` |
| `$not` | 排除精确匹配的值 | `{ "dev": { "$not": false } }` |
| `$null` | 值为 `null` | `{ "name": { "$null": true } }` |



### 4.3 API 调用示例

```bash
# 单条件过滤
POST /api/users/list
{
    "filter": {
        "name": {"$contains": "张"}
    }
}

# 多条件过滤（AND）
POST /api/users/list
{
    "filter": {
        "name": {"$contains": "张"},
        "age": {"$gte": 18}
    }
}

# 组合过滤
POST /api/users/list
{
    "filter": {
        "status": {"$in": [1, 2]},
        "name": {"$starts": "张"}
    }
}
```

---

## 5. CRUD 服务

### 5.1 GenericCrudService 方法

```rust
use cmx_api::GenericCrudService;

// 创建
let dataset = GenericCrudService::<UserBmc>::create(
    &mm, db_id, data
).await?;

// 获取
let dataset = GenericCrudService::<UserBmc>::get(
    &mm, db_id, id.into()
).await?;

// 更新
let dataset = GenericCrudService::<UserBmc>::update(
    &mm, db_id, id.into(), data
).await?;

// 删除
let dataset = GenericCrudService::<UserBmc>::delete(
    &mm, db_id, id.into()
).await?;

// 列表查询（带过滤）
let dataset = GenericCrudService::<UserBmc, UserFilter>::list(
    &mm, db_id, Some(filter), Some(list_options)
).await?;

// 分页查询
let (dataset, total) = GenericCrudService::<UserBmc, UserFilter>::page(
    &mm, db_id, Some(filter), list_options
).await?;

// 统计数量
let count = GenericCrudService::<UserBmc, UserFilter>::count(
    &mm, db_id, Some(filter)
).await?;
```

### 5.2 ListOptions

```rust
use modql::filter::ListOptions;

let list_options = ListOptions {
    limit: Some(20),           // 限制数量
    offset: Some(0),           // 偏移量
    order_bys: Some("name".into()),  // 排序
};

// 多字段排序
let list_options = ListOptions {
    order_bys: Some("-created_at,name".into()),  // 先按创建时间降序，再按名称升序
    ..Default::default()
};
```

---

## 6. Handler 函数

### 6.1 标准 Handler

```rust
use axum::{extract::State, Json};
use cmx_api::{ApiResp, Result};
use cmx_database::DatabaseManager;

/// 创建 Handler
pub async fn create<MC>(
    State(mm): State<DatabaseManager>,
    Json(data): Json<serde_json::Value>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    let dataset = GenericCrudService::<MC>::create(&mm, "default", data).await?;
    Ok(Json(ApiResp::ok(dataset)))
}
```

### 6.2 自定义 Handler

```rust
/// 按名称查询
pub async fn get_by_name(
    State(mm): State<DatabaseManager>,
    Json(params): Json<GetByNameParams>,
) -> Result<Json<ApiResp<DataSet>>> {
    let filter = UserFilter {
        name: Some(OpValsString(vec![
            OpValString::Eq(params.name)
        ])),
        ..Default::default()
    };
    
    let dataset = GenericCrudService::<UserBmc, UserFilter>::list(
        &mm, params.get_db_id(), Some(filter), None
    ).await?;
    
    Ok(Json(ApiResp::ok(dataset)))
}
```

---

## 7. 路由注册

### 7.1 使用宏注册

```rust
use cmx_api::register_crud_routes;

let router = Router::new().with_state(mm);

// 注册标准 CRUD 路由
let router = register_crud_routes!(router, UserBmc, UserFilter, "/api/users");
```

### 7.2 注册的接口

| 方法 | 路径 | 说明 |
|-----|------|------|
| POST | `/api/users/create` | 创建 |
| GET | `/api/users/get?id=xxx` | 获取 |
| POST | `/api/users/update` | 更新 |
| GET | `/api/users/delete?id=xxx` | 删除 |
| POST | `/api/users/list` | 列表 |
| POST | `/api/users/page` | 分页 |

### 7.3 组合自定义路由

```rust
let router = register_crud_routes!(router, UserBmc, UserFilter, "/api/users");

// 添加自定义路由
router
    .route("/api/users/by-name", axum::routing::post(get_by_name))
    .route("/api/users/batch-create", axum::routing::post(batch_create))
```

---

## 8. 多数据库支持

### 8.1 概述

cmx-api 支持通过 `db_id` 参数指定操作哪个数据库，适用于多租户场景。

### 8.2 使用方式

#### GET 请求

```bash
GET /api/users/get?id=123&db_id=tenant1
```

#### POST 请求（body 参数）

```bash
POST /api/users/create
{
    "name": "张三",
    "email": "zhangsan@example.com",
    "db_id": "tenant1"
}
```

#### 分页查询

```bash
POST /api/users/page
{
    "filter": {"name": {"$contains": "张"}},
    "offset": 0,
    "limit": 20,
    "db_id": "tenant1"
}
```

### 8.3 默认值

如果未指定 `db_id`，默认使用 `"default"`。

---

## 9. 自定义扩展

### 9.1 扩展模式

#### 模式一：继承扩展

```rust
pub struct UserService;

impl UserService {
    /// 扩展方法
    pub async fn get_by_email(
        mm: &DatabaseManager,
        db_id: &str,
        email: &str,
    ) -> Result<DataSet> {
        let filter = UserFilter {
            email: Some(OpValsString(vec![
                OpValString::Eq(email.to_string())
            ])),
            ..Default::default()
        };
        
        GenericCrudService::<UserBmc, UserFilter>::list(
            mm, db_id, Some(filter), None
        ).await
    }
    
    /// 覆盖方法
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: Value,
    ) -> Result<DataSet> {
        // 添加验证逻辑
        if data.get("email").is_none() {
            return Err(Error::bad_request("缺少 email"));
        }
        
        // 调用父类方法
        GenericCrudService::<UserBmc>::create(mm, db_id, data).await
    }
}
```

#### 模式二：组合模式

```rust
pub struct OrderWithUserService;

impl OrderWithUserService {
    pub async fn get_order_with_user(
        mm: &DatabaseManager,
        db_id: &str,
        order_id: &str,
    ) -> Result<Value> {
        let order = GenericCrudService::<OrderBmc>::get(
            mm, db_id, order_id.into()
        ).await?;
        
        let user_id = order.iter()
            .next()
            .and_then(|row| row.get("user_id"))
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            });
        
        let user = if let Some(user_id) = user_id {
            Some(GenericCrudService::<UserBmc>::get(
                mm, db_id, user_id.into()
            ).await?)
        } else {
            None
        };
        
        Ok(json!({
            "order": order,
            "user": user
        }))
    }
}
```

### 9.2 推荐目录结构

```
your-app/
├── src/
│   ├── model/
│   │   └── user/
│   │       ├── mod.rs
│   │       ├── bmc.rs        # DbBmc 实现
│   │       ├── filter.rs     # 过滤器
│   │       ├── service.rs    # 自定义 Service
│   │       └── handler.rs    # 自定义 Handler
│   └── api/
│       └── routes.rs         # 路由注册
```

---

## 10. 最佳实践

### 10.1 错误处理

```rust
use cmx_api::{Error, Result};

pub async fn custom_operation() -> Result<()> {
    // 参数验证
    if invalid_input {
        return Err(Error::bad_request("参数错误"));
    }
    
    // 业务逻辑
    database_operation()
        .map_err(|e| Error::internal_error(format!("操作失败: {}", e)))?;
    
    Ok(())
}
```

### 10.2 日志记录

```rust
use tracing::{debug, info, warn};

pub async fn create_user(
    mm: &DatabaseManager,
    db_id: &str,
    data: Value,
) -> Result<DataSet> {
    info!("创建用户开始");
    debug!("用户数据: {:?}", data);
    
    let result = GenericCrudService::<UserBmc>::create(mm, db_id, data).await;
    
    match &result {
        Ok(dataset) => info!("用户创建成功"),
        Err(e) => warn!("用户创建失败: {}", e),
    }
    
    result
}
```

### 10.3 时间戳字段

确保数据库表有以下字段（如果 `has_timestamps()` 返回 true）：

```sql
CREATE TABLE users (
    code VARCHAR(36) PRIMARY KEY,
    name VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(36),
    updated_by VARCHAR(36)
);
```

### 10.4 命名约定

| 组件 | 命名 | 示例 |
|-----|------|------|
| 实体 | 名词 | `User` |
| DbBmc | 实体 + Bmc | `UserBmc` |
| Filter | 实体 + Filter | `UserFilter` |
| Service | 实体 + Service | `UserService` |
| Handler | 动作 | `get_by_name`, `batch_create` |

---

## 附录

### A. 完整示例

参见 `examples/custom-crud/` 目录。

### B. 相关文档

- [自定义CRUD扩展机制设计.md](../../.trae/documents/自定义CRUD扩展机制设计.md)
- [cmx-api模块检查报告.md](../../.trae/documents/cmx-api模块检查报告.md)

### C. API 参考

参见源码文档注释。
