# cmx SQL 参数封装重构 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `Vec<DataValue>` 体系补齐「带类型 NULL + Optional 构造糖 + 占位符 builder」,根治 NULL 丢失类型、构造冗长、占位符漂移三大痛点。

**Architecture:** 分层设计 —— `SqlTypeMarker`(纯标记枚举)、`DataValue::NullTyped` 变体、`From<Option<T>>`、`dv!` 宏、`ParamsBuilder` 全部放 cmx-core(无 sqlx 依赖,wasm 可用);sqlx 绑定翻译、`SqlParam` 枚举、`*_typed` API 放 cmx-database。**判定原则:只操作 DataValue/字符串、不需要 sqlx 的工具一律放 cmx-core**,确保 `cmx-plugin-sdk`(wasm 侧)可用。cmx-iam 的 permission/rule 作为首批重构验证。

**Tech Stack:** Rust, sqlx 0.9, serde, cmx-core/cmx-database/cmx-iam

**设计依据:** 见 `20260625_cmx-sql-param-封装重构方案.md`(已确认决策:SqlTypeMarker 在 cmx-core;Array 实现单层同类型;iam 先重构 permission+rule)。

---

## 文件结构总览

| 层 | 文件 | 职责 | 操作 |
|----|------|------|------|
| cmx-core | `crates/libs/cmx-core/src/model/cell.rs` | SqlTypeMarker 枚举、DataValue::NullTyped 变体、SqlParam 枚举、From<Option<T>>、dv! 宏、ParamsBuilder | 修改 |
| cmx-core | `crates/libs/cmx-core/src/wasm_types/database.rs` | DbRequest.data_values 变体(打通 wasm 边界) | 修改 |
| cmx-database | `crates/libs/cmx-infra/cmx-database/src/executor/mod.rs` | bind 函数识别 NullTyped/Array | 修改 |
| cmx-database | `crates/libs/cmx-infra/cmx-database/src/manager/mod.rs` | *_typed 新 API | 修改 |
| cmx-database | `crates/libs/cmx-infra/cmx-database/src/transaction/api.rs` | SqlParams::Typed 变体 | 修改 |
| cmx-database | `crates/libs/cmx-infra/cmx-database/src/host_functions.rs` | do_query/do_execute 识别 data_values 走 datavalues 分支 | 修改 |
| cmx-database | `crates/libs/cmx-infra/cmx-database/src/lib.rs` | re-export 新类型 | 修改 |
| cmx-iam | `crates/libs/cmx-iam/src/permission/service.rs` | 应用新糖重构 | 修改 |
| cmx-iam | `crates/libs/cmx-iam/src/rule/service.rs` | 应用新糖 + builder 重构 | 修改 |
| 测试 | `crates/libs/cmx-core/tests/dv_macro.rs` 等 | 单元测试 | 新建 |

---

## Task 1: SqlTypeMarker 枚举 + DataValue::NullTyped 变体

**Files:**
- Modify: `crates/libs/cmx-core/src/model/cell.rs`(枚举定义区 L28-50 + Serialize/Deserialize 区)

- [ ] **Step 1: 在 cell.rs 添加 SqlTypeMarker 枚举**

在 `DataValue` 枚举定义**之前**(L28 前)插入:

```rust
/// SQL 列类型标记(不依赖 sqlx,用于描述 NULL 的目标绑定类型)。
///
/// 仅在 [`DataValue::NullTyped`] 中携带,告知绑定层
/// 这个 NULL 应绑定为哪种数据库列类型。
///
/// # 设计动机
///
/// sqlx 绑定 `None::<T>` 时,目标类型由 `T` 决定。
/// 若 NULL 占位符对应非字符串列(INTEGER/TIMESTAMP/UUID 等),
/// 绑定 `None::<String>` 会导致 PostgreSQL prepare 类型不匹配。
/// `NullTyped` 让调用方显式声明目标类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlTypeMarker {
    Bool,
    Int,        // i64 / BIGINT
    Float,      // f64 / DOUBLE PRECISION
    Decimal,    // NUMERIC
    Text,       // TEXT / VARCHAR
    Timestamp,  // TIMESTAMPTZ
    Date,       // DATE
    Uuid,
    Json,       // JSONB
    Binary,     // BYTEA
}
```

- [ ] **Step 2: 给 DataValue 枚举添加 NullTyped 变体**

在 `cell.rs` 的 `DataValue` 枚举中,`Null` 变体之后插入:

```rust
pub enum DataValue {
    Null,
    /// 带类型信息的 NULL。
    ///
    /// 绑定到非字符串列(INTEGER/TIMESTAMP/UUID 等)时必须使用,
    /// 否则绑定层无法确定 NULL 的目标 SQL 类型。
    /// 序列化为普通 null(类型信息仅用于绑定,不参与传输)。
    NullTyped(SqlTypeMarker),
    Bool(bool),
    // ... 其余变体保持不变
}
```

- [ ] **Step 3: 更新 Serialize 实现(NullTyped 序列化为 null)**

找到 cell.rs 中 DataValue 的 `Serialize` impl(约 L64-191),在 `DataValue::Null` 分支旁补 `NullTyped`:

```rust
// 在 match 块内,Null 分支之后:
DataValue::Null | DataValue::NullTyped(_) => serializer.serialize_unit()?,
```

(即把 NullTyped 与 Null 合并到同一分支,都序列化为 null。)

- [ ] **Step 4: 更新 Deserialize 实现(NullTyped 不可反序列化)**

Deserialize 无需特殊处理:由于 NullTyped 序列化为 null,反序列化 null 时只会得到 `Null`。确认现有 Deserialize 的 `JsonValue::Null => DataValue::Null` 分支已覆盖,**无需改动**。

- [ ] **Step 5: 编译验证(预期会有 exhaustive match 错误)**

```bash
cargo check -p cmx-core 2>&1 | grep "error\[E0004\]" | head
```
Expected: 可能无错误(cmx-core 内部 match 较少),或少量 exhaustive match 错误。若有,在本文件内补全 `NullTyped` 分支。

- [ ] **Step 6: 全 workspace 编译,定位所有 exhaustive match 错误**

```bash
cargo check --workspace 2>&1 | grep "NullTyped" | head -30
```
Expected: 列出所有需要补 `NullTyped` 分支的 match 表达式(主要在 cmx-database 的 bind 函数,留到 Task 4 处理)。**记录这些位置,Task 4 会逐个补全**。

- [ ] **Step 7: 提交**

```bash
git add crates/libs/cmx-core/src/model/cell.rs
git commit -m "feat(cmx-core): 新增 SqlTypeMarker 与 DataValue::NullTyped 变体(带类型 NULL)"
```

---

## Task 2: From<Option<T>> 构造糖

