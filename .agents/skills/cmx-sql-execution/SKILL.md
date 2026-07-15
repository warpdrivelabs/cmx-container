---
name: cmx-sql-execution
description: 指导在 Rust 代码中执行 SQL 的规范,涵盖 DatabaseManager API 选择、DataValue 参数构造(dv! 宏/From<Option<T>>/ParamsBuilder)、带类型 NULL(NullTyped)、事务模式。Invoke when 编写手写 SQL 执行代码、构造 Vec<DataValue> 参数、构建动态 UPDATE SET 子句、处理 NULL 绑定类型问题、或在业务 Service 中调用 execute_sql/query_sql 系列 API。
---

# cmx SQL 执行规范

> 本文档指导 AI 在 cmx-container 项目中编写**执行 SQL 的 Rust 代码**时遵循的规范。
> 涵盖 DatabaseManager API 选择、DataValue 参数构造、ParamsBuilder 动态 SQL、带类型 NULL、事务模式。

---

## 一、何时调用本技能

### 1.1 必须调用的场景

| 场景 | 关键词 | 本技能解决的问题 |
|------|--------|-----------------|
| 在 Service 层手写 SQL 并执行 | "execute_sql" "query_sql" "raw sql" | API 选择、参数构造、结果提取 |
| 构造 `Vec<DataValue>` 参数 | "DataValue" "params" "参数构造" | dv! 宏、From<Option<T>>、NullTyped |
| 构建动态 UPDATE SET 子句 | "动态 UPDATE" "set 子句" "条件更新" | ParamsBuilder 自动管理占位符 |
| 处理 NULL 绑定到非字符串列 | "NULL 类型" "NullTyped" "prepare 失败" | SqlTypeMarker + NullTyped |
| 在事务中执行多条 SQL | "事务" "txn_id" "transaction" | 事务 API、txn_id 传递 |
| 从 DataSet 提取查询结果 | "DataSet" "提取结果" "反序列化" | row 遍历、to_json_value |
| WASM plugin 传带类型参数 | "data_values" "wasm sql" "DbRequest" | data_values 优先于 params JSON |

### 1.2 不需要调用的场景

| 场景 | 应调用的技能 |
|------|-------------|
| 编写 DDL / migrations SQL 文件 | `sql-guide`(关注 init/migrations 目录规范) |
| 使用 modql + sea-query 构建查询 | `modql`(关注 Filter/ListOptions/sea-query 集成) |
| 使用 GenericCrudService 标准 CRUD | `axum-handler-generator`(关注 Service 模式) |
| 设计 axum handler / 路由 | `axum-handler-generator` |

---

## 二、DatabaseManager API 层级与选择

### 2.0 cmx-database vs cmx-database-pg（先选 crate）

项目有**两个并行数据库 crate**，API 高度对齐，必须先选对 crate：

| 维度 | cmx-database（sqlx） | cmx-database-pg（tokio-postgres） |
|------|----------------------|-----------------------------------|
| **定位** | 默认主用，多数据库驱动（PG/MySQL/SQLite） | PG-only 性能优化分支 |
| **全局入口** | `get_default_db_manager()` | `get_default_pg_db_manager()` |
| **API 对齐** | execute_sql_with_datavalues / query_sql_with_datavalues 等 | 同名同签，完全对齐 |
| **ZmcDataSet** | ✅ `query_sql_zmc` / `query_sql_zmc_with_datavalues`（sqlx PgRow） | ✅ 同名方法（tokio-postgres Row） |
| **query_zmc_streaming** | ✅ 有（写入 `Vec<u8>`） | ✅ 有（写入 `Vec<u8>`） |
| **独有能力** | 无 | 4 项（见下方详表） |
| **消费方** | 9 个 crate 独占依赖 | 0 个 crate 独占依赖（4 个同时挂两者） |

**cmx-database-pg 真正独有的 4 项能力**：

| # | 独有能力 | 位置 | 说明 |
|---|---------|------|------|
| ① | **`query_sql_zmc_stream_chunks`** | `manager/mod.rs:374` + `connection/mod.rs:207` | 真·分帧流式：基于 `mpsc::Sender<Bytes>`，逐行编码为长度分帧发送，峰值内存 O(单行)，16KB 攒批刷写，header 帧先发、空结果容错。cmx-database **完全无此方法** |
| ② | **数组类型列读取还原** | `executor/mod.rs:435-452`（`PgResultConverter::convert_rows`） | 读取阶段支持 TEXT_ARRAY / INT8_ARRAY / UUID_ARRAY -> `DataValue::Array`。cmx-database 读取方向**不还原数组**（只在绑定时 `bind_pg_array_postgres` 支持写入） |
| ③ | **`get_conn()` 方法** | `connection/mod.rs:112` | 返回 `deadpool_postgres::Object`，供事务层跨 await 手动驱动 BEGIN/COMMIT。cmx-database 用 sqlx 的 `pool.begin()`，无需此方法 |
| ④ | **4 个 ToSql 适配器** | `executor/mod.rs:24-123` | `PgInt` / `PgDateTime` / `PgDateTimeNull` / `PgIntNull`。tokio-postgres 类型校验严格（i64 绑 INT4 列会 WrongType），需宽度/时区自适应包装。sqlx 隐式协调，不需要 |

