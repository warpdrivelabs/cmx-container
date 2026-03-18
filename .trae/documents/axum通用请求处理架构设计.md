# Axum 通用请求处理架构设计文档

## 1. 设计理念

**核心目标**：开发一个通用 CRUD 框架，参考 example/lib-core、example/lib-rest-core、example/lib-web 的成熟模式，构建支持 GET/POST 两种方法的通用 CRUD 架构，SQL 执行通过 cmx-database 封装 API 实现，返回结果使用 cmx-core 模块封装的 DataSet。

**代码位置**：cmx-api 模块

**使用场景**：用户使用该框架时，只需定义模型结构和 Filter，系统自动生成 CRUD 接口。

**重要决策**：
- 数据库执行方式：完全使用 cmx-database 的封装 API
- 主键类型：固定为 varchar（String）类型
- 认证权限：暂不考虑认证和权限
- 字段选择：返回所有字段
- 路由注册：提供宏来简化路由注册
- 时间戳审计：参考复用 example/lib-core 中的 prep_fields_for_create 和 prep_fields_for_update

---

## 2. 接口设计

### 2.1 只使用 GET 和 POST 方法

| 方法 | 路径 | 说明 | 参数位置 |
|------|------|------|----------|
| POST | `/api/{resource}/create` | 创建 | body (JSON) |
| GET | `/api/{resource}/get` | 获取单条 | 查询参数 ?id=xxx |
| POST | `/api/{resource}/update` | 更新 | body (JSON，包含 id) |
| GET | `/api/{resource}/delete` | 删除 | 查询参数 ?id=xxx |
| POST | `/api/{resource}/list` | 列表查询 | body (JSON) |
| POST | `/api/{resource}/page` | 分页查询 | body (JSON) |

---

## 3. 核心技术栈

| 组件 | 作用 | 版本/参考 |
|------|------|----------|
| `sea-query` | SQL 构建 | - |
| `modql` | FilterNodes + Fields | 0.4.1 |
| `DataSet` | 返回结果 | cmx-core |
| `RestResponse` | 响应包装 | cmx-core |
| `DbBmc` trait | 表元信息 | example/lib-core |
| `SVRContext` | 上下文 | cmx-core 模块 |
| `cmx-database` | SQL 执行 | 封装 API |
| `cmx-api` | 代码放置位置 | 你的模块 |
| Handler 风格 | 请求处理 | example/lib-web |

---

## 4. 示例表结构：cmx_domain

作为框架使用的示例表：

```sql
CREATE TABLE public.cmx_domain (
    code        varchar(64)  NOT NULL PRIMARY KEY,
    name        varchar(200) NOT NULL,
    description text,
    type        varchar(50),
    tags        text,
    sort_order  integer   DEFAULT 0,
    status      integer   DEFAULT 1,
    archived    integer   DEFAULT 0,
    created_at  timestamp DEFAULT CURRENT_TIMESTAMP,
    updated_at  timestamp DEFAULT CURRENT_TIMESTAMP,
    created_by  varchar(100),
    create_name varchar(100),
    updated_by  varchar(100),
    update_name varchar(100)
);
```

---

## 5. 参考代码风格

### 5.1 lib-web Handler 风格（参考 example/lib-web）

```rust
// 参考 example/lib-web/src/handlers/handlers_user.rs

use axum::extract::State;
use axum::Json;

/// Handler 签名风格
pub async fn create_handler(
    State(mm): State&lt;ModelManager&gt;,      // 状态提取
    Json(payload): Json&lt;CreatePayload&gt;,   // JSON 参数
) -&gt; Result&lt;RestResponse&lt;DataSet&gt;&gt; {      // 返回 RestResponse&lt;DataSet&gt;
    // 业务逻辑
    Ok(RestResponse::from(dataset))
}
```

### 5.2 lib-core DbBmc 风格（参考 example/lib-core）

```rust
// 参考 example/lib-core 的 DbBmc trait 和 crud_fns

pub trait DbBmc {
    const TABLE: &amp;'static str;
    const PK_COLUMN: &amp;'static str;  // 主键列名，默认为 "code"
    fn table_ref() -&gt; TableRef { ... }
    fn has_timestamps() -&gt; bool { true }
    fn has_owner_id() -&gt; bool { false }
}

// 具体实现
pub struct DomainBmc;
impl DbBmc for DomainBmc {
    const TABLE: &amp;'static str = "cmx_domain";
    const PK_COLUMN: &amp;'static str = "code";
}

// 使用方式
let id = DomainBmc::create(&amp;ctx, &amp;mm, data).await?;
let domain: Domain = DomainBmc::get(&amp;ctx, &amp;mm, code).await?;
DomainBmc::update(&amp;ctx, &amp;mm, code, data).await?;
DomainBmc::delete(&amp;ctx, &amp;mm, code).await?;
```

