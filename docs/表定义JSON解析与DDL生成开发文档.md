# 表定义 JSON 解析与 DDL 生成开发文档

## 1. 架构概览

本系统采用三层 crate 架构，将表元数据的定义、处理和执行职责清晰分离：

```
┌─────────────────────────────────────────────────────────┐
│                     cmx-metadata                        │
│  loader    ── JSON 加载                                  │
│  config    ── 多配置文件管理（拓扑排序）                   │
│  i18n      ── 多语言伴生表生成                            │
│  ddl/      ── DDL 生成（DdlDialect trait）               │
│  parser/   ── DDL 解析（DdlParser trait）                │
│  executor  ── DDL 执行（PgTableDefineExecutor）          │
└────────────────────────┬────────────────────────────────┘
                         │ 依赖
┌────────────────────────▼────────────────────────────────┐
│                     cmx-database                        │
│         底层 SQL 执行（execute_sql_by_ids 等）            │
│         连接池管理 / 事务管理                              │
└────────────────────────┬────────────────────────────────┘
                         │ 依赖
┌────────────────────────▼────────────────────────────────┐
│                      cmx-core                           │
│  TableDefine / ColumnDefine / IndexDefine / FieldType   │
│  TableDefineDbExecutor trait                            │
└─────────────────────────────────────────────────────────┘
```

**职责分工：**

| Crate | 职责 |
|-------|------|
| `cmx-core` | 定义核心结构体（`TableDefine`、`ColumnDefine`、`IndexDefine`、`FieldType`）和执行器 trait |
| `cmx-database` | 底层数据库连接池管理、事务管理、SQL 执行（`execute_sql_by_ids` 等） |
| `cmx-metadata` | JSON 加载、DDL 生成/解析、增量 DDL diff、i18n 伴生表生成、DDL 执行（依赖 cmx-database） |

---

## 2. 模块结构

### 2.1 cmx-metadata 模块清单

```
crates/libs/cmx-metadata/src/
├── lib.rs              # 模块入口，导出 MetadataError
├── error.rs            # MetadataError 统一错误类型
├── loader.rs           # JSON 加载函数
├── config.rs           # TableDefinesConfig / TableDefinesConfigManager
├── i18n.rs             # derive_i18n_table_define
├── executor.rs         # DDL 执行（PgTableDefineExecutor）
├── ddl/
│   ├── mod.rs          # DdlDialect trait + 便捷函数
│   ├── postgres.rs     # PostgresDdlDialect 实现
│   └── diff.rs         # DdlDiff 增量 DDL 生成
└── parser/
    ├── mod.rs          # DdlParser trait + 便捷函数
    └── postgres.rs     # PostgresDdlParser 实现
```

### 2.2 各模块功能说明

| 模块 | 核心功能 |
|------|---------|
| `loader.rs` | `table_define_from_str`（单表解析）、`table_defines_from_str`（多表解析）、`load_table_define_from_path` / `load_table_defines_from_path`（文件加载）；支持三种 JSON 根格式 |
| `config.rs` | `TableDefinesConfig`（配置描述）、`TableDefinesConfigManager`（多配置管理、拓扑排序加载）、`load_and_apply_table_defines_from_path`（加载并执行建表） |
| `i18n.rs` | `derive_i18n_table_define`：根据基础表的 `i18n` 标志生成后缀 `_i18n` 的多语言伴生表 |
| `ddl/mod.rs` | `DdlDialect` trait 定义及 `table_to_pg_ddl` / `tables_to_pg_ddl` / `table_to_pg_ddl_roundtrip` 便捷函数 |
| `ddl/postgres.rs` | `PostgresDdlDialect`：FieldType 到 PG 类型映射、CREATE TABLE / INDEX / COMMENT / ALTER / DROP 生成 |
| `ddl/diff.rs` | `DdlDiff`：两版 TableDefine 比对 + 增量 DDL 生成 |
| `parser/mod.rs` | `DdlParser` trait 定义及 `pg_ddl_to_table_defines` / `pg_ddl_to_table_define` 便捷函数 |
| `parser/postgres.rs` | `PostgresDdlParser`：正则解析 CREATE TABLE / CREATE INDEX / COMMENT ON，还原为 TableDefine |
| `executor.rs` | `execute_ddl_by_ids` / `execute_ddl_statement_by_ids`（异步 DDL 执行）、`PgTableDefineExecutor`（实现 `TableDefineDbExecutor` trait） |