**Files:**
- Modify: `crates/libs/cmx-core/src/model/cell.rs`(From 实现区,约 L197-277)

- [ ] **Step 1: 在现有 From 实现块后追加 Option 转换**

在 cell.rs 的 From 实现区(最后一个 `impl From<...> for DataValue` 之后)添加:

```rust
// ==========================================
// Option<T> 构造糖 —— 消除 .map(DataValue::X).unwrap_or(DataValue::Null)
// ==========================================

impl From<Option<String>> for DataValue {
    fn from(v: Option<String>) -> Self {
        v.map(DataValue::String).unwrap_or(DataValue::Null)
    }
}

impl From<Option<&str>> for DataValue {
    fn from(v: Option<&str>) -> Self {
        v.map(|s| DataValue::String(s.to_string())).unwrap_or(DataValue::Null)
    }
}

impl From<Option<i64>> for DataValue {
    fn from(v: Option<i64>) -> Self {
        v.map(DataValue::Int).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Int))
    }
}

impl From<Option<i32>> for DataValue {
    fn from(v: Option<i32>) -> Self {
        v.map(|i| DataValue::Int(i as i64)).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Int))
    }
}

impl From<Option<f64>> for DataValue {
    fn from(v: Option<f64>) -> Self {
        v.map(DataValue::Float).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Float))
    }
}

impl From<Option<bool>> for DataValue {
    fn from(v: Option<bool>) -> Self {
        v.map(DataValue::Bool).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Bool))
    }
}

impl From<Option<Uuid>> for DataValue {
    fn from(v: Option<Uuid>) -> Self {
        v.map(DataValue::Uuid).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Uuid))
    }
}

impl From<Option<DateTime<Utc>>> for DataValue {
    fn from(v: Option<DateTime<Utc>>) -> Self {
        v.map(DataValue::DateTime).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Timestamp))
    }
}

impl From<Option<Decimal>> for DataValue {
    fn from(v: Option<Decimal>) -> Self {
        v.map(DataValue::Decimal).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Decimal))
    }
}
```

> **关键:** 整型/时间/Uuid 等 `Option` 的 None 走 `NullTyped(对应类型)`,而非 `Null`,这样绑定到非字符串列时类型正确。字符串的 None 走 `Null`(默认 TEXT,兼容旧行为)。

- [ ] **Step 2: 编译验证**

```bash
cargo check -p cmx-core 2>&1 | grep -E "error|warning: conflicting" | head
```
Expected: 无 error。注意检查是否有 `conflicting implementations`(因为已有 `From<String>`,新增 `From<Option<String>>` 不冲突,但要确认)。

- [ ] **Step 3: 提交**

```bash
git add crates/libs/cmx-core/src/model/cell.rs
git commit -m "feat(cmx-core): DataValue 实现 From<Option<T>> 构造糖"
```

---

## Task 3: dv! 宏 + helper 函数

**Files:**
- Modify: `crates/libs/cmx-core/src/model/cell.rs`(宏定义)
- Modify: `crates/libs/cmx-core/src/lib.rs`(宏导出,若需要跨 crate)

- [ ] **Step 1: 在 cell.rs 添加 dv! 宏**

在文件末尾(impl 块之后)添加宏定义:

```rust
/// 构造 `Vec<DataValue>` 的便捷宏。
///
/// # 语法
/// - `str expr`    → `DataValue::String(expr)`(expr: String 或 &str)
/// - `int expr`    → `DataValue::Int(expr)`(expr: i64)
/// - `str? expr`   → `Option<String>` 的糖(None → Null)
/// - `int? expr`   → `Option<i64>` 的糖(None → NullTyped(Int))
/// - `bool expr`   / `bool? expr`   → 同理
/// - `ts? expr`    → `Option<DateTime<Utc>>` 的糖
/// - `uuid? expr`  → `Option<Uuid>` 的糖
/// - `null T`      → `DataValue::NullTyped(SqlTypeMarker::T)`
///
/// # 示例
/// ```ignore
/// let params: Vec<DataValue> = dv![
///     str  id,
///     str? name,           // Option<String>
///     int? sort_order,     // Option<i64>,None 时带 Int 类型
///     null Uuid,           // 显式带类型的 NULL
/// ];
/// ```
#[macro_export]
macro_rules! dv {
    // 空参数
    () => { ::std::vec::Vec::<$crate::model::cell::DataValue>::new() };

    // null + 类型标记
    (null $t:ident) => {
        $crate::model::cell::DataValue::NullTyped($crate::model::cell::SqlTypeMarker::$t)
    };

    // 单个元素(结尾,递归终止)
    ($kind:ident $e:expr) => {
        $crate::model::cell::__dv_one($kind, $e)
    };
    ($kind:tt ? $e:expr) => {
        $crate::model::cell::__dv_opt($kind, $e)
    };

    // 多元素:首元素 + 剩余
    ($kind:ident $e:expr, $($rest:tt)+) => {
        {
            let mut v = ::std::vec![$crate::model::cell::__dv_one($kind, $e)];
            v.extend($crate::dv!($($rest)+));
            v
        }
    };
    ($kind:tt ? $e:expr, $($rest:tt)+) => {
        {
            let mut v = ::std::vec![$crate::model::cell::__dv_opt($kind, $e)];
            v.extend($crate::dv!($($rest)+));
            v
        }
    };
}
```

- [ ] **Step 2: 在 cell.rs 添加宏驱动的 helper 函数(非 pub,仅宏内部用)**

```rust
// ==========================================
// dv! 宏内部 helper(不对外暴露,仅宏调用)
// ==========================================

/// 供 dv! 宏调用:把类型标记 token 映射到构造函数。
/// 注意:这里的 `kind` 参数不是值,而是宏传递的占位,
/// 实际通过 const 匹配分发。
#[doc(hidden)]
pub fn __dv_str<S: Into<String>>(v: S) -> DataValue {
    DataValue::String(v.into())
}
#[doc(hidden)]
pub fn __dv_int(v: i64) -> DataValue {
    DataValue::Int(v)
}
#[doc(hidden)]
pub fn __dv_bool(v: bool) -> DataValue {
    DataValue::Bool(v)
}

