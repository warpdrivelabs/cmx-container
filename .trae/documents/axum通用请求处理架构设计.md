# Entity 强类型数据参数设计方案

## 1. 问题背景

当前 `cmx-api/crud/service.rs` 中的 `create` 和 `update` 方法使用 `serde_json::Value` 作为 data 参数：

```rust
pub async fn create(
    mm: &DatabaseManager,
    db_id: &str,
    mut data: Value,  // 问题：JSON 无法正确表达时间类型
) -> Result<DataSet>
```

**问题**：

1. JSON 中时间类型只能表示为字符串，无法保证类型安全
2. 缺少字段元数据，无法自动获取字段列表构建 SQL
3. 无法在编译时检查字段类型
4. 不支持批量操作

## 2. 解决方案

使用 `modql::field::HasSeaFields` trait，将 data 参数改为强类型 Entity，直接构建 sea-query 表达式。

### 2.1 核心改动

```rust
// 改进前
pub async fn create(
    mm: &DatabaseManager,
    db_id: &str,
    data: Value,
) -> Result<DataSet>

// 改进后
pub async fn create<MC, E>(
    mm: &DatabaseManager,
    db_id: &str,
    data: E,
) -> Result<DataSet>
where
    MC: DbBmc,
    E: HasSeaFields,
```

### 2.2 modql::field::HasSeaFields 功能

`HasSeaFields` trait 提供以下能力：

```rust
pub trait HasSeaFields {
    /// 获取非 None 字段的 sea-query 表达式
    fn not_none_sea_fields() -> SeaFields;
    
    /// 获取列引用（用于 SELECT）
    fn sea_column_refs() -> Vec<ColumnRef>;
}

/// SeaFields 提供的方法
impl SeaFields {
    /// 返回 (columns, sea_values) 用于 INSERT
    pub fn for_sea_insert() -> (Vec<ColumnRef>, Vec<SimpleExpr>);
    
    /// 返回 fields 用于 UPDATE
    pub fn for_sea_update() -> Vec<(ColumnRef, SimpleExpr)>;
}
```

## 3. Entity 定义规范

### 3.1 统一 Entity 文件（合并 entity.rs 和 dto.rs）

所有实体元数据定义放在一个文件中：

