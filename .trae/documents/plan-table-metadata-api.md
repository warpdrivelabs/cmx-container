# 表元数据 API 接口实现计划

## 目标

在 `cmx-api` 模块中为 `cmx_meta_table_define` 表提供**列表查询**和**分页查询** HTTP 接口，支持 filter 条件过滤。

## 技术分析

### 现有基础设施

| 组件 | 位置 | 状态 |
|------|------|------|
| `TableMetadataBmc` | `cmx-plugin::...::table_metadata::bmc` | ✅ 已实现 `DbBmc` |
| `TableMetadataFilter` | `cmx-plugin::...::table_metadata::filter` | ✅ 已实现 `FilterNodes`（支持 table_name/db_id/plugin_id/domain_code/application_code/module_code/archived 过滤） |
| `TableMetadataService::list/page` | `cmx-plugin::...::table_metadata::service` | ✅ 委托给 `GenericCrudService` |
| 通用 `rest::handler::list/page` | `cmx-api::rest::handler` | ✅ 泛型 handler，接受 `DbBmc + FilterNodes` |
| `cmx-api` 依赖 `cmx-plugin` | `Cargo.toml` | ✅ 已声明 |

### 关键结论

`TableMetadataService::list` 和 `page` 直接委托给 `GenericCrudService`，与通用 `rest::handler::list/page` 行为完全一致。因此无需自定义业务逻辑，直接使用通用泛型 handler 即可。

### 方案

由于只需 list/page 两个查询接口，不使用 `declare_crud_handlers!` 宏（会生成 8 个 handler），而是**手动注册两个路由**。为了支持 OpenAPI 文档，在 `crud_handlers.rs` 中定义两个带 `#[utoipa::path]` 注解的包装函数。

## 实施步骤

### 步骤 1：在 `crud_handlers.rs` 中添加表元数据的 list/page 包装函数

**文件**：`crates/libs/cmx-api/src/routes/crud_handlers.rs`

在文件末尾添加 `table_metadata_crud` 模块声明（使用 `declare_crud_handlers!` 宏），传入 `cmx-plugin` 中的类型：

```rust
declare_crud_handlers!(
    table_metadata_crud,
    cmx_plugin::infrastructure::database::table_metadata::TableMetadataDetail,
    cmx_plugin::infrastructure::database::table_metadata::TableMetadataBmc,
    cmx_plugin::infrastructure::database::table_metadata::TableMetadataForCreate,
    cmx_plugin::infrastructure::database::table_metadata::TableMetadataForUpdate,
    cmx_plugin::infrastructure::database::table_metadata::TableMetadataFilter,
    "TableMetadata",
    "/table-metadata"
);
```

> 宏会生成 8 个带 `#[utoipa::path]` 注解的 handler 函数。我们只注册 list 和 page 路由。

### 步骤 2：在 `routes.rs` 中注册 list/page 路由

**文件**：`crates/libs/cmx-api/src/routes/routes.rs`

在 `api_routes()` 函数中添加：

```rust
// 注册表元数据查询路由（仅 list 和 page）
let router = router
    .route("/table-metadata/list", post(crate::routes::crud_handlers::table_metadata_crud::list))
    .route("/table-metadata/page", post(crate::routes::crud_handlers::table_metadata_crud::page));
```

### 步骤 3：在 `openapi.rs` 中注册 OpenAPI paths

**文件**：`crates/libs/cmx-api/src/openapi.rs`

在 `#[openapi(paths(...))]` 中添加：

```rust
// TableMetadata handlers
crate::routes::crud_handlers::table_metadata_crud::list,
crate::routes::crud_handlers::table_metadata_crud::page,
```

在 `components(schemas(...))` 中添加 filter 的 schema（供 Swagger UI 展示 filter 结构）：

```rust
cmx_plugin::infrastructure::database::table_metadata::TableMetadataFilter,
```

## 涉及文件

| 操作 | 文件 | 说明 |
|------|------|------|
| 修改 | `crates/libs/cmx-api/src/routes/crud_handlers.rs` | 添加 `declare_crud_handlers!` 宏调用 |
| 修改 | `crates/libs/cmx-api/src/routes/routes.rs` | 注册 list/page 路由 |
| 修改 | `crates/libs/cmx-api/src/openapi.rs` | 注册 OpenAPI paths 和 schemas |

## API 接口

### POST /table-metadata/list

请求体：
```json
{
  "filter": {
    "table_name": { "eq": "cmx_xxx" },
    "plugin_id": { "eq": "my-plugin" },
    "db_id": { "eq": "target_db" }
  },
  "order_bys": "create_time.desc"
}
```

响应：`ApiResp<DataSet>`

### POST /table-metadata/page

请求体：
```json
{
  "filter": {
    "plugin_id": { "eq": "my-plugin" }
  },
  "current": 1,
  "size": 20
}
```

响应：`ApiResp<DataSet>` + pagination 信息

### 支持的过滤字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `table_name` | `OpValsString` | 表名（支持 eq/contains/startsWith/endsWith） |
| `db_id` | `OpValsString` | 数据库 ID |
| `plugin_id` | `OpValsString` | 插件 ID |
| `domain_code` | `OpValsString` | 域编码 |
| `application_code` | `OpValsString` | 应用编码 |
| `module_code` | `OpValsString` | 模块编码 |
| `archived` | `OpValsInt64` | 归档状态 |
