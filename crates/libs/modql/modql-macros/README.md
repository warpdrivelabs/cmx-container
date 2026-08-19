# modql-macros

> modql 的过程宏 crate：为业务结构体生成过滤节点转换（`FilterNodes`）、字段元数据（`Fields`）、sea-query 值转换（`SeaFieldValue`）等 impl，是 modql 过滤语言「derive 一下就能用」的发动机。**不面向最终用户直接依赖**——请经父 crate `modql` 的重导出使用。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2021-orange.svg)]()
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green.svg)]()
[![Authors](https://img.shields.io/badge/authors-jeremychone-lightgrey.svg)]()

> 注：与父 crate `modql` 同源的内部 fork（上游 jeremychone/rust-modql），版本跟随 workspace（0.1.12），保留自身 `edition 2021` 与 `MIT OR Apache-2.0` 许可。

---

## 项目简介

`modql` 的过滤语言建立在「业务 Filter 结构体 → `FilterNode` 列表 → `FilterGroups` → sea-query `Condition`」的转换链上。这条链的每一步 `From` / `TryFrom` impl 如果手写，每个实体都要几十行样板。本 crate 用过程宏把这层样板全部消掉：

- **`#[derive(FilterNodes)]`**：识别结构体中 `Option<OpVal*>` 类型的字段，为每个 `Some` 字段生成一个 `FilterNode`，并一次性生成 `IntoFilterNodes` + 三个 `From` impl（→ `Vec<FilterNode>` / `FilterGroup` / `FilterGroups`），开 `with-sea-query` 时再送一个 `TryFrom<YourFilter> for sea_query::Condition`。
- **`#[derive(Fields)]`**：生成 `HasFields` impl（`field_names` / `field_metas` 静态元数据 + `sql_columns` / `sql_placeholders` 便捷串），并支持 `#[field(...)]` 属性改名/跳过/类型转换。
- **`#[derive(SeaFieldValue)]`**（with-sea-query）：为单字段 tuple struct 与纯变体 enum 生成 `From<T> for sea_query::Value` + `sea_query::Nullable`。

### 架构位置

```text
业务 crate（cmx-biz / cmx-iam / cmx-auth …）
    #[derive(FilterNodes)] struct XxxFilter { ... }
        │  （宏在此展开生成 impl）
        ▼
modql（父 crate，重导出所有宏：modql::filter::FilterNodes 等）
        │  FilterNodes → FilterGroups → Condition
        ▼
sea-query → 参数化 SQL
```

本 crate 是 `proc-macro = true` 的纯编译期产物：运行时零代码、零依赖痕迹；唯一使用者是 `modql`（父 crate 把 `modql-macros` 作为 optional 依赖经 default feature `modql-macros` 拉入，并在 `filter` / `field` 模块重导出全部宏）。

---

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `syn`（2，full） | DeriveInput 解析、属性 Meta 匹配 |
| `quote` / `proc-macro2` | TokenStream 生成 |
| `sea-query`（optional，feature `with-sea-query`） | `SeaFieldValue` 生成代码里引用 `sea_query::Value` 等 |

### 下游使用方

| 使用方 | 引用方式 | 说明 |
|--------|---------|------|
| `modql`（父 crate） | `modql-macros = { workspace = true, optional = true }`，default feature 拉入；`with-sea-query` 联动其同名 feature | **唯一直接消费者**。modql 在 `filter` / `field` 模块重导出全部宏 |
| cmx-biz / cmx-iam / cmx-auth / cmx-plugin / cmx-database 等业务 crate | 经 `modql::filter::FilterNodes`、`modql::field::Fields` **间接使用** | 不直接依赖本 crate（description 明言 "Not intended to be used directly"） |

---

## 核心功能与特性

| 宏 | feature | 生成物 | 属性 |
|----|---------|--------|------|
| `FilterNodes` | 无条件 | `IntoFilterNodes` + `From<T> for Vec<FilterNode>` / `FilterGroup` / `FilterGroups`（+ with-sea-query 时 `TryFrom<T> for sea_query::Condition`） | `#[modql(...)]`（字段级）；结构体级 `#[modql(...)]` |
| `Fields` | 无条件 | `HasFields`（`field_names` / `field_metas`） | `#[field(...)]`（字段级） |
| `SeaFieldValue` | with-sea-query | `From<T> for sea_query::Value` + `sea_query::Nullable`（单字段 tuple struct / 纯变体 enum） | 无 |
| `SqliteFromRow` | with-rusqlite（未启用） | `rusqlite::Row → T` 行构造 | `#[field(...)]` |
| `SqliteFromValue` / `SqliteToValue` | with-rusqlite（未启用） | `FromSql` / `ToSql` | 无 |
| `FromSqliteRow` / `FromSqliteValue` / `ToSqliteValue` | with-rusqlite（未启用） | 上三者的 `#[deprecated]` 别名 | — |

---

## 模块结构

```text
modql-macros
├── src
│   ├── lib.rs                      # proc_macro 入口：全部 #[proc_macro_derive] 声明与文档
│   ├── derives_filter/
│   │   ├── mod.rs                  #   FilterNodes 展开：字段收集 → 5 组 impl 生成
│   │   └── utils.rs                #   #[modql(...)] 字段属性解析（MoqlFilterFieldAttr）
│   ├── derives_field/
│   │   ├── mod.rs                  #   Fields 入口
│   │   └── derive_fields.rs        #   HasFields/FieldMetas 静态生成（443 行，最大模块）
│   ├── derives_sea/
│   │   └── derive_field_sea_value.rs # (with-sea-query) SeaFieldValue：Value/Nullable impl
│   ├── derives_rusqlite/           # (with-rusqlite) Sqlite* 三宏（当前未启用，封存）
│   └── utils/
│       ├── mod.rs                  #   get_struct_fields / get_type_name / 属性读取通用件
│       ├── modql_field.rs          #   #[field(...)] 属性解析（ModqlFieldProp）
│       └── struct_modql_attr.rs    #   结构体级 #[modql(...)] 属性解析
└── Cargo.toml                      # [lib] proc-macro = true
```

---

## 关键 API（过程宏清单，`src/lib.rs`）

```rust
/// 为 Filter 结构体生成过滤节点转换（attributes(modql) 声明字段属性语法）
#[proc_macro_derive(FilterNodes, attributes(modql))]
pub fn derive_filter_nodes(input: TokenStream) -> TokenStream;

/// 为实体/创建结构体生成字段元数据（attributes(field, modql)）
#[proc_macro_derive(Fields, attributes(field, modql))]
pub fn derive_fields(input: TokenStream) -> TokenStream;

/// (with-sea-query) 单字段 tuple struct / 纯变体 enum → sea_query::Value + Nullable
#[cfg(feature = "with-sea-query")]
#[proc_macro_derive(SeaFieldValue)]
pub fn derive_field_sea_value(input: TokenStream) -> TokenStream;

/// (with-rusqlite，当前未启用) rusqlite 行/值互转三件套 + deprecated 别名
#[cfg(feature = "with-rusqlite")]
#[proc_macro_derive(SqliteFromRow, attributes(field, fields))]
pub fn derive_sqlite_from_row(input: TokenStream) -> TokenStream;
// 另有 SqliteFromValue / SqliteToValue 与 FromSqliteRow / FromSqliteValue / ToSqliteValue
```

### 字段属性速查

| 派生宏 | 属性写法 | 作用 |
|--------|---------|------|
| `FilterNodes` | `#[modql(rel = "project")]` | 该字段的节点挂 `rel` 前缀（也可结构体级统设） |
| | `#[modql(cast_as = "NUMERIC")]` | 生成节点的 `options.cast_as`（sea-query 值侧 cast） |
| | `#[modql(cast_column_as = "TEXT")]` | `options.cast_column_as`（列侧 cast） |
| | `#[modql(to_sea_condition_fn = "fn_name")]` | 注入自定义条件生成函数（`ToSeaConditionFnHolder`） |
| | `#[modql(to_sea_value_fn = "fn_name")]` | 注入自定义取值函数（`ToSeaValueFnHolder`） |
| `Fields` | `#[field(skip)]` | 该字段不进元数据 |
| | `#[field(name = "col_name")]` | 字段名 ≠ 列名时映射 |
| | `#[field(rel = "table")]` | 列引用带表前缀 |
| | `#[field(cast_as = "INT")]` | 值类型转换 |
| | `#[field(write_placeholder = "jsonb(?)")]` | 写入占位符定制（JSONB 列等） |

---

## 使用示例

> 所有宏都从 `modql` 引（`use modql::filter::FilterNodes` / `use modql::field::Fields`），不要直接依赖本 crate。

### 一、FilterNodes：业务过滤器一步到位（cmx-biz 真实模式）

```rust
use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

// derive 一次，得到：IntoFilterNodes + From → Vec<FilterNode>/FilterGroup/FilterGroups
// （开 with-sea-query 还送 TryFrom<DomainFilter> for sea_query::Condition）
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct DomainFilter {
    pub code: Option<OpValsString>,     // 只有 Option<OpVal*> 字段会被识别
    pub name: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub memo: String,                   // 非 Option<OpVal*> 字段被静默忽略（不生成节点）
}

// 因此这些写法全部直接可用：
use modql::filter::FilterGroups;
let _group:  FilterGroups = DomainFilter::default().into();   // From<T> for FilterGroups
let _cond: sea_query::Condition =
    FilterGroups::from(DomainFilter::default()).try_into().unwrap();
```

### 二、#[modql(...)] 字段属性：rel 前缀与 cast（关联表过滤 / 类型转换）

```rust
use modql::filter::{FilterNodes, OpValsString};

#[derive(Debug, Clone, FilterNodes, Default)]
pub struct ProjectFilter {
    /// 过滤挂在关联上：生成节点的 rel = Some("project")
    /// （结构体级 #[modql(rel = "project")] 可为全部字段统设，字段级优先）
    #[modql(rel = "project")]
    pub title: Option<OpValsString>,

    /// 列在 DB 里是 TEXT、比较前需转换：生成 FilterNodeOptions { cast_column_as: Some("BIGINT") }
    #[modql(cast_column_as = "BIGINT")]
    pub external_no: Option<OpValsString>,
}

// 展开效果等价于手写：
// FilterNode::new_with_rel(Some("project".into()), "title", opvals)
// FilterNode { options: FilterNodeOptions { cast_column_as: Some("BIGINT".into()), .. }, .. }
```

### 三、Fields + #[field(...)]：列名映射与 INSERT 骨架

```rust
use modql::field::{Fields, HasFields};

#[derive(Fields)]
struct TaskCreat {
    #[field(name = "task_title")]        // Rust 字段名 ≠ DB 列名
    title: String,
    #[field(rel = "t", cast_as = "INT")] // 列引用 "t"."done"，值侧 cast
    done: bool,
    #[field(skip)]                       // 不进元数据（服务端补的列）
    created_by: Option<String>,
}

// HasFields 由宏实现（created_by 被 skip，不进元数据）：
let cols = TaskCreat::sql_columns();        // 由 FieldMetas 拼出列引用串（含 name/rel/cast_as 映射）
let phs  = TaskCreat::sql_placeholders();   // ?, ?（每个非 skip 字段一个 ?）
let _sql = format!("INSERT INTO task ({cols}) VALUES ({phs})");
```

### 四、SeaFieldValue：enum 直接当 SQL 值用（with-sea-query）

```rust
use modql::field::SeaFieldValue;

/// 纯变体 enum：按变体名字符串化为 sea_query::Value::String（当前无 rename 支持）
#[derive(SeaFieldValue)]
pub enum DocKind {
    Md,
    Pdf,
    Unknown,
}
// 宏生成（示意）：
// impl From<DocKind> for sea_query::Value { /* → Value::String(Some("Md")) 等 */ }
// impl sea_query::Nullable for DocKind { /* → Value::String(None) */ }
// 于是 DocKind 可以直接出现在 sea-query 的值位置：query.value(DocKind::Pdf)

/// 单字段 tuple struct：内部基元直接映射（如 i64 → Value::BigInt）
#[derive(SeaFieldValue)]
pub struct EpochTime(pub i64);
```

---

## 附录：FilterNodes 展开对照

下面这段手写代码，就是 `#[derive(FilterNodes)]` 为例一 `DomainFilter` 生成的等价物（摘自 `derives_filter/mod.rs` 的代码生成逻辑，可当作排障时的心算模型）：

```rust
// 宏为每个 Option<OpVal*> 字段展开的节点构造（以 code 字段为例，with-sea-query 开启时）：
if let Some(val) = self.code {
    let op_vals: Vec<modql::filter::OpVal> = val.0.into_iter().map(|n| n.into()).collect();
    let fn_holder = None;   // 或 to_sea_condition_fn/to_sea_value_fn 注入的 holder
    let node = modql::filter::FilterNode {
        rel: None,                                   // 或 #[modql(rel = ...)] / 结构体级 rel
        name: "code".to_string(),                    // stringify!(字段名)，非列名
        opvals: op_vals,
        options: modql::filter::FilterNodeOptions { cast_as: None, cast_column_as: None },
        for_sea_condition: fn_holder,
    };
    nodes.push(node);
}

// 以及四个 From/TryFrom impl：
impl modql::filter::IntoFilterNodes for DomainFilter {
    fn filter_nodes(self, rel: Option<String>) -> Vec<modql::filter::FilterNode> { /* 上面的循环 */ }
}
impl From<DomainFilter> for Vec<modql::filter::FilterNode> { /* filter_nodes(val, None) */ }
impl From<DomainFilter> for modql::filter::FilterGroup { /* 经 Vec<FilterNode> 中转 */ }
impl From<DomainFilter> for modql::filter::FilterGroups { /* 同上 */ }
impl TryFrom<DomainFilter> for sea_query::Condition {   // 仅 with-sea-query
    type Error = modql::filter::IntoSeaError;
    /* FilterGroup::from(val).try_into() */
}
```

注意 `name` 取的是 **Rust 字段名字符串**（`stringify!`），不是 `#[serde(rename)]` 也不是 `#[field(name)]`——若过滤字段名与 DB 列名不同，需用 `#[modql(rel)]` + 服务侧列映射，或调整 Filter 结构体字段命名。

---

## Features 说明

| feature | 说明 |
|---------|------|
| `with-sea-query` | 拉入 sea-query 依赖，启用 `SeaFieldValue` 宏，并让 `FilterNodes` 展开时额外生成 `TryFrom<T> for sea_query::Condition` 与 `for_sea_condition` 挂点填充。由父 crate 的 `with-sea-query` 联动开启，无需手工指定 |
| `with-rusqlite` | 空声明（同父 crate：代码有 `cfg` 引用但未启用，防 `unexpected_cfgs` 警告）。启用后解锁 `SqliteFromRow` / `SqliteFromValue` / `SqliteToValue` 三宏及 deprecated 别名 |

无 default feature——本 crate 的拉入与开关完全由 `modql` 父 crate 的 feature 图控制。

---

## 值得注意的实现细节与已知限制

- **FilterNodes 只识别 `Option<OpVal*>` 字段**：实现按字段类型名字符串匹配（前缀 `"Option "` 且包含 `"OpVal"`），源码注释明确不支持类型别名与非标准写法的 `Option`（如全限定路径）。其余字段静默忽略——过滤器结构体里混入普通业务字段是安全的，但别指望它们参与过滤。
- **`to_sea_condition_fn` 与 `to_sea_value_fn` 同设不会报错**：源码标注 `TODO: Fail if both ... are defined`，当前行为是 condition_fn 优先，注意自查。
- **两套字段属性前缀易混**：`FilterNodes` 的字段属性是 `#[modql(...)]`（`#[proc_macro_derive(FilterNodes, attributes(modql))]`），`Fields` 的才是 `#[field(...)]`——写错属性名会落到 `attributes` 未声明而编译失败。
- **lib.rs 文档示例与宏名不一致**：`SeaFieldValue` 的文档注释中 tuple struct 示例写作 `#[derive(modql::field::Field)]`，而实际宏名是 `SeaFieldValue`（同段 enum 示例用的才是正确名）。照抄文档示例会编译失败，以本 README 与 `derives_sea/derive_field_sea_value.rs` 为准。
- **宏在 `cfg!` 编译期分支生成不同代码**：`FilterNodes` 的展开随本 crate 自身的 `with-sea-query` feature 变化（父 crate 联动），因此 modql 与 modql-macros 的 feature 状态必须一致——workspace 已用 `with-sea-query = ["modql-macros", "sea-query", "modql-macros/with-sea-query"]` 保证。