> ⚠️ **注意区分**：`query_zmc_streaming`（写入 `Vec<u8>`）**两者都有**；唯独 `*_stream_chunks`（mpsc 通道）是 pg 独有。

**选择规则**：

```
需要 query_sql_zmc_stream_chunks（mpsc 分帧流式）?
├─ 是 -> ★ cmx-database-pg（独占能力）
└─ 否 -> 需要数组列读取还原（DataValue::Array 从数据库读取）?
    ├─ 是 -> ★ cmx-database-pg
    └─ 否 -> ★★★ cmx-database（默认首选）
```

**依赖现状**（无任何 crate 独家依赖 cmx-database-pg）：

| 情形 | crate 数 | 具体 |
|------|---------|------|
| 同时依赖两者 | 4 | cmx-api、cmx-biz、cmx-database-test、web-server |
| 只依赖 cmx-database-pg | **0** | 无 |
| 只依赖 cmx-database | 9 | cmx-api-types、cmx-iam、cmx-audit、cmx-auth、cmx-storage、cmx-metadata、cmx-plugin、cmx-portal、cmx-service |

**能否将 cmx-database-pg 的消费方替换为 cmx-database？**

🟢 **可以无痛替换**（占大多数场景）：
- 凡只用到 `execute_sql*` / `query_sql*` / `query_sql_zmc` / `query_sql_zmc_with_datavalues` / `crud::*` / `transaction::*` / `migration::*` / `host_functions` / `ZmcDataSet` 的消费方
- 注意 `SqlParams::SeaValues` -> `SqlxValues` 的枚举变体替换
- 具体使用点：cmx-api 的 `dct.rs`/`doc.rs`、web-server 的 `datasource.rs`、cmx-biz 的 `zmc_loader.rs`

🔴 **不能简单替换**（需迁移实现）：
- 依赖 `query_sql_zmc_stream_chunks` 的场景（如 `mem_bench.rs`、需要 O(单行) 内存的流式消费）
- 依赖数组列读取还原（`DataValue::Array` 从数据库读取）的场景
- 直接依赖 `TokioPgRowSource` 全路径的代码（如 `cmx-database-test` 的 `e2e_server.rs:338`、`mem_bench.rs`）需改为 `SqlxPgRowSource`

> **默认使用 `cmx-database`**。除非必须使用上述 4 项独有能力，否则不引入 cmx-database-pg。
>
> **两 crate 的 `with_json` 系列 API 均不推荐**：`execute_sql_with_json` / `query_sql_with_json` 仅维护旧代码，新代码必须用 `_with_datavalues`。
>
> **导出对称性缺口**（不影响功能）：cmx-database 把 `SqlxPgRowSource` 提升到了 crate 根（`lib.rs:29`），而 pg 侧的 `TokioPgRowSource` 只能走全路径 `cmx_database_pg::zmcdataset::TokioPgRowSource`。

### 2.1 API 全景

`DatabaseManager`(cmx-database / cmx-database-pg 两者 API 对齐)提供 4 组 execute + 4 组 query + 2 组 zmc 方法,按参数类型区分:

```
DatabaseManager (cmx-database / cmx-database-pg 两者 API 对齐)
├── execute_sql(db_id, txn_id, sql)                              -> 无参数
├── execute_sql_with_json(db_id, txn_id, sql, Value)             -> JSON 参数(旧,向后兼容,不推荐)
├── execute_sql_with_datavalues(db_id, txn_id, sql, Vec<DataValue>) -> DataValue 参数(★推荐)
├── execute_sql_typed(db_id, txn_id, sql, Vec<SqlParam>)         -> 强类型参数(带类型 NULL)
├── execute_sql_with_sqlxvalues(db_id, txn_id, sql, SqlxValues)  -> sea-query 构建器参数
│
├── query_sql(db_id, txn_id, sql, dataset_id)                              -> 无参数
├── query_sql_with_json(db_id, txn_id, sql, Value, dataset_id)             -> JSON 参数(旧,不推荐)
├── query_sql_with_datavalues(db_id, txn_id, sql, Vec<DataValue>, dataset_id) -> ★推荐
├── query_sql_typed(db_id, txn_id, sql, Vec<SqlParam>, dataset_id)         -> 强类型
├── query_sql_with_sqlxvalues(db_id, txn_id, sql, SqlxValues, dataset_id)  -> sea-query
│
├── query_sql_zmc(db_id, sql, dataset_id)                                  -> 零拷贝 ZmcDataSet(只读,不参与事务)
├── query_sql_zmc_with_datavalues(db_id, sql, Vec<DataValue>, dataset_id)  -> 零拷贝 + DataValue 参数
│
└── [仅 cmx-database-pg] query_sql_zmc_stream_chunks(db_id, sql, params, dataset_id, col_names, chunk_tx)
    -> 真·分帧流式(峰值内存 O(单行),超大结果集网络零内存路径)
```