/// 必填元素的统一入口(宏根据 $kind 选择 helper)。
/// 因宏无法直接做函数选择,这里用 trait + 泛型分发。
/// 见 Step 2 的替代实现(下方使用 trait 方式更简洁)。
#[doc(hidden)]
pub fn __dv_one<T: Into<DataValue>>(_marker: impl DvKind, v: T) -> DataValue {
    v.into()
}
```

> ⚠️ **实现注意:** 宏的 `$kind:ident` 分发在纯宏层面较繁琐。更稳健的方式是用 **`From` trait 驱动**——宏只负责收集表达式,类型推断交给 `Into<DataValue>`。若 Step 1 的宏展开有问题,改用如下简化版:

```rust
// 简化版 dv! —— 纯 From 驱动,每个元素须满足 Into<DataValue>
#[macro_export]
macro_rules! dv {
    () => { ::std::vec::Vec::<$crate::model::cell::DataValue>::new() };
    (null $t:ident) => {
        $crate::model::cell::DataValue::NullTyped($crate::model::cell::SqlTypeMarker::$t)
    };
    ($($e:expr),+ $(,)?) => {
        ::std::vec![
            $(::<$crate::model::cell::DataValue>::from($e)),+
        ].into_iter().map(|dv| dv).collect::<::std::vec::Vec<_>>()
    };
}
```

调用方式:`dv![id.clone(), data.name.clone(), data.sort_order]`(每个 expr 须 `Into<DataValue>`)。**推荐采用此简化版**,配合 Task 2 的 `From<Option<T>>`,可空字段直接传 `Option` 值。

- [ ] **Step 3: 编写单元测试验证宏**

Create: `crates/libs/cmx-core/tests/dv_macro.rs`

```rust
use cmx_core::model::cell::{DataValue, SqlTypeMarker};

// 注意:dv! 宏通过 #[macro_export] 导出,路径为 cmx_core::dv!
// 需确认 cmx-core 是否 re-export。若宏用 $crate 路径,跨 crate 调用为 cmx_core::dv!

#[test]
fn dv_empty() {
    let v: Vec<DataValue> = cmx_core::dv!();
    assert!(v.is_empty());
}

#[test]
fn dv_string_and_option() {
    let name: Option<String> = Some("alice".into());
    let desc: Option<String> = None;
    let v = cmx_core::dv![
        "id123".to_string(),
        name,
        desc,
    ];
    assert_eq!(v.len(), 3);
    assert_eq!(v[0], DataValue::String("id123".into()));
    assert_eq!(v[1], DataValue::String("alice".into()));
    assert_eq!(v[2], DataValue::Null);
}

#[test]
fn dv_option_int_null_typed() {
    let n: Option<i64> = None;
    let v = cmx_core::dv![n];
    assert_eq!(v[0], DataValue::NullTyped(SqlTypeMarker::Int));
}