---

## 6. DataSet 结构（cmx-core）

```rust
// DataSet 结构
pub struct DataSet {
    pub id: String,           // 数据集标识
    pub schema: Schema,       // 字段定义
    pub rows: Vec&lt;Row&gt;,       // 数据行
}

// Schema 结构
pub struct Schema {
    pub id: String,
    pub fields: Vec&lt;Field&gt;,
}

// Field 结构
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub label: String,
}
```

---

## 7. 接口详细设计

### 7.1 创建

```
POST /api/domain
Content-Type: application/json

{ "code": "FIN", "name": "财务域", "type": "business" }

响应:
{
    "success": true,
    "data": [
        { "code": "FIN", "name": "财务域", "type": "business" }
    ]
}
```

### 7.2 获取单条（GET + 查询参数）

```
GET /api/domain/get?id=FIN

响应:
{
    "success": true,
    "data": [
        { "code": "FIN", "name": "财务域", "type": "business", "description": "..." }
    ]
}
```

### 7.3 更新（POST + body，包含 id）

```
POST /api/domain/update
Content-Type: application/json

{ "id": "FIN", "name": "财务域（已更新）", "description": "新描述" }

响应:
{
    "success": true,
    "data": [
        { "code": "FIN", "name": "财务域（已更新）", "description": "新描述" }
    ]
}
```

### 7.4 删除（GET + 查询参数）

```
GET /api/domain/delete?id=FIN

响应:
{
    "success": true,
    "data": [
        { "deleted_code": "FIN" }
    ]
}
```

### 7.5 列表查询（POST + JSON）

```
POST /api/domain/list
Content-Type: application/json

{
    "filter": { "type": "business" },
    "offset": 0,
    "limit": 10,
    "order_bys": "sort_order"
}

响应:
{
    "success": true,
    "data": [...]
}
```

### 7.6 分页查询（POST + JSON）

```
POST /api/domain/page
Content-Type: application/json

{
    "filter": { "status": 1, "archived": 0 },
    "offset": 0,
    "limit": 10,
    "order_bys": "-sort_order"
}

响应:
{
    "success": true,
    "data": [...],
    "page_info": { "total": 100, "page_size": 10, "page_number": 1 }
}
```

---

## 8. 核心技术参考（modql 0.4.1）

### 8.1 模型定义

```rust
/// 领域实体（使用 modql::field::Fields）
#[derive(Debug, Clone, modql::field::Fields, FromRow, Serialize)]
pub struct Domain {
    pub code: String,
    pub name: String,
    pub description: Option&lt;String&gt;,
    pub r#type: Option&lt;String&gt;,
    pub tags: Option&lt;String&gt;,
    pub sort_order: Option&lt;i32&gt;,
    pub status: Option&lt;i32&gt;,
    pub archived: Option&lt;i32&gt;,
    pub created_at: Option&lt;NaiveDateTime&gt;,
    pub updated_at: Option&lt;NaiveDateTime&gt;,
    pub created_by: Option&lt;String&gt;,
    pub create_name: Option&lt;String&gt;,
    pub updated_by: Option&lt;String&gt;,
    pub update_name: Option&lt;String&gt;,
}

/// 查询过滤器（使用 modql::filter::FilterNodes）
#[derive(modql::filter::FilterNodes, Deserialize, Default, Debug)]
pub struct DomainFilter {
    pub code: Option&lt;OpValsString&gt;,
    pub name: Option&lt;OpValsString&gt;,
    pub r#type: Option&lt;OpValsString&gt;,
    pub status: Option&lt;OpValsInt64&gt;,
    pub archived: Option&lt;OpValsInt64&gt;,
}
```

### 8.2 构建查询

```rust
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Condition, PostgresQueryBuilder};

// 构建查询
let cond: Condition = filter.try_into()?;
let mut query = sea_query::Query::select();
query.from(table).columns(Domain::sea_column_refs());
query.cond_where(cond);
list_options.apply_to_sea_query(&amp;mut query);

// 获取 SQL
let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
```

---

## 9. 目录结构