### 2.2 选择决策树

```
需要执行 SQL?
├─ 参数来源是什么?
│   ├─ 手写 SQL + 手动构造参数
│   │   ├─ 参数中含 Option<T>(可能为 NULL)且目标列非 TEXT
│   │   │   └─ ★ execute_sql_with_datavalues (配合 From<Option<T>> 自动产生 NullTyped)
│   │   ├─ 需要精确控制每个 NULL 的目标类型(罕见)
│   │   │   └─ execute_sql_typed (显式传 Vec<SqlParam>)
│   │   └─ 无 Option / 目标列全是 TEXT
│   │       └─ execute_sql_with_datavalues (直接 DataValue::String 等)
│   │
│   ├─ sea-query QueryBuilder 构建
│   │   └─ execute_sql_with_sqlxvalues (与 modql/GenericCrudService 集成)
│   │
│   ├─ 旧代码遗留 JSON 参数
│   │   └─ execute_sql_with_json (仅维护,新代码不使用)
│   │
│   └─ 无参数(DDL、SELECT 1 等)
│       └─ execute_sql / query_sql
│
├─ 是否在事务中?
│   ├─ 是 → txn_id: Some(txn_id)
│   └─ 否 → txn_id: None
│
└─ 返回类型?
    ├─ INSERT/UPDATE/DELETE → execute_* (返回 u64 影响行数)
    └─ SELECT → query_* (返回 DataSet)
```

### 2.3 ★推荐默认选择

**新代码默认使用 `execute_sql_with_datavalues` / `query_sql_with_datavalues`**:
- 参数类型 `Vec<DataValue>` 构造直观(dv! 宏 / .into() / 直接 DataValue::X)
- `From<Option<T>>` 自动处理 None → NullTyped(带类型),无需手动判断
- 绑定层(postgres/mysql/sqlite)已全面支持 NullTyped 和 Array

仅在以下情况使用 `execute_sql_typed`:
- 需要显式声明某个 NULL 的目标类型(如 `SqlParam::Null(SqlTypeMarker::Uuid)`)
- 参数语义更贴近 SQL 而非 Rust 类型

---

## 三、DataValue 参数构造

### 3.1 基础类型直接构造

```rust
use cmx_core::model::cell::DataValue;

let params = vec![
    DataValue::String(id.clone()),           // TEXT / VARCHAR
    DataValue::Int(count),                   // BIGINT / INT
    DataValue::Bool(enabled),                // BOOLEAN
    DataValue::Float(rate),                  // DOUBLE PRECISION
    DataValue::Decimal(amount),              // NUMERIC
    DataValue::DateTime(created_at),         // TIMESTAMPTZ
    DataValue::Date(birth_date),             // DATE
    DataValue::Uuid(uuid),                   // UUID
    DataValue::Binary(bytes),                // BYTEA
    DataValue::Json(json_string),            // JSONB
    DataValue::Null,                         // NULL(绑定为 None::<String>)
];
```

### 3.2 From<Option<T>> 构造糖(★消除冗长模式)

cmx-core 为 DataValue 实现了 `From<Option<T>>`,**消除 `.map(DataValue::X).unwrap_or(DataValue::Null)` 冗长模式**:

```rust
// ❌ 旧写法(冗长,且 NULL 丢失类型)
let params = vec![
    name.map(DataValue::String).unwrap_or(DataValue::Null),        // Option<String>
    sort_order.map(DataValue::Int).unwrap_or(DataValue::Null),     // Option<i64> → NULL 无类型!
];

// ✅ 新写法(.into() 配合 From<Option<T>>)
let params: Vec<DataValue> = vec![
    name.into(),        // Option<String> → DataValue::String 或 Null
    sort_order.into(),  // Option<i64> → DataValue::Int 或 NullTyped(Int) ★带类型
];
```

