---
name: "modql"
description: "使用 modql 库实现 MongoDB 风格的查询过滤语言，支持 sea-query 集成。适用于需要构建动态查询过滤、分页排序的 Web API 开发场景。"
---

# Modql - Rust 模型查询语言

modql 是一个 Rust 库，提供 MongoDB 风格的查询过滤语法，支持 sea-query 集成。

> **版本更新 (2026-06-12)：**
> - `modql` 升级到 `0.5.0`
> - `sea-query` 升级到 `1.0.1`（稳定版）
> - `sea-query-binder` 已更名为 `sea-query-sqlx`（`0.9.1`）
> - `sqlx` 升级到 `0.9.0`，`sqlx::query` 现在要求 `impl SqlSafeStr`（需用 `sqlx::AssertSqlSafe(sql)` 包装动态 SQL）
> - 工程内使用本地 modql 副本（`path = "crates/libs/modql"`），已移除 `rusqlite` 支持（避免 `libsqlite3-sys` 版本冲突）

## 核心概念

### 1. Filter（过滤器）

Filter 用于表达查询条件，支持多种操作符。

```rust
use modql::filter::{FilterNodes, OpValsString, OpValsInt64, OpValsBool};
use serde::{Deserialize, Serialize};

// 定义 Filter 结构体
#[derive(FilterNodes, Deserialize, Default, Debug)]
pub struct TaskFilter {
    id: Option<OpValsInt64>,        // i64 类型过滤
    title: Option<OpValsString>,   // 字符串类型过滤
    done: Option<OpValsBool>,       // 布尔类型过滤
}
```

### 2. OpVal 操作符类型

#### OpValsString（字符串操作符）

| JSON 操作符                        | Rust 枚举                                 | 说明                 |
|---------------------------------|-----------------------------------------|--------------------|
| `$eq`                           | `OpValString::Eq(String)`               | 精确匹配               |
| `$not`                          | `OpValString::Not(String)`              | 不等于                |
| `$in`                           | `OpValString::In(Vec<String>)`          | 在列表中（OR）           |
| `$notIn`                        | `OpValString::NotIn(Vec<String>)`       | 不在列表中              |
| `$contains`                     | `OpValString::Contains(String)`         | 包含                 |
| `$containsAny`                  | `OpValString::ContainsAny(Vec<String>)` | 包含任意               |
| `$containsAll`                  | `OpValString::ContainsAll(Vec<String>)` | 包含所有               |
| `$notContains`                  | `OpValString::NotContains(String)`      | 不包含                |
| `$startsWith`                   | `OpValString::StartsWith(String)`       | 开头是                |
| `$endsWith`                     | `OpValString::EndsWith(String>)`        | 结尾是                |
| `$lt` / `$lte` / `$gt` / `$gte` | `OpValString::Lt/Lte/Gt/Gte(String)`    | 比较运算               |
| `$null`                         | `OpValString::Null(bool)`               | 是否为 NULL           |
| `$containsCi`                   | `OpValString::ContainsCi(String)`       | 忽略大小写包含            |
| `$ilike`                        | `OpValString::Ilike(String)`            | PostgreSQL 忽略大小写匹配 |

#### OpValsInt64 / OpValsInt32 / OpValsFloat64（数值操作符）

| JSON 操作符                        | 说明          |
|---------------------------------|-------------|
| `$eq`                           | 精确匹配        |
| `$in`                           | 在列表中        |
| `$not` / `$notIn`               | 不等于 / 不在列表中 |
| `$lt` / `$lte` / `$gt` / `$gte` | 比较运算        |
| `$null`                         | 是否为 NULL    |

#### OpValsBool（布尔操作符）

| JSON 操作符 | 说明       |
|----------|----------|
| `$eq`    | 精确匹配     |
| `$not`   | 不等于      |
| `$null`  | 是否为 NULL |

### 3. Fields（字段元信息）

```rust
use modql::field::Fields;
use sea_query::FromRow;

#[derive(Debug, Clone, Fields, FromRow, Serialize)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub done: bool,
}
```

