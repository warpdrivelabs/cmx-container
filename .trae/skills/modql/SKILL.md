---
name: "modql"
description: "使用 modql 库实现 MongoDB 风格的查询过滤语言，支持 sea-query 集成。适用于需要构建动态查询过滤、分页排序的 Web API 开发场景。"
---

# Modql - Rust 模型查询语言

modql 是一个 Rust 库，提供 MongoDB 风格的查询过滤语法，支持 sea-query 和 rusqlite 集成。

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
- `Task::field_refs()` - 返回 FieldRef 列表
- `Task::sea_column_refs()` - 返回 sea-query ColumnRef（需启用 with-sea-query）
- `Task::sea_idens()` - 返回 sea-query DynIden（需启用 with-sea-query）

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

// 7. 生成 SQL
let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
```

## Cargo 依赖配置

```toml
[dependencies]
modql = { workspace = true, features = ["with-sea-query", "with-ilike"] }
sea-query = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = "1"
```

## 依赖管理规范

根据项目规范，依赖必须在 workspace `Cargo.toml` 中统一定义：

```toml
[workspace.dependencies]
modql = { version = "0.4", features = ["with-sea-query", "with-ilike"] }
sea-query = { version = "0.32", features = ["thread-safe"] }
```

子 crate 使用 `workspace = true` 引用：

```toml
modql = { workspace = true, features = ["with-sea-query"] }
```

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