#[test]
fn dv_null_marker() {
    let v: DataValue = cmx_core::dv!(null Uuid);
    assert_eq!(v, DataValue::NullTyped(SqlTypeMarker::Uuid));
}
```

- [ ] **Step 4: 运行测试验证**

```bash
cargo test -p cmx-core --test dv_macro 2>&1 | tail -20
```
Expected: 4 个测试通过。若宏展开失败,回退到 Step 2 的简化版宏,调整测试以匹配简化版语法。

- [ ] **Step 5: 提交**

```bash
git add crates/libs/cmx-core/src/model/cell.rs crates/libs/cmx-core/tests/dv_macro.rs
git commit -m "feat(cmx-core): 新增 dv! 宏用于批量构造 Vec<DataValue>"
```

---

## Task 4: bind 层识别 NullTyped(postgres/mysql/sqlite)

**Files:**
- Modify: `crates/libs/cmx-infra/cmx-database/src/executor/mod.rs:128-197`(三个 bind 函数)

- [ ] **Step 1: 修改 bind_data_value_postgres(添加 NullTyped + Array 分支)**

将 `bind_data_value_postgres`(executor/mod.rs:128-149)的 match 替换为:

```rust
pub fn bind_data_value_postgres<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    use cmx_core::model::cell::SqlTypeMarker::*;
    match param {
        DataValue::Null => query.bind(None::<String>),
        DataValue::NullTyped(t) => match t {
            Bool      => query.bind(None::<bool>),
            Int       => query.bind(None::<i64>),
            Float     => query.bind(None::<f64>),
            Decimal   => query.bind(None::<rust_decimal::Decimal>),
            Text      => query.bind(None::<String>),
            Timestamp => query.bind(None::<chrono::DateTime<chrono::Utc>>),
            Date      => query.bind(None::<chrono::NaiveDate>),
            Uuid      => query.bind(None::<uuid::Uuid>),
            Json      => query.bind(None::<serde_json::Value>),
            Binary    => query.bind(None::<Vec<u8>>),
        },
        DataValue::Bool(v)   => query.bind(*v),
        DataValue::Int(v)    => query.bind(*v),
        DataValue::Float(v)  => query.bind(*v),
        DataValue::String(v) => query.bind(v.as_str()),
        DataValue::Decimal(v)=> query.bind(*v),
        DataValue::DateTime(v)=> query.bind(*v),
        DataValue::Date(v)   => query.bind(*v),
        DataValue::Json(v)   => query.bind(v.clone()),
        DataValue::Binary(v) => query.bind(v.as_slice()),
        DataValue::Uuid(v)   => query.bind(*v),
        DataValue::Array(els) => bind_pg_array_postgres(query, els),
        DataValue::ShortStr(s) => query.bind(s.as_str()),
        DataValue::LongStr(s)  => query.bind(s.as_str()),
    }
}
```

- [ ] **Step 2: 实现 PG 数组绑定辅助函数**

在 executor/mod.rs 中(bind 函数附近)添加:

```rust
/// 将单层同类型数组绑定为 PostgreSQL 数组。
///
/// 元素类型由首个元素推断;空数组绑定为空 text 数组。
/// 仅支持单层、元素同类型(对应 cmx-iam 的 IN 查询场景)。
fn bind_pg_array_postgres<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    els: &'q [DataValue],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    if els.is_empty() {
        return query.bind(None::<Vec<String>>);
    }
    // 按首个元素类型分发
    match &els[0] {
        DataValue::String(_) => {
            let v: Vec<&str> = els.iter().filter_map(|e| match e {
                DataValue::String(s) => Some(s.as_str()), _ => None
            }).collect();
            query.bind(v)
        }
        DataValue::Int(_) => {
            let v: Vec<i64> = els.iter().filter_map(|e| match e {
                DataValue::Int(i) => Some(*i), _ => None
            }).collect();
            query.bind(v)
        }
        DataValue::Uuid(_) => {
            let v: Vec<uuid::Uuid> = els.iter().filter_map(|e| match e {
                DataValue::Uuid(u) => Some(*u), _ => None
            }).collect();
            query.bind(v)
        }
        _ => query.bind(None::<Vec<String>>),  // 不支持的元素类型退化为 NULL
    }
}
```

- [ ] **Step 3: 修改 bind_data_value_mysql**

将 `bind_data_value_mysql`(mod.rs:153-173)的 match 替换。MySQL 驱动对原生类型支持弱,`NullTyped` 统一走字符串兜底(MySQL 的 NULL 无类型):

```rust
pub fn bind_data_value_mysql<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match param {
        // MySQL 的 NULL 无类型区分,统一 None
        DataValue::Null | DataValue::NullTyped(_) => query.bind(None::<String>),
        DataValue::Bool(v)   => query.bind(*v),
        DataValue::Int(v)    => query.bind(*v),
        DataValue::Float(v)  => query.bind(*v),
        DataValue::String(v) => query.bind(v.clone()),
        DataValue::Decimal(v)=> query.bind(v.to_string()),
        DataValue::DateTime(v)=> query.bind(v.to_rfc3339()),
        DataValue::Date(v)   => query.bind(v.to_string()),
        DataValue::Json(v)   => query.bind(v.to_string()),
        DataValue::Binary(v) => query.bind(v.as_slice()),
        DataValue::Uuid(v)   => query.bind(v.to_string()),
        // MySQL 数组:序列化为逗号分隔(MySQL 无原生数组)
        DataValue::Array(els) => {
            let s = els.iter().map(|e| match e {
                DataValue::String(s) => s.clone(),
                other => format!("{:?}", other),
            }).collect::<Vec<_>>().join(",");
            query.bind(s)
        }
        DataValue::ShortStr(s) => query.bind(s.to_string()),
        DataValue::LongStr(s)  => query.bind(s.to_string()),
    }
}
```

- [ ] **Step 4: 修改 bind_data_value_sqlite**

与 MySQL 类似(SQLite 动态类型,NULL 无类型):

```rust
pub fn bind_data_value_sqlite<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match param {
        DataValue::Null | DataValue::NullTyped(_) => query.bind(None::<String>),
        DataValue::Bool(v)   => query.bind(*v),
        DataValue::Int(v)    => query.bind(*v),
        DataValue::Float(v)  => query.bind(*v),
        DataValue::String(v) => query.bind(v.clone()),
        DataValue::Decimal(v)=> query.bind(v.to_string()),
        DataValue::DateTime(v)=> query.bind(v.to_rfc3339()),
        DataValue::Date(v)   => query.bind(v.to_string()),
        DataValue::Json(v)   => query.bind(v.to_string()),
        DataValue::Binary(v) => query.bind(v.as_slice()),
        DataValue::Uuid(v)   => query.bind(v.to_string()),
        // SQLite 数组:序列化为 JSON 字符串
        DataValue::Array(els) => query.bind(serde_json::to_string(els).unwrap_or_default()),
        DataValue::ShortStr(s) => query.bind(s.to_string()),
        DataValue::LongStr(s)  => query.bind(s.to_string()),
    }
}
```

- [ ] **Step 5: 全 workspace 编译(补全其它 exhaustive match)**

```bash
cargo check --workspace 2>&1 | grep -E "error\[E0004\]" -A 3 | head -40
```
Expected: 列出其它未处理 NullTyped 的 match(如 cmx-database 的 from_row、cell.rs 的 TryFrom 等)。逐个补全 `NullTyped` 分支(通常映射到 Null 或跳过)。

- [ ] **Step 6: 提交**

```bash
git add crates/libs/cmx-infra/cmx-database/src/executor/mod.rs
# 以及 Step 5 中补全的其它文件
git commit -m "fix(cmx-database): bind 层识别 NullTyped 携带类型 + 修复 Array/ShortStr/LongStr 绑定"
```

---

## Task 5: SqlParam 枚举 + From 互通(cmx-core)

> **放置位置:** `cmx-core`,与 DataValue/SqlTypeMarker 同处。
>
> **理由:** SqlParam 是 DataValue 的上层封装,宿主端和 wasm plugin 都应能用。
> 放 cmx-core 确保 `cmx-plugin-sdk`(wasm 侧)也能引用。

**Files:**
- Modify: `crates/libs/cmx-core/src/model/cell.rs`(在 SqlTypeMarker 之后)
- Modify: `crates/libs/cmx-core/src/lib.rs`(re-export)

- [ ] **Step 1: 在 cell.rs 添加 SqlParam 枚举**

在 `SqlTypeMarker` 枚举定义之后(Task 1 已添加)插入:

```rust
/// 面向 SQL 绑定的参数类型,内含带类型的 NULL。
///
/// 比 [`DataValue`] 更贴近 SQL 语义,适合手写 SQL 时
/// 需要精确控制 NULL 目标类型的场景。
/// 宿主端和 wasm plugin 都可使用。
/// 可通过 `From`/`Into` 与 `DataValue` 互通。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SqlParam {
    Null(SqlTypeMarker),
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Decimal(Decimal),
    Timestamp(DateTime<Utc>),
    Date(NaiveDate),
    Uuid(Uuid),
    Json(String),
    Binary(Vec<u8>),
    /// 单层同类型数组(IN 查询用)
    Array(Vec<SqlParam>),
}

impl From<DataValue> for SqlParam {
    fn from(v: DataValue) -> Self {
        match v {
            DataValue::Null => SqlParam::Null(SqlTypeMarker::Text),
            DataValue::NullTyped(t) => SqlParam::Null(t),
            DataValue::Bool(b) => SqlParam::Bool(b),
            DataValue::Int(i) => SqlParam::Int(i),
            DataValue::Float(f) => SqlParam::Float(f),
            DataValue::String(s) => SqlParam::Text(s),
            DataValue::Decimal(d) => SqlParam::Decimal(d),
            DataValue::DateTime(dt) => SqlParam::Timestamp(dt),
            DataValue::Date(d) => SqlParam::Date(d),
            DataValue::Json(s) => SqlParam::Json(s),
            DataValue::Binary(b) => SqlParam::Binary(b),
            DataValue::Uuid(u) => SqlParam::Uuid(u),
            DataValue::Array(els) => SqlParam::Array(els.into_iter().map(Into::into).collect()),
            DataValue::ShortStr(s) => SqlParam::Text(s.to_string()),
            DataValue::LongStr(s) => SqlParam::Text(s.to_string()),
        }
    }
}

