# WASM 边界 / 反模式

> 本文件是 cmx-sql-execution 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

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
