# sea-query 1.0 + modql 0.5 升级修复方案

## 概述

项目升级了 `sea-query` 从 0.32 到 `1.0.0-rc.34`，`modql` 从 0.4 到 `0.5.0`，同时使用 `sqlx 0.9`。
`cargo check` 报 53 个编译错误，全部集中在 `cmx-database` crate 中。

## 当前状态分析

### 核心不兼容问题

| 问题 | 原因 | 影响范围 |
|------|------|---------|
| `build_sqlx` 方法不存在 | `sea-query-binder 0.7` 绑定的是旧 `sea-query 0.32`，与 `sea-query 1.0` 不兼容 | `crud_fns.rs` (9处) |
| `SqlxValues` 不满足 `IntoArguments` | `sea-query-binder 0.7` 实现的是 `IntoArguments<'_, Postgres>`，sqlx 0.9 不再接受生命周期参数 | `connection/mod.rs`, `transaction/core.rs` |
| `TableRef::Table` 参数变化 | `sea-query 1.0` 中 `TableRef::Table` 变为 `Table(TableName, Option<DynIden>)` | `crud/mod.rs` (1处) |
| `Expr::col(Asterisk).count()` 报错 | `count()` 方法移到 `ExprTrait` trait，需 import | `crud_fns.rs` (1处) |
| `Expr::col(...).is_in(...)` 报错 | `is_in()` 方法移到 `ExprTrait` trait，需 import | `crud_fns.rs` (1处) |
| `sea_query::Value` 枚举变体变化 | `Value::Bool(Some(...))` 等类型变化 | `crud_fns.rs`, `crud/utils.rs` |

### 依赖关系

- `sea-query-binder 0.7` → 依赖 `sea-query 0.32` + `sqlx 0.8` → **不兼容**
- `sea-query-sqlx 0.9.1`（新名称） → 依赖 `sea-query ^1.0.0` + `sqlx ^0.9` → **兼容**
- `modql 0.5.0` 的本地源码已适配 `sea-query 1.0`（使用 `TableName`、`ExprTrait` 等新 API）

## 修改方案

### 第 1 步：更新 workspace Cargo.toml 依赖

**文件**: `Cargo.toml`（workspace 根目录）

- 将 `sea-query-binder = "0.7"` 替换为 `sea-query-sqlx = "0.9.1"`
- 更新 features 为 `["sqlx-postgres", "with-uuid", "with-time", "with-chrono", "with-json"]`

```toml
# 旧
sea-query-binder = { version = "0.7", features = ["sqlx-postgres", "with-uuid", "with-time", "with-chrono", "with-json"] }

# 新
sea-query-sqlx = { version = "0.9.1", features = ["sqlx-postgres", "with-uuid", "with-time", "with-chrono", "with-json"] }
```

### 第 2 步：更新 cmx-database Cargo.toml

**文件**: `crates/libs/cmx-infra/cmx-database/Cargo.toml`

```toml
# 旧
sea-query-binder = { workspace = true }

# 新
sea-query-sqlx = { workspace = true }
```

### 第 3 步：更新 crud/mod.rs — TableRef::Table

**文件**: `crates/libs/cmx-infra/cmx-database/src/crud/mod.rs`（第 32 行）

```rust
// 旧
TableRef::Table(SIden(Self::TABLE).into_iden())

// 新 — sea-query 1.0 的 TableRef::Table 接受 TableName 和 Option<DynIden>
TableRef::Table(SIden(Self::TABLE).into_iden().into(), None)
```

说明：`DynIden` 实现了 `Into<TableName>`（通过 `MaybeQualifiedTwice` trait），所以 `.into_iden().into()` 可以将 `DynIden` 转为 `TableName`。

### 第 4 步：更新 crud/crud_fns.rs — 核心 CRUD 函数

**文件**: `crates/libs/cmx-infra/cmx-database/src/crud/crud_fns.rs`

#### 4a. 更新 import

```rust
// 旧
use sea_query::{Asterisk, Condition, Expr, IntoIden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;

// 新
use sea_query::{Asterisk, Condition, Expr, ExprTrait, IntoIden, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder;
```

关键变化：
- 添加 `ExprTrait` import（`count()`、`is_in()` 等方法已移到此 trait）
- `sea_query_binder` → `sea_query_sqlx`

#### 4b. 所有 `build_sqlx` 调用不变

`sea-query-sqlx 0.9.1` 仍然导出 `SqlxBinder` trait 和 `build_sqlx` 方法，API 签名兼容。9 处 `build_sqlx` 调用无需修改。

#### 4c. `json_value_to_sea_query` 函数

