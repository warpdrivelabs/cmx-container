# 宏属性完整参考（Fields / FilterNodes / SeaFieldValue + 速查表）

> 本文件是 modql 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

宏属性完整参考（Fields / FilterNodes / SeaFieldValue + 速查表）

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
| `skip`               | 标志       | 排除该字段，不参与 Fields 输出（被跳过的字段输出 Default::default()） | `#[field(skip)]`                  |
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

### 宏属性速查表

| 宏                 | 结构体属性                            | 字段属性                                                                           | Feature        |
|-------------------|----------------------------------|--------------------------------------------------------------------------------|----------------|
| `Fields`          | `#[modql(rel, names_as_consts)]` | `#[field(skip, name, rel, cast_as)]`                                           | -              |
| `FilterNodes`     | `#[modql(rel)]`                  | `#[modql(rel, cast_as, cast_column_as, to_sea_condition_fn, to_sea_value_fn)]` | -              |
| `SeaFieldValue`   | 无                                | 无                                                                              | with-sea-query |

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