---

## 3. 核心结构体（cmx-core）

### 3.1 TableDefine

定义于 `cmx-core/src/model/cell.rs`：

```rust
pub struct TableDefine {
    pub table_name: String,          // 数据库表名
    pub display_name: String,        // UI 显示名
    pub columns: Vec<ColumnDefine>,  // 列定义
    pub primary_keys: Vec<String>,   // 主键列名列表
    pub indexes: Vec<IndexDefine>,   // 索引定义
    pub version: u32,                // 表定义版本（默认 1）
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub i18n: bool,                  // 是否支持多语言
    pub comment: Option<String>,     // 表注释
    pub schema: Option<String>,      // 所属 schema
    pub tablespace: Option<String>,  // 表空间
    pub is_partitioned: bool,        // 是否分区表
    pub partition_type: Option<PartitionType>,  // 分区类型
    pub partition_columns: Vec<String>,         // 分区列
    pub extensions: HashMap<String, JsonValue>, // 扩展属性
}
```

### 3.2 ColumnDefine

```rust
pub struct ColumnDefine {
    pub name: String,                    // 数据库列名
    pub label: String,                   // UI 显示名（同时用作 COMMENT ON COLUMN）
    pub field_type: FieldType,           // 逻辑类型
    pub is_primary_key: bool,            // 是否主键
    pub is_nullable: bool,               // 是否允许为空
    pub default_value: Option<String>,   // 默认值
    pub i18n: bool,                      // 是否参与多语言翻译
    pub length: Option<u32>,             // VARCHAR 长度
    pub precision: Option<u32>,          // NUMERIC 精度
    pub scale: Option<u32>,              // NUMERIC 小数位
    pub db_type: Option<String>,         // 原始数据库类型（round-trip 用）
    pub ordinal: Option<u32>,            // 列序号
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub is_foreign_key: bool,            // 是否外键引用
    pub foreign_key_table: Option<String>,   // 外键目标表
    pub foreign_key_column: Option<String>,  // 外键目标列
    pub extensions: HashMap<String, JsonValue>,
}
```

### 3.3 IndexDefine 与 IndexKind

```rust
pub struct IndexDefine {
    pub name: String,            // 索引名
    pub columns: Vec<String>,    // 索引列
    pub kind: IndexKind,         // 索引类型
}

pub enum IndexKind {
    Unique,   // 唯一索引
    Normal,   // 普通索引
}
```

### 3.4 FieldType

```rust
pub enum FieldType {
    String,    // 短文本（VARCHAR）
    Int,       // 整数（BIGINT）
    Float,     // 浮点数（DOUBLE PRECISION）
    Decimal,   // 高精度十进制（NUMERIC）
    DateTime,  // 日期时间（TIMESTAMP WITH TIME ZONE）
    Date,      // 日期（DATE）
    Bool,      // 布尔（BOOLEAN）
    Text,      // 长文本（TEXT）
    Binary,    // 二进制（BYTEA）
    Array,     // 数组（JSONB）
    Json,      // JSON（JSONB）
    Uuid,      // UUID（UUID）
}
```

### 3.5 PartitionType

```rust
pub enum PartitionType {
    Range,     // 范围分区
    List,      // 列表分区
    Hash,      // 哈希分区
    Interval,  // 间隔分区
}
```

---

## 4. JSON 加载

### 4.1 支持的 JSON 格式

`loader.rs` 支持三种 JSON 根格式，通过 `#[serde(untagged)]` 自动识别：

**格式一：单个表对象**
```json
{
  "table_name": "cmx_domain",
  "display_name": "域",
  "columns": [...],
  "version": 1
}
```

**格式二：`tables` 字段包装**
```json
{
  "tables": [
    { "table_name": "t1", ... },
    { "table_name": "t2", ... }
  ]
}
```

**格式三：顶层数组**
```json
[
  { "table_name": "t1", ... },
  { "table_name": "t2", ... }
]
```

### 4.2 加载 API