```rust
// models/domain/entity.rs

use chrono::{DateTime, Utc};
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
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
    /// 创建时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    /// 创建者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// 创建者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_name: Option<String>,
    /// 更新者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    /// 更新者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_name: Option<String>,
}

/// 创建请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct DomainForCreate {
    /// 唯一标识码（主键）
    pub code: String,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
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

## 4. GenericCrudService 改造

### 4.1 create 方法（单个创建）

```rust
/// 创建单个实体
pub async fn create<MC, E>(
    mm: &DatabaseManager,
    db_id: &str,
    data: E,
) -> Result<DataSet>
where
    MC: DbBmc,
    E: HasSeaFields,
{
    info!("{:<12} - GenericCrudService::create - table: {}, db_id: {}", 
        "CRUD", MC::TABLE, db_id);

    let mut fields = data.not_none_sea_fields();
    prep_fields_for_create::<MC>(&mut fields, None);

    let (columns, sea_values) = fields.for_sea_insert();
    let mut query = Query::insert();
    query
        .into_table(MC::table_ref())
        .columns(columns)
        .values(sea_values)?
        .returning(Query::returning().columns([SIden(MC::PK_COLUMN)]));

    let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
    debug!("{:<12} - SQL: {}", "CRUD", sql);

    let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
        .await
        .map_err(|e| Error::internal_error(format!("创建失败 [{}]: {}", MC::TABLE, e)))?;

    info!("{:<12} - 创建成功, 影响行数: {}", "CRUD", rows_affected);

    Ok(DataSet::default())
}
```

### 4.2 create\_many 方法（批量创建）

```rust
/// 批量创建多个实体
pub async fn create_many<MC, E>(
    mm: &DatabaseManager,
    db_id: &str,
    data: Vec<E>,
) -> Result<DataSet>
where
    MC: DbBmc,
    E: HasSeaFields,
{
    info!("{:<12} - GenericCrudService::create_many - table: {}, count: {}", 
        "CRUD", MC::TABLE, data.len());

    if data.is_empty() {
        return Err(Error::bad_request("创建数据不能为空"));
    }

    let mut query = Query::insert();

    for item in data {
        let mut fields = item.not_none_sea_fields();
        prep_fields_for_create::<MC>(&mut fields, None);
        let (columns, sea_values) = fields.for_sea_insert();

        query
            .into_table(MC::table_ref())
            .columns(columns.clone())
            .values(sea_values)?;
    }

    query.returning(Query::returning().columns([SIden(MC::PK_COLUMN)]));

    let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
    debug!("{:<12} - SQL: {}", "CRUD", sql);

    let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
        .await
        .map_err(|e| Error::internal_error(format!("批量创建失败 [{}]: {}", MC::TABLE, e)))?;

    info!("{:<12} - 批量创建成功, 影响行数: {}", "CRUD", rows_affected);

    Ok(DataSet::default())
}
```

### 4.3 update 方法（单个更新）

```rust
/// 更新单个实体
pub async fn update<MC, E>(
    mm: &DatabaseManager,
    db_id: &str,
    id: Value,
    data: E,
) -> Result<DataSet>
where
    MC: DbBmc,
    E: HasSeaFields,
{
    info!("{:<12} - GenericCrudService::update - table: {}, id: {:?}", 
        "CRUD", MC::TABLE, id);

    let mut fields = data.not_none_sea_fields();
    prep_fields_for_update::<MC>(&mut fields, None);

    let fields = fields.for_sea_update();
    let mut query = Query::update();
    query
        .table(MC::table_ref())
        .values(fields)
        .and_where(Expr::col(SIden(MC::PK_COLUMN)).eq(json_value_to_sea_query(id.clone())));

    let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
    debug!("{:<12} - SQL: {}", "CRUD", sql);

    let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
        .await
        .map_err(|e| Error::internal_error(format!("更新失败 [{}]: {}", MC::TABLE, e)))?;

    info!("{:<12} - 更新成功, 影响行数: {}", "CRUD", rows_affected);

    Self::get(mm, db_id, id).await
}
```

### 4.4 update\_many 方法（批量更新）

```rust
/// 批量更新多个实体
pub async fn update_many<MC, E>(
    mm: &DatabaseManager,
    db_id: &str,
    data: Vec<UpdateItem<E>>,
) -> Result<DataSet>
where
    MC: DbBmc,
    E: HasSeaFields,
{
    info!("{:<12} - GenericCrudService::update_many - table: {}, count: {}", 
        "CRUD", MC::TABLE, data.len());

    if data.is_empty() {
        return Err(Error::bad_request("更新数据不能为空"));
    }

    let mut total_affected = 0u64;

    for item in data {
        let mut fields = item.data.not_none_sea_fields();
        prep_fields_for_update::<MC>(&mut fields, None);

        let fields = fields.for_sea_update();
        let mut query = Query::update();
        query
            .table(MC::table_ref())
            .values(fields)
            .and_where(Expr::col(SIden(MC::PK_COLUMN)).eq(json_value_to_sea_query(item.id.clone())));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        
        let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| Error::internal_error(format!("批量更新失败 [{}]: {}", MC::TABLE, e)))?;
        
        total_affected += rows_affected;
    }

    info!("{:<12} - 批量更新成功, 总影响行数: {}", "CRUD", total_affected);

    Ok(DataSet::default())
}

/// 更新项
#[derive(Debug, Deserialize)]
pub struct UpdateItem<E> {
    pub id: Value,
    pub data: E,
}
```

### 4.5 delete 方法（批量删除，支持单个）

```rust
/// 删除实体（支持单个和批量）
///
/// # 参数
/// * `ids` - 主键值列表（单个删除传一个元素即可）
pub async fn delete<MC>(
    mm: &DatabaseManager,
    db_id: &str,
    ids: Vec<Value>,
) -> Result<DataSet>
where
    MC: DbBmc,
{
    info!("{:<12} - GenericCrudService::delete - table: {}, count: {}", 
        "CRUD", MC::TABLE, ids.len());

    if ids.is_empty() {
        return Ok(DataSet::default());
    }

    let mut query = Query::delete();
    query
        .from_table(MC::table_ref())
        .and_where(Expr::col(SIden(MC::PK_COLUMN)).is_in(
            ids.iter().map(|v| json_value_to_sea_query(v.clone())).collect::<Vec<_>>()
        ));

    let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
    debug!("{:<12} - SQL: {}", "CRUD", sql);

    let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
        .await
        .map_err(|e| Error::internal_error(format!("删除失败 [{}]: {}", MC::TABLE, e)))?;

    info!("{:<12} - 删除成功, 影响行数: {}", "CRUD", rows_affected);

    Ok(DataSet::default())
}
```

## 5. Handler 改造

### 5.1 通用 Handler

```rust
use crate::crud::traits::DbBmc;
use crate::crud::service::GenericCrudService;
use crate::error::Result;
use crate::response::ApiResp;
use axum::Json;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use modql::field::HasSeaFields;
use serde::de::DeserializeOwned;