`#[derive(Fields)]` 提供以下方法：

- `Task::field_names()` - 返回字段名列表
- `Task::field_refs()` - 返回 FieldRef 列表（已废弃，使用 field_metas）
- `Task::field_metas()` - 返回 FieldMetas（包含完整元信息）
- `Task::sea_column_refs()` - 返回 sea-query ColumnRef（需启用 with-sea-query）
- `Task::sea_idens()` - 返回 sea-query DynIden（需启用 with-sea-query）
- `Task::sea_column_refs_with_rel(rel)` - 返回带指定关系的 ColumnRef（需启用 with-sea-query）
- `task.not_none_sea_fields()` - 返回非 None 值的 SeaFields（需启用 with-sea-query）
- `task.all_sea_fields()` - 返回所有字段的 SeaFields（需启用 with-sea-query）
- `Task::sea_apply_select_columns(&mut select)` - 自动应用 SELECT 列（含别名）（需启用 with-sea-query）

## 宏属性完整参考

### `#[derive(Fields)]` 属性

注册的属性标签：`field`（字段级）、`modql`（结构体级）

#### 结构体级属性 `#[modql(...)]`

| 属性                            | 类型       | 说明                            | 示例                                   |
|-------------------------------|----------|-------------------------------|--------------------------------------|
| `rel = "table_name"`          | `String` | 为所有字段设置默认关系（表名），字段级 `rel` 可覆盖 | `#[modql(rel = "todo_table")]`       |
| `names_as_consts`             | 标志       | 为每个字段名生成常量，常量名 = 字段名大写        | `#[modql(names_as_consts)]`          |
| `names_as_consts = "PREFIX_"` | `String` | 为每个字段名生成带前缀的常量                | `#[modql(names_as_consts = "COL_")]` |

**`names_as_consts` 示例：**

```rust
#[derive(Fields)]
#[modql(names_as_consts)]
pub struct Todo {
  pub id: i64,
  pub title: String,
}
// 生成: Todo::ID = "id", Todo::TITLE = "title"

#[derive(Fields)]
#[modql(names_as_consts = "COL_")]
pub struct Project {
  pub id: i64,
  pub name: String,
}
// 生成: Project::COL_ID = "id", Project::COL_NAME = "name"
```

#### 字段级属性 `#[field(...)]`

| 属性                   | 类型       | 说明                                                                | 示例                                |
|----------------------|----------|-------------------------------------------------------------------|-----------------------------------|
| `skip`               | 标志       | 排除该字段，不参与 Fields 输出（被跳过的字段在 SqliteFromRow 中使用 Default::default()） | `#[field(skip)]`                  |
| `name = "col_name"`  | `String` | 重命名字段，将结构体属性名映射为不同的列名                                             | `#[field(name = "description")]`  |
| `rel = "table_name"` | `String` | 覆盖结构体级 `rel`，为该字段指定独立的关系（表名）                                      | `#[field(rel = "special_table")]` |
| `cast_as = "type"`   | `String` | 在 sea-query 中对值进行类型转换（`CAST(value AS type)`）（需 with-sea-query）    | `#[field(cast_as = "integer")]`   |

**`#[field(...)]` 综合示例：**

```rust
#[derive(Debug, Default, Fields)]
#[modql(rel = "todo_table")]
pub struct Todo {
  pub id: i64,

  // 覆盖结构体 rel，并重命名列
  #[field(rel = "special_todo_table", name = "special_title_col")]
  pub title: String,

  // 仅重命名列（desc -> description）
  #[field(name = "description")]
  pub desc: Option<String>,

  // 跳过该字段，不参与 Fields 输出
  #[field(skip)]
  pub other: Option<String>,
}
// field_names() => ["id", "special_title_col", "description"]
// field_metas() 中 rel 分别为: Some("todo_table"), Some("special_todo_table"), Some("todo_table")
```

---

### `#[derive(FilterNodes)]` 属性