**关键规则**:
- `Option<String>.into()` → `DataValue::String` 或 `DataValue::Null`(TEXT 列,兼容)
- `Option<i64>.into()` → `DataValue::Int` 或 `DataValue::NullTyped(Int)` ★带类型
- `Option<bool>.into()` → `DataValue::Bool` 或 `DataValue::NullTyped(Bool)`
- `Option<Uuid>.into()` → `DataValue::Uuid` 或 `DataValue::NullTyped(Uuid)`
- `Option<DateTime<Utc>>.into()` → `DataValue::DateTime` 或 `DataValue::NullTyped(Timestamp)`
- `Option<NaiveDate>.into()` → `DataValue::Date` 或 `DataValue::NullTyped(Date)`
- `Option<Decimal>.into()` → `DataValue::Decimal` 或 `DataValue::NullTyped(Decimal)`

> **为什么整型/时间/Uuid 的 None 走 NullTyped 而非 Null?**
> PostgreSQL prepare 时,`None::<String>` 绑定到 INTEGER/TIMESTAMP/UUID 列会类型不匹配。
> `NullTyped(Int)` 让绑定层知道应绑 `None::<i64>`,类型正确。

### 3.3 语义判断:None→0 vs None→NULL

**必须逐处核对原语义,不盲目改 .into()**:

```rust
// 语义 A: None 表示 0(有默认值)
data.sort_order.unwrap_or(0).into()  // → DataValue::Int(0)

// 语义 B: None 表示 NULL(数据库存 NULL)
data.sort_order.into()  // → DataValue::NullTyped(Int)
```

### 3.4 dv! 宏(批量构造)

`dv!` 宏基于 `Into<DataValue>` trait 驱动,适合批量构造参数:

```rust
use cmx_core::dv;

// 空参数
let params: Vec<DataValue> = dv!();

// 批量构造(每个 expr 须 Into<DataValue>)
let params = dv![
    id.clone(),                    // String → DataValue::String
    data.code.clone(),             // String
    data.sort_order.unwrap_or(0),  // i64 → DataValue::Int
    data.description.clone(),      // Option<String> → DataValue::String 或 Null
    data.parent_id.clone(),        // Option<String>
];

// 显式带类型的 NULL(非 Vec,返回单个 DataValue)
let null_uuid: DataValue = dv!(null Uuid);  // → NullTyped(Uuid)
```

> **dv! vs vec![]:**
> `dv!` 的优势在于 `Option<T>` 直接传入即自动 `.into()`,而 `vec![]` 需要每个元素显式 `.into()`。
> 简单场景(2-3 个参数)可用 `vec![a.into(), b.into()]`,复杂场景用 `dv!` 更简洁。

### 3.5 数组参数(IN 查询)

PostgreSQL 支持 `ANY($1)` 数组绑定,使用 `DataValue::Array`:

```rust
// 单层同类型数组(IN 查询)
let role_ids: Vec<String> = vec!["r1".into(), "r2".into()];
let params = vec![DataValue::Array(
    role_ids.iter().map(|id| DataValue::String(id.clone())).collect(),
)];

let sql = "SELECT * FROM cmx_role_permission WHERE role_id = ANY($1)";
let dataset = mm.query_sql_with_datavalues(&db_id, txn_id, sql, params, "role_perms").await?;
```

> **注意:** Array 仅支持单层、元素同类型(String/i64/Uuid),绑定层按首个元素推断类型。
> MySQL/SQLite 不支持原生数组,绑定层会退化为逗号分隔字符串/JSON 字符串。

---

## 四、ParamsBuilder:动态 UPDATE SET 子句

### 4.1 问题:占位符漂移

手写动态 UPDATE 时,「SQL SET 子句顺序」与「params Vec push 顺序」必须双重一致,极易出错:

```rust
// ❌ 旧模式(易错:idx 漂移、sets 和 params 顺序不一致)
let mut sets: Vec<String> = Vec::new();
let mut params: Vec<DataValue> = vec![DataValue::String(rule_id.to_string())]; // WHERE $1
let mut idx = 2;
if let Some(name) = data.name {
    sets.push(format!("name = ${idx}"));
    params.push(DataValue::String(name));
    idx += 1;
}
if let Some(priority) = data.priority {
    sets.push(format!("priority = ${idx}"));
    params.push(DataValue::Int(priority));
    idx += 1;
}
// ...
```

### 4.2 解决:ParamsBuilder 自动管理编号

```rust
use cmx_core::ParamsBuilder;

// SET 从 $1 起,WHERE id 参数放最后
let mut b = ParamsBuilder::new(0);  // start_offset = 0 → SET 从 $1 起
b.set_opt("name", data.name)              // Option<String> → None 跳过该列
 .set_opt("priority", data.priority)      // Option<i64> → None 跳过
 .set_opt("status", data.status);         // Option<i64>
let (set_clause, mut params) = b.build();

if set_clause.is_empty() {
    return Err(TraitError::Business("未提供任何更新字段".into()));
}

// WHERE id 参数放最后,占位符编号 = SET 参数数 + 1
let where_idx = params.len() + 1;
params.push(DataValue::String(rule_id.to_string()));
let sql = format!(
    "UPDATE cmx_rule SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
);

mm.execute_sql_with_datavalues(&db_id, None, &sql, params).await?;
```

