# 表元数据存储功能优化方案

## 一、现有问题分析

### 1.1 当前代码问题

1. **增删改操作未封装**：需要分别调用 `insert_or_update_version` 和 `upsert_metadata`，外部调用繁琐
2. **缺少列表查询和分页查询**：只有 `find_all_metadata` 方法，不支持分页
3. **不支持模糊查询**：`table_name` 只支持精确匹配
4. **详情查询不完整**：不支持通过主键 `id` 查询详情
5. **删除操作未封装**：需要分别删除两个表的记录
6. **未使用项目统一的 CRUD 模式**：未使用 `cmx_core` 的参数类型和 `modql`

### 1.2 需求总结

| 功能   | 描述                                                          |
| ---- | ----------------------------------------------------------- |
| 新增   | 封装两个表的插入操作，使用 `GenericCrudService` 模式                       |
| 更新   | 封装两个表的更新操作，使用 `UpdatePayload`                               |
| 删除   | 封装两个表的删除操作，使用 `DeletePayload`                               |
| 列表查询 | 只查询 `cmx_meta_table_define` 表，使用 `ListParams` 和 `modql` 过滤器 |
| 分页查询 | 只查询 `cmx_meta_table_define` 表，使用 `PageParams`               |
| 详情查询 | 联查两个表，使用 `GetParams` 或自定义条件                                 |

## 二、技术选型

### 2.1 使用项目现有组件

```rust
// 使用 cmx_core 的参数类型
use cmx_core::{
    DeletePayload,    // 删除参数
    GetParams,        // 单条查询参数
    ListParams,       // 列表查询参数
    PageParams,       // 分页查询参数
    UpdatePayload,    // 更新参数
};

// 使用 modql 实现过滤器
use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsInt64, ListOptions};

// 使用 GenericCrudService（位于 cmx-database 的 crud 模块）
use cmx_database::crud::{GenericCrudService, DbBmc};
use cmx_database::DatabaseManager;
```

## 三、数据结构设计

### 3.1 实体定义

```rust
/// 表元数据列表记录（用于列表和分页查询，不含 metadata）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadata {
    /// 主键ID
    pub id: String,
    /// 表名
    pub table_name: String,
    /// 数据库ID
    pub db_id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 当前版本
    pub version: String,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
    /// 归档标志
    pub archived: i32,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 创建人
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 表元数据详情（联查结果，包含 metadata）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataDetail {
    pub id: String,
    pub table_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    /// 元数据JSON（从版本表获取）
    pub metadata: serde_json::Value,
    pub archived: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 创建请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataForCreate {
    pub table_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub metadata: serde_json::Value,
}

/// 更新请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataForUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}
```

### 3.2 过滤器定义

```rust
/// 表元数据查询过滤器
///
/// 使用 modql 实现，支持多种操作符
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct TableMetadataFilter {
    /// 按表名过滤（支持模糊查询：Contains, StartsWith, EndsWith）
    pub table_name: Option<OpValsString>,
    /// 按数据库ID过滤
    pub db_id: Option<OpValsString>,
    /// 按插件ID过滤
    pub plugin_id: Option<OpValsString>,
    /// 按域编码过滤
    pub domain_code: Option<OpValsString>,
    /// 按应用编码过滤
    pub application_code: Option<OpValsString>,
    /// 按模块编码过滤
    pub module_code: Option<OpValsString>,
    /// 按归档状态过滤
    pub archived: Option<OpValsInt64>,
}
```

### 3.3 DbBmc 定义

```rust
/// cmx_meta_table_define 表的 Bmc
pub struct TableMetadataBmc;

impl DbBmc for TableMetadataBmc {
    const TABLE: &'static str = "cmx_meta_table_define";
    const PK_COLUMN: &'static str = "id";
    
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }
}

/// cmx_meta_table_define_version 表的 Bmc
pub struct TableMetadataVersionBmc;

impl DbBmc for TableMetadataVersionBmc {
    const TABLE: &'static str = "cmx_meta_table_define_version";
    const PK_COLUMN: &'static str = "id";
    
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }
}
```