注册的属性标签：`modql`（字段级和结构体级）

#### 结构体级属性 `#[modql(...)]`

| 属性                   | 类型       | 说明                              | 示例                           |
|----------------------|----------|---------------------------------|------------------------------|
| `rel = "table_name"` | `String` | 为所有过滤字段设置默认关系（表名），字段级 `rel` 可覆盖 | `#[modql(rel = "task_tbl")]` |

#### 字段级属性 `#[modql(...)]`

| 属性                                | 类型       | 说明                                                                          | 示例                                                      |
|-----------------------------------|----------|-----------------------------------------------------------------------------|---------------------------------------------------------|
| `rel = "table_name"`              | `String` | 覆盖结构体级 `rel`，为该过滤字段指定独立的关系（表名）                                              | `#[modql(rel = "foo_rel")]`                             |
| `cast_as = "type"`                | `String` | 对 sea-query 值进行类型转换（`CAST(value AS type)`）（需 with-sea-query）                | `#[modql(cast_as = "integer")]`                         |
| `cast_column_as = "type"`         | `String` | 对 sea-query 列进行类型转换（`CAST(column AS type)`）（需 with-sea-query）               | `#[modql(cast_column_as = "text")]`                     |
| `to_sea_condition_fn = "fn_name"` | `String` | 自定义函数：将 `OpValValue` 转换为 `sea_query::ConditionExpression`（需 with-sea-query） | `#[modql(to_sea_condition_fn = "my_to_sea_condition")]` |
| `to_sea_value_fn = "fn_name"`     | `String` | 自定义函数：将 `serde_json::Value` 转换为 `sea_query::Value`（需 with-sea-query）        | `#[modql(to_sea_value_fn = "my_to_sea_value")]`         |

**`to_sea_condition_fn` 签名：**

```rust
fn my_to_sea_condition(col: &ColumnRef, op_val_value: OpValValue) -> SeaResult<ConditionExpression>
```

**`to_sea_value_fn` 签名：**

```rust
fn my_to_sea_value(json_value: serde_json::Value) -> SeaResult<sea_query::Value>
```

> **注意：** `to_sea_condition_fn` 和 `to_sea_value_fn` 互斥，不能同时使用。它们仅适用于 `OpValsValue` 类型的字段。

**`#[derive(FilterNodes)]` 综合示例：**

```rust
#[derive(Clone, FilterNodes, Default)]
#[modql(rel = "task_tbl")]
pub struct TaskFilter {
  id: Option<OpValsInt64>,

  // 对列进行类型转换: CAST("task_tbl"."title" AS text) = ?
  #[modql(cast_column_as = "text")]
  title: Option<OpValsString>,

  // 覆盖结构体 rel
  #[modql(rel = "foo_rel")]
  label: Option<OpValsString>,

  // 自定义 sea-query 条件转换函数
  #[modql(to_sea_condition_fn = "my_to_sea_condition")]
  ctime: Option<OpValsValue>,
}
```

---

### `#[derive(SeaFieldValue)]` 属性（需 with-sea-query）

无字段属性。仅支持以下类型：

- **单元素元组结构体**：自动实现 `From<T> for sea_query::Value` 和 `sea_query::Nullable`
  - 支持的内部类型：`bool`, `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `f32`, `f64`, `String`, `char`
- **简单枚举**（仅含无数据变体）：变体名作为 `sea_query::Value::String` 的值

```rust
#[derive(modql::field::SeaFieldValue)]
pub struct EpochTime(pub i64);
// 生成: impl From<EpochTime> for sea_query::Value { ... }
// 生成: impl sea_query::Nullable for EpochTime { ... }

#[derive(modql::field::SeaFieldValue)]
pub enum Kind {
  Md,
  Pdf,
  Unknown,
}
// 生成: impl From<Kind> for sea_query::Value { ... } (变体名作为字符串值)
```

---

### `#[derive(SqliteFromRow)]` 属性（需 with-rusqlite）

