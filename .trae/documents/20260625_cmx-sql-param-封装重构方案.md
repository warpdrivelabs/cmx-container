# cmx SQL 参数封装重构方案(带类型 NULL + 构造糖 + builder + wasm 打通)

> **状态:** 设计评审中
> **目标:** 解决 `Vec<DataValue>` 迁移后遗留的四大痛点 —— NULL 丢失类型、Optional 构造繁琐、占位符手动对齐、**wasm 边界无法传带类型参数**
> **范围:** cmx-core(DataValue + SqlParam + DbRequest + dv! + ParamsBuilder)、cmx-database(绑定层 + 宿主桥 + 新 API)、cmx-iam(调用处重构)

---

## 〇、关键发现:wasm 边界是 JSON,带类型 NULL 会失效(本次设计重点)

调查 wasm plugin 的 SQL 执行链路后,发现一个**之前被完全遗漏的根本问题**:

```
plugin(wasm)
  DbRequest { params: Option<serde_json::Value> }   ← 边界只有 JSON
     ↓  rmp_serde (MsgPack)
宿主 DatabaseHostFunctions::do_query
     ↓  query_sql_with_json(sql, params_json)        ← 硬编码走 JSON 分支
  json_to_data_values → serde_json::from_value(Null) → DataValue::Null
     ↓  bind None::<String>                          ← 类型信息彻底丢失
PostgreSQL                                           ← 非 TEXT 列炸
```

**结论:** 即使把 `NullTyped` 加到 DataValue,**只要 wasm 边界仍是 `serde_json::Value`**,wasm plugin 传的任何 NULL 都会退化成无类型的 `DataValue::Null`,无法修复非 TEXT 列的绑定。

**事实依据:**
- `cmx-plugin-sdk` 依赖 `cmx-core` 但**不依赖** `cmx-database`(sqlx 无法编译到 wasm)
- `DbRequest` 定义在 `cmx-core/wasm_types/database.rs`,`params` 类型是 `Option<serde_json::Value>`
- 宿主 `host_functions.rs:71` 的 `do_query` 硬编码调 `query_sql_with_json`
- `DataValue` **已在 cmx-core**,且已手写 `Serialize/Deserialize`,MsgPack(rmp-serde)可直接序列化

**因此本次必须打通 wasm 边界**:给 `DbRequest` 增加 `data_values: Option<Vec<DataValue>>` 变体,让 plugin 能直接传带类型的 `DataValue`(含 `NullTyped`),宿主识别后走 `query_sql_with_datavalues`。

---

## 一、问题根因(为什么原方案不够好)

原迁移方案只做了「`Value::Array(vec![...])` → `vec![DataValue::...]`」的字面替换,**没有解决两个根本缺陷**:

### 缺陷 1:`DataValue::Null` 绑定时丢失类型信息

`executor/mod.rs` 的三个 bind 函数把所有 NULL 硬编码成 `None::<String>`:

```rust
// bind_data_value_postgres (executor/mod.rs:134)
DataValue::Null       => query.bind(None::<String>),  // sqlx 推断为 TEXT
DataValue::Array(_)   => query.bind(None::<String>),  // 未实现,静默变 NULL
DataValue::ShortStr(_)=> query.bind(None::<String>),  // 未实现,静默变 NULL
DataValue::LongStr(_) => query.bind(None::<String>),  // 未实现,静默变 NULL
```

**后果:** sqlx 的 `bind` 要求 `T: Encode + Type<Postgres>`。`None::<String>` 使 sqlx 认为 NULL 目标类型是 TEXT。当占位符 `$N` 对应 `INTEGER`/`TIMESTAMP`/`UUID`/`BOOL` 列时,**PostgreSQL prepare 阶段报类型不匹配**(`expected type integer, got type text`)。

当前所有 `unwrap_or(DataValue::Null)` 绑定到非字符串列的场景都处于「靠运气」状态。

### 缺陷 2:Optional 字段构造冗长

cmx-iam 中约 **30 处** `.map(DataValue::String).unwrap_or(DataValue::Null)`,集中在 permission/rule 的 create/update。例(`permission/service.rs:978-994`):