## 四、API 设计

### 4.1 服务层设计

```rust
/// 表元数据服务
///
/// 封装两个表的增删改查操作
pub struct TableMetadataService;

impl TableMetadataService {
    // ==================== 创建操作 ====================
    
    /// 创建表元数据
    /// 同时写入 cmx_meta_table_define 和 cmx_meta_table_define_version
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: TableMetadataForCreate,
    ) -> Result<DataSet>;

    /// 批量创建表元数据
    pub async fn create_many(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<TableMetadataForCreate>,
    ) -> Result<DataSet>;

    // ==================== 查询操作 ====================
    
    /// 通过主键获取详情（联查版本表获取 metadata）
    pub async fn get(
        mm: &DatabaseManager,
        db_id: &str,
        params: GetParams,
    ) -> Result<DataSet>;

    /// 通过 table_name + db_id 获取详情
    pub async fn get_by_table_name(
        mm: &DatabaseManager,
        db_id: &str,
        table_name: &str,
        target_db_id: &str,
    ) -> Result<DataSet>;

    /// 列表查询（只查询主表）
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        params: ListParams<TableMetadataFilter>,
    ) -> Result<DataSet>;

    /// 分页查询（只查询主表）
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        params: PageParams<TableMetadataFilter>,
    ) -> Result<(DataSet, i64)>;

    // ==================== 更新操作 ====================
    
    /// 更新表元数据
    /// 同时更新两个表
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        payload: UpdatePayload<TableMetadataForUpdate>,
    ) -> Result<DataSet>;

    /// 批量更新
    pub async fn update_many(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<UpdatePayload<TableMetadataForUpdate>>,
    ) -> Result<DataSet>;

    // ==================== 删除操作 ====================
    
    /// 删除表元数据
    /// 同时删除两个表的记录
    pub async fn delete(
        mm: &DatabaseManager,
        db_id: &str,
        payload: DeletePayload,
    ) -> Result<DataSet>;

    // ==================== 版本历史查询 ====================
    
    /// 查询表的所有版本历史
    pub async fn list_versions(
        mm: &DatabaseManager,
        db_id: &str,
        table_name: &str,
        target_db_id: Option<&str>,
    ) -> Result<DataSet>;
}
```

## 五、实现步骤

### 步骤 1：定义数据结构

1. 定义 `TableMetadata` 实体（列表记录）
2. 定义 `TableMetadataDetail` 实体（详情记录）
3. 定义 `TableMetadataForCreate` 创建 DTO
4. 定义 `TableMetadataForUpdate` 更新 DTO
5. 定义 `TableMetadataFilter` 过滤器（使用 modql）
6. 定义 `TableMetadataBmc` 和 `TableMetadataVersionBmc`

### 步骤 2：实现创建操作

1. 实现 `create` 方法：

   * 生成主键 ID

   * 插入 `cmx_meta_table_define_version` 记录

   * 插入 `cmx_meta_table_define` 记录

   * 返回创建的详情记录

2. 实现 `create_many` 方法：批量创建

### 步骤 3：实现查询操作

1. 实现 `get` 方法：

   * 通过主键 ID 查询 `cmx_meta_table_define`

   * 联查 `cmx_meta_table_define_version` 获取 metadata

   * 返回详情记录

2. 实现 `get_by_table_name` 方法：

   * 通过 table\_name + db\_id 查询

3. 实现 `list` 方法：

   * 使用 `GenericCrudService::list`

   * 只查询 `cmx_meta_table_define` 表

   * 支持过滤器

4. 实现 `page` 方法：

   * 使用 `GenericCrudService::page`

   * 返回分页结果

### 步骤 4：实现更新操作