```
crates/libs/cmx-api/src/
├── lib.rs
├── error.rs
├── response.rs
│
├── rest/                    # REST 协议层
│   ├── mod.rs
│   ├── params.rs            # 参数解析
│   └── handler.rs           # 请求处理（参考 lib-web）
│
├── crud/                    # 通用 CRUD 核心
│   ├── mod.rs
│   ├── traits.rs            # DbBmc trait（参考 lib-core）
│   ├── macros.rs           # 声明宏 - 用于简化路由注册
│   ├── utils.rs            # 工具函数 - prep_fields_for_create/prep_fields_for_update
│   └── service.rs          # GenericCrudService
│
└── models/                  # 业务模型
    ├── mod.rs
    └── domain.rs           # Domain + DomainBmc + DomainFilter
```

---

## 10. 核心组件

### 10.1 DbBmc trait（参考 lib-core）

```rust
/// 表元信息 trait
pub trait DbBmc {
    const TABLE: &amp;'static str;
    const PK_COLUMN: &amp;'static str = "code";  // 主键列名，默认 "code"
    fn table_ref() -&gt; TableRef { ... }
    fn has_timestamps() -&gt; bool { true }
    fn has_owner_id() -&gt; bool { false }
}

/// 领域 Bmc
pub struct DomainBmc;
impl DbBmc for DomainBmc {
    const TABLE: &amp;'static str = "cmx_domain";
    const PK_COLUMN: &amp;'static str = "code";
}
```

### 10.2 GenericCrudService（返回 DataSet）

```rust
use cmx_core::model::data::dataset::DataSet;

/// 通用 CRUD 服务
pub struct GenericCrudService&lt;MC, F&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt;,
{
    _marker: PhantomData&lt;(MC, F)&gt;,
}

impl&lt;MC, F&gt; GenericCrudService&lt;MC, F&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + Default,
{
    /// 创建
    pub async fn create(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        data: serde_json::Value,
    ) -&gt; Result&lt;DataSet&gt; {
        // 使用 sea-query 构建 SQL
        let mut query = Query::insert();
        // ... 构建插入语句
        // 使用 prep_fields_for_create 处理时间戳和审计字段

        // 使用 cmx-database API 执行
        let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
        mm.execute_sql_with_params(db_id, None, &amp;sql, serde_json::json!(values)).await?;

        Ok(DataSet::from_values(...))
    }

    /// 获取单条（主键类型为 String）
    pub async fn get(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        id: String,  // 主键值，String 类型
    ) -&gt; Result&lt;DataSet&gt; {
        let mut query = Query::select();
        query.from(MC::table_ref()).columns(...);
        // 使用 MC::PK_COLUMN 作为主键列名

        let dataset = mm.query_sql_with_params(
            db_id, None, &amp;sql, serde_json::json!(values), "result"
        ).await?;

        Ok(dataset)
    }

    /// 更新
    pub async fn update(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        id: String,
        data: serde_json::Value,
    ) -&gt; Result&lt;DataSet&gt; {
        // 使用 sea-query 构建 SQL
        let mut query = Query::update();
        // ... 构建更新语句
        // 使用 prep_fields_for_update 处理时间戳和审计字段

        // 使用 cmx-database API 执行
        let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
        mm.execute_sql_with_params(db_id, None, &amp;sql, serde_json::json!(values)).await?;

        Ok(DataSet::from_values(...))
    }

    /// 删除
    pub async fn delete(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        id: String,
    ) -&gt; Result&lt;DataSet&gt; { ... }

    /// 列表查询
    pub async fn list(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        filter: Option&lt;F&gt;,
        options: Option&lt;ListOptions&gt;,
    ) -&gt; Result&lt;DataSet&gt; { ... }

    /// 分页查询
    pub async fn page(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        filter: Option&lt;F&gt;,
        options: ListOptions,
    ) -&gt; Result&lt;(DataSet, i64)&gt; { ... }

    /// 统计数量
    pub async fn count(
        ctx: &amp;SVRContext,
        mm: &amp;DatabaseManager,
        db_id: &amp;str,
        filter: Option&lt;F&gt;,
    ) -&gt; Result&lt;i64&gt; { ... }
}
```

### 10.3 Handler（参考 lib-web 风格）

