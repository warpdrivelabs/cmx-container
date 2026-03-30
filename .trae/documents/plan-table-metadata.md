# 插件表元数据存储功能设计方案

## 一、需求概述

### 1.1 表结构分析

**cmx\_meta\_table\_define** (表定义元数据主表):

| 字段           | 类型           | 说明                 |
| ------------ | ------------ | ------------------ |
| id           | VARCHAR(64)  | 主键                 |
| table\_name  | VARCHAR(100) | 表名 (**唯一索引**)      |
| db\_id       | VARCHAR(100) | 数据库ID (**唯一索引**)   |
| plugin\_id   | VARCHAR(64)  | 插件ID (**可选，仅作记录**) |
| version      | VARCHAR(50)  | 当前使用的元数据插件版本       |
| domain_code      | VARCHAR(64)  | 域编码       |
| application_code      | VARCHAR(64)  | 应用编码       |
| module_code      | VARCHAR(64)  | 模块编码       |
| archived     | INT4         | 归档标志               |
| create\_time | TIMESTAMP    | 创建时间               |
| update\_time | TIMESTAMP    | 更新时间               |
| create\_by   | VARCHAR(100) | 创建人                |
| create\_name | VARCHAR(100) | 创建人名称              |
| update\_by   | VARCHAR(100) | 更新人                |
| update\_name | VARCHAR(100) | 更新人名称              |

**cmx\_meta\_table\_define\_version** (表元数据版本表):

| 字段           | 类型           | 说明                 |
| ------------ | ------------ | ------------------ |
| id           | VARCHAR(64)  | 主键                 |
| table\_name  | VARCHAR(100) | 表名 (**唯一索引**)      |
| db\_id       | VARCHAR(100) | 数据库ID (**唯一索引**)   |
| plugin\_id   | VARCHAR(64)  | 插件ID (**可选，仅作记录**) |
| version      | VARCHAR(50)  | 插件版本 (**唯一索引**)    |
| domain_code      | VARCHAR(64)  | 域编码       |
| application_code      | VARCHAR(64)  | 应用编码       |
| module_code      | VARCHAR(64)  | 模块编码       |
| metadata     | JSONB        | 元数据JSON (**联查获取**) |
| archived     | INT4         | 归档标志               |
| create\_time | TIMESTAMP    | 创建时间               |
| update\_time | TIMESTAMP    | 更新时间               |
| create\_by   | VARCHAR(100) | 创建人                |
| create\_name | VARCHAR(100) | 创建人名称              |
| update\_by   | VARCHAR(100) | 更新人                |
| update\_name | VARCHAR(100) | 更新人名称              |

**唯一索引设计**:

* `cmx_meta_table_define`: UNIQUE(table\_name, db\_id,plugin\_id)，主键为 id

* `cmx_meta_table_define_version`: UNIQUE(table\_name, db\_id,plugin\_id, version))，主键为 id

### 1.2 核心逻辑

1. **安装/升级时**：

   * `cmx_meta_table_define_version`: 如果该版本元数据已存在则更新，否则插入

   * `cmx_meta_table_define`: 更新为新版本的表元数据（记录插件最高版本）

2. **降级时**：

   * `cmx_meta_table_define` 不变（元数据用更高版本的，因为数据库表结构不会回滚）
   * `cmx_meta_table_define_version` 不变

3. **多节点场景**：

   * 先查询判断是更新还是插入或者不操作

4. **查询逻辑**：

   * `db_id`、`plugin_id` `table_name` `domain_code` `application_code` `module_code` 是可选条件，传了值才作为过滤条件，列表或者分页查询不连查cmx_meta_table_define_version

   * `cmx_meta_table_define` 详情查询 需联查 `cmx_meta_table_define_version` 获取 `metadata` 字段

### 1.3 技术选型

* **SQL 构建**: sea\_query (Query/InsertStatement/UpdateStatement/DeleteStatement)

* **SQL 执行**: cmx-database 模块的 DatabaseManager API

  * `execute_sql_with_sqlxvalues()` 执行增删改

  * `query_sql_with_sqlxvalues()` 执行查询

* **事务处理**: 元数据存储与表创建不在同一事务中，各自独立

## 二、功能模块设计

### 2.1 模块结构

```
cmx-plugin/src/
├── infrastructure/
│   └── database/
│       ├── mod.rs
│       └── table_metadata.rs    # 新增：表元数据Repository
```

### 2.2 数据结构设计

```rust
// cmx_meta_table_define_version 记录
#[derive(Debug, Clone)]
pub struct TableMetadataVersionRecord {
   pub id: String,
   pub table_name: String,
   pub db_id: String,
   pub plugin_id: String,
   pub version: String,
   /// 域编码
   pub domain_code: String,
   /// 应用编码
   pub application_code: String,
   /// 模块编码
   pub module_code: String,
   pub metadata: serde_json::Value,
   pub archived: i32,
   pub create_time: DateTime<Utc>,
   pub update_time: DateTime<Utc>,
   pub create_by: Option<String>,
   pub create_name: Option<String>,
   pub update_by: Option<String>,
   pub update_name: Option<String>,
}

// cmx_meta_table_define + metadata 联查结果
#[derive(Debug, Clone)]
pub struct TableMetadataRecord {
    pub id: String,
    pub table_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,           // 当前版本
    pub metadata: serde_json::Value, // 联查获取
    pub archived: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

// 查询条件结构
#[derive(Debug, Clone, Default)]
pub struct TableMetadataQuery {
   pub table_name: Option<String>,
   pub db_id: Option<String>,
   pub plugin_id: Option<String>,
   pub domain_code: Option<String>,
   pub application_code: Option<String>,
   pub module_code: Option<String>,
}
```