### 4.3 ParamsBuilder API

| 方法 | 说明 |
|------|------|
| `new(start_offset)` | 创建 builder,占位符从 `start_offset + 1` 起编号 |
| `set(col, val)` | 必填列赋值,val 须 `Into<DataValue>` |
| `set_opt(col, val)` | 可选列赋值,**None 跳过该列**(不加入 SET) |
| `set_opt_null(col, val)` | 可选列赋值,**None 写入无类型 NULL**(`DataValue::Null`,绑 TEXT) |
| `build()` | 返回 `(set_clause: String, params: Vec<DataValue>)` |
| `len()` / `is_empty()` | 查询当前赋值数 |
| `next_placeholder()` | 查询下一个占位符编号 |

### 4.4 set_opt vs set_opt_null

```rust
// set_opt: None → 跳过该列(不更新)
b.set_opt("name", None::<String>);  // SET 子句不含 name

// set_opt_null: None → 写入 SET name = NULL(无类型,绑 TEXT)
// 注意:当前实现产生 DataValue::Null(非 NullTyped),仅适用于 TEXT 列。
// 若目标列是 INTEGER/TIMESTAMP/UUID,应改用 set + 显式 NullTyped:
b.set_opt_null("description", None::<String>);  // SET description = $N (Null)
b.set("deleted_at", DataValue::NullTyped(SqlTypeMarker::Timestamp));  // 非 TEXT 列的 NULL
```

### 4.5 占位符编号策略

ParamsBuilder 的 `start_offset` 取决于 SQL 结构:

| SQL 结构 | start_offset | SET 起始占位符 | 说明 |
|---------|-------------|--------------|------|
| `UPDATE t SET ... WHERE id = $1` | 0 | $1 | WHERE 参数放最后(★推荐) |
| `UPDATE t SET ... WHERE id = $N` (N=SET 数+1) | 0 | $1 | 同上,WHERE 编号动态计算 |
| `WHERE $1 = ... THEN SET ...` (罕见) | 1 | $2 | WHERE 在前,SET 从 $2 起 |

**推荐模式:** SET 从 $1 起,WHERE 参数放 params 最后,编号 = SET 数 + 1。避免 WHERE 和 SET 占位符交叉。

---

## 五、带类型 NULL:NullTyped

### 5.1 问题:NULL 丢失类型

PostgreSQL prepare 时,占位符需要知道目标列类型:

```rust
// ❌ 问题:NULL 绑定到非 TEXT 列
DataValue::Null  // 绑定为 None::<String> → INTEGER 列 prepare 类型不匹配!

// ✅ 解决:显式声明 NULL 的目标类型
DataValue::NullTyped(SqlTypeMarker::Int)  // 绑定为 None::<i64> → INTEGER 列类型正确
```

### 5.2 SqlTypeMarker 枚举

```rust
pub enum SqlTypeMarker {
    Bool,       // BOOLEAN
    Int,        // BIGINT / INTEGER
    Float,      // DOUBLE PRECISION / REAL
    Decimal,    // NUMERIC
    Text,       // TEXT / VARCHAR
    Timestamp,  // TIMESTAMPTZ
    Date,       // DATE
    Uuid,       // UUID
    Json,       // JSONB
    Binary,     // BYTEA
}
```

### 5.3 何时需要手动 NullTyped

大多数场景 `From<Option<T>>` 会自动产生正确的 NullTyped:
- `Option<i64>.into()` → `NullTyped(Int)` ✓
- `Option<Uuid>.into()` → `NullTyped(Uuid)` ✓

**需要手动 NullTyped 的场景**:
- SQL 占位符对应非字符串列,但参数来源不是 Option(如条件分支)
- 显式构造 NULL 参数

```rust
use cmx_core::model::cell::{DataValue, SqlTypeMarker};

// 条件分支:根据情况传 NULL
let parent_id_param = if has_parent {
    DataValue::String(parent_id)
} else {
    DataValue::NullTyped(SqlTypeMarker::Text)  // 显式 TEXT 类型 NULL
};

// 或用 dv! 宏的 null 语法
let null_uuid: DataValue = cmx_core::dv!(null Uuid);
```

### 5.4 绑定层行为