| 函数 | 说明 |
|------|------|
| `table_define_from_str(s)` | 从 JSON 字符串解析单个 TableDefine |
| `table_defines_from_str(s)` | 从 JSON 字符串解析多个 TableDefine |
| `load_table_define_from_path(path)` | 从文件加载单个 TableDefine |
| `load_table_defines_from_path(path)` | 从文件加载多个 TableDefine |

### 4.3 配置管理

`TableDefinesConfig` 描述一组表定义文件：

```json
{
  "name": "core_tables",
  "description": "核心系统表",
  "files": ["domain_app_module_tables.json", "sys_config.json"],
  "depends_on": [],
  "priority": 0
}
```

`TableDefinesConfigManager` 管理多套配置：
- `from_config_paths(paths)` — 从多个配置文件路径创建
- `sorted_configs()` — 按依赖关系拓扑排序（Kahn 算法 + 优先级）
- `load_all_tables(base_path)` — 按排序顺序加载全部表定义
- `load_tables_by_config_name(base_path, name)` — 按配置名加载指定表定义
- `load_and_apply_table_defines_from_path(path, executor)` — 加载并通过 executor 执行建表

循环依赖和缺失依赖均会返回 `MetadataError::ConfigDependency` 错误。

---

## 5. DdlDialect Trait 设计

`DdlDialect` 是 DDL 生成的核心抽象，每种数据库方言实现此 trait：

```rust
pub trait DdlDialect {
    fn dialect_name(&self) -> &str;
    fn map_column_type(&self, col: &ColumnDefine) -> String;

    // 建表
    fn generate_create_table(&self, table: &TableDefine) -> Result<String, MetadataError>;
    fn generate_create_indexes(&self, table: &TableDefine) -> Result<Vec<String>, MetadataError>;
    fn generate_comments(&self, table: &TableDefine) -> Result<Vec<String>, MetadataError>;

    // 完整 DDL（默认实现：CREATE TABLE + COMMENT + INDEX）
    fn generate_full_ddl(&self, table: &TableDefine) -> Result<String, MetadataError>;
    fn generate_full_ddl_for_tables(&self, tables: &[TableDefine]) -> Result<String, MetadataError>;

    // 增量变更
    fn generate_add_column(&self, table_name: &str, schema: Option<&str>, col: &ColumnDefine) -> Result<String, MetadataError>;
    fn generate_drop_column(&self, table_name: &str, schema: Option<&str>, col_name: &str) -> Result<String, MetadataError>;
    fn generate_alter_column(&self, table_name: &str, schema: Option<&str>, old_col: &ColumnDefine, new_col: &ColumnDefine) -> Result<Vec<String>, MetadataError>;

    // 删表
    fn generate_drop_table(&self, table: &TableDefine) -> Result<String, MetadataError>;
}
```

**便捷函数：**

| 函数 | 说明 |
|------|------|
| `table_to_pg_ddl(table)` | 单表 PostgreSQL DDL |
| `tables_to_pg_ddl(tables)` | 多表 PostgreSQL DDL |
| `table_to_pg_ddl_roundtrip(table)` | round-trip 模式（优先使用 `db_type`） |

---

## 6. DdlParser Trait 设计

`DdlParser` 是 DDL 解析的核心抽象，将 DDL 文本还原为 `TableDefine`：

```rust
pub trait DdlParser {
    fn dialect_name(&self) -> &str;

    /// 解析单条 CREATE TABLE 语句
    fn parse_create_table(&self, ddl: &str) -> Result<TableDefine, MetadataError>;

    /// 解析完整 DDL（可含多条 CREATE TABLE / CREATE INDEX / COMMENT ON）
    fn parse_ddl(&self, ddl: &str) -> Result<Vec<TableDefine>, MetadataError>;
}
```

**PostgresDdlParser 解析流程：**

1. 按 `;` 分割语句
2. 第一遍：解析所有 `CREATE TABLE` 语句，建立 `HashMap<table_name, TableDefine>`
3. 第二遍：解析 `CREATE INDEX` 和 `COMMENT ON`，关联到对应的 TableDefine
4. 按原 DDL 中 CREATE TABLE 出现顺序返回结果

**便捷函数：**

| 函数 | 说明 |
|------|------|
| `pg_ddl_to_table_defines(ddl)` | 解析 PG DDL 为多个 TableDefine |
| `pg_ddl_to_table_define(ddl)` | 解析 PG DDL 为单个 TableDefine |