impl From<SqlParam> for DataValue {
    fn from(p: SqlParam) -> Self {
        match p {
            SqlParam::Null(t) => DataValue::NullTyped(t),
            SqlParam::Bool(b) => DataValue::Bool(b),
            SqlParam::Int(i) => DataValue::Int(i),
            SqlParam::Float(f) => DataValue::Float(f),
            SqlParam::Text(s) => DataValue::String(s),
            SqlParam::Decimal(d) => DataValue::Decimal(d),
            SqlParam::Timestamp(dt) => DataValue::DateTime(dt),
            SqlParam::Date(d) => DataValue::Date(d),
            SqlParam::Json(s) => DataValue::Json(s),
            SqlParam::Binary(b) => DataValue::Binary(b),
            SqlParam::Uuid(u) => DataValue::Uuid(u),
            SqlParam::Array(els) => DataValue::Array(els.into_iter().map(Into::into).collect()),
        }
    }
}
```

- [ ] **Step 2: 在 cmx-core/lib.rs re-export**

修改 `crates/libs/cmx-core/src/lib.rs`,确保 SqlParam 可被跨 crate 引用:

```rust
pub use model::cell::{DataValue, SqlTypeMarker, SqlParam};
```

- [ ] **Step 3: 编译验证 + 提交**

```bash
cargo check -p cmx-core 2>&1 | grep "error" | head
git add crates/libs/cmx-core/src/model/cell.rs crates/libs/cmx-core/src/lib.rs
git commit -m "feat(cmx-core): 新增 SqlParam 枚举(wasm 可用),与 DataValue 互通"
```

---

## Task 6: 新 API query_sql_typed / execute_sql_typed

**Files:**
- Modify: `crates/libs/cmx-infra/cmx-database/src/transaction/api.rs`(SqlParams 枚举 + 执行函数)
- Modify: `crates/libs/cmx-infra/cmx-database/src/manager/mod.rs`(DatabaseManager 方法)

- [ ] **Step 1: 在 SqlParams 枚举添加 Typed 变体**

修改 `transaction/api.rs:237-244`:

```rust
pub enum SqlParams {
    Json(serde_json::Value),
    DataValues(Vec<DataValue>),
    SqlxValues(SqlxValues),
    /// 强类型参数(带类型 NULL 支持)。SqlParam 来自 cmx-core。
    Typed(Vec<cmx_core::model::cell::SqlParam>),
}
```

- [ ] **Step 2: 在 execute_sql_with_params 处理 Typed 分支**

在 `execute_sql_with_params`(api.rs:463)的两个 match(pool 和 txn 路径)中,把 Typed 转成 DataValues 后走现有逻辑:

```rust
SqlParams::Typed(params) => {
    let values: Vec<DataValue> = params.into_iter().map(Into::into).collect();
    // txn 路径:
    txn.execute_with_datavalues(&sql, &values).await?
    // pool 路径:
    pool.execute_with_datavalues(&sql, &values).await
}
```

(在 `query_sql_with_params` 中同理处理。)

- [ ] **Step 3: 在 DatabaseManager 添加 *_typed 方法**

修改 `manager/mod.rs`,在 `execute_sql_with_datavalues`(L260)和 `query_sql_with_datavalues`(L324)之后添加:

```rust
/// 执行 SQL(强类型参数,支持带类型 NULL)。
pub async fn execute_sql_typed(
    &self,
    db_id: &str,
    txn_id: Option<&str>,
    sql: &str,
    params: Vec<cmx_core::model::cell::SqlParam>,
) -> Result<u64> {
    crate::transaction::execute_sql_with_params(
        db_id, txn_id, sql,
        crate::transaction::SqlParams::Typed(params),
    ).await
}

/// 查询 SQL(强类型参数,支持带类型 NULL)。
pub async fn query_sql_typed(
    &self,
    db_id: &str,
    txn_id: Option<&str>,
    sql: &str,
    params: Vec<cmx_core::model::cell::SqlParam>,
    dataset_id: &str,
) -> Result<DataSet> {
    crate::transaction::query_sql_with_params(
        db_id, txn_id, sql,
        crate::transaction::SqlParams::Typed(params),
        dataset_id,
    ).await
}
```

- [ ] **Step 4: 编译验证 + 提交**

```bash
cargo check -p cmx-database 2>&1 | grep "error" | head
git add crates/libs/cmx-infra/cmx-database/src/transaction/api.rs crates/libs/cmx-infra/cmx-database/src/manager/mod.rs
git commit -m "feat(cmx-database): 新增 query_sql_typed/execute_sql_typed 强类型 API"
```

---

## Task 7: ParamsBuilder

> **放置位置:** `cmx-core`,与 `DataValue`/`SqlTypeMarker`/`dv!` 同处。
>
> **为什么不在 cmx-database:** `ParamsBuilder` 只操作 `DataValue` + 字符串,
> 零 sqlx/tokio 依赖。而 `cmx-database` 依赖 sqlx,无法编译到 wasm。
> `cmx-plugin-sdk`(wasm 侧)依赖 `cmx-core` 但**不依赖** `cmx-database`,
> 若放在 cmx-database 会阻断 wasm plugin 使用 builder 构造参数。
> cmx-core 的依赖(serde/chrono/uuid 等)全部 wasm 兼容。

**Files:**
- Create: `crates/libs/cmx-core/src/model/builder.rs`
- Modify: `crates/libs/cmx-core/src/model/mod.rs`(mod 声明 + re-export)
- Modify: `crates/libs/cmx-core/src/lib.rs`(re-export,供跨 crate 使用)
- Test: `crates/libs/cmx-core/tests/params_builder.rs`

- [ ] **Step 1: 创建 builder.rs**

Create: `crates/libs/cmx-core/src/model/builder.rs`

```rust
//! 动态 UPDATE SET 子句 + 参数构造器。
//!
//! 解决手写动态 UPDATE 时「SQL SET 子句顺序」与「params push 顺序」
//! 必须双重一致、极易漂移的问题。builder 自动管理占位符编号。
//!
//! 纯域构造工具,无 sqlx 依赖,wasm 可用。

use crate::model::cell::DataValue;

/// 动态 SET 子句构造器。
///
/// 自动管理 `$N` 占位符编号,消除「SQL SET 子句顺序」与
/// 「params Vec 顺序」必须双重一致的漂移风险。
///
/// # 示例
///
/// ```
/// use cmx_core::ParamsBuilder;
/// use cmx_core::model::cell::DataValue;
///
/// let mut b = ParamsBuilder::new(1); // WHERE id = $1 已占用,SET 从 $2 起
/// b.set("name", "alice".to_string())
///  .set_opt("sort_order", Some(5_i64))
///  .set_opt("description", None::<String>); // None → 跳过该列
/// let (set_clause, params) = b.build();
/// assert_eq!(set_clause, "name = $2, sort_order = $3");
/// ```
pub struct ParamsBuilder {
    assignments: Vec<String>,
    params: Vec<DataValue>,
    next_index: usize,
}

impl ParamsBuilder {
    /// 创建 builder,占位符从 `start_offset + 1` 开始编号。
    ///
    /// `start_offset` = 已被占用的占位符数。
    /// 例如 WHERE 子句已用 `$1`,SET 子句应从 `$2` 起,则传 `1`。
    pub fn new(start_offset: usize) -> Self {
        Self {
            assignments: Vec::new(),
            params: Vec::new(),
            next_index: start_offset + 1,
        }
    }