| 数据库 | NullTyped 行为 | 其他注意 |
|--------|---------------|---------|
| PostgreSQL | 按 SqlTypeMarker 分发到 `None::<T>`(类型精确) | `ShortStr`/`LongStr` 绑定为 `&str`;`Array` 按 PG 数组绑定 |
| MySQL | 统一 `None::<String>`(MySQL NULL 无类型) | `ShortStr`/`LongStr` 绑定为 String |
| SQLite | 统一 `None::<String>`(SQLite 动态类型) | 同 MySQL |

---

## 六、事务模式

### 6.1 事务内执行 SQL

```rust
// 1. 开启事务
let txn_id = mm.get_transaction_context().begin(&db_id).await?;

// 2. 事务内执行(传 txn_id: Some)
let result = mm.execute_sql_with_datavalues(
    &db_id,
    Some(&txn_id),   // ★ 事务内执行
    "INSERT INTO cmx_permission (id, code) VALUES ($1, $2)",
    dv![id, code],
).await?;

// 3. 提交或回滚
match verify_result {
    Ok(_) => mm.commit_transaction(&txn_id).await?,
    Err(e) => {
        mm.rollback_transaction(&txn_id).await?;
        return Err(e);
    }
}
```

### 6.2 事务内查询

```rust
let dataset = mm.query_sql_with_datavalues(
    &db_id,
    Some(&txn_id),   // 事务内查询
    "SELECT id, code FROM cmx_permission WHERE domain_code = $1",
    dv![domain_code],
    "perm_scope",    // dataset_id(用于日志/调试)
).await?;
```

### 6.3 非事务执行

```rust
// txn_id: None → 自动提交
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

---

## 七、从 DataSet 提取结果

### 7.1 遍历行

```rust
let dataset = mm.query_sql_with_datavalues(&db_id, None, sql, params, "query_name").await?;
let schema = dataset.schema.as_ref();

for row in dataset.iter() {
    let id: String = row.get_by_name_as::<String>(schema, "id").unwrap_or_default();
    let name: Option<String> = row.get_by_name_as::<String>(schema, "name");
    let count: i64 = row.get_by_name_as::<i64>(schema, "count").unwrap_or(0);
}
```

### 7.2 提取单行(首行)

```rust
let row = dataset.iter().next()
    .ok_or_else(|| IamError::Business("记录不存在".into()))?;
let json_val = row.to_json_value(schema);
let permission: Permission = serde_json::from_value(json_val)?;
```

### 7.3 提取整列为 Vec

```rust
let ids: Vec<String> = dataset.iter()
    .filter_map(|row| row.get_by_name_as::<String>(schema, "id"))
    .collect();
```

---

## 八、完整示例:权限创建(事务 + DataValue + Option 糖)

```rust
use cmx_core::model::cell::DataValue;
use cmx_core::ParamsBuilder;

async fn create_permission(
    &self,
    txn_id: &str,
    data: &PermissionForCreate,
) -> Result<Permission, TraitError> {
    let id = cmx_utils::id::snowflake_id_str();
    let full_code_path = format!("/{}", data.code);
    let level = 1i64;

    let sql = "INSERT INTO cmx_permission \
               (id, code, name, resource_type, parent_id, sort_order, description, \
                domain_code, app_code, module_code, extension, status, archived, \
                parent_code, full_code_path, is_leaf, level) \
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 0, $13, $14, 1, $15)";

    // ★ 使用 .into() 糖,Option<String> 自动转 Null,Option<i64> 自动转 NullTyped(Int)
    let params = vec![
        DataValue::String(id),
        DataValue::String(data.code.clone()),
        DataValue::String(data.name.clone()),
        data.resource_type.clone().into(),      // Option<String> → String 或 Null
        data.parent_id.clone().into(),           // Option<String>
        data.sort_order.unwrap_or(0).into(),     // ★保留 None→0 语义
        data.description.clone().into(),         // Option<String>
        data.domain_code.clone().into(),
        data.app_code.clone().into(),
        data.module_code.clone().into(),
        data.extension.clone().into(),
        DataValue::Int(1),                       // status 默认 1
        parent_code.clone().into(),              // Option<String>
        DataValue::String(full_code_path),
        DataValue::Int(level),
    ];

    self.mm
        .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
        .await
        .map_err(|e| TraitError::Business(format!("新增权限失败: {e}")))?;

    // 查询返回
    let sql = "SELECT * FROM cmx_permission WHERE id = $1";
    let ds = self.mm
        .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, vec![DataValue::String(id)], "perm")
        .await?;
    extract_permission(ds)
}
```

## 九、完整示例:动态 UPDATE(ParamsBuilder)

```rust
use cmx_core::ParamsBuilder;
use cmx_core::model::cell::DataValue;