---

## 7. FieldType 与 PostgreSQL 类型双向映射

### 7.1 正向映射（FieldType -> PostgreSQL）

由 `PostgresDdlDialect::map_column_type` 实现：

| FieldType | PostgreSQL 类型 | 条件 |
|-----------|----------------|------|
| `String` | `VARCHAR(n)` | `length` 有值时 |
| `String` | `TEXT` | `length` 无值时 |
| `Int` | `BIGINT` | - |
| `Float` | `DOUBLE PRECISION` | - |
| `Decimal` | `NUMERIC(p,s)` | `precision`/`scale` 有值时 |
| `Decimal` | `NUMERIC` | 无精度参数时 |
| `DateTime` | `TIMESTAMP WITH TIME ZONE` | - |
| `Date` | `DATE` | - |
| `Bool` | `BOOLEAN` | - |
| `Text` | `TEXT` | - |
| `Binary` | `BYTEA` | - |
| `Array` | `JSONB` | - |
| `Json` | `JSONB` | - |
| `Uuid` | `UUID` | - |

当 `prefer_db_type = true` 时，优先返回 `col.db_type` 的值（round-trip 模式）。

### 7.2 反向映射（PostgreSQL -> FieldType）

由 `PostgresDdlParser` 的 `parse_pg_type` 实现：

| PostgreSQL 类型 | FieldType | db_type 记录 |
|----------------|-----------|-------------|
| `VARCHAR(n)` / `CHARACTER VARYING(n)` | `String` | `VARCHAR(n)` |
| `TEXT` | `Text` | `TEXT` |
| `BIGINT` / `INT8` | `Int` | `BIGINT` |
| `INTEGER` / `INT` / `INT4` | `Int` | `INTEGER` |
| `SMALLINT` / `INT2` | `Int` | `SMALLINT` |
| `SERIAL` | `Int` | `SERIAL` |
| `BIGSERIAL` | `Int` | `BIGSERIAL` |
| `DOUBLE PRECISION` / `FLOAT8` | `Float` | `DOUBLE PRECISION` |
| `REAL` / `FLOAT4` | `Float` | `REAL` |
| `NUMERIC(p,s)` / `DECIMAL(p,s)` | `Decimal` | `NUMERIC(p,s)` |
| `BOOLEAN` / `BOOL` | `Bool` | `BOOLEAN` |
| `TIMESTAMP WITH TIME ZONE` / `TIMESTAMPTZ` | `DateTime` | `TIMESTAMP WITH TIME ZONE` |
| `DATE` | `Date` | `DATE` |
| `BYTEA` | `Binary` | `BYTEA` |
| `JSONB` | `Json` | `JSONB` |
| `JSON` | `Json` | `JSON` |
| `UUID` | `Uuid` | `UUID` |
| 未识别类型 | `String` | 原始类型字符串 |

---

## 8. 增量 DDL（DdlDiff）

### 8.1 变更类型

```rust
/// 表级别变更
pub enum TableChange {
    CreateTable(TableDefine),         // 新增表
    DropTable(String),                // 删除表
    AlterTable {                      // 修改表
        table_name: String,
        schema: Option<String>,
        column_changes: Vec<ColumnChange>,
        index_changes: Vec<IndexChange>,
        comment_change: Option<String>,
    },
}

/// 列变更
pub enum ColumnChange {
    AddColumn(ColumnDefine),                          // 新增列
    DropColumn(String),                               // 删除列
    AlterColumn { old: ColumnDefine, new: ColumnDefine }, // 修改列
}

/// 索引变更
pub enum IndexChange {
    AddIndex(IndexDefine),   // 新增索引
    DropIndex(String),       // 删除索引
}
```

### 8.2 比对算法

`DdlDiff::diff(old, new)` 的比对逻辑：

1. **表级别**：以 `table_name` 为 key 建立 HashMap，分别检测新增表、删除表、修改表
2. **列级别**：以 `column.name` 为 key 比对，检测新增列、删除列、修改列
3. **列变更判定**：`field_type`、`is_nullable`、`default_value`、`length`、`precision`、`scale` 任一不同即视为变更
4. **索引级别**：以 `index.name` 为 key 比对，检测新增索引、删除索引

### 8.3 API