    /// 添加必填列赋值。`val` 须满足 `Into<DataValue>`。
    pub fn set(&mut self, col: &str, val: impl Into<DataValue>) -> &mut Self {
        let idx = self.next_index;
        self.next_index += 1;
        self.assignments.push(format!("{col} = ${idx}"));
        self.params.push(val.into());
        self
    }

    /// 添加可选列赋值。`None` 时**跳过该列**(不加入 SET),避免无谓赋值。
    pub fn set_opt(&mut self, col: &str, val: Option<impl Into<DataValue>>) -> &mut Self {
        if let Some(v) = val {
            self.set(col, v.into());
        }
        self
    }

    /// 添加可选列赋值(None 时仍写入 NULL,带类型)。
    /// 与 `set_opt` 区别:None 会写入 `SET col = NULL`,而非跳过。
    pub fn set_opt_null(&mut self, col: &str, val: Option<impl Into<DataValue>>) -> &mut Self {
        self.set(col, val.map(Into::into).unwrap_or(DataValue::Null));
        self
    }

    /// 返回 (`"col1 = $2, col2 = $3"`, params)。
    /// 若无任何赋值,返回空字符串(调用方应处理「无字段更新」的情况)。
    pub fn build(self) -> (String, Vec<DataValue>) {
        let clause = self.assignments.join(", ");
        (clause, self.params)
    }
}
```

- [ ] **Step 2: 在 model/mod.rs 声明子模块并 re-export**

修改 `crates/libs/cmx-core/src/model/mod.rs`,在现有 `pub mod` 声明后添加:

```rust
pub mod builder;
pub use builder::ParamsBuilder;
```

- [ ] **Step 3: 在 cmx-core 的 lib.rs re-export(供跨 crate 便捷使用)**

修改 `crates/libs/cmx-core/src/lib.rs`,添加:

```rust
pub use model::ParamsBuilder;
```

> 如此 `cmx_iam` / `cmx_plugin_sdk` 等均可直接 `use cmx_core::ParamsBuilder;`。

- [ ] **Step 4: 编写单元测试**

Create: `crates/libs/cmx-core/tests/params_builder.rs`

```rust
use cmx_core::ParamsBuilder;
use cmx_core::model::cell::DataValue;

#[test]
fn build_basic() {
    let mut b = ParamsBuilder::new(1);
    b.set("name", "alice".to_string())
     .set_opt("age", Some(30_i64));
    let (clause, params) = b.build();
    assert_eq!(clause, "name = $2, age = $3");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], DataValue::String("alice".into()));
}

#[test]
fn set_opt_none_skips() {
    let mut b = ParamsBuilder::new(0);
    b.set("a", "x".to_string())
     .set_opt("b", None::<String>);
    let (clause, params) = b.build();
    assert_eq!(clause, "a = $1"); // b 被跳过,无 $2
    assert_eq!(params.len(), 1);
}

#[test]
fn empty_builder() {
    let b = ParamsBuilder::new(0);
    let (clause, params) = b.build();
    assert_eq!(clause, "");
    assert!(params.is_empty());
}

#[test]
fn set_opt_null_writes_null() {
    let mut b = ParamsBuilder::new(0);
    b.set_opt_null("desc", None::<String>);
    let (clause, params) = b.build();
    assert_eq!(clause, "desc = $1");
    assert_eq!(params[0], DataValue::Null);
}
```

- [ ] **Step 5: 编译 + 测试 + 提交**

```bash
cargo test -p cmx-core --test params_builder 2>&1 | tail -10
git add crates/libs/cmx-core/src/model/builder.rs \
        crates/libs/cmx-core/src/model/mod.rs \
        crates/libs/cmx-core/src/lib.rs \
        crates/libs/cmx-core/tests/params_builder.rs
git commit -m "feat(cmx-core): 新增 ParamsBuilder 解决动态 UPDATE 占位符漂移(wasm 可用)"
```

---

## Task 8: wasm 边界打通 —— DbRequest.data_values + 宿主 do_query 识别

> **本 Task 是本次设计的核心增量**,让 wasm plugin 能传带类型 NULL 的参数(否则前面所有 `NullTyped` 工作对 wasm 完全失效)。
>
> **背景:** 当前 wasm 边界参数格式只有 `serde_json::Value`(`DbRequest.params`),宿主 `do_query` 硬编码走 `query_sql_with_json`,NULL 经 `json_to_data_values` 退化为无类型 `DataValue::Null`。本 Task 增加 `data_values` 变体,让 plugin 直接传 `Vec<DataValue>`(含 `NullTyped`)。

**Files:**
- Modify: `crates/libs/cmx-core/src/wasm_types/database.rs`(DbRequest 加 data_values 字段)
- Modify: `crates/libs/cmx-infra/cmx-database/src/host_functions.rs`(do_query/do_execute 识别 data_values)

- [ ] **Step 1: DbRequest 增加 data_values 字段**

修改 `crates/libs/cmx-core/src/wasm_types/database.rs:13-29`:

```rust
use crate::model::cell::DataValue;

/// 数据库请求
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbRequest {
    /// SQL 语句
    pub sql: String,
    /// 旧:JSON 参数数组(向后兼容)。
    #[serde(default)]
    pub params: Option<JsonValue>,
    /// 新:带类型的 DataValue 参数数组(含 NullTyped)。
    /// 与 params 互斥;若同时设置,**data_values 优先**(确保带类型信息不被 JSON 退化)。
    #[serde(default)]
    pub data_values: Option<Vec<DataValue>>,
    #[serde(default)]
    pub dataset_id: Option<String>,
    #[serde(default)]
    pub db_id: Option<String>,
    #[serde(default)]
    pub txn_id: Option<String>,
}
```

> **MsgPack 兼容:** `DataValue` 已手写 `Serialize/Deserialize`,rmp-serde 走 serde trait,可直接序列化。`#[serde(default)]` 保证旧 plugin(不发送 data_values 字段)反序列化为 None,**完全向后兼容**。

- [ ] **Step 2: 宿主 do_query 识别 data_values**

修改 `crates/libs/cmx-infra/cmx-database/src/host_functions.rs` 的 `do_query`(L68-81 的 match 块):