```rust
let params = vec![
    DataValue::String(id),
    data.resource_type.clone().map(DataValue::String).unwrap_or(DataValue::Null),  // ← 冗长
    data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Int(0)),
    data.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),    // ← 冗长
    // ... 还有 11 个
];
```

### 缺陷 3:多列 INSERT/UPDATE 占位符手动对齐

约 5 处「N 列 VALUES/SET + WHERE」的 `$1..$N` 占位符与 vec 位置必须严格对齐,动态 UPDATE(`rule/service.rs`)还需 SQL SET 子句与 params push 顺序双重维护,**极易漂移**。

---

## 二、分层架构设计

基于你的决策(三个宿主侧问题都要解决,NULL 方案方式1+2都要,wasm 边界用 data_values 变体打通),采用**分层架构**,确保 wasm plugin 与宿主端共享同一套带类型参数能力:

```
┌──────────────────────────┐    ┌──────────────────────────┐
│  cmx-iam (宿主调用层)     │    │  wasm plugin (plugin-sdk)│
│  dv! 宏 + ParamsBuilder   │    │  dv! 宏 + ParamsBuilder   │
│  + .into()                │    │  (同样可用)               │
└────────────┬─────────────┘    └────────────┬─────────────┘
             │                               │ DbRequest{ data_values }
             │ Vec<DataValue>                │ rmp_serde (MsgPack)
             ▼                               ▼
     ┌───────────────────────────────────────────────┐
     │        wasm 边界 / 宿主入口                     │
     │  DbRequest.data_values: Option<Vec<DataValue>>│
     │  (新,带类型) | DbRequest.params: Option<JSON> │
     │  (旧,向后兼容)                                 │
     └────────────┬──────────────────┬───────────────┘
                  │ JSON 路径(旧)     │ DataValue 路径(新)
                  ▼                  ▼
┌─────────────────────────────────────────────────────┐
│  cmx-database (SQL 绑定层,仅宿主)                    │
│  • SqlParams 复数枚举(Json/DataValues/Typed)        │
│  • bind 函数识别 NullTyped/Array                    │
│  • query_sql_with_json / _datavalues / _typed       │
│  • do_query 识别 data_values 走 datavalues 分支      │
└──────────────────┬──────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│  cmx-core (纯域类型,无 sqlx,✅ wasm 可用)                  │
│  • DataValue 增加 NullTyped(SqlTypeMarker) 变体              │
│  • SqlTypeMarker: 不依赖 sqlx 的轻量标记枚举                  │
│  • SqlParam: 带类型 NULL 的参数枚举(宿主+wasm 共享)         │
│  • From<Option<T>> + dv! 宏 + ParamsBuilder                  │
│  • DbRequest 增加 data_values 变体(跨 wasm 边界)            │
└──────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────┐
│  cmx-iam (调用层)                                    │
│  使用 dv! 宏 + ParamsBuilder + .into()              │
└──────────────────┬──────────────────────────────────┘
                   │ Vec<DataValue> / SqlParam
┌──────────────────▼──────────────────────────────────┐
│  cmx-database (SQL 绑定层)  ← SqlType 在这里         │
│  • bind 函数识别 NullTyped(SqlType)                 │
│  • 新 API: query_sql_typed / execute_sql_typed      │
└──────────────────┬──────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────────────┐
│  cmx-core (纯域类型,无 sqlx,✅ wasm 可用)                  │
│  • DataValue 增加 NullTyped(SqlTypeMarker) 变体              │
│  • SqlTypeMarker: 不依赖 sqlx 的轻量标记枚举                  │
│  • From<Option<T>> 转换 + dv! 宏                              │
│  • ParamsBuilder(动态 SET 子句构造器)                        │
└──────────────────────────────────────────────────────────────┘
```

### 关键设计决策:wasm 边界与 SqlParam 的归属

`SqlTypeMarker`、`SqlParam`、`dv!`、`ParamsBuilder`、`DbRequest.data_values` **全部放 cmx-core**(无 sqlx 依赖,wasm 可用)。原因:

- **`cmx-plugin-sdk`(wasm 侧)依赖 `cmx-core` 但不依赖 `cmx-database`**(sqlx 无法编译到 wasm)。任何 wasm plugin 需要用的类型必须在 cmx-core。
- `DataValue` **已经在 cmx-core**(`cmx_core::model::cell::DataValue`),且已派生 `Serialize/Deserialize`,MsgPack 可直接序列化,跨 wasm 边界无障碍。
- `SqlParam` 作为「带类型 NULL 的参数枚举」,是 DataValue 的上层便捷封装,**宿主和 wasm plugin 都应能用**,因此放 cmx-core。

`cmx-database` 只保留:bind 函数(sqlx 绑定逻辑)、`SqlParams` 复数聚合枚举、`*_typed` API、`DatabaseHostFunctions`(宿主桥,识别 `data_values` 走 datavalues 分支)。

> **判定原则:只操作 DataValue/字符串、不需要 sqlx 的工具一律放 cmx-core,确保 wasm 侧可用。** 只在真正需要 sqlx 的绑定/连接/事务层才放 cmx-database。

---

## 三、各组件设计

### 3.1 组件一:`SqlTypeMarker` + `DataValue::NullTyped`(cmx-core)

**文件:** `crates/libs/cmx-core/src/model/cell.rs`

```rust
/// SQL 列类型标记(不依赖 sqlx,用于描述 NULL 的目标类型)。
///
/// 仅在 `DataValue::NullTyped` 中携带,告诉绑定层
/// 这个 NULL 应绑定为哪种数据库类型。
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

#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Null,
    /// 带类型信息的 NULL —— 绑定到非字符串列时必须使用。
    NullTyped(SqlTypeMarker),
    Bool(bool),
    Int(i64),
    // ... 其余不变
}
```

`Serialize`/`Deserialize`:`NullTyped` 序列化为 `{"$nullTyped": "Int"}` 之类(与 `Null` 区分),反序列化兼容。

### 3.2 组件二:`From<Option<T>>` 构造糖(cmx-core)

**文件:** `crates/libs/cmx-core/src/model/cell.rs`

```rust
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
// Option<f64>, Option<bool>, Option<Uuid>, Option<DateTime<Utc>> 同理
```

**调用处对比:**
```rust
// 前: data.description.clone().map(DataValue::String).unwrap_or(DataValue::Null)
// 后: data.description.clone().into()   // 或 DataValue::from(data.description.clone())
```

> 注意:`Option<i64>` 走 `NullTyped(Int)` 而非 `Null`,因为整型列大多不接受 TEXT 型 NULL —— 这正是缺陷1的修复点。

### 3.3 组件三:`dv!` 宏(cmx-core)

**文件:** `crates/libs/cmx-core/src/model/cell.rs`(或单独 `macros.rs`)

提供批量构造 + 可空标注:

```rust
/// 构造 Vec<DataValue> 的便捷宏。
/// - `str x`   → DataValue::String(x)
/// - `str? x`  → Option<String> 的糖(None 时 NullTyped(Text))
/// - `int? x`  → Option<i64> 的糖(None 时 NullTyped(Int))
/// - `null Int`→ DataValue::NullTyped(SqlTypeMarker::Int)
#[macro_export]
macro_rules! dv {
    // 空
    () => { Vec::<$crate::model::cell::DataValue>::new() };
    // 带类型的 null
    (null $t:ident) => { $crate::model::cell::DataValue::NullTyped($crate::model::cell::SqlTypeMarker::$t) };
    // 可空:类型? expr
    ($kind:ident ? $e:expr $(,)?) => {
        $crate::model::cell::DataValue::__from_opt($kind, $e)
    };
    // 递归多条
    ($($tt:tt)+) => { /* 展开为 vec![...] */ };
}
```

> 宏设计为**内部 helper 函数驱动**(`__from_opt`),降低宏复杂度,提升可测试性。

**调用处对比:**
```rust
// 前(15 行):
let params = vec![
    DataValue::String(id),
    data.resource_type.clone().map(DataValue::String).unwrap_or(DataValue::Null),
    data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Int(0)),
    data.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
    DataValue::Int(level),
];
// 后:
let params = dv![
    str  id,
    str? data.resource_type,
    int? data.sort_or_default(),   // 或 int? data.sort_order
    str? data.description,
    int  level,
];
```

### 3.4 组件四:`SqlParam` 上层枚举(cmx-core)