| 方法 | 说明 |
|------|------|
| `DdlDiff::diff(old, new)` | 比对两组 TableDefine，返回 `Vec<TableChange>` |
| `DdlDiff::changes_to_ddl(dialect, changes)` | 将变更列表转为 DDL 语句 |
| `DdlDiff::diff_to_ddl(dialect, old, new)` | 一步到位：比对 + 生成 DDL |

### 8.4 生成的 DDL 示例

```sql
-- AddColumn
ALTER TABLE "public"."cmx_domain" ADD COLUMN "status" TEXT;

-- DropColumn
ALTER TABLE "public"."cmx_domain" DROP COLUMN "old_col";

-- AlterColumn（类型变更 + nullable 变更 + 默认值变更）
ALTER TABLE "test" ALTER COLUMN "name" TYPE TEXT;
ALTER TABLE "test" ALTER COLUMN "name" SET NOT NULL;
ALTER TABLE "test" ALTER COLUMN "name" SET DEFAULT 'unnamed';

-- AddIndex
CREATE UNIQUE INDEX "uk_new" ON "test" ("code");

-- DropIndex
DROP INDEX IF EXISTS "idx_old";

-- DropTable
DROP TABLE IF EXISTS "old_table" CASCADE;
```

---

## 9. 双向转换矩阵

系统支持在 JSON、TableDefine 和 DDL 之间进行双向转换：

```
    JSON 文件                 TableDefine 结构体              DDL 语句
  ┌──────────┐    serde     ┌─────────────────┐  DdlDialect ┌──────────┐
  │  .json   │ ──────────►  │   TableDefine   │ ──────────► │  DDL     │
  │          │ ◄──────────  │                 │ ◄────────── │  文本    │
  └──────────┘    serde     └─────────────────┘  DdlParser  └──────────┘
```

| 转换方向 | 实现方式 | 关键函数/trait |
|---------|---------|--------------|
| JSON -> TableDefine | serde 反序列化 | `table_define_from_str` / `table_defines_from_str` |
| TableDefine -> JSON | serde 序列化 | `serde_json::to_string` |
| TableDefine -> DDL | DdlDialect trait | `generate_full_ddl` / `generate_create_table` |
| DDL -> TableDefine | DdlParser trait | `parse_ddl` / `parse_create_table` |
| JSON -> DDL | 组合调用 | 先加载再生成 |
| DDL -> JSON | 组合调用 | 先解析再序列化 |

**Round-trip 保障：** 解析器将原始数据库类型存入 `col.db_type`，生成时可通过 `prefer_db_type: true` 优先使用 `db_type`，确保 DDL -> TableDefine -> DDL 不丢失类型精度（如 `INTEGER` 与 `BIGINT` 的区分）。

---

## 10. i18n 多语言伴生表

当 `TableDefine.i18n = true` 时，`derive_i18n_table_define` 生成一张伴生表：

- 表名：`{原表名}_i18n`
- 显示名：`{原显示名}（多语言）`
- 固定列：`ref_id`（Int, NOT NULL）、`locale`（String, NOT NULL）
- 翻译列：从原表中筛选 `i18n = true` 的列
- 主键：`(ref_id, locale)`
- 继承原表的 `schema`、`tablespace`、`extensions`

**示例：**
`cmx_domain`（`i18n: true`，列 `name` 和 `description` 标记 `i18n: true`）
->
`cmx_domain_i18n`（列：`ref_id`, `locale`, `name`, `description`，主键 `(ref_id, locale)`）

---

## 11. DDL 执行（cmx-metadata/executor.rs）

DDL 执行功能位于 `cmx-metadata/src/executor.rs`，依赖 `cmx-database` 提供的底层 SQL 执行能力。

### 11.1 执行函数

```rust
/// 逐条执行多条 DDL 语句
pub async fn execute_ddl_by_ids(
    db_id: &str,
    txn_id: Option<&str>,
    statements: &[String],
) -> Result<(), MetadataError>;

/// 执行单条 DDL 语句
pub async fn execute_ddl_statement_by_ids(
    db_id: &str,
    txn_id: Option<&str>,
    statement: &str,
) -> Result<u64, MetadataError>;
```

底层通过 `cmx_database::execute_sql_by_ids` 执行，支持通过 `db_id` 查找连接池、通过 `txn_id` 加入事务。