需要检查 `sea_query::Value` 的变体是否变化。sea-query 1.0 中 `Value::Bool`、`Value::BigInt`、`Value::Double`、`Value::String` 的内部类型可能变化（例如从 `Option<T>` 变为包装类型）。需要对照 modql 源码确认。

根据 modql 源码（`example/rust-modql`），它仍然使用 `sea_query::Value::Bool(None)` 等模式，说明 Value 枚举的 Option 包装在 1.0 中仍然保留。此函数可能不需要修改。

### 第 5 步：更新 connection/mod.rs

**文件**: `crates/libs/cmx-infra/cmx-database/src/connection/mod.rs`

```rust
// 旧
use sea_query_binder::SqlxValues;

// 新
use sea_query_sqlx::SqlxValues;
```

`SqlxValues` 在 `sea-query-sqlx` 中仍然以相同方式导出（`pub struct SqlxValues(pub sea_query::Values)`），`sqlx::query_with` 和 `IntoArguments` 的实现在 0.9 版本中已适配 sqlx 0.9。

### 第 6 步：更新 transaction/core.rs

**文件**: `crates/libs/cmx-infra/cmx-database/src/transaction/core.rs`

```rust
// 旧
use sea_query_binder::SqlxValues;

// 新
use sea_query_sqlx::SqlxValues;
```

### 第 7 步：更新 transaction/api.rs

**文件**: `crates/libs/cmx-infra/cmx-database/src/transaction/api.rs`

```rust
// 旧
use sea_query_binder::SqlxValues;

// 新
use sea_query_sqlx::SqlxValues;
```

### 第 8 步：更新 manager/mod.rs

**文件**: `crates/libs/cmx-infra/cmx-database/src/manager/mod.rs`（第 282、348 行）

```rust
// 旧
params: sea_query_binder::SqlxValues,

// 新
params: sea_query_sqlx::SqlxValues,
```

### 第 9 步：检查其他 crate 是否使用 sea_query_binder

需搜索整个 workspace 中所有对 `sea_query_binder` 的引用并替换。

## 修改文件清单

| # | 文件路径 | 修改内容 |
|---|---------|---------|
| 1 | `Cargo.toml` | `sea-query-binder` → `sea-query-sqlx` 版本和 features |
| 2 | `crates/libs/cmx-infra/cmx-database/Cargo.toml` | 依赖名替换 |
| 3 | `crates/libs/cmx-infra/cmx-database/src/crud/mod.rs` | `TableRef::Table` 参数更新 |
| 4 | `crates/libs/cmx-infra/cmx-database/src/crud/crud_fns.rs` | 添加 `ExprTrait` import，替换 `sea_query_binder` |
| 5 | `crates/libs/cmx-infra/cmx-database/src/connection/mod.rs` | 替换 `sea_query_binder` |
| 6 | `crates/libs/cmx-infra/cmx-database/src/transaction/core.rs` | 替换 `sea_query_binder` |
| 7 | `crates/libs/cmx-infra/cmx-database/src/transaction/api.rs` | 替换 `sea_query_binder` |
| 8 | `crates/libs/cmx-infra/cmx-database/src/manager/mod.rs` | 替换 `sea_query_binder` |

## 假设与决策

1. **`sea-query-sqlx` 是 `sea-query-binder` 的继任者**：包名从 `sea-query-binder` 改为 `sea-query-sqlx`，但 API（`SqlxBinder`、`SqlxValues`）保持兼容
2. **`modql 0.5.0` 已适配 sea-query 1.0**：本地源码确认使用新 API（`TableName`、`ExprTrait`、`ColumnRef::Column(ColumnName)` 等）
3. **`sea_query::Value` 枚举变体在 1.0 中保持 Option 包装**：基于 modql 源码中的使用方式判断

## 验证步骤

1. 更新 workspace `Cargo.toml` 依赖
2. 更新 `cmx-database/Cargo.toml`
3. 全局替换 `sea_query_binder` → `sea_query_sqlx`
4. 修复 `TableRef::Table` 调用
5. 添加 `ExprTrait` import
6. 运行 `rtk cargo check` 验证编译通过
7. 如仍有错误，根据错误信息逐一修复（如 `Value` 枚举变体变化）
8. 更新 modql skill 文档

## 风险点

- `sea_query::Value` 的变体可能在 1.0 中有细微变化（如 `String` 的内部类型从 `Option<Box<str>>` 变为其他），需要在编译后根据实际错误调整
- `sea-query-sqlx` 的 features 名称可能与 `sea-query-binder` 略有不同
- `executor/mod.rs` 中 `SqliteArguments<'q>` 在 sqlx 0.9 中可能不再接受生命周期参数（第 178、180 行报错），但这属于 sqlx 0.9 升级问题，不在本次 sea-query/modql 升级范围内