```rust
use cmx_core::response::RestResponse;

/// 创建
pub async fn create&lt;MC, F&gt;(
    State(mm): State&lt;ModelManager&gt;,
    Json(data): Json&lt;serde_json::Value&gt;,
) -&gt; Result&lt;RestResponse&lt;DataSet&gt;&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + DeserializeOwned + Default,
{
    let ctx = Ctx::root_ctx();
    let dataset = GenericCrudService::&lt;MC, F&gt;::create(
        &amp;ctx, &amp;mm, "default", data
    ).await?;

    Ok(RestResponse::from(dataset))
}

/// 获取单条（GET + 查询参数，主键类型为 String）
pub async fn get_by_id&lt;MC, F&gt;(
    Query(params): Query&lt;GetParams&gt;,  // 查询参数 ?id=xxx
    State(mm): State&lt;ModelManager&gt;,
) -&gt; Result&lt;RestResponse&lt;DataSet&gt;&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + DeserializeOwned + Default,
{
    let ctx = Ctx::root_ctx();
    let dataset = GenericCrudService::&lt;MC, F&gt;::get(
        &amp;ctx, &amp;mm, "default", params.id
    ).await?;

    Ok(RestResponse::from(dataset))
}

/// 更新（POST + body，包含 id）
pub async fn update&lt;MC, F&gt;(
    State(mm): State&lt;ModelManager&gt;,
    Json(mut data): Json&lt;serde_json::Value&gt;,  // JSON body，包含 id 字段
) -&gt; Result&lt;RestResponse&lt;DataSet&gt;&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + DeserializeOwned + Default,
{
    let ctx = Ctx::root_ctx();
    // 从 body 中提取 id
    let id = data.get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::MissingField("id"))?
        .to_string();
    // 移除 body 中的 id 字段
    data.as_object_mut().map(|obj| obj.remove("id"));
    
    let dataset = GenericCrudService::&lt;MC, F&gt;::update(
        &amp;ctx, &amp;mm, "default", id, data
    ).await?;

    Ok(RestResponse::from(dataset))
}

/// 删除（GET + 查询参数）
pub async fn delete_by_id&lt;MC, F&gt;(
    Query(params): Query&lt;DeleteParams&gt;,  // 查询参数 ?id=xxx
    State(mm): State&lt;ModelManager&gt;,
) -&gt; Result&lt;RestResponse&lt;DataSet&gt;&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + DeserializeOwned + Default,
{
    let ctx = Ctx::root_ctx();
    let dataset = GenericCrudService::&lt;MC, F&gt;::delete(
        &amp;ctx, &amp;mm, "default", params.id
    ).await?;

    Ok(RestResponse::from(dataset))
}

/// 列表查询（POST + JSON body）
pub async fn list&lt;MC, F&gt;(
    State(mm): State&lt;ModelManager&gt;,
    Json(params): Json&lt;PageParams&lt;F&gt;&gt;,
) -&gt; Result&lt;RestResponse&lt;DataSet&gt;&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + DeserializeOwned + Default,
{
    let ctx = Ctx::root_ctx();
    let list_options = ListOptions {
        limit: params.limit,
        offset: params.offset,
        order_bys: params.order_bys.as_ref().map(|s| s.as_str().into()),
    };

    let dataset = GenericCrudService::&lt;MC, F&gt;::list(
        &amp;ctx, &amp;mm, "default", params.filter, Some(list_options)
    ).await?;

    Ok(RestResponse::from(dataset))
}

/// 分页查询（POST + JSON body）
pub async fn page&lt;MC, F&gt;(
    State(mm): State&lt;ModelManager&gt;,
    Json(params): Json&lt;PageParams&lt;F&gt;&gt;,
) -&gt; Result&lt;RestPagedResponse&lt;DataSet&gt;&gt;
where
    MC: DbBmc,
    F: TryInto&lt;FilterGroups&gt; + DeserializeOwned + Default,
{
    let ctx = Ctx::root_ctx();
    let list_options = ListOptions {
        limit: params.limit.or(Some(20)),
        offset: params.offset,
        order_bys: params.order_bys.as_ref().map(|s| s.as_str().into()),
    };

    let (dataset, total) = GenericCrudService::&lt;MC, F&gt;::page(
        &amp;ctx, &amp;mm, "default", params.filter, list_options
    ).await?;

    Ok(RestPagedResponse::new(dataset, total, params.get_limit(), 1))
}
```

---

## 11. 路由注册（含宏简化）

### 11.1 手动注册方式

```rust
// cmx-api 模块中（参考 lib-web 风格）
pub fn routes(mm: ModelManager) -&gt; Router {
    Router::new()
        .with_state(mm)
        // 创建
        .route("/api/domain/create", post(create::&lt;DomainBmc, DomainFilter&gt;))
        // 获取单条（GET + 查询参数 ?id=xxx）
        .route("/api/domain/get", get(get_by_id::&lt;DomainBmc, DomainFilter&gt;))
        // 更新（POST + body，包含 id）
        .route("/api/domain/update", post(update::&lt;DomainBmc, DomainFilter&gt;))
        // 删除（GET + 查询参数 ?id=xxx）
        .route("/api/domain/delete", get(delete_by_id::&lt;DomainBmc, DomainFilter&gt;))
        // 列表查询（POST + JSON body）
        .route("/api/domain/list", post(list::&lt;DomainBmc, DomainFilter&gt;))
        // 分页查询（POST + JSON body）
        .route("/api/domain/page", post(page::&lt;DomainBmc, DomainFilter&gt;))
}
```