> **⚠️ 放置位置:** `cmx-core`,而非 cmx-database。
>
> **理由:** `SqlParam` 是 DataValue 的上层封装,宿主和 wasm plugin **都应能用**。
> `cmx-plugin-sdk` 依赖 cmx-core,放这里 wasm plugin 可直接构造带类型参数。
> cmx-database 只保留 sqlx 绑定逻辑(bind 函数)。

**文件:** `crates/libs/cmx-core/src/model/cell.rs`(与 DataValue/SqlTypeMarker 同处)

面向「明确知道列类型」的场景,提供更强的类型化入口:

```rust
/// 面向 SQL 绑定的参数类型,内含带类型的 NULL。
/// 比 DataValue 更贴近 SQL 语义,适合手写 SQL 的强类型场景。
/// 宿主端和 wasm plugin 都可使用。
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

impl From<DataValue> for SqlParam { /* 逐变体映射,NullTyped→Null(marker) */ }
impl From<SqlParam> for DataValue { /* 反向 */ }
```

### 3.5 组件五:bind 层识别 NullTyped(cmx-database)

**文件:** `crates/libs/cmx-infra/cmx-database/src/executor/mod.rs`

修改三个 bind 函数(`bind_data_value_postgres/mysql/sqlite`),让 `NullTyped` 绑定正确的 sqlx 类型:

```rust
pub fn bind_data_value_postgres<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    use SqlTypeMarker::*;
    match param {
        DataValue::Null => query.bind(None::<String>),  // 向后兼容:默认 TEXT
        DataValue::NullTyped(t) => match t {
            Bool      => query.bind(None::<bool>),
            Int       => query.bind(None::<i64>),
            Float     => query.bind(None::<f64>),
            Decimal   => query.bind(None::<Decimal>),
            Text      => query.bind(None::<String>),
            Timestamp => query.bind(None::<DateTime<Utc>>),
            Date      => query.bind(None::<NaiveDate>),
            Uuid      => query.bind(None::<Uuid>),
            Json      => query.bind(None::<serde_json::Value>),
            Binary    => query.bind(None::<Vec<u8>>),
        },
        DataValue::Array(v) => /* 实现:走 PG 数组绑定 */,
        // ShortStr/LongStr 改为正确绑定(见 3.7)
        ...
    }
}
```

> MySQL/SQLite 同理(`NullTyped(Text)` 仍走 `None::<String>`,其它按驱动能力映射)。

### 3.6 组件六:新 API `*_typed`(cmx-database)

**文件:** `crates/libs/cmx-infra/cmx-database/src/manager/mod.rs` + `transaction/api.rs`

新增接收 `Vec<SqlParam>` 的 API(与 datavalues/json 并列):

```rust
// manager/mod.rs
pub async fn query_sql_typed(&self, db_id, txn_id, sql, params: Vec<SqlParam>, dataset_id) -> Result<DataSet>
pub async fn execute_sql_typed(&self, db_id, txn_id, sql, params: Vec<SqlParam>) -> Result<u64>
```

底层在 `transaction/api.rs` 给 `SqlParams` 枚举加 `Typed(Vec<SqlParam>)` 变体,内部转 `Vec<DataValue>` 后走现有 `execute_with_datavalues`(或直接 bind SqlParam,二选一,推荐前者以复用逻辑)。

### 3.7 组件七:`ParamsBuilder`(cmx-core)

> **⚠️ 放置位置:** `cmx-core`,而非 cmx-database。
>
> **理由:** `ParamsBuilder` 只操作 `DataValue` + 字符串,零 sqlx/tokio 依赖。
> 而 `cmx-database` 依赖 sqlx,无法编译到 wasm。`cmx-plugin-sdk`(wasm 侧)
> 依赖 `cmx-core` 但**不依赖** `cmx-database`,若放 cmx-database 会阻断 wasm plugin 使用。
> cmx-core 的依赖(serde/chrono/uuid/modql 等)全部 wasm 兼容。

**文件:** `crates/libs/cmx-core/src/model/builder.rs`(新建)

解决动态 UPDATE 的占位符漂移:

```rust
/// 动态构造 UPDATE SET 子句 + 参数,自动管理占位符编号。
pub struct ParamsBuilder {
    assignments: Vec<String>,   // "name = $2"
    params: Vec<DataValue>,
    next_index: usize,           // 从 start 起
}

impl ParamsBuilder {
    /// start = 已有占位符数(如 WHERE 的 $1 已用,start=2)。
    pub fn new(start_offset: usize) -> Self { ... }

    /// 必填列。
    pub fn set(&mut self, col: &str, val: impl Into<DataValue>) -> &mut Self {
        let idx = self.next_index; self.next_index += 1;
        self.assignments.push(format!("{col} = ${idx}"));
        self.params.push(val.into());
        self
    }

    /// 可选列:None 时跳过(不加入 SET),避免无谓赋值。
    pub fn set_opt(&mut self, col: &str, val: Option<impl Into<DataValue>>) -> &mut Self {
        if let Some(v) = val { self.set(col, v.into()); }
        self
    }

    /// 返回 ("col1 = $2, col2 = $3", params)。
    pub fn build(self) -> (String, Vec<DataValue>) { ... }
}
```

**调用处对比(`rule/service.rs` 动态 UPDATE):**
```rust
// 前:手动拼 SQL + 手动 push params,两处顺序必须一致
let mut params: Vec<DataValue> = vec![DataValue::String(rule_id.to_string())];
let mut sets = vec![];
if let Some(name) = name { sets.push(format!("name = ${}", sets.len()+2)); params.push(DataValue::String(name)); }
// ... 易漂移

// 后:
let mut b = ParamsBuilder::new(1);  // WHERE id = $1 已占
b.set_opt("name", name.map(Into::into))
 .set_opt("sort_order", sort_order.map(Into::into))
 .set_opt("status", status.map(Into::into));
let (set_clause, mut params) = b.build();
params.push(DataValue::String(rule_id.to_string()));  // WHERE 参数
let sql = format!("UPDATE cmx_rule SET {set_clause} WHERE id = $1");
```

### 3.8 组件八:wasm 边界打通 —— DbRequest.data_values(cmx-core + cmx-database)

> **本组件是本次设计的核心增量**,让 wasm plugin 也能传带类型 NULL 的参数。

**涉及文件:**
- `crates/libs/cmx-core/src/wasm_types/database.rs` — DbRequest 增加字段
- `crates/libs/cmx-infra/cmx-database/src/host_functions.rs` — do_query/do_execute 识别新字段

**(a) DbRequest 增加 data_values 字段(cmx-core):**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbRequest {
    pub sql: String,
    /// 旧:JSON 参数(向后兼容,wasm plugin 旧版本仍可用)。
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    /// 新:带类型的 DataValue 参数数组(含 NullTyped)。
    /// 与 params 互斥;若同时设置,data_values 优先。
    #[serde(default)]
    pub data_values: Option<Vec<cmx_core::model::cell::DataValue>>,
    #[serde(default)]
    pub dataset_id: Option<String>,
    #[serde(default)]
    pub db_id: Option<String>,
    #[serde(default)]
    pub txn_id: Option<String>,
}
```

> **MsgPack 兼容性:** `DataValue` 已手写 `Serialize/Deserialize`,rmp-serde 走 serde trait,可直接序列化。`#[serde(default)]` 确保旧 plugin(不发 data_values)反序列化为 None,向后兼容。

**(b) 宿主 do_query/do_execute 识别 data_values(cmx-database):**

修改 `host_functions.rs` 的 `do_query`(L68-81)和 `do_execute`(L128-141):