/// 创建单个实体 Handler
pub async fn create<MC, E>(
    Json(data): Json<E>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = "default".to_string();
    
    let dataset = GenericCrudService::<MC>::create(&mm, &db_id, data).await?;
    
    Ok(Json(ApiResp::ok(dataset)))
}

/// 批量创建实体 Handler
pub async fn create_many<MC, E>(
    Json(data): Json<Vec<E>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = "default".to_string();
    
    let dataset = GenericCrudService::<MC>::create_many(&mm, &db_id, data).await?;
    
    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新单个实体 Handler
pub async fn update<MC, E>(
    Json(payload): Json<UpdatePayload<E>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = "default".to_string();
    
    let dataset = GenericCrudService::<MC>::update(
        &mm, &db_id, payload.id, payload.data
    ).await?;
    
    Ok(Json(ApiResp::ok(dataset)))
}

/// 批量更新实体 Handler
pub async fn update_many<MC, E>(
    Json(data): Json<Vec<UpdateItem<E>>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    E: HasSeaFields + DeserializeOwned,
{
    let mm = get_default_db_manager();
    let db_id = "default".to_string();
    
    let dataset = GenericCrudService::<MC>::update_many(&mm, &db_id, data).await?;
    
    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除实体 Handler（支持单个和批量）
pub async fn delete<MC>(
    Json(payload): Json<DeletePayload>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    let mm = get_default_db_manager();
    let db_id = "default".to_string();
    
    let dataset = GenericCrudService::<MC>::delete(&mm, &db_id, payload.ids).await?;
    
    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新请求 Payload
#[derive(Debug, Deserialize)]
pub struct UpdatePayload<E> {
    pub id: serde_json::Value,
    pub data: E,
}

/// 更新项
#[derive(Debug, Deserialize)]
pub struct UpdateItem<E> {
    pub id: serde_json::Value,
    pub data: E,
}

/// 删除请求 Payload
#[derive(Debug, Deserialize)]
pub struct DeletePayload {
    /// 主键 ID 列表（单个删除传一个元素）
    pub ids: Vec<serde_json::Value>,
}
```

## 6. 接口设计

### 6.1 接口列表

| 方法   | 路径                            | 说明          | 参数位置              |
| ---- | ----------------------------- | ----------- | ----------------- |
| POST | `/api/{resource}/create`      | 创建单个        | body (JSON)       |
| POST | `/api/{resource}/create-many` | 批量创建        | body (JSON Array) |
| GET  | `/api/{resource}/get`         | 获取单条        | 查询参数 ?id=xxx      |
| POST | `/api/{resource}/update`      | 更新单个        | body (JSON)       |
| POST | `/api/{resource}/update-many` | 批量更新        | body (JSON Array) |
| POST | `/api/{resource}/delete`      | 删除（支持单个和批量） | body (JSON)       |
| POST | `/api/{resource}/list`        | 列表查询        | body (JSON)       |
| POST | `/api/{resource}/page`        | 分页查询        | body (JSON)       |

### 6.2 请求示例

**创建单个**

```json
POST /api/domains/create
{ "code": "FIN", "name": "财务域" }
```

**批量创建**

```json
POST /api/domains/create-many
[
    { "code": "FIN", "name": "财务域" },
    { "code": "HR", "name": "人力资源域" }
]
```

**更新单个**

```json
POST /api/domains/update
{ "id": "FIN", "data": { "name": "财务域（已更新）" } }
```

**批量更新**

```json
POST /api/domains/update-many
[
    { "id": "FIN", "data": { "name": "财务域（已更新）" } },
    { "id": "HR", "data": { "name": "人力资源域（已更新）" } }
]
```

**删除单个**

```json
POST /api/domains/delete
{ "ids": ["FIN"] }
```

**批量删除**

```json
POST /api/domains/delete
{ "ids": ["FIN", "HR", "IT"] }
```

## 7. 路由注册宏

### 7.1 宏定义（区分 ForCreate 和 ForUpdate）

```rust
/// 注册 CRUD 路由的宏
///
/// # 参数
/// * `$router` - Router 实例
/// * `$bmc` - Bmc 类型（如 DomainBmc）
/// * `$filter` - Filter 类型（如 DomainFilter）
/// * `$entity_create` - 创建 Entity 类型（如 DomainForCreate）
/// * `$entity_update` - 更新 Entity 类型（如 DomainForUpdate）
/// * `$path` - 路径前缀（如 "/domains"）
#[macro_export]
macro_rules! register_crud_routes {
    ($router:expr, $bmc:ty, $filter:ty, $entity_create:ty, $entity_update:ty, $path:expr) => {
        $router = $router
            // 创建操作（使用 ForCreate）
            .route(concat!($path, "/create"), post(create::<$bmc, $entity_create>))
            .route(concat!($path, "/create-many"), post(create_many::<$bmc, $entity_create>))
            // 查询操作
            .route(concat!($path, "/get"), get(get_by_id::<$bmc, $filter>))
            // 更新操作（使用 ForUpdate）
            .route(concat!($path, "/update"), post(update::<$bmc, $entity_update>))
            .route(concat!($path, "/update-many"), post(update_many::<$bmc, $entity_update>))
            // 删除操作（支持单个和批量）
            .route(concat!($path, "/delete"), post(delete::<$bmc>))
            // 列表和分页查询
            .route(concat!($path, "/list"), post(list::<$bmc, $filter>))
            .route(concat!($path, "/page"), post(page::<$bmc, $filter>));
    };
}
```

### 7.2 使用示例

```rust
// routes.rs
pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();
    
    // 注册 Domain CRUD 路由
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

## 8. prep\_fields\_for\_create/update 改造

### 8.1 改造后（基于 SeaFields）

```rust
use modql::field::SeaFields;
use sea_query::SimpleExpr;

/// 为创建操作准备字段
pub fn prep_fields_for_create<MC>(fields: &mut SeaFields, user_id: Option<&str>)
where
    MC: DbBmc,
{
    // 添加 owner_id
    if MC::has_owner_id() {
        if let Some(uid) = user_id {
            fields.push(("owner_id", SimpleExpr::Value(uid.into())));
        }
    }
    
    // 添加主键（如果不存在）
    if !fields.has_field(MC::PK_COLUMN) {
        let pk_value = snowflake_id_str();
        fields.push((MC::PK_COLUMN, SimpleExpr::Value(pk_value.into())));
    }
}

/// 为更新操作准备字段
pub fn prep_fields_for_update<MC>(fields: &mut SeaFields, user_id: Option<&str>)
where
    MC: DbBmc,
{
    // 更新操作通常不需要添加额外字段
    // 时间戳由数据库自动处理
}
```

## 9. 目录结构调整

```
crates/libs/cmx-api/src/
├── crud/
│   ├── mod.rs
│   ├── traits.rs            # DbBmc trait
│   ├── macros.rs            # 路由注册宏
│   ├── utils.rs             # prep_fields_for_create/update
│   └── service.rs           # GenericCrudService
│
├── rest/
│   ├── mod.rs
│   ├── params.rs            # 参数定义
│   └── handler.rs           # 通用 Handler
│
└── models/
    └── domain/
        ├── mod.rs
        ├── bmc.rs           # DomainBmc
        ├── entity.rs        # Domain, DomainForCreate, DomainForUpdate（合并）
        ├── filter.rs        # DomainFilter
        └── handler.rs       # 自定义 Handler
```

## 10. 实施步骤

### 阶段一：基础设施

1. 更新 Cargo.toml 确认 modql features
2. 合并 entity.rs 和 dto.rs，统一实体定义
3. 为 Entity 添加 `#[derive(modql::field::Fields)]`

### 阶段二：utils.rs 改造

1. 修改 `prep_fields_for_create` 支持 SeaFields
2. 修改 `prep_fields_for_update` 支持 SeaFields

### 阶段三：Service 改造

1. 修改 `GenericCrudService` 使用 `HasSeaFields`
2. 添加 `create_many` 批量创建方法
3. 添加 `update_many` 批量更新方法
4. 合并删除方法为单个 `delete`（支持批量）

### 阶段四：Handler 改造

1. 修改通用 Handler 支持 Entity 类型
2. 添加批量操作 Handler
3. 添加请求 Payload 结构体
4. 更新路由注册宏（区分 ForCreate/ForUpdate）

### 阶段五：测试验证

1. 编写单元测试
2. 验证时间类型正确序列化
3. 验证批量操作正确执行

## 11. 总结

| 改进项       | 改进前                 | 改进后                                   |
| --------- | ------------------- | ------------------------------------- |
| data 参数类型 | `serde_json::Value` | 强类型 Entity                            |
| 字段处理      | 手动遍历 Value          | `HasSeaFields::not_none_sea_fields()` |
| 时间类型      | JSON 字符串            | `DateTime<Utc>`                       |
| 类型安全      | 运行时检查               | 编译时检查                                 |
| 创建/更新区分   | 无                   | ForCreate / ForUpdate                 |
| 批量创建      | 不支持                 | `create_many`                         |
| 批量更新      | 不支持                 | `update_many`                         |
| 删除方法      | GET + Query         | POST + JSON Body（支持批量）                |
| Entity 文件 | entity.rs + dto.rs  | 合并为 entity.rs                         |

