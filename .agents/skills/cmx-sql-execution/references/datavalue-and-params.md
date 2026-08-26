# DataValue 参数构造 / ParamsBuilder / NullTyped

> 本文件是 cmx-sql-execution 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

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