### 11.2 PgTableDefineExecutor

实现了 `cmx-core` 中定义的 `TableDefineDbExecutor` trait：

```rust
pub struct PgTableDefineExecutor {
    pub db_id: String,
    pub txn_id: Option<String>,
}

impl TableDefineDbExecutor for PgTableDefineExecutor {
    fn create_table(&self, define: &TableDefine) -> Result<(), BaseError>;
    fn upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError>;
    fn create_or_upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError>; // 默认实现
}
```

- `create_table`：调用 `PostgresDdlDialect` 生成 CREATE TABLE + COMMENT + INDEX 语句后逐条执行
- `upgrade_table`：当前返回 `Unimplemented`（需先查询当前表结构再 diff）
- `create_or_upgrade_table`：先尝试 create，失败则调用 upgrade

**注意：** `TableDefineDbExecutor` trait 是同步签名，内部通过 `tokio::runtime::Handle::current().block_on()` 桥接异步执行。

---

## 12. 设计决策

### 12.1 `db_type` 字段保留

`ColumnDefine.db_type` 存储原始数据库类型字符串（如 `INTEGER`、`SMALLINT`、`BIGSERIAL`）。该字段的作用：

- **正向生成：** 默认模式下忽略 `db_type`，完全由 `field_type` + `length/precision/scale` 决定输出类型
- **Round-trip 模式：** 设置 `prefer_db_type: true` 后优先使用 `db_type`，避免 `INTEGER` 被映射为 `BIGINT` 等精度丢失
- **反向解析：** Parser 总是将原始类型写入 `db_type`，确保信息无损

### 12.2 外键处理

`ColumnDefine` 中的 `is_foreign_key` / `foreign_key_table` / `foreign_key_column` 仅作为**元数据记录**，不生成 `FOREIGN KEY` 约束。原因：

- ERP 系统中外键约束影响批量导入性能
- 外键关系由应用层逻辑保证
- 元数据记录便于文档生成和 UI 展示关联关系

### 12.3 列注释生成

`ColumnDefine.label` 用于生成 `COMMENT ON COLUMN` 语句。当 `label` 非空时，DDL 输出包含：
```sql
COMMENT ON COLUMN "public"."cmx_domain"."code" IS '域编码';
```

### 12.4 默认值渲染

`render_default_value` 函数智能处理默认值：

| 类型 | 示例 | 输出 |
|------|------|------|
| 数值 | `"0"`, `"3.14"` | `0`, `3.14` |
| 布尔/NULL | `"true"`, `"NULL"` | `TRUE`, `NULL` |
| SQL 函数 | `"CURRENT_TIMESTAMP"`, `"now()"` | 原样输出 |
| 字符串 | `"active"` | `'active'` |
| 含引号 | `"it's"` | `'it''s'` |

---

## 13. 错误处理

### 13.1 MetadataError（cmx-metadata）

```rust
pub enum MetadataError {
    Io(std::io::Error),           // 文件 IO 错误
    Json(serde_json::Error),      // JSON 解析错误
    DdlGeneration(String),        // DDL 生成错误
    DdlParse(String),             // DDL 解析错误
    ConfigNotFound(String),       // 配置未找到
    ConfigDependency(String),     // 配置依赖错误（缺失或循环）
}
```

### 13.2 BaseError（cmx-core）

```rust
pub enum BaseError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Unimplemented(String),
    ConfigNotFound(String),
    ConfigDependency(String),
    DdlGeneration(String),
    DdlParse(String),
}
```

---

## 14. 可扩展性：添加新数据库方言

添加 MySQL / Oracle / SQLite 支持只需实现两个 trait：

### 14.1 实现 DdlDialect

```rust
// ddl/mysql.rs
pub struct MySqlDdlDialect;

impl DdlDialect for MySqlDdlDialect {
    fn dialect_name(&self) -> &str { "MySQL" }

    fn map_column_type(&self, col: &ColumnDefine) -> String {
        match col.field_type {
            FieldType::String => match col.length {
                Some(len) => format!("VARCHAR({})", len),
                None => "TEXT".to_string(),
            },
            FieldType::Int => "BIGINT".to_string(),
            FieldType::Bool => "TINYINT(1)".to_string(),
            FieldType::DateTime => "DATETIME".to_string(),
            FieldType::Json => "JSON".to_string(),
            FieldType::Uuid => "CHAR(36)".to_string(),
            // ... 其他类型
        }
    }

    fn generate_create_table(&self, table: &TableDefine) -> Result<String, MetadataError> {
        // MySQL 语法：ENGINE=InnoDB, COMMENT='...', AUTO_INCREMENT 等
        todo!()
    }
    // ... 实现其他方法
}
```