async fn update_rule(
    &self,
    rule_id: &str,
    data: UpdateRuleRequest,
) -> Result<(), TraitError> {
    // ★ ParamsBuilder 自动管理占位符,SET 从 $1 起
    let mut b = ParamsBuilder::new(0);
    b.set_opt("name", data.name)               // Option<String>
     .set_opt("priority", data.priority)       // Option<i64> → NullTyped(Int)
     .set_opt("status", data.status)           // Option<i64>
     .set_opt("description", data.description); // Option<String>
    let (set_clause, mut params) = b.build();

    if set_clause.is_empty() {
        return Err(TraitError::Business("未提供任何更新字段".into()));
    }

    // WHERE id 放最后
    let where_idx = params.len() + 1;
    params.push(DataValue::String(rule_id.to_string()));
    let sql = format!(
        "UPDATE cmx_rule SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
    );

    self.mm.execute_sql_with_datavalues(&self.db_id, None, &sql, params).await?;
    Ok(())
}
```

---

## 十、WASM 边界:DbRequest.data_values

### 10.1 问题:JSON 退化带类型 NULL

WASM plugin 通过 `DbRequest` 传参给宿主。旧路径只有 `params: Option<JsonValue>`,NULL 经 `json_to_data_values` 退化为无类型 `DataValue::Null`。

### 10.2 解决:data_values 字段

```rust
// WASM plugin 端(cmx-plugin-sdk)
use cmx_core::wasm_types::DbRequest;
use cmx_core::model::cell::{DataValue, SqlTypeMarker};

let req = DbRequest {
    sql: "INSERT INTO t (id, optional_int) VALUES ($1, $2)".into(),
    data_values: Some(vec![
        DataValue::String(id),
        DataValue::NullTyped(SqlTypeMarker::Int),  // ★带类型 NULL,跨边界保留
    ]),
    ..Default::default()
};
```

### 10.3 宿主端优先级

宿主 `do_query`/`do_execute` 用 `match (data_values, params)` 元组匹配:
1. **data_values 优先**(带类型 NULL 生效)
2. params JSON(向后兼容,旧 plugin 走这里)
3. 无参数

### 10.4 NullTyped 序列化格式

`DataValue::NullTyped(SqlTypeMarker::Int)` 序列化为字符串 `"$null:Int"`(与 Binary 的 `B64:` 前缀模式一致),跨 JSON/MsgPack 往返保留类型信息。

---

## 十一、反模式

### 11.1 ❌ 使用 execute_sql_with_json(新代码)

```rust
// ❌ 旧路径,JSON 退化 NULL 类型
let params = serde_json::json!([id, name, null]);
mm.execute_sql_with_json(&db_id, None, sql, params).await?;
```

```rust
// ✅ 使用 execute_sql_with_datavalues
let params = dv![id, name, None::<String>];
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

### 11.2 ❌ 手动 .map().unwrap_or(DataValue::Null)

```rust
// ❌ 冗长,且整型 NULL 丢失类型
let params = vec![
    name.map(DataValue::String).unwrap_or(DataValue::Null),
    count.map(DataValue::Int).unwrap_or(DataValue::Null),  // NULL 无类型!
];
```

```rust
// ✅ .into() 糖
let params: Vec<DataValue> = vec![
    name.into(),    // Option<String> → String 或 Null
    count.into(),   // Option<i64> → Int 或 NullTyped(Int) ★
];
```

### 11.3 ❌ 手动管理占位符编号

```rust
// ❌ idx 漂移风险
let mut idx = 2;
if let Some(name) = data.name {
    sets.push(format!("name = ${idx}"));
    params.push(DataValue::String(name));
    idx += 1;
}
```

```rust
// ✅ ParamsBuilder 自动管理
let mut b = ParamsBuilder::new(0);
b.set_opt("name", data.name);
```

### 11.4 ❌ 盲目把 unwrap_or(DataValue::Int(0)) 改成 .into()

```rust
// 原代码语义:None → 0(有默认值)
data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Int(0))

// ❌ 错误改法:None → NullTyped(Int),语义变了(NULL ≠ 0)
data.sort_order.into()

// ✅ 正确改法:保留 None→0 语义
data.sort_order.unwrap_or(0).into()
```

### 11.5 ❌ 在 vec![] 中混用 .into() 和 DataValue::X 导致类型推断歧义

```rust
// ❌ 可能报类型推断错误(vec![] 的元素类型不明确)
let params = vec![
    id,           // String → ?
    count.into(), // ? → ?
];
```

```rust
// ✅ 显式标注或用 dv! 宏
let params: Vec<DataValue> = vec![
    DataValue::String(id),
    count.into(),
];
// 或
let params = dv![id, count];
```

### 11.6 ❌ 滥用 cmx-database-pg 替代 cmx-database

