---
name: modql
description: 使用 modql 库实现 MongoDB 风格的查询过滤语言（Filter / OpVals / ListOptions），支持 sea-query 集成。当用户设计 Entity / Filter / BMC 动态查询过滤、分页排序、构建 Web API 查询条件，或提到 FilterNodes、derive(Fields)、ListOptions、OpVals 时必用。
---

# Modql - Rust 模型查询语言

modql 是一个 Rust 库，提供 MongoDB 风格的查询过滤语法，支持 sea-query 集成。

> **版本现状（2026-08 与 workspace 核对）：**
> - `modql`：workspace 启用**本地 path 依赖** `modql = { path = "crates/libs/modql", version = "0.1.12" }`（0.5.0 行已注释停用）
> - `sea-query` = `1.0.0`、`sea-query-sqlx` = `0.9.1`
> - `sqlx` 0.9 系：`sqlx::query` 要求 `impl SqlSafeStr`（动态 SQL 需 `sqlx::AssertSqlSafe(sql)` 包装）
> - 本地副本**已移除 `rusqlite` 支持**（避免 `libsqlite3-sys` 版本冲突）——Sqlite* 系列派生宏在本工程不可用，本文档不再收录其教程

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

`#[derive(Fields)]` / `#[derive(FilterNodes)]` / `#[derive(SeaFieldValue)]` 的全部结构体级与字段级属性、默认值、组合规则与速查表见 [references/macro-attributes.md](references/macro-attributes.md)。

---
## JSON 过滤表达式示例

单字段多条件（AND）、多 Filter 组（OR）、边界操作符的 JSON 写法与完整使用示例代码见 [references/json-filter-examples.md](references/json-filter-examples.md)。**速记**：同一字段的多条件默认 AND；不同字段组并列默认 AND，显式 `{"or": [...]}` 才 OR。

---
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