### 11.2 使用宏简化注册（推荐）

```rust
// 使用宏一次性注册所有 CRUD 路由
pub fn routes(mm: ModelManager) -&gt; Router {
    let mut router = Router::new().with_state(mm);

    // 使用宏注册 domain 的所有 CRUD 路由
    register_crud_routes!(router, DomainBmc, DomainFilter, "/api/domain");

    router
}
```

### 11.3 宏定义（macros.rs）

```rust
/// 注册 CRUD 路由的宏
///
/// # 参数
/// * `router` - Router 实例
/// * `bmc` - Bmc 类型（如 DomainBmc）
/// * `filter` - Filter 类型（如 DomainFilter）
/// * `path` - 路径前缀（如 "/api/domain"）
#[macro_export]
macro_rules! register_crud_routes {
    ($router:expr, $bmc:ty, $filter:ty, $path:expr) =&gt; {
        $router = $router
            .route(concat!($path, "/create"), post(create::&lt;$bmc, $filter&gt;))
            .route(concat!($path, "/get"), get(get_by_id::&lt;$bmc, $filter&gt;))
            .route(concat!($path, "/update"), post(update::&lt;$bmc, $filter&gt;))
            .route(concat!($path, "/delete"), get(delete_by_id::&lt;$bmc, $filter&gt;))
            .route(concat!($path, "/list"), post(list::&lt;$bmc, $filter&gt;))
            .route(concat!($path, "/page"), post(page::&lt;$bmc, $filter&gt;));
    };
}
```

---

## 12. 框架使用示例

用户使用该框架开发时，只需：

```rust
// 1. 定义 Bmc
pub struct DomainBmc;
impl DbBmc for DomainBmc {
    const TABLE: &amp;'static str = "cmx_domain";
    const PK_COLUMN: &amp;'static str = "code";
}

// 2. 注册路由（使用宏，推荐）
let mut router = Router::new().with_state(mm);
register_crud_routes!(router, DomainBmc, DomainFilter, "/api/domain");

// 或者手动注册
Router::new()
    .route("/api/domain/create", post(create::&lt;DomainBmc, DomainFilter&gt;))
    .route("/api/domain/get", get(get_by_id::&lt;DomainBmc, DomainFilter&gt;))
    .route("/api/domain/update", post(update::&lt;DomainBmc, DomainFilter&gt;))
    .route("/api/domain/delete", get(delete_by_id::&lt;DomainBmc, DomainFilter&gt;))
    .route("/api/domain/list", post(list::&lt;DomainBmc, DomainFilter&gt;))
    .route("/api/domain/page", post(page::&lt;DomainBmc, DomainFilter&gt;))
```

---

## 13. 总结

**框架特点**：

1. **代码位置**：放到 cmx-api 模块

2. **参考来源**：
   - lib-core: DbBmc trait、crud_fns、prep_fields_for_create/prep_fields_for_update
   - lib-rest-core: 响应结构
   - lib-web: Handler 风格

3. **返回格式**：使用 cmx-core 的 RestResponse 包装 DataSet

4. **主键支持**：固定为 varchar（String）类型，通过 DbBmc::PK_COLUMN 配置列名

5. **接口设计**（6个接口）：
   - POST `/api/{resource}/create` - 创建（body JSON）
   - GET `/api/{resource}/get?id=xxx` - 获取单条（查询参数）
   - POST `/api/{resource}/update` - 更新（body JSON，包含 id）
   - GET `/api/{resource}/delete?id=xxx` - 删除（查询参数）
   - POST `/api/{resource}/list` - 列表查询（body JSON）
   - POST `/api/{resource}/page` - 分页查询（body JSON）

6. **Handler 风格**：参考 lib-web，使用 `State&lt;ModelManager&gt;`、`Json&lt;T&gt;`、`Query&lt;T&gt;`、`Result&lt;RestResponse&lt;DataSet&gt;&gt;`

7. **其他特性**：
   - 数据库执行：完全使用 cmx-database 封装 API
   - 认证权限：暂不考虑
   - 字段选择：返回所有字段
   - 路由注册：提供宏 `register_crud_routes!` 简化注册
   - 时间戳审计：复用 example/lib-core 的 prep_fields_for_create/prep_fields_for_update

**示例表**：cmx_domain（主键为 varchar 类型，列名为 code）