注册的属性标签：`field`（字段级）、`fields`（结构体级）

复用 `#[field(...)]` 属性（与 `Fields` 宏相同）：

| 属性                   | 类型       | 说明                               | 示例                               |
|----------------------|----------|----------------------------------|----------------------------------|
| `skip`               | 标志       | 跳过该字段，使用 `Default::default()` 填充 | `#[field(skip)]`                 |
| `name = "col_name"`  | `String` | 重命名列                             | `#[field(name = "description")]` |
| `rel = "table_name"` | `String` | 设置关系                             | `#[field(rel = "table")]`        |
| `cast_as = "type"`   | `String` | 类型转换                             | `#[field(cast_as = "integer")]`  |

生成的方法：

- `sqlite_from_row(row: &rusqlite::Row) -> Result<Self>` - 从完整行构建
- `sqlite_from_row_partial(row: &rusqlite::Row, prop_names: &[&str]) -> Result<Self>` - 从部分列构建（Option 字段若不在
  prop_names 中则为 None）

> **已废弃别名：** `FromSqliteRow` → 请使用 `SqliteFromRow`

---

### `#[derive(SqliteFromValue)]` 属性（需 with-rusqlite）

无字段属性。仅支持以下类型：

- **单元素元组结构体**：自动实现 `rusqlite::types::FromSql`
- **简单枚举**（仅含无数据变体）：变体名作为字符串从 SQLite 读取

```rust
#[derive(SqliteFromValue)]
pub struct SimpleId(i64);

#[derive(SqliteFromValue)]
pub enum DItemKind {
  Md,
  Pdf,
  Unknown,
}
```

> **已废弃别名：** `FromSqliteValue` → 请使用 `SqliteFromValue`

---

### `#[derive(SqliteToValue)]` 属性（需 with-rusqlite）

无字段属性。仅支持以下类型：

- **单元素元组结构体**：自动实现 `rusqlite::types::ToSql`
- **简单枚举**（仅含无数据变体）：变体名作为字符串写入 SQLite

```rust
#[derive(SqliteToValue)]
pub struct SimpleId(i64);

#[derive(SqliteToValue)]
pub enum DItemKind {
  Md,
  Pdf,
  Unknown,
}
```

> **已废弃别名：** `ToSqliteValue` → 请使用 `SqliteToValue`

---

### 宏属性速查表

| 宏                 | 结构体属性                            | 字段属性                                                                           | Feature        |
|-------------------|----------------------------------|--------------------------------------------------------------------------------|----------------|
| `Fields`          | `#[modql(rel, names_as_consts)]` | `#[field(skip, name, rel, cast_as)]`                                           | -              |
| `FilterNodes`     | `#[modql(rel)]`                  | `#[modql(rel, cast_as, cast_column_as, to_sea_condition_fn, to_sea_value_fn)]` | -              |
| `SeaFieldValue`   | 无                                | 无                                                                              | with-sea-query |
| `SqliteFromRow`   | 复用 `#[field(...)]`               | `#[field(skip, name, rel, cast_as)]`                                           | with-rusqlite  |
| `SqliteFromValue` | 无                                | 无                                                                              | with-rusqlite  |
| `SqliteToValue`   | 无                                | 无                                                                              | with-rusqlite  |

### 4. ListOptions（分页和排序）

```rust
use modql::filter::ListOptions;

// JSON 格式
let list_options: ListOptions = serde_json::from_value(json!({
    "offset": 0,
    "limit": 10,
    "order_bys": "!title"  // ! 表示降序
})) ?;
```

## JSON 过滤表达式示例

### 单字段多条件（AND 关系）

```json
{
  "title": {
    "$startsWith": "Hello",
    "$contains": "World"
  },
  "done": false
}
```

### 多 Filter 组（OR 关系）

```json
{
  "filters": [
    {
      "id": {
        "$gt": 123
      },
      "title": {
        "$contains": "World"
      }
    },
    {
      "title": {
        "$startsWith": "Hello"
      }
    }
  ]
}
```