1. 实现 `update` 方法：

   * 更新 `cmx_meta_table_define` 记录

   * 更新或插入 `cmx_meta_table_define_version` 记录

   * 返回更新后的详情记录

### 步骤 5：实现删除操作

1. 实现 `delete` 方法：

   * 先查询要删除的记录（获取 table\_name, db\_id）

   * 删除 `cmx_meta_table_define_version` 记录

   * 删除 `cmx_meta_table_define` 记录

### 步骤 6：实现版本历史查询

1. 实现 `list_versions` 方法：

   * 查询指定表的所有版本历史

## 六、SQL 示例

### 6.1 详情查询（联查）

```sql
SELECT t.id, t.table_name, t.db_id, t.plugin_id, t.version,
       t.domain_code, t.application_code, t.module_code,
       v.metadata,
       t.archived, t.create_time, t.update_time,
       t.create_by, t.create_name, t.update_by, t.update_name
FROM cmx_meta_table_define t
LEFT JOIN cmx_meta_table_define_version v
  ON t.table_name = v.table_name
  AND t.version = v.version
  AND t.db_id = v.db_id
WHERE t.id = $id
```

### 6.2 列表查询（只查主表）

```sql
SELECT id, table_name, db_id, plugin_id, version,
       domain_code, application_code, module_code,
       archived, create_time, update_time,
       create_by, create_name, update_by, update_name
FROM cmx_meta_table_define
WHERE table_name LIKE '%keyword%'  -- modql Contains 操作符
  AND db_id = $db_id
  AND plugin_id = $plugin_id
ORDER BY create_time DESC
```

## 七、文件结构

```
cmx-plugin/src/infrastructure/database/
├── mod.rs
├── table_metadata/
│   ├── mod.rs           # 导出
│   ├── entity.rs        # 实体定义
│   ├── filter.rs        # 过滤器定义
│   ├── bmc.rs           # DbBmc 定义
│   └── service.rs       # 服务实现
```

## 八、使用示例

### 8.1 创建

```rust
let data = TableMetadataForCreate {
    table_name: "cmx_user".to_string(),
    db_id: "default".to_string(),
    plugin_id: "user-plugin".to_string(),
    version: "1.0.0".to_string(),
    domain_code: "system".to_string(),
    application_code: "core".to_string(),
    module_code: "user".to_string(),
    metadata: serde_json::json!({"columns": [...]}),
};

let result = TableMetadataService::create(&db_manager, "default", data).await?;
```

### 8.2 分页查询（支持模糊查询）

```rust
let params = PageParams {
    filter: Some(TableMetadataFilter {
        table_name: Some(OpValsString(vec![OpValString::Contains("user".to_string())])),
        db_id: Some(OpValsString(vec![OpValString::Eq("default".to_string())])),
        ..Default::default()
    }),
    current: Some(1),
    size: Some(20),
    order_bys: Some("-create_time".to_string()),
};

let (dataset, total) = TableMetadataService::page(&db_manager, "default", params).await?;
```

### 8.3 详情查询

```rust
// 通过 ID 查询
let params = GetParams { id: "123456".to_string() };
let result = TableMetadataService::get(&db_manager, "default", params).await?;

// 通过 table_name + db_id 查询
let result = TableMetadataService::get_by_table_name(
    &db_manager, "default", "cmx_user", "default"
).await?;
```

### 8.4 删除

```rust
let payload = DeletePayload {
    ids: vec![serde_json::json!("id1"), serde_json::json!("id2")],
};

let result = TableMetadataService::delete(&db_manager, "default", payload).await?;
```

## 九、实施计划

1. **阶段一**：创建模块结构和数据结构定义
2. **阶段二**：实现创建操作（封装两个表）
3. **阶段三**：实现查询操作（列表、分页、详情）
4. **阶段四**：实现更新操作（封装两个表）
5. **阶段五**：实现删除操作（封装两个表）
6. **阶段六**：清理旧代码，更新调用方