### 14.2 实现 DdlParser

```rust
// parser/mysql.rs
pub struct MySqlDdlParser;

impl DdlParser for MySqlDdlParser {
    fn dialect_name(&self) -> &str { "MySQL" }

    fn parse_create_table(&self, ddl: &str) -> Result<TableDefine, MetadataError> {
        // 解析 MySQL CREATE TABLE 语法
        todo!()
    }

    fn parse_ddl(&self, ddl: &str) -> Result<Vec<TableDefine>, MetadataError> {
        todo!()
    }
}
```

### 14.3 注册便捷函数

在 `ddl/mod.rs` 和 `parser/mod.rs` 中添加对应的便捷函数。

---

## 15. 示例 DDL 输出

以 `cmx_domain` 表为例，输入 JSON 定义（见 `domain_app_module_tables.json`），生成的完整 PostgreSQL DDL 如下：

```sql
CREATE TABLE "public"."cmx_domain" (
    "id" BIGINT NOT NULL,
    "code" VARCHAR(32) NOT NULL,
    "name" VARCHAR(64) NOT NULL,
    "description" TEXT,
    "type" VARCHAR(32),
    "tags" TEXT,
    "sort_order" BIGINT DEFAULT 0,
    "created_at" TIMESTAMP WITH TIME ZONE,
    "updated_at" TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY ("id")
);

COMMENT ON TABLE "public"."cmx_domain" IS '企业应用域，如财务域、供应链域、人力资源域等；为「域-应用-模块」三层结构的顶层';

COMMENT ON COLUMN "public"."cmx_domain"."id" IS '主键';
COMMENT ON COLUMN "public"."cmx_domain"."code" IS '域编码';
COMMENT ON COLUMN "public"."cmx_domain"."name" IS '域名称';
COMMENT ON COLUMN "public"."cmx_domain"."description" IS '说明';
COMMENT ON COLUMN "public"."cmx_domain"."type" IS '域类型';
COMMENT ON COLUMN "public"."cmx_domain"."tags" IS '标签';
COMMENT ON COLUMN "public"."cmx_domain"."sort_order" IS '排序号';
COMMENT ON COLUMN "public"."cmx_domain"."created_at" IS '创建时间';
COMMENT ON COLUMN "public"."cmx_domain"."updated_at" IS '更新时间';

CREATE UNIQUE INDEX "uk_cmx_domain_code" ON "public"."cmx_domain" ("code");
CREATE INDEX "idx_cmx_domain_type" ON "public"."cmx_domain" ("type");
```

---

## 附录：关键文件路径

| 文件 | 路径 |
|------|------|
| 核心结构体 | `crates/libs/cmx-core/src/model/cell.rs` |
| 旧版加载/配置/i18n | `crates/libs/cmx-core/src/model/meta/base.rs` |
| metadata 入口 | `crates/libs/cmx-metadata/src/lib.rs` |
| JSON 加载器 | `crates/libs/cmx-metadata/src/loader.rs` |
| 配置管理 | `crates/libs/cmx-metadata/src/config.rs` |
| i18n 伴生表 | `crates/libs/cmx-metadata/src/i18n.rs` |
| DdlDialect trait | `crates/libs/cmx-metadata/src/ddl/mod.rs` |
| PostgreSQL DDL 生成 | `crates/libs/cmx-metadata/src/ddl/postgres.rs` |
| 增量 DDL diff | `crates/libs/cmx-metadata/src/ddl/diff.rs` |
| DdlParser trait | `crates/libs/cmx-metadata/src/parser/mod.rs` |
| PostgreSQL DDL 解析 | `crates/libs/cmx-metadata/src/parser/postgres.rs` |
| DDL 执行 | `crates/libs/cmx-metadata/src/executor.rs` |
| 示例 JSON | `crates/libs/cmx-core/src/model/domain_app_module_tables.json` |