### 使用示例代码

```rust
use modql::filter::{FilterNodes, IntoFilterNodes, ListOptions};
use modql::SIden;
use sea_query::{Condition, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder;

// 1. 定义 Filter
#[derive(FilterNodes, Deserialize, Default, Debug)]
pub struct TaskFilter {
    id: Option<OpValsInt64>,
    title: Option<OpValsString>,
    done: Option<OpValsBool>,
}

// 2. 从 JSON 解析 Filter
let filter: TaskFilter = serde_json::from_value(json!({
    "title": {"$startsWith": "Hello", "$contains": "World"},
    "done": false
})) ?;

// 3. 转换为 FilterGroups
let filter_groups: modql::filter::FilterGroups = filter.filter_nodes(None).into();

// 4. 转换为 sea-query Condition
let cond: Condition = filter_groups.into_sea_condition() ?;

// 5. 构建查询
let mut query = Query::select();
query.from(SIden("task"));
query.columns(Task::sea_column_refs());
query.cond_where(cond);

// 6. 应用 ListOptions
let list_options: ListOptions = serde_json::from_value(json!({
    "offset": 0,
    "limit": 10,
    "order_bys": "!created_at"
})) ?;
list_options.apply_to_sea_query( & mut query);

// 7. 生成 SQL（sea-query-sqlx 0.9.1）
let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
```

## Cargo 依赖配置

```toml
[dependencies]
modql = { workspace = true, features = ["with-sea-query", "with-ilike"] }
sea-query = { workspace = true }
sea-query-sqlx = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = "1"
```

## 依赖管理规范

根据项目规范，依赖必须在 workspace `Cargo.toml` 中统一定义：

```toml
[workspace.dependencies]
# 数据库 - ORM 支持（本地副本，已适配 sea-query 1.0.1）
modql = { path = "crates/libs/modql" }
# 数据库 - SQL 查询构建器
sea-query = { version = "1.0.1", features = ["with-chrono", "with-time", "with-json", "with-uuid"] }
# 数据库 - SQL 参数绑定工具（原 sea-query-binder 已更名为 sea-query-sqlx）
sea-query-sqlx = { version = "0.9.1", features = ["sqlx-postgres", "with-uuid", "with-time", "with-chrono", "with-json"] }
```

子 crate 使用 `workspace = true` 引用：

```toml
modql = { workspace = true, features = ["with-sea-query"] }
sea-query = { workspace = true }
sea-query-sqlx = { workspace = true }
```

## sqlx 0.9 动态 SQL 安全包装

sqlx 0.9 引入了 `SqlSafeStr` trait，`sqlx::query` 和 `sqlx::query_with` 要求 SQL 字符串实现此 trait。动态 SQL 需用 `sqlx::AssertSqlSafe` 包装：

```rust
// 动态 SQL（如从 sea-query 构建的 SQL）
let sql = "SELECT * FROM task WHERE id = $1";
let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
    .bind(123)
    .fetch_all(&pool)
    .await?;

// 带参数的查询
let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
let rows = sqlx::query_with(sqlx::AssertSqlSafe(&sql), values)
    .fetch_all(&pool)
    .await?;
```

> `AssertSqlSafe` 表示已人工审计 SQL 注入风险，sqlx 内部会将 `&str` 克隆为 `Arc<str>` 以匹配 `'static` 生命周期要求。

## 常见错误处理

modql 使用 `thiserror` 定义错误：

```rust
use modql::Error;

fn some_function() -> modql::Result<T> {
    // 可能返回错误的操作
}
```

## 触发场景

当用户提到以下场景时，应主动使用此 skill：

- "实现查询过滤功能"
- "添加搜索/筛选功能"
- "构建动态 SQL 查询"
- "实现分页查询"
- "需要支持前端传来的过滤条件"
- "使用 sea-query 构建查询"
- "实现类似 GraphQL 的过滤语法"
- "查询参数解析"
- "filter 参数处理"