```rust
// do_query 的 match params 分支改为:
match (request.data_values, params) {
    // 新路径:带类型 DataValue(优先)
    (Some(data_values), _) => {
        db_manager
            .query_sql_with_datavalues(&db_id, request_txn_id.as_deref(), &sql, data_values, &dataset_id)
            .await
            .map_err(|e| e.to_string())
    }
    // 旧路径:JSON(向后兼容)
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

**(c) plugin-sdk 可选增强(plugin-sdk):**

`cmx-plugin-sdk` 的 `HostCaller` 可增加便捷构造方法(可选,非必须,因 DbRequest 字段已 public):

```rust
// plugin-sdk/src/host_calls.rs 新增便捷构造器
impl DbRequest {
    /// 用带类型的 DataValue 构造请求(wasm plugin 推荐)。
    pub fn with_data_values(sql: impl Into<String>, data_values: Vec<DataValue>) -> Self {
        DbRequest {
            sql: sql.into(),
            data_values: Some(data_values),
            ..Default::default()
        }
    }
}
```

**plugin 侧使用示例:**
```rust
// wasm plugin 现在可以这样传带类型 NULL:
let req = DbRequest::with_data_values(
    "INSERT INTO t(id, parent_id, sort_order) VALUES ($1, $2, $3)",
    vec![
        DataValue::String(id),
        DataValue::NullTyped(SqlTypeMarker::Uuid),  // ← 带 Uuid 类型的 NULL
        sort_order.into(),  // Option<i64>,None→NullTyped(Int)
    ],
);
let resp = self.host.db_query(req)?;
```

---

## 四、实施任务分解(高层)

> 详细 bite-sized 步骤将在正式 plan 文档展开。每个 Task 独立可编译、可测试、可提交。

| Task | 内容 | crate | 依赖 |
|------|------|-------|------|
| 1 | `SqlTypeMarker` + `DataValue::NullTyped` 变体 + Serde | cmx-core | 无 |
| 2 | `From<Option<T>>` 构造糖 | cmx-core | T1 |
| 3 | `dv!` 宏 + helper 函数 | cmx-core | T1,T2 |
| 4 | `SqlParam` 枚举 + From 互通(**cmx-core**) | cmx-core | T1 |
| 5 | bind 层识别 `NullTyped` + 修复 `Array/ShortStr/LongStr`(三处) | cmx-database | T1 |
| 6 | 新 API `query_sql_typed`/`execute_sql_typed` | cmx-database | T4 |
| 7 | `ParamsBuilder`(**cmx-core**) | cmx-core | T2 |
| 8 | **wasm 打通:DbRequest.data_values + 宿主 do_query 识别** | cmx-core + cmx-database | T1 |
| 9 | cmx-iam 调用处重构(permission/rule 优先,用新糖) | cmx-iam | T1-T8 |
| 10 | 单元测试 + 全量验证 | 全部 | T9 |

---

## 五、向后兼容性

| 改动 | 兼容性 | 说明 |
|------|--------|------|
| `DataValue::NullTyped` 新增变体 | ⚠️ 需审查 exhaustive match | 所有 `match dv { ... }` 需补 `NullTyped` 分支。**这是破坏性改动**,需全局补全(主要是 cell.rs 的 Serde 和 executor 的 bind)。 |
| `DataValue::Null` 行为不变 | ✅ | 默认仍绑 TEXT,旧代码不受影响 |
| `From<Option<T>>` 新增 | ✅ | 纯增量,可能引入歧义(已有 `From<String>` vs `From<Option<String>>`),需确认无冲突 |
| 新 API `*_typed` | ✅ | 纯增量,旧 `*_datavalues` 保留 |
| `dv!` 宏 / ParamsBuilder | ✅ | 纯增量,可选使用 |

**主要风险:** `DataValue` 是核心枚举,新增变体会触发所有 exhaustive match 的编译错误。这其实是**好事**(编译器强制审查),但需在 Task 1 一次性补全所有 match 分支。

---

## 六、已确认决策点

1. **✅ SqlTypeMarker 放 cmx-core**:作为无 sqlx 依赖的纯标记枚举,`DataValue::NullTyped(SqlTypeMarker)` 可随 DataSet 传输。cmx-database 负责翻译成 sqlx 绑定。**`NullTyped` 序列化为 null**(与 Null 一致,类型信息在传输层无意义,只在绑定时需要)。

2. **✅ Array 绑定本次实现单层同类型**:实现 `DataValue::Array` 绑定为 PG 数组(元素需同类型,如 `Vec<String>`、`Vec<i64>`)。覆盖 cmx-iam 的 4 处 IN 查询嵌套数组。复杂嵌套留后。

3. **✅ cmx-iam 重构先 permission + rule**:本次只重构 `permission/service.rs` 和 `rule/service.rs`(~25处 Optional + 动态 UPDATE)。验证新 API 稳定后,再推广到 user/role/role_group(作为独立后续工作)。