```rust
// 前(L68-81):
// match params {
//     Some(params_value) => db_manager.query_sql_with_json(...).await...,
//     None => db_manager.query_sql(...).await...,
// }

// 后:
match (request.data_values, params) {
    // 新路径:带类型 DataValue(优先)—— 这是修复 wasm NULL 类型的关键
    (Some(data_values), _) => {
        db_manager
            .query_sql_with_datavalues(&db_id, request_txn_id.as_deref(), &sql, data_values, &dataset_id)
            .await
            .map_err(|e| e.to_string())
    }
    // 旧路径:JSON(向后兼容,旧 wasm plugin 走这里)
    (None, Some(params_value)) => {
        db_manager
            .query_sql_with_json(&db_id, request_txn_id.as_deref(), &sql, params_value, &dataset_id)
            .await
            .map_err(|e| e.to_string())
    }
    // 无参数
    (None, None) => {
        db_manager
            .query_sql(&db_id, request_txn_id.as_deref(), &sql, &dataset_id)
            .await
            .map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 3: 宿主 do_execute 同样识别 data_values**

修改 `do_execute`(L128-141 的 match 块),同样改为 `(request.data_values, params)` 元组匹配,`Some(data_values)` 分支走 `execute_sql_with_datavalues`。

- [ ] **Step 4: 验证 DbRequest 的 Default 派生**

确认 `DbRequest` 加了 `#[derive(Default)]`(或确保所有字段都有 Default)。`sql` 是 String,Default 为空串;`Option` 字段默认 None。若 struct 原本没 Default,Step 1 已加上。

- [ ] **Step 5: 编译验证(全 workspace,确认 wasm 边界无破坏)**

```bash
cargo check --workspace 2>&1 | grep "error" | head
```
Expected: 无 error。重点确认 cmx-plugin-sdk / cmx-plugin-demo 编译通过(它们用 DbRequest)。

- [ ] **Step 6: 提交**

```bash
git add crates/libs/cmx-core/src/wasm_types/database.rs crates/libs/cmx-infra/cmx-database/src/host_functions.rs
git commit -m "feat(wasm): DbRequest 增加 data_values 变体,打通带类型 NULL 跨边界传递"
```

---

## Task 9: cmx-iam permission/service.rs 重构(应用新糖)

**Files:**
- Modify: `crates/libs/cmx-iam/src/permission/service.rs`(create/update 的 params 构造)

- [ ] **Step 1: 添加 dv! 宏与 DataValue 导入**

确认 `permission/service.rs` 顶部已导入 `DataValue`(L2 区域)。添加宏导入:

```rust
use cmx_core::dv;  // 若 dv! 通过 #[macro_export] 已在 crate 根可见,此行可省
```

- [ ] **Step 2: 重构 create_permission 的 params(L978-994)**

将原 15 行 params 构造改为:

```rust
// 原(L978-994):
// let params = vec![
//     DataValue::String(id),
//     data.resource_type.clone().map(DataValue::String).unwrap_or(DataValue::Null),
//     ...
// ];

// 改为(依赖 From<Option<T>>):
let params = vec![
    DataValue::String(id),
    DataValue::String(data.code.clone()),
    DataValue::String(data.name.clone()),
    data.resource_type.clone().into(),   // Option<String> → DataValue
    data.parent_id.clone().into(),
    data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Int(0)),
    data.description.clone().into(),
    data.domain_code.clone().into(),
    data.app_code.clone().into(),
    data.module_code.clone().into(),
    data.extension.clone().into(),
    DataValue::Int(1),
    parent_code.clone().into(),
    DataValue::String(full_code_path),
    DataValue::Int(level),
];
```

> 或使用 `dv!` 宏(若 Task 3 采用了宏版):
> ```rust
> let params = dv![
>     DataValue::String(id),
>     data.code.clone(),
>     // ...
> ];
> ```

- [ ] **Step 3: 重构其余 Optional 模式处**

用 grep 定位 `permission/service.rs` 中所有 `.map(DataValue::String).unwrap_or(DataValue::Null)`:

```bash
grep -n "map(DataValue::String).unwrap_or(DataValue::Null)" crates/libs/cmx-iam/src/permission/service.rs
```

对每处,改为 `.into()`(配合 `From<Option<String>>`)。**注意:** 改为 `.into()` 时需确保左侧类型上下文是 `DataValue`(在 vec! 内,由首个元素推断;单独赋值需 `let x: DataValue = ...into()`)。

- [ ] **Step 4: 重构整型 Optional(L984, L1176, L1177, L1222, L1223)**

`sort_order`/`status` 等整型 Optional 改为 `.into()`(None 时自动 `NullTyped(Int)`):

```rust
// 原: data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Int(0))
// 改(若语义是 None=0): data.sort_order.unwrap_or(0).into()  → DataValue::Int
// 改(若语义是 None=NULL): data.sort_order.into()  → NullTyped(Int)
```

> ⚠️ **语义判断:** 原代码 `unwrap_or(DataValue::Int(0))` 表示 None→0,不是 NULL。保留此语义:`.map(|v| DataValue::Int(v)).unwrap_or(DataValue::Int(0))` 或 `data.sort_order.unwrap_or(0).into()`。**不要盲目改成 .into()**,需逐处核对原语义。

- [ ] **Step 5: 编译验证**

```bash
cargo check -p cmx-iam 2>&1 | grep "error" | head
```
Expected: 无 error。注意 `.into()` 的类型推断歧义(若报错,显式标注 `DataValue::from(...)` 或 turbofish)。

- [ ] **Step 6: 提交**

```bash
git add crates/libs/cmx-iam/src/permission/service.rs
git commit -m "refactor(iam/permission): 应用 From<Option<T>> 糖简化 params 构造"
```

---

## Task 10: cmx-iam rule/service.rs 重构(builder + 新糖)

**Files:**
- Modify: `crates/libs/cmx-iam/src/rule/service.rs`(动态 UPDATE L599-646 + 其余 Optional)

- [ ] **Step 1: 添加 ParamsBuilder 导入**

```rust
use cmx_core::ParamsBuilder;  // ParamsBuilder 在 cmx-core,wasm 也可用
```

- [ ] **Step 2: 重构动态 UPDATE(L599-646)**

将原手动拼 SET + push params 改为 ParamsBuilder:

```rust
// 原模式(L599-646 简化):
// let mut params: Vec<DataValue> = vec![DataValue::String(rule_id.to_string())];
// let mut sets = vec![];
// if let Some(name) = name { sets.push(format!("name = ${}", params.len()+1)); params.push(DataValue::String(name)); }
// if let Some(priority) = priority { sets.push(format!("priority = ${}", params.len()+1)); params.push(DataValue::Int(priority)); }
// let set_clause = sets.join(", ");
// let sql = format!("UPDATE cmx_rule SET {set_clause} WHERE id = $1");
// let params_value = Value::Array(params);

// 改为:
let mut b = ParamsBuilder::new(1);  // WHERE id = $1 已占
b.set_opt("name", name.map(|s| s.to_string()))           // Option<String>
 .set_opt("priority", priority)                          // Option<i64>
 .set_opt("status", status);                             // Option<i64>
let (set_clause, mut params) = b.build();

if set_clause.is_empty() {
    // 无字段更新,直接返回或跳过
    return Ok(());
}

params.push(DataValue::String(rule_id.to_string()));  // WHERE 参数放最后
let sql = format!("UPDATE cmx_rule SET {set_clause} WHERE id = $1");

let _ = self.mm
    .execute_sql_with_datavalues(&self.db_id, Some(txn_id), &sql, params)
    .await
    .map_err(|e| TraitError::from(IamError::Business(format!("更新规则失败: {e}"))))?;
```

