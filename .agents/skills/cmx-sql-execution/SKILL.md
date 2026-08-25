---
name: cmx-sql-execution
description: 指导在 Rust 代码中执行 SQL 的规范，涵盖 DatabaseManager API 选择、DataValue 参数构造（dv! 宏 / From<Option<T>> / ParamsBuilder）、带类型 NULL（NullTyped）、事务模式。当用户编写手写 SQL 执行代码、构造 Vec<DataValue> 参数、构建动态 UPDATE SET 子句、处理 NULL 绑定类型问题、或在业务 Service 中调用 execute_sql / query_sql 系列 API 时必用。
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
| ① | **`query_sql_zmc_stream_chunks`** | `manager/mod.rs` + `connection/mod.rs` | 真·分帧流式：基于 `mpsc::Sender<Bytes>`，逐行编码为长度分帧发送，峰值内存 O(单行)，16KB 攒批刷写，header 帧先发、空结果容错。cmx-database **完全无此方法** |
| ② | **数组类型列读取还原** | `executor/mod.rs`（`PgResultConverter::convert_rows`） | 读取阶段支持 TEXT_ARRAY / INT8_ARRAY / UUID_ARRAY -> `DataValue::Array`。cmx-database 读取方向**不还原数组**（只在绑定时 `bind_pg_array_postgres` 支持写入） |
| ③ | **`get_conn()` 方法** | `connection/mod.rs` | 返回 `deadpool_postgres::Object`，供事务层跨 await 手动驱动 BEGIN/COMMIT。cmx-database 用 sqlx 的 `pool.begin()`，无需此方法 |
| ④ | **4 个 ToSql 适配器** | `executor/mod.rs` | `PgInt` / `PgDateTime` / `PgDateTimeNull` / `PgIntNull`。tokio-postgres 类型校验严格（i64 绑 INT4 列会 WrongType），需宽度/时区自适应包装。sqlx 隐式协调，不需要 |

> ⚠️ **注意区分**：`query_zmc_streaming`（写入 `Vec<u8>`）**两者都有**；唯独 `*_stream_chunks`（mpsc 通道）是 pg 独有。

**选择规则**：

```
需要 query_sql_zmc_stream_chunks（mpsc 分帧流式）?
├─ 是 -> ★ cmx-database-pg（独占能力）
└─ 否 -> 需要数组列读取还原（DataValue::Array 从数据库读取）?
    ├─ 是 -> ★ cmx-database-pg
    └─ 否 -> ★★★ cmx-database（默认首选）
```

**依赖现状**（cmx-api 单体拆分为 `cmx-apis/*` 后的格局，2026-08 核对）：

| 情形 | crate 数 | 具体 |
|------|---------|------|
| 同时依赖两者 | 5（+1 test） | cmx-api-core、cmx-common-api、cmx-biz、cmx-platform-app、cmx-service-base（另 tests/cmx-database-test） |
| 只依赖 cmx-database-pg | 3 | cmx-rowsource、cmx-job-store-pg、cmx-web-monitor |
| 只依赖 cmx-database | 7 | cmx-iam、cmx-metadata、cmx-plugin、cmx-service、cmx-api-types、cmx-biz-api、cmx-plugin-api |

**能否将 cmx-database-pg 的消费方替换为 cmx-database？**

🟢 **可以无痛替换**（占大多数场景）：
- 凡只用到 `execute_sql*` / `query_sql*` / `query_sql_zmc` / `query_sql_zmc_with_datavalues` / `crud::*` / `transaction::*` / `migration::*` / `host_functions` / `ZmcDataSet` 的消费方
- 注意 `SqlParams::SeaValues` -> `SqlxValues` 的枚举变体替换
- 数据源注册真源：`cmx-service-base/src/datasource.rs`、`cmx-platform-app/src/config/datasource.rs`
- `execute_sql_with_json` 已无调用方（仅两栈 manager 内保留定义），新代码禁止再用

🔴 **不能简单替换**（需迁移实现）：
- 依赖 `query_sql_zmc_stream_chunks` 的场景（当前仅 `crates/tests/cmx-database-test` 的 `mem_bench.rs` / `e2e_server.rs` 与 pg 自身，需要 O(单行) 内存的流式消费）
- 依赖数组列读取还原（`DataValue::Array` 从数据库读取）的场景
- 直接依赖 `TokioPgRowSource` 全路径的代码（如 `cmx-database-test` 的 `e2e_server.rs`、`mem_bench.rs`）需改为 `SqlxPgRowSource`

> **默认使用 `cmx-database`**。除非必须使用上述 4 项独有能力，否则不引入 cmx-database-pg。
>
> **两 crate 的 `with_json` 系列 API 均不推荐**：`execute_sql_with_json` / `query_sql_with_json` 仅维护旧代码，新代码必须用 `_with_datavalues`。
>
> **导出对称性缺口**（不影响功能）：cmx-database 把 `SqlxPgRowSource` 提升到了 crate 根（`lib.rs`），而 pg 侧的 `TokioPgRowSource` 只能走全路径 `cmx_database_pg::zmcdataset::TokioPgRowSource`。

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

---

## 三、references 索引（细节层，按需读取）

| 文件 | 何时读 | 核心内容 |
|------|--------|----------|
| `references/datavalue-and-params.md` | 构造 SQL 参数时 | DataValue 基础构造 / From<Option<T>> 糖 / None→0 vs None→NULL 语义 / dv! 宏 / 数组参数(IN) / ParamsBuilder 全 API / set_opt vs set_opt_null / NullTyped 与 SqlTypeMarker |
| `references/transactions-and-dataset.md` | 写事务、取结果时 | 事务内执行/查询 / 非事务执行 / DataSet 遍历/单行/整列提取 / 权限创建完整示例（事务+DataValue+Option 糖）/ 动态 UPDATE 完整示例（ParamsBuilder） |
| `references/wasm-boundary-antipatterns.md` | WASM 插件内执行 SQL、或自查写法时 | DbRequest.data_values 优先级 / NullTyped 序列化格式 / 8 类反模式（with_json / 手写 unwrap_or / 手动占位符 / 语义误改 / 混用 .into() / 滥用 pg / zmc 事务误用） |

**读取原则**：先读 SKILL.md §二 选对 API → 按场景读 1 个 reference → 写完对照 wasm-boundary-antipatterns.md 自查。WASM 插件侧另有 [wasm-plugin-developer](../wasm-plugin-developer/SKILL.md) 技能负责工程三层架构与 DbRequest 差异速览。

---

## 四、关键源文件参考

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
| `crates/libs/cmx-iam/src/permission/service/`（目录，含 import.rs 等） | 实战示例:权限 CRUD + 事务 + Option 糖 |
| `crates/libs/cmx-iam/src/rule/service.rs` | 实战示例:ParamsBuilder 动态 UPDATE |

---

## 五、与其他技能的协同

| 协同技能 | 关系 | 触发场景 |
|---------|------|---------|
| `axum-handler-generator` | 上游:handler 调 Service,Service 内执行 SQL | 写完 handler 后需要实现 Service 的 SQL 逻辑 |
| `modql` | 互补:modql 关注 Filter/sea-query,本技能关注 raw SQL | 用 GenericCrudService 时调 modql;手写 SQL 时调本技能 |
| `sql-guide` | 互补:sql-guide 关注 DDL/migrations 文件,本技能关注 Rust 代码执行 SQL | 写 .sql 文件调 sql-guide;写 .rs SQL 执行调本技能 |
| `pg-table-generator` | 上游:生成表结构后,本技能指导如何查询该表 | 先用 pg-table-generator 设计表,再用本技能写查询 |
