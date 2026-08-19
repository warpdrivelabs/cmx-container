# modql

> Model Query Language（[joql.org](https://joql.org) 风格）的 Rust 实现：一套**序列化无关**的动态查询过滤语言——FilterNode / FilterGroups / OpVal 算子体系 + ListOptions 排序分页，并提供与 sea-query 的直接互转（`FilterGroups → Condition`）。自上游 [jeremychone/rust-modql](https://github.com/jeremychone/rust-modql) fork 的内部维护版，是 CMX 全部业务域 Filter 结构与通用 CRUD 查询的过滤引擎。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2021-orange.svg)]()
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green.svg)]()
[![Authors](https://img.shields.io/badge/authors-Jeremy%20Chone-lightgrey.svg)]()

> 注：本 crate 为上游 `rust-modql` 的内部 fork：版本号已改为跟随 workspace（0.1.12，经 registry nora 发布），保留自身 `edition 2021`、`MIT OR Apache-2.0` 许可与上游作者署名（修改部分由 CMX 团队维护）。

---

## 项目简介

`modql` 解决的问题是：**业务实体需要一种「结构化、可序列化、可编译成 SQL 条件」的动态过滤语言**。手写 `WHERE` 拼接既不安全也难以前后端传输；而 modql 用「属性名 + 算子 + 值」的三元组体系描述过滤，序列化无关（Rust 结构体 / JSON 皆可），并能一步编译为 sea-query 的 `Condition`，进而生成参数化 SQL。

### 核心模型

- **FilterNode**：`属性名 + Vec<OpVal>`（可带 `rel` 关联前缀，如 `project.title` 中的 `project`）。大量 `From` 元组实现让 `("name", "foo")`、`("id", OpValInt64::Gt(1))` 这类写法直接 `into()` 成节点。
- **FilterGroup / FilterGroups**：组内节点 **AND**，组间 **OR**——`(A AND B) OR (C)` 的经典表达力，且与 joql/GraphQL 风格列表语义一致。
- **OpVal 算子族**：`OpValString`（30 个变体：Eq/In/Lt/Gt/Contains(Any/All)/StartsWith(Any)/EndsWith(Any)/Empty/Null 及各自 Not/Ci 大小写不敏感版、Ilike）、`OpValInt64/Int32/Float64/Bool/Value`。JSON 形如 `{"$contains": "World", "$startsWith": "Hello"}`。
- **ListOptions**：`limit / offset / order_bys`；`OrderBy` 从字符串解析，`!` 前缀表示降序（`"!created_at"` → `"created_at" DESC`）。
- **sea-query 互转**（feature `with-sea-query`）：`TryFrom<FilterGroups> for Condition`、`ListOptions::apply_to_sea_query` 一行落到 `SelectStatement`。

### 在 CMX 中的角色

CMX 各业务域（域/用户/插件/存储等）的查询 API 统一定义 `XxxFilter` 结构（derive `FilterNodes` + `Deserialize`），由 cmx-database 的 `GenericCrudService` 统一消费：过滤列表 `Vec<F>` → `FilterGroups` → `Condition` → `cond_where` → 参数化 SQL。前端 JSON 查询条件与后端 SQL 构建由此对齐。

---

## 与其他 crate 的关系

### 上游依赖

| 依赖 | feature | 用途 |
|------|---------|------|
| `modql-macros`（子 crate） | default / with-sea-query | `FilterNodes` / `Fields` / `SeaFieldValue` 派生宏 |
| `serde` / `serde_json` | default | JSON 反序列化（`{"$op": value}` 算子格式） |
| `sea-query`（1.0.1，thread-safe） | with-sea-query（连带 with-ilike） | `Condition` / `SelectStatement` / `Iden` 互转 |

### 下游使用方（grep 实测，cmx-container workspace 内 13 个消费 crate）

| 消费组 | crate | 启用 feature | 用途 |
|--------|-------|-------------|------|
| **开 with-sea-query**（编译 SQL 条件） | `cmx-biz` | `["with-sea-query"]` | 各域 Filter 结构（如 `DomainFilter`）+ CRUD 查询 |
| | `cmx-iam` / `cmx-auth` | `["with-sea-query"]` | 用户/角色/权限过滤查询 |
| | `cmx-database` / `cmx-database-pg` | `["with-sea-query"]` | `GenericCrudService::list`：FilterGroups→Condition→cond_where |
| | `cmx-plugin` / `cmx-common-api` | `["with-sea-query"]` | 插件与通用 API 过滤 |
| **裸引用**（只吃 default = modql-macros） | `cmx-core` / `cmx-api-core` / `cmx-api-types` / `cmx-biz-api` / `cmx-plugin-api` / `cmx-storage` / `cmx-platform-app` | — | Filter/OpVals 类型透传、derive 宏间接依赖 |
| 跨 workspace | — | — | cmx-portalservice / cmx-flowengine 经上述 crate 间接消费，不直接引 modql |

---

## 核心功能与特性

| 功能 | 入口 | 一句话 |
|------|------|--------|
| 过滤节点 | `FilterNode::new` / 元组 `From` | 属性名 + 算子值列表；`rel` 支持关联前缀 |
| AND/OR 组合 | `FilterGroups` | 组内 AND、组间 OR；`add_group` / 多种 `From` |
| 算子体系 | `OpValString` 等 6 族 | 30 个字符串算子（含 Ci 大小写不敏感、Ilike）、数值/布尔/JSON 值算子 |
| JSON 反序列化 | `Deserialize` / `OpValueToOpValType` | `{"name": {"$contains": "abc"}}` 直接进 Filter 结构 |
| sea-query 条件 | `TryFrom<FilterGroups> for Condition` | 过滤一步编译为参数化 `Condition` |
| 排序分页 | `ListOptions` / `OrderBys` | `!` 前缀降序；`apply_to_sea_query` 落到 select |
| 字段元数据 | `HasFields` / `FieldMeta(s)` + `Fields` derive | 列名清单 / SQL 列引用串 / 占位符串 |
| Iden 工具 | `SIden` / `StringIden` | 静态/动态字符串包装成 sea-query `Iden`；`cast_as` / `cast_column_as` 值与列的类型转换 |

---

## 模块结构

```text
modql
├── src
│   ├── lib.rs              # 模块导出：field / filter / includes / sea_utils(cfg) / sqlite(cfg)
│   ├── error.rs            # Error（4 个 JSON 解析变体）/ Result
│   ├── filter/             # 过滤语言核心
│   │   ├── nodes/
│   │   │   ├── node.rs     #   FilterNode / FilterNodeOptions / IntoFilterNodes / 元组 From 宏
│   │   │   └── group.rs    #   FilterGroup（AND）/ FilterGroups（OR）+ TryFrom→Condition
│   │   ├── ops/            # 算子族：op_val_string / op_val_nums / op_val_bool / op_val_value + OpVal 枚举
│   │   ├── json/           # JSON 反序列化（"$op" 格式、OrderBys 解析）
│   │   ├── list_options/   # ListOptions + order_by.rs（OrderBy/OrderBys、apply_to_sea_query）
│   │   └── into_sea/       # (with-sea-query) ForSeaCondition 等自定义条件挂点
│   ├── field/              # 字段元数据：HasFields / FieldMeta(s) / sea / sqlite 派生支持
│   ├── sea_utils/          # (with-sea-query) SIden / StringIden / cast 工具
│   ├── includes.rs         # 预留占位（Includes/IncludeNode，Not used yet）
│   └── sqlite.rs           # (with-rusqlite) 保留的 rusqlite 支持（当前未启用）
└── modql-macros/           # 过程宏子 crate（FilterNodes/Fields/SeaFieldValue/Sqlite*）
```

---

## 关键类型 / API

### 过滤节点与组合（`src/filter/nodes/`）

```rust
pub struct FilterNode {
    pub rel: Option<String>,                 // 关联前缀（如 project.title 的 "project"）
    pub name: String,                        // 属性名
    pub opvals: Vec<OpVal>,                  // 算子值列表（同一节点内 AND）
    pub options: FilterNodeOptions,          // cast_as / cast_column_as（DB 类型转换）
}

impl FilterNode {
    pub fn new(name: impl Into<String>, opvals: impl Into<Vec<OpVal>>) -> FilterNode;
    pub fn new_with_rel(rel: Option<String>, name: impl Into<String>, opvals: impl Into<Vec<OpVal>>) -> FilterNode;
}
// From 元组：("name", "foo") → Eq；("id", OpValInt64::Gt(1))；("name", vec![OpValString::Contains("a")]) …

pub trait IntoFilterNodes {
    fn filter_nodes(self, rel: Option<String>) -> Vec<FilterNode>;   // FilterNodes derive 为它生成实现
}

/// 组内节点 AND
pub struct FilterGroup(Vec<FilterNode>);
/// 组间 OR
pub struct FilterGroups(Vec<FilterGroup>);

impl FilterGroups {
    pub fn add_group(&mut self, group: Vec<FilterNode>) -> &mut Self;
    pub fn groups(&self) -> &Vec<FilterGroup>;
    pub fn into_vec(self) -> Vec<FilterGroup>;
}
// From：Vec<FilterNode> / Vec<Vec<FilterNode>> / FilterNode / FilterGroup，以及
// From<Vec<F>> where F: IntoFilterNodes（Vec<DomainFilter> → FilterGroups，组间 OR）
```

### 算子体系（`src/filter/ops/`）

```rust
pub enum OpVal { String(OpValString), Int64(OpValInt64), Int32(OpValInt32),
                 Float64(OpValFloat64), Bool(OpValBool), Value(OpValValue) }

pub struct OpValsString(pub Vec<OpValString>);   // Filter 结构体字段类型；From<&str>/String → [Eq]

// OpValString 全部 30 个变体（其余族语义相同）：
// Eq / Not / In / NotIn / Lt / Lte / Gt / Gte
// Contains / NotContains / ContainsAny / NotContainsAny / ContainsAll
// StartsWith / NotStartsWith / StartsWithAny / NotStartsWithAny
// EndsWith / NotEndsWith / EndsWithAny / NotEndsWithAny
// Empty(bool) / Null(bool)
// ContainsCi / NotContainsCi / StartsWithCi / NotStartsWithCi / EndsWithCi / NotEndsWithCi / Ilike
```

### sea-query 互转（feature `with-sea-query`）

```rust
impl TryFrom<FilterGroup> for Condition { type Error = IntoSeaError; /* Condition::all() */ }
impl TryFrom<FilterGroups> for Condition { type Error = IntoSeaError; /* Condition::any() */ }

impl FilterGroups {
    pub fn into_sea_condition(self) -> SeaResult<Condition>;
}

pub fn sea_is_col_value_null(col: ColumnRef, null: bool) -> Condition;
```

### 排序分页（`src/filter/list_options/`）

```rust
#[derive(Default, Debug, Clone, Deserialize)]
pub struct ListOptions {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order_bys: Option<OrderBys>,
}

impl ListOptions {
    pub fn from_limit(limit: i64) -> Self;
    pub fn from_offset_limit(offset: i64, limit: i64) -> Self;
    pub fn with_limit(mut self, limit: i64) -> Self;
    pub fn with_offset(mut self, offset: i64) -> Self;
    pub fn append_order_by(mut self, order_by: impl Into<OrderBy>) -> Self;
    /// (with-sea-query) 落到 select：limit/offset（负数按 0）+ order_by
    pub fn apply_to_sea_query(self, select_query: &mut SelectStatement);
}

pub enum OrderBy { Asc(String), Desc(String) }
impl<T: AsRef<str>> From<T> for OrderBy;          // "!created_at" → Desc("created_at")

pub struct OrderBys(Vec<OrderBy>);
impl OrderBys {
    pub fn join_for_sql(&self) -> String;          // `"a"."b" DESC, "c" ASC`
    pub fn push(&mut self, order_by: impl Into<OrderBy>);
}
```

### 字段元数据与 Iden 工具（`src/field/` / `src/sea_utils/`）

```rust
pub trait HasFields {
    fn field_names() -> &'static [&'static str];
    fn field_metas() -> &'static FieldMetas;
    fn sql_columns() -> String;                   // 列引用串
    fn sql_placeholders() -> String;              // "?, ?, ?"
}

pub struct SIden(pub &'static str);               // &'static str → sea-query Iden
pub struct StringIden(pub String);                // String → Iden（cast_as 目标类型等）
```

---

## 使用示例

### 一、为业务实体定义 Filter 结构（derive FilterNodes，cmx-biz 真实模式）

```rust
use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// Domain 查询过滤器（crates/libs/cmx-biz/src/domain/filter.rs 原样）
/// 字段全部是 Option<OpValsXxx>——宏只为这类字段生成过滤节点
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct DomainFilter {
    pub code: Option<OpValsString>,     // {"code": {"$startsWith": "SYS"}}
    pub name: Option<OpValsString>,
    pub r#type: Option<OpValsString>,
    pub status: Option<OpValsInt64>,    // {"status": {"$in": [1, 2]}}
    pub archived: Option<OpValsInt64>,
}

// JSON 查询条件直接反序列化进来（serde + modql 的 "$op" 格式）
let f: DomainFilter = serde_json::from_value(serde_json::json!({
    "code": {"$startsWith": "SYS"},
    "status": {"$lt": 9}
}))
.unwrap();
// f 的每个 Some 字段编译为一个 FilterNode，字段之间 AND
```

### 二、FilterGroups → Condition → sea-query 参数化 SQL（cmx-database 真实调用链）

```rust
use modql::filter::{FilterGroups, IntoFilterNodes, ListOptions};
use sea_query::{PostgresQueryBuilder, Query, Condition};

// ① 由业务 Filter 列表构造（Vec<F: IntoFilterNodes> → FilterGroups，元素间 OR）
let filters: Vec<DomainFilter> = vec![f0, f1];
let filters: FilterGroups = Vec::into(filters);

// ② 编译为 sea-query Condition（组内 AND、组间 OR）
let cond: Condition = filters.try_into().unwrap();

// ③ 组装查询：cond_where + 排序分页一并落上（GenericCrudService::list 的核心三行）
let mut query = Query::select();
query.from_char('t');                                   // 实际为 MC::table_ref()
query.cond_where(cond);

let lo = ListOptions::from_offset_limit(0, 20)
    .append_order_by("!created_at");                    // "!" 前缀 = DESC
lo.apply_to_sea_query(&mut query);

let (sql, values) = query.build(PostgresQueryBuilder);
// SELECT * FROM t WHERE (...) ORDER BY "created_at" DESC LIMIT 20 —— 值经 $n 参数化
```

### 三、手写节点 / 显式 OR 语义

```rust
use modql::filter::{FilterGroups, OpValString};
use sea_query::Condition;

// 元组 From：(&str, OpValString) 直接变 FilterNode
let n1 = ("name", OpValString::StartsWithCi("cmx"));
let n2 = ("status", OpValString::In(vec!["active".into(), "pending".into()]));

// 单组 = 组内 AND：(name LIKE cmx% AND status IN (...))
let both: FilterGroups = vec![n1.into(), n2.into()].into();
let cond: Condition = both.try_into().unwrap();

// 两组 = 组间 OR
let mut groups = FilterGroups::from(vec![n1]);
groups.add_group(vec![n2.into()]);
let or_cond: Condition = groups.try_into().unwrap();
```

### 四、HasFields 派生：INSERT 列/占位符生成

```rust
use modql::field::{Fields, HasFields};

#[derive(Fields)]                       // 生成 field_names/field_metas（忽略 id 之类的可加属性）
struct TodoCreat {
    title: String,
    done: bool,
}

// 通用 CRUD 可直接拼插入语句骨架
let cols = TodoCreat::sql_columns();             // title, done
let phs  = TodoCreat::sql_placeholders();        // ?, ?
let _sql = format!("INSERT INTO todo ({cols}) VALUES ({phs}) RETURNING *");
```

---

## Features 说明

| feature | 默认 | 拉入 | 说明 |
|---------|------|------|------|
| `modql-macros` | ✔（default） | 子 crate | `FilterNodes` / `Fields` 派生宏（不开则纯手写节点） |
| `with-sea-query` | — | `modql-macros` + `sea-query` + 宏的 with-sea-query | `Condition` 互转、`apply_to_sea_query`、`SeaFieldValue` 宏、sea_utils——**做 SQL 查询必开** |
| `with-ilike` | — | `sea-query/backend-postgres` | `OpValString::Ilike` 等 PG 专属算子的后端支持 |
| `with-rusqlite` | — | （空） | 仅保留声明：代码中有 `cfg` 引用但当前未启用，防 `unexpected_cfgs` 警告 |

---

## 值得注意的实现细节

- **内部 fork 的版本策略**：`Cargo.toml` 注释保留了上游 `version = "0.5.0"`，实际 `version.workspace = true`（0.1.12，registry nora 内部发布）。升级/对比上游时以 tags 为准，勿信版本号。
- **`#\[lints.rust\] unsafe_code = "forbid"`**：全 crate 禁 unsafe。
- **includes.rs 是占位**：`Includes` / `IncludeNode` 类型已定义但模块自注 `PLACEHOLDER for now. Not used yet.`，勿依赖。
- **与 rusqlite 相关的代码处于封存态**：`sqlite.rs`、`field/sqlite`、宏的 `Sqlite*` 系列均由未启用的 `with-rusqlite` 门控（examples 也已注释），CMX 线上只走 sea-query 链路。
- **条件挂点 `for_sea_condition`**：`FilterNode` 带 `Option<ForSeaCondition>`，配合 derive 属性 `to_sea_condition_fn` / `to_sea_value_fn` 可为单字段注入自定义 SQL 条件生成逻辑（cmx-auth 的加密字段查询等复杂场景用）。