> ⚠️ **占位符重排:** 原代码 WHERE 参数在 params[0],builder 把 SET 参数放前面、WHERE 参数放最后。这要求 SQL 的 `$1` 对应 WHERE。**上面写法中 SET 从 `$2` 起(builder new(1)),WHERE 的 `$1` 对应最后 push 的参数——但 params 顺序是 [set..., where],SQL 占位符是 $2,$3...,$1。** 这不匹配!
>
> **修正方案:** builder 的 SET 占位符从 `$1` 起,WHERE 参数的占位符放最后。调整:
> ```rust
> let mut b = ParamsBuilder::new(0);  // SET 从 $1 起
> // ... build ...
> let n_set = params.len();
> params.push(DataValue::String(rule_id.to_string()));  // WHERE 参数
> let where_idx = n_set + 1;
> let sql = format!("UPDATE cmx_rule SET {set_clause} WHERE id = ${where_idx}");
> ```

- [ ] **Step 3: 重构其余 Optional 处(grep 定位)**

```bash
grep -n "map(DataValue::String).unwrap_or(DataValue::Null)\|map(DataValue::Int).unwrap_or(DataValue::Null)" crates/libs/cmx-iam/src/rule/service.rs
```

对每处改为 `.into()`,注意整型 None→0 vs NULL 的语义(参考 Task 8 Step 4)。

- [ ] **Step 4: 重构 enforcer.rs 的嵌套数组(L204-207)**

`rule/enforcer.rs` 的 `DataValue::Array(vec![...])` 现在可正确绑定(Task 4 已实现):

```rust
// 原:
// let role_id_array = DataValue::Array(role_ids.iter().map(|id| DataValue::String(id.clone())).collect());
// let params = vec![role_id_array];
// 这部分无需改动,但确认 Array 现在能正确绑定为 PG 数组
```

- [ ] **Step 5: 编译验证 + 提交**

```bash
cargo check -p cmx-iam 2>&1 | grep "error" | head
git add crates/libs/cmx-iam/src/rule/service.rs crates/libs/cmx-iam/src/rule/enforcer.rs
git commit -m "refactor(iam/rule): 应用 ParamsBuilder 简化动态 UPDATE + Optional 糖"
```

---

## Task 11: 全量验证 + 收尾

- [ ] **Step 1: 全 workspace 编译**

```bash
cargo check --workspace 2>&1 | tail -5
```
Expected: Finished,无 error。

- [ ] **Step 2: 运行所有新增测试**

```bash
cargo test -p cmx-core --test dv_macro 2>&1 | tail -10
cargo test -p cmx-database --test params_builder 2>&1 | tail -10
```
Expected: 全部通过。

- [ ] **Step 3: clippy 检查改动 crate**

```bash
cargo clippy -p cmx-core -p cmx-database -p cmx-iam 2>&1 | grep -E "warning:|error:" | grep -v "too_many_arguments" | head
```
Expected: 无本次改动引入的 warning(忽略 cmx-debug/cmx-auth 预存的 too_many_arguments)。

- [ ] **Step 4: 文档更新**

更新 `20260624_cmx-iam_json_to_datavalues_migration.md`,在末尾标注「已由 `20260625_cmx-sql-param-封装重构方案.md` 增强解决 NULL 类型/构造糖/builder 问题」。

- [ ] **Step 5: 最终提交**

```bash
git add -A
git commit -m "docs: 更新迁移方案文档,标注增强方案" --allow-empty
```

---

## 验证清单

- [ ] `cargo check --workspace` 无 error(含 cmx-plugin-sdk / cmx-plugin-demo)
- [ ] `DataValue::NullTyped(SqlTypeMarker::Int)` 绑定 PG 时为 `None::<i64>`(类型正确)
- [ ] `DataValue::Null` 仍绑 `None::<String>`(向后兼容)
- [ ] `DataValue::Array(vec![String])` 绑定为 PG text 数组
- [ ] `dv![]` 宏可构造空/单/多元素 Vec<DataValue>
- [ ] `Option<String>.into()` 产生 `DataValue::String` 或 `DataValue::Null`
- [ ] `Option<i64>.into()` 产生 `DataValue::Int` 或 `DataValue::NullTyped(Int)`
- [ ] `ParamsBuilder` 正确生成 `col = $N` 且编号连续
- [ ] `query_sql_typed`/`execute_sql_typed` 可用
- [ ] `SqlParam` 在 cmx-core 可用(wasm plugin 可引用)
- [ ] **DbRequest 带 data_values 字段可经 MsgPack 序列化/反序列化往返**(wasm 边界)
- [ ] **宿主 do_query 收到 data_values 时走 query_sql_with_datavalues(带类型 NULL 生效)**
- [ ] **旧 wasm plugin(只发 params JSON)仍正常工作**(向后兼容)
- [ ] cmx-iam permission/rule 重构后行为不变(单测或手动验证)

---

## 风险与回退

| 风险 | 缓解 |
|------|------|
| `DataValue` 新增变体导致大量 exhaustive match 错误 | Task 1 Step 6 全局定位,Task 4 逐个补全;编译器强制审查,不会遗漏 |
| `dv!` 宏跨 crate 路径问题(`$crate` 解析) | Task 3 提供 From 驱动简化版作为 fallback;测试在独立 test crate 验证 |
| ParamsBuilder 占位符编号与 SQL 不一致 | Task 10 Step 2 详细说明编号规则,单测验证 |
| 整型 None→0 vs None→NULL 语义混淆 | Task 9/10 Step 4 强调逐处核对原语义,不盲目改 .into() |
| **DbRequest 加字段破坏旧 wasm plugin** | `#[serde(default)]` 确保旧 plugin 不发 data_values 时反序列化为 None;旧 plugin 走 params JSON 分支不变 |
| **DataValue 的 MsgPack 序列化往返** | DataValue 已手写 serde Serialize/Deserialize;Task 8 Step 5 全 workspace 编译验证;建议加 MsgPack 往返单测 |
| **宿主 data_values 与 params 同时存在的歧义** | 明确约定 data_values 优先(Task 8 Step 2 元组匹配顺序),避免 JSON 退化带类型信息 |

**回退策略:** 每个 Task 独立提交,若某层出问题可 `git revert` 单个 commit,不影响其它层(底层 cmx-core 改动是增量的,不破坏旧 API)。wasm 协议改动(DbRequest.data_values)是**纯增量字段**,即使回退宿主 do_query 改动,旧逻辑仍完整可用。