```rust
// ❌ 反模式：非流式场景引入 cmx-database-pg
use cmx_database_pg::get_default_pg_db_manager;
let mm = get_default_pg_db_manager();
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

```rust
// ✅ 正确：默认用 cmx-database
use cmx_database::get_default_db_manager;
let mm = get_default_db_manager();
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

> cmx-database-pg 仅在需要 `query_sql_zmc_stream_chunks` 或数组列读取还原时引入。

### 11.7 ❌ 用 cmx-database-pg 的 with_json API

```rust
// ❌ 反模式：cmx-database-pg 的 with_json 同样不推荐
use cmx_database_pg::get_default_pg_db_manager;
let mm = get_default_pg_db_manager();
mm.query_sql_with_json(&db_id, None, sql, json!([id]), "ds").await?;
```

```rust
// ✅ 正确：两 crate 均用 _with_datavalues
mm.query_sql_with_datavalues(&db_id, None, sql, dv![id], "ds").await?;
```

### 11.8 ❌ 在事务内调 query_sql_zmc（ZmcDataSet 不参与事务）

```rust
// ❌ 反模式：query_sql_zmc 是只读连接池路径，不走事务
let txn_id = mm.get_transaction_context().begin(&db_id).await?;
let zmc_ds = mm.query_sql_zmc_with_datavalues(&db_id, sql, params, "ds").await?;
// ⚠️ zmc_ds 不在事务内，读到的是其他连接的快照
mm.commit_transaction(&txn_id).await?;
```

```rust
// ✅ 正确：事务内用 query_sql_with_datavalues（返回 DataSet）
let ds = mm.query_sql_with_datavalues(&db_id, Some(&txn_id), sql, params, "ds").await?;
```

> `query_sql_zmc*` 系列只读、走连接池、不参与事务；业务单据装载是只读场景才用 ZmcDataSet。

---

## 十二、关键源文件参考

| 文件 | 职责 |
|------|------|
| `crates/libs/cmx-infra/cmx-database/src/manager/mod.rs` | cmx-database DatabaseManager API(execute_sql_with_datavalues / query_sql_zmc 等) |
| `crates/libs/cmx-infra/cmx-database/src/transaction/api.rs` | SqlParams 枚举、execute_sql_with_params 底层 |
| `crates/libs/cmx-infra/cmx-database/src/executor/mod.rs` | bind_data_value_postgres/mysql/sqlite 绑定层 |
| `crates/libs/cmx-infra/cmx-database/src/host_functions.rs` | WASM do_query/do_execute(data_values 优先) |
| `crates/libs/cmx-infra/cmx-database/src/zmc.rs` | sqlx 侧 ZmcRowSource 实现 + query_zmc 出口 |
| `crates/libs/cmx-infra/cmx-database-pg/src/manager/mod.rs` | cmx-database-pg DatabaseManager API(含 query_sql_zmc_stream_chunks 独占) |
| `crates/libs/cmx-infra/cmx-database-pg/src/connection/mod.rs` | DbPool 层 query_zmc / query_zmc_stream_chunks / query_zmc_streaming 实现 |
| `crates/libs/cmx-infra/cmx-database-pg/src/zmcdataset/mod.rs` | tokio-postgres 侧 ZmcDataSet + 分帧编码器 |
| `crates/libs/cmx-core/src/model/cell.rs` | DataValue/SqlTypeMarker/SqlParam/dv! 宏/From<Option<T>> |
| `crates/libs/cmx-core/src/model/builder.rs` | ParamsBuilder |
| `crates/libs/cmx-core/src/wasm_types/database.rs` | DbRequest(data_values 字段) |
| `crates/libs/cmx-iam/src/permission/service.rs` | 实战示例:权限 CRUD + 事务 + Option 糖 |
| `crates/libs/cmx-iam/src/rule/service.rs` | 实战示例:ParamsBuilder 动态 UPDATE |

---

## 十三、与其他技能的协同

| 协同技能 | 关系 | 触发场景 |
|---------|------|---------|
| `axum-handler-generator` | 上游:handler 调 Service,Service 内执行 SQL | 写完 handler 后需要实现 Service 的 SQL 逻辑 |
| `modql` | 互补:modql 关注 Filter/sea-query,本技能关注 raw SQL | 用 GenericCrudService 时调 modql;手写 SQL 时调本技能 |
| `sql-guide` | 互补:sql-guide 关注 DDL/migrations 文件,本技能关注 Rust 代码执行 SQL | 写 .sql 文件调 sql-guide;写 .rs SQL 执行调本技能 |
| `pg-table-generator` | 上游:生成表结构后,本技能指导如何查询该表 | 先用 pg-table-generator 设计表,再用本技能写查询 |
