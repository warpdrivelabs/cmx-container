# cmx-database 模块代码审查报告

**审查日期**: 2026-03-10
**审查依据**: `docs/cmx-database-redesign.md` v2.0.0 设计文档
**代码路径**: `crates/libs/cmx-infra/cmx-database/`

---

## 目录

1. [严重问题（Bug / 正确性）](#1-严重问题bug--正确性)
2. [设计文档与代码不一致项](#2-设计文档与代码不一致项)
3. [代码质量问题](#3-代码质量问题)
4. [架构合理性问题](#4-架构合理性问题)
5. [测试覆盖问题](#5-测试覆盖问题)
6. [修改建议优先级汇总](#6-修改建议优先级汇总)

---

## 1. 严重问题（Bug / 正确性）

### 1.1 [P0-BUG] `execute_sql_with_params_by_ids` 事务路径忽略参数

**[未修复]**

**文件**: `src/transaction/api.rs:246-251`

**问题描述**: 当 `txn_id` 为 `Some` 时，虽然在函数开头解析了 `params`（行 241-244），但事务路径直接调用 `txn.execute(&sql)` 而非带参数版本，导致参数被完全忽略。

```rust
// 行 246-251: params 已解析但未使用
Some(txn_id) => {
    with_transaction_by_id(txn_id, |txn| Box::pin(async move {
        let result = txn.execute(&sql).await?;  // ← 未使用 params
        Ok(result)
    })).await
},
```

**影响**: 使用事务执行参数化 SQL 时，参数不会被绑定，导致 SQL 执行失败或数据错误。

**修改建议**:
1. 为 `DbTransaction` 添加 `execute_with_params(&mut self, sql: &str, params: &[ParamValue])` 方法
2. 在事务路径中调用该方法传入 `params`
3. 参数绑定逻辑可以从非事务路径中提取为公共辅助函数，避免重复

---

### 1.2 [P0-BUG] `query_sql_with_params_by_ids` 事务路径忽略参数

**[未修复]**

**文件**: `src/transaction/api.rs:384-389`

**问题描述**: 与 1.1 完全相同的问题。事务路径调用 `txn.query(&sql, &dataset_id)` 而非带参数版本。

```rust
// 行 384-389: params 已解析但未使用
Some(txn_id) => {
    with_transaction_by_id(txn_id, |txn| Box::pin(async move {
        let result = txn.query(&sql, &dataset_id).await?;  // ← 未使用 params
        Ok(result)
    })).await
},
```

**修改建议**:
1. 为 `DbTransaction` 添加 `query_with_params(&mut self, sql: &str, params: &[ParamValue], dataset_id: &str)` 方法
2. 在事务路径中调用该方法

---

### 1.3 [P0-BUG] `with_transaction_by_id` 持有 MutexGuard 跨 await

**[未修复]**

**文件**: `src/transaction/api.rs:165-173`

**问题描述**: 当前实现在 `txn_holder_mutex.lock()` 后跨 `f(&mut txh.txn).await` 调用持有锁，导致：
- future 非 `Send`，无法在 tokio 多线程 runtime 中使用
- 闭包执行期间阻塞其他对同一事务的访问（即使是元数据查询）

```rust
// 行 167-173: MutexGuard 跨 await 持有
let mut txh_g = txn_holder_mutex.lock().unwrap();  // 获取锁
if let Some(txh) = txh_g.as_mut() {
    let result = f(&mut txh.txn).await;  // ← 跨 await 持有锁
    result
}
```

**设计文档要求**: §5.5.3 明确推荐"取出-使用-放回"模式。

**修改建议**: 实现设计文档中的"取出-使用-放回"模式：
```rust
pub async fn with_transaction_by_id<T, F, Fut>(txn_id: &str, f: F) -> Result<T>
where
    F: FnOnce(&mut DbTransaction) -> Fut + Send,
    Fut: Future<Output = Result<T>> + Send,
{
    let holder = get_txn_holder_by_id(txn_id)
        .ok_or(Error::NoTxn)?;

    // 短暂持锁取出
    let mut txn = {
        let mut guard = holder.lock().unwrap();
        guard.take().ok_or(Error::NoTxn)?
    }; // guard 释放

    // 无锁执行
    let result = f(&mut txn.txn).await;

    // 短暂持锁放回
    {
        let mut guard = holder.lock().unwrap();
        *guard = Some(txn);
    }

    result
}
```

同时移除 `futures` crate 的 `BoxFuture` 依赖（行 8），改用泛型 `Fut` 参数。

---

### 1.4 [P0-BUG] `resume_suspended_txn` 死锁

**[已修复]** - 修复方式：在 core.rs:190-202 中先释放 `suspended_txns` 锁，然后再尝试获取 `txn_holder` 锁，最后在需要放回时重新获取 `suspended_txns` 锁，避免了死锁。

**文件**: `src/transaction/core.rs:190-202`

**问题描述**: 方法先获取 `suspended_txns` 的锁（行 191），在 `else` 分支中再次尝试获取同一锁（行 197），由于 `std::sync::Mutex` 不可重入，会导致**死锁**。

```rust
pub fn resume_suspended_txn(&self) {
    if let Some(suspended) = self.suspended_txns.lock().unwrap().pop() {  // 第一次锁
        let mut txh_g = self.txn_holder.lock().unwrap();
        if txh_g.is_none() {
            *txh_g = Some(suspended);
        } else {
            // 当前有活跃事务，将挂起的事务放回栈中
            self.suspended_txns.lock().unwrap().push(suspended);  // ← 死锁！
        }
    }
}
```

**修改建议**: 先释放第一个锁，再决定是否放回：
```rust
pub fn resume_suspended_txn(&self) {
    let suspended = self.suspended_txns.lock().unwrap().pop();
    if let Some(suspended) = suspended {
        let mut txh_g = self.txn_holder.lock().unwrap();
        if txh_g.is_none() {
            *txh_g = Some(suspended);
        } else {
            drop(txh_g); // 先释放 txn_holder 锁
            self.suspended_txns.lock().unwrap().push(suspended);
        }
    }
}
```

---

### 1.5 [P1-BUG] `remove_db_pool` 在异步上下文中调用 `block_on` 会 panic

**[已修复]** - 修复方式：在 connection/mod.rs:218-220 中将 `remove_db_pool` 改为 async 函数，直接调用异步的 `unregister`，移除了 `block_on` 调用。

**文件**: `src/connection/mod.rs:218-220`

**问题描述**: `remove_db_pool` 是同步函数，内部使用 `tokio::runtime::Handle::current().block_on()` 调用异步的 `unregister`。如果在 tokio runtime 内部调用此函数，会触发 panic: "Cannot start a runtime from within a runtime"。

```rust
pub fn remove_db_pool(key: &str) {
    let registry = get_global_registry();
    let rt = tokio::runtime::Handle::current();
    rt.block_on(async {  // ← 在 tokio runtime 内调用会 panic
        registry.unregister(key).await;
    });
}
```

**修改建议**: 改为 `async fn`：
```rust
pub async fn remove_db_pool(key: &str) {
    get_global_registry().unregister(key).await;
}
```
并更新所有调用点。

---

### 1.6 [P1-BUG] 类型转换优先级错误导致数值类型被误识别为字符串

**[未修复]**

**文件**: `src/transaction/conversion.rs:207-231`, `src/executor/mod.rs:160-181`

**问题描述**: `get_postgres_value_from_row` 等方法先尝试 `try_get::<String>`，对于 PostgreSQL 的部分列类型（如 `TEXT` 表示的数字），可能成功返回字符串形式的数值，而非正确的 `DataValue::Int` 或 `DataValue::Float`。

```rust
// 行 209-211: String 优先于 i64
if let Ok(value) = row.try_get::<String, _>(index) {
    DataValue::String(value)  // ← 数字列可能在此被捕获为 String
} else if let Ok(value) = row.try_get::<i64, _>(index) {
```

**修改建议**: 应先根据列的 `type_info()` 判断类型，再用对应的 Rust 类型提取：
```rust
fn get_postgres_value_from_row(row: &PgRow, index: usize) -> DataValue {
    let type_name = row.column(index).type_info().to_string().to_lowercase();
    match type_name.as_str() {
        t if t.contains("int") => row.try_get::<i64, _>(index)
            .map(DataValue::Int).unwrap_or(DataValue::Null),
        t if t.contains("float") || t.contains("double") => row.try_get::<f64, _>(index)
            .map(DataValue::Float).unwrap_or(DataValue::Null),
        t if t.contains("bool") => row.try_get::<bool, _>(index)
            .map(DataValue::Bool).unwrap_or(DataValue::Null),
        t if t.contains("decimal") || t.contains("numeric") => row.try_get::<Decimal, _>(index)
            .map(DataValue::Decimal).unwrap_or(DataValue::Null),
        _ => row.try_get::<String, _>(index)
            .map(DataValue::String).unwrap_or(DataValue::Null),
    }
}
```

---

### 1.7 [P1-BUG] `QueryBuilder.to_sql()` 条件始终输出 `1=1`，参数为空

**[未修复]**

**文件**: `src/types/mod.rs:411-419, 450`

**问题描述**: WHERE 子句对每个条件都输出 `1=1` 而非实际条件表达式。`params` 向量从未被填充，始终返回空数组。

```rust
// 行 413-418: 所有条件都被替换为 1=1
for (i, _cond) in self.conditions.iter().enumerate() {
    if i > 0 { sql.push_str(" AND "); }
    sql.push_str("1=1");  // ← 条件丢失
}
// ...
// 行 450: params 永远为空
(sql, params)
```

**修改建议**: 实现 `ConditionExpr` 到 SQL 的转换逻辑，并正确填充参数：
```rust
fn condition_to_sql(cond: &ConditionExpr, params: &mut Vec<ParamValue>) -> String {
    match cond {
        ConditionExpr::Compare { left, op, right } => {
            let left_sql = value_expr_to_sql(left, params);
            let right_sql = value_expr_to_sql(right, params);
            let op_sql = compare_op_to_sql(op);
            format!("{} {} {}", left_sql, op_sql, right_sql)
        },
        ConditionExpr::Logical { left, op, right } => {
            let l = condition_to_sql(left, params);
            let r = condition_to_sql(right, params);
            let op_str = match op { LogicalOp::And => "AND", LogicalOp::Or => "OR" };
            format!("({} {} {})", l, op_str, r)
        },
        ConditionExpr::Nested(inner) => format!("({})", condition_to_sql(inner, params)),
    }
}
```

---

## 2. 设计文档与代码不一致项

### 2.1 [结构] 模块目录结构不一致

**设计文档 §4.1** 提出的目标结构：

```
pool/ → manager.rs, config.rs, health.rs, inner.rs
transaction/ → manager.rs, context.rs, propagation.rs, options.rs, handle.rs
executor/ → trait.rs, sqlx_impl.rs, converter.rs, params.rs
repository/ → mod.rs, generic.rs
wasm_api/ → mod.rs
monitoring/ → mod.rs
```

**实际代码结构**：

```
config/ → mod.rs
connection/ → mod.rs        (对应设计文档的 pool/)
transaction/ → mod.rs, core.rs, api.rs, context.rs, conversion.rs, metadata.rs, registry.rs
executor/ → mod.rs           (仅 ParamValue + ResultConverter)
manager/ → mod.rs
monitoring/ → mod.rs
types/ → mod.rs              (设计文档未提及)
```

**缺失模块**: `repository/`, `wasm_api/`（功能内联在 `transaction/api.rs`）

**修改建议**:
- 设计文档的 §4.1 目录树应更新为反映当前实际结构，作为"当前结构"
- 在设计文档中增加"目标结构"与"当前结构"的对照，明确每个 Phase 需要做的结构调整
- 不建议立即大规模重命名目录，应在各 Phase 实施时逐步调整

---

### 2.2 [核心] Dbx 类型状态模式未实现

**设计文档 §4.3.1, §6.2**: 提出 `Dbx<NoTransaction>` / `Dbx<InTransaction>` 类型状态模式。

**实际代码**: `Dbx` 使用运行时 `with_txn: bool` 标志控制事务能力。

```rust
// 实际 (core.rs:22-31)
pub struct Dbx {
    db_pool: DbPool,
    txn_holder: Arc<Mutex<Option<TxnHolder>>>,
    with_txn: bool,  // ← 运行时标志
    suspended_txns: Arc<Mutex<Vec<TxnHolder>>>,
}
```

**说明**: 这属于设计文档 Phase 4（P2）的规划内容，当前未实现是预期的。设计文档 §2.1 已将此列为"现有问题"。

**修改建议**: 无需立即修改代码。设计文档已正确标注为 Phase 4 任务。

---

### 2.3 [核心] 错误类型层次未实现

**设计文档 §7.1**: 提出 `DatabaseError` → `ConnectionError` / `TransactionError` / `QueryError` 层次化错误类型。

**实际代码** (`error.rs`): 使用扁平的 `Error` 枚举。

```rust
// 实际
pub enum Error {
    TxnCantCommitNoOpenTxn,
    CannotBeginTxnWithTxnFalse,
    NoTxn,
    DbNotFound(String),
    ConnectionTimeout,
    PoolExhausted,
    // ...
}
```

**说明**: 设计文档 Phase 2（P1）任务项"改进错误类型"。

**修改建议**: Phase 2 实施时：
1. 引入 `thiserror` 替代 `derive_more` + 手动 `Display` 实现
2. 将错误按类别归入子枚举
3. 添加 `ErrorContext`（db_id, txn_id, sql 等上下文信息）
4. 先实现一个向后兼容的包装层，旧 `Error` 枚举通过 `From` 转换到新类型

---

### 2.4 [核心] QueryExecutor trait 未实现

**设计文档 §5.3.2**: 定义了 `QueryExecutor` trait 和 `SqlxExecutor` 实现。

**实际代码**: 无此 trait。SQL 执行分散在 `DbTransaction::execute/query`、`api.rs` 的各函数和 `connection/mod.rs` 的直接 pool 查询中。

**说明**: 属于 Phase 4（P2）任务。

**修改建议**: Phase 4 实施时统一查询执行接口。

---

### 2.5 [核心] Repository 模块不存在

**设计文档 §6.3**: 定义了 `CrudRepository` trait 和 `GenericCrudRepository`。

**实际代码**: 无 `repository/` 目录。MEMORY.md 记录曾有按数据库类型分拆的实现（`PostgresCrudRepository` 等），但当前代码中已不存在。

**说明**: 属于 Phase 4（P2）任务。

**修改建议**: Phase 4 实施时按设计文档创建统一的 `GenericCrudRepository`。

---

### 2.6 [依赖] `futures` crate 仍在使用

**设计文档**: 明确移除了 `futures` 依赖。

**实际代码**:
- `Cargo.toml:33`: `futures = "0.3.30"`
- `src/transaction/api.rs:8`: `use futures::future::BoxFuture;`
- `api.rs:160`: `F: FnOnce(&mut DbTransaction) -> BoxFuture<'_, Result<T>> + Send`

**修改建议**:
1. 移除 `futures` 依赖
2. 将 `with_transaction_by_id` 改为泛型 `Fut` 参数（见 1.3 的修改建议）
3. 所有 `Box::pin(async move { ... })` 调用点改为直接传闭包

---

### 2.7 [依赖] `sea-query` / `sea-query-binder` 仍在依赖中

**设计文档 §10.1.4**: 明确列出 `sea-query` 应被替代。

**实际代码**: `Cargo.toml:20-22` 仍引入 `sea-query` 和 `sea-query-binder`。

**修改建议**: 检查是否有代码引用这两个 crate，若无则移除依赖。若仍有引用，在 Phase 4 时迁移到 `ParamValue` + 内联 SQL 方案。

---

### 2.8 [依赖] `thiserror` 未引入，使用 `derive_more` 替代

**设计文档 D.1**: 列出 `thiserror` 作为核心依赖。

**实际代码**: 使用 `derive_more::From` + 手动 `impl Display` + `impl std::error::Error`。

**修改建议**: Phase 2 实施错误重构时引入 `thiserror`，替换当前的手动实现。

---

### 2.9 [依赖] 实际依赖中存在设计文档未列出的 crate

| 实际依赖 | 用途 | 设计文档是否列出 |
|---------|------|:---:|
| `derive_more` | From derive 宏 | 否 |
| `serde_with` | 序列化辅助 | 否 |
| `log` (0.4.29) | 日志（与 tracing 重复） | 否 |
| `rand` (0.8.5) | 随机数 | 否 |
| `sea-query` | SQL 构建 | 否（设计文档标记为移除） |
| `sea-query-binder` | SQL 参数绑定 | 否（设计文档标记为移除） |

**修改建议**:
1. `log` 应迁移到统一使用 `tracing`（`tracing` 兼容 `log` API）
2. `rand` 检查是否仍被使用，如仅用于测试可移至 `dev-dependencies`
3. 设计文档 D 节应列出当前实际使用的依赖（而非仅列出目标依赖）

---

## 3. 代码质量问题

### 3.1 [重复代码] 参数绑定逻辑三次重复

**文件**: `src/transaction/api.rs:257-309`, `api.rs:394-447`

**问题描述**: 在 `execute_sql_with_params_by_ids` 和 `query_sql_with_params_by_ids` 的非事务路径中，PostgreSQL、MySQL、SQLite 的参数绑定代码完全相同，各重复 3 次（共 6 处）。

**修改建议**: 提取为辅助函数：
```rust
fn bind_params<'q, DB: sqlx::Database>(
    query: sqlx::query::Query<'q, DB, <DB as sqlx::Database>::Arguments<'q>>,
    params: &[ParamValue],
) -> sqlx::query::Query<'q, DB, <DB as sqlx::Database>::Arguments<'q>> {
    // 统一的参数绑定逻辑
}
```
或者使用宏消除重复。

---

### 3.2 [重复代码] 类型转换逻辑完全重复

**文件**: `src/transaction/conversion.rs` vs `src/executor/mod.rs`

**问题描述**: `TransactionConverter for DbTransaction` 和 `ResultConverter` 包含几乎完全相同的代码：
- `convert_postgres_rows_to_dataset` ≈ `convert_postgres_rows`
- `get_postgres_value_from_row` ≈ `get_postgres_value_from_row`
- `map_sql_type_to_field_type` ≈ `map_sql_type_to_field_type`

两处共约 560 行代码，其中约 500 行是重复的。

**修改建议**:
1. 保留 `ResultConverter` 中的静态方法作为唯一实现
2. 移除 `TransactionConverter` trait
3. 在 `DbTransaction::query` 中直接调用 `ResultConverter` 的方法

---

### 3.3 [重复代码] DbPool 三分支 match 多处重复

**文件**: 多处

以下位置出现了几乎相同的 `match dbx.db() { Postgres(..) => ..., MySql(..) => ..., Sqlite(..) => ... }` 模式：
- `api.rs:206-219` (execute_sql_by_ids 非事务路径)
- `api.rs:256-310` (execute_sql_with_params_by_ids 非事务路径)
- `api.rs:343-355` (query_sql_by_ids 非事务路径)
- `api.rs:392-447` (query_sql_with_params_by_ids 非事务路径)
- `monitoring/mod.rs:50-59` (health check)
- `connection/mod.rs:260-292` (create pool)

**修改建议**: 为 `DbPool` 添加统一方法：
```rust
impl DbPool {
    pub async fn execute(&self, sql: &str) -> sqlx::Result<u64> {
        match self {
            DbPool::Postgres(pool) => Ok(sqlx::query(sql).execute(pool).await?.rows_affected()),
            DbPool::MySql(pool) => Ok(sqlx::query(sql).execute(pool).await?.rows_affected()),
            DbPool::Sqlite(pool) => Ok(sqlx::query(sql).execute(pool).await?.rows_affected()),
        }
    }

    pub async fn fetch_all_postgres(&self, sql: &str) -> sqlx::Result<Vec<PgRow>> { ... }
    // 或使用泛型方案
}
```

---

### 3.4 [代码组织] `transaction/core.rs` 职责过重

**文件**: `src/transaction/core.rs` (675 行)

**问题描述**: 该文件包含：
- `Dbx` 结构体及其所有方法
- `DbTransaction` 枚举及其方法
- `TxnHolder` 结构体
- `IsolationLevel` 枚举
- `Propagation` 枚举
- `Deref`/`DerefMut` 实现

这违反了设计文档 §3.2.1 的单一职责原则。

**修改建议**: 按照设计文档 §4.1 的目标，拆分为：
- `transaction/handle.rs` → `TxnHolder`（事务句柄）
- `transaction/propagation.rs` → `Propagation`、`IsolationLevel`
- `transaction/core.rs` → `Dbx`、`DbTransaction`

---

### 3.5 [编码规范] `#[allow(non_snake_case)]` 不必要

**文件**: `src/config/mod.rs:27, 55`

**问题描述**: `PoolConfig` 和 `DbConfig` 上标注了 `#[allow(non_snake_case)]`，但所有字段名都已经是 snake_case。

**修改建议**: 移除 `#[allow(non_snake_case)]`。

---

### 3.6 [编码规范] `DbConfig` 缺少 `Debug` derive

**文件**: `src/config/mod.rs:56-57`

**问题描述**: `DbConfig` 只有 `Clone`，缺少 `Debug`，在日志输出和调试时不便。`PoolConfig` 已有 `Debug`。

**修改建议**: 添加 `#[derive(Clone, Debug)]`。注意 `db_url` 包含敏感信息（密码），可以考虑自定义 `Debug` 实现对 URL 脱敏。

---

### 3.7 [编码规范] 注释掉的代码未清理

**文件**: `src/connection/mod.rs:234-240`, `src/manager/mod.rs:232-236`

**问题描述**: 存在注释掉的旧代码块，应清理。

**修改建议**: 删除注释掉的代码。

---

## 4. 架构合理性问题

### 4.1 [架构] `DatabaseManager.begin_transaction` 创建的 Dbx 未被管理

**文件**: `src/manager/mod.rs:93-96`

**问题描述**: `begin_transaction` 方法内部创建了一个临时的 `dbx_with_txn`，调用 `begin_txn` 后仅返回 `txn_id`，而 `dbx_with_txn` 在函数结束时被丢弃。虽然事务句柄已注册到全局 `TxnHolder` 注册表中（因此后续通过 `txn_id` 仍可操作事务），但 `Dbx` 内部的 `txn_holder` 引用被遗弃。

```rust
pub async fn begin_transaction(&self, db_id: &str, options: TransactionOptions) -> Result<String> {
    let dbx = self.get_dbx(db_id)?;                      // 获取非事务 Dbx
    let dbx_with_txn = dbx.with_transaction()?;           // 创建事务 Dbx
    dbx_with_txn.begin_txn(db_id, options.propagation).await  // Dbx 被丢弃
}
```

**潜在问题**:
1. 通过 `DatabaseManager` 创建的事务，其 `Dbx.commit_txn()` / `rollback_txn()` 路径无法使用（Dbx 已被丢弃）
2. 只能通过全局 `commit_txn_by_id` / `rollback_txn_by_id` 操作
3. 挂起/恢复事务功能（RequiresNew/NotSupported）无法正确工作，因为 `suspended_txns` 栈随 Dbx 丢弃

**修改建议**: `DatabaseManager` 应持有创建的 Dbx 实例映射（按 txn_id 索引），或者设计文档 Phase 2 中将事务管理完全移到 `TransactionManager` 中管理。

---

### 4.2 [架构] 全局 static 注册表与 DatabaseManager 重复

**文件**: `src/connection/mod.rs:208`, `src/transaction/registry.rs:10`, `src/transaction/metadata.rs:24`, `src/manager/mod.rs:223`

**问题描述**: 存在 4 个全局 `OnceLock` static 实例：
1. `GLOBAL_REGISTRY` (connection/mod.rs) — 连接池注册表
2. `GLOBAL_TXN_HOLDER_REGISTRY` (registry.rs) — TxnHolder 注册表
3. `GLOBAL_TXN_REGISTRY` (metadata.rs) — 事务元数据注册表
4. `DEFAULT_MANAGER` (manager/mod.rs) — 默认 DatabaseManager

`DatabaseManager` 的 `PoolManager` 直接引用 `GLOBAL_REGISTRY`，形成了"实例包装全局状态"的半成品模式。

**设计文档要求**: §3.1 目标 1 "最小化全局状态"，§6.1 `txn_registry` 应为 `DatabaseManager` 实例字段。

**修改建议**: Phase 2 实施时，将 `GLOBAL_TXN_HOLDER_REGISTRY` 和 `GLOBAL_TXN_REGISTRY` 迁移为 `DatabaseManager` 的实例字段，使多实例成为可能。

---

### 4.3 [架构] `TransactionContextStack` 和 `SuspendedTransaction` 未被使用

**文件**: `src/transaction/context.rs`

**问题描述**:
- `TransactionContextStack` 和 `SuspendedTransaction` 被定义并导出，但在整个代码库中没有任何调用点
- `TransactionFrame` 使用 `Arc<TxnHolder>` 但 `TxnHolder` 包含 `DbTransaction`（非 `Sync`），这会导致编译错误
- `TransactionManager` trait 已定义但无实现

**修改建议**:
1. 暂时标记为 `#[allow(dead_code)]` 或移除，避免误导
2. Phase 3 实施事务栈时重新设计（将 `Arc<TxnHolder>` 改为 `TxnHolder` 或其他 `Sync` 友好的引用方式）

---

### 4.4 [架构] 健康检查只验证存在性，未实际执行查询

**文件**: `src/manager/mod.rs:186-193`

**问题描述**: `PoolManager::health_check` 仅检查注册表中是否存在该 `db_id`，未执行任何 SQL 查询验证数据库连通性。

```rust
pub async fn health_check(&self, db_id: &str) -> Result<bool> {
    if self.registry.get(db_id).is_some() {
        Ok(true)  // ← 仅检查存在性
    } else {
        Err(Error::NoDb)
    }
}
```

而 `monitoring/mod.rs:46-63` 的 `check_db_health` 会执行 `SELECT 1`。

**修改建议**: `health_check` 应执行实际的数据库查询：
```rust
pub async fn health_check(&self, db_id: &str) -> Result<bool> {
    let (dbx, config) = self.registry.get(db_id).ok_or(Error::NoDb)?;
    let timeout = tokio::time::Duration::from_secs(config.health_check_timeout);
    match tokio::time::timeout(timeout, async {
        match dbx.db() {
            DbPool::Postgres(pool) => { sqlx::query("SELECT 1").execute(pool).await?; },
            DbPool::MySql(pool) => { sqlx::query("SELECT 1").execute(pool).await?; },
            DbPool::Sqlite(pool) => { sqlx::query("SELECT 1").execute(pool).await?; },
        }
        Ok::<_, crate::Error>(true)
    }).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(Error::ConnectionTimeout),
    }
}
```

---

### 4.5 [架构] 监控任务无关闭机制

**文件**: `src/monitoring/mod.rs:16-32`

**问题描述**: `start_monitoring` spawns 两个无限循环的 tokio task，没有关闭信号机制。`DatabaseManager::shutdown` 只调用 `cleanup_completed_transactions`，不会停止监控任务。

**修改建议**: 使用 `tokio::sync::watch` 或 `tokio_util::sync::CancellationToken` 实现优雅关闭：
```rust
pub async fn start_monitoring(cancel: CancellationToken) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    perform_health_check().await;
                }
            }
        }
    });
}
```

---

### 4.6 [架构] `DatabaseManager::shutdown` 不完善

**文件**: `src/manager/mod.rs:117-122`

**问题描述**: shutdown 仅清理已完成事务的元数据，未执行：
1. 停止监控任务
2. 回滚活跃事务
3. 关闭连接池
4. 等待进行中的操作完成

```rust
pub async fn shutdown(&self) -> Result<()> {
    info!("DatabaseManager 开始关闭");
    crate::transaction::cleanup_completed_transactions();
    info!("DatabaseManager 已关闭");
    Ok(())
}
```

**修改建议**: 实现完整的关闭流程：
```rust
pub async fn shutdown(&self) -> Result<()> {
    info!("DatabaseManager 开始关闭");
    // 1. 停止接受新操作（标记为 closing）
    // 2. 等待活跃事务完成或超时后强制回滚
    // 3. 关闭监控任务
    // 4. 关闭所有连接池
    // 5. 清理注册表
    crate::transaction::cleanup_completed_transactions();
    info!("DatabaseManager 已关闭");
    Ok(())
}
```

---

## 5. 测试覆盖问题

### 5.1 集成测试仅覆盖 PostgreSQL

**文件**: `tests/integration_test.rs`

**问题描述**: 所有 7 个集成测试均使用 PostgreSQL。MySQL 和 SQLite 完全没有集成测试覆盖。

**修改建议**:
1. 添加 SQLite 内存数据库测试（无需外部依赖）
2. 条件编译 MySQL 测试（需要 MySQL 实例时跳过）

---

### 5.2 缺少边界场景和异常路径测试

| 未覆盖场景 | 说明 |
|-----------|------|
| 参数化 SQL 执行 | `execute_sql_with_params_by_ids` / `query_sql_with_params_by_ids` 无测试 |
| 事务传播行为 | RequiresNew、Supports、NotSupported、Mandatory、Never 均无测试 |
| 事务超时 | 超时自动回滚未测试 |
| 并发事务 | 多个事务并发操作未测试 |
| 连接池耗尽 | `PoolExhausted` 错误路径未测试 |
| 错误路径 | 不存在的 db_id、重复注册、无效 SQL 等 |
| 挂起/恢复事务 | `resume_suspended_txn` 未测试 |

---

### 5.3 测试使用硬编码的外部数据库地址

**文件**: `tests/integration_test.rs:4`

```rust
const TEST_DB_URL: &str = "postgresql://postgres:postgres@192.168.137.80:5432/postgres";
```

**修改建议**: 使用环境变量 `DATABASE_URL`，CI 环境可配置，本地使用默认值：
```rust
fn get_test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/postgres".to_string())
}
```

---

## 6. 修改建议优先级汇总

### P0 - 必须立即修复（影响正确性）

| # | 问题 | 文件 | 章节 |
|---|------|------|------|
| 1 | `execute_sql_with_params_by_ids` 事务路径忽略参数 | api.rs:246-251 | §1.1 |
| 2 | `query_sql_with_params_by_ids` 事务路径忽略参数 | api.rs:384-389 | §1.2 |
| 3 | `with_transaction_by_id` 持有 MutexGuard 跨 await | api.rs:165-173 | §1.3 |
| 4 | `resume_suspended_txn` 死锁 | core.rs:190-199 | §1.4 |
| 5 | `remove_db_pool` 在异步上下文 block_on 会 panic | connection/mod.rs:218-224 | §1.5 |

### P1 - 应尽快修复（影响可靠性/可维护性）

| # | 问题 | 文件 | 章节 |
|---|------|------|------|
| 6 | 类型转换优先级错误 | conversion.rs, executor/mod.rs | §1.6 |
| 7 | QueryBuilder.to_sql() 条件始终 1=1 | types/mod.rs:411-419 | §1.7 |
| 8 | 移除 `futures` 依赖 | Cargo.toml, api.rs | §2.6 |
| 9 | 检查并移除 `sea-query`/`sea-query-binder` | Cargo.toml | §2.7 |
| 10 | 消除参数绑定重复代码（6处） | api.rs | §3.1 |
| 11 | 消除类型转换重复代码 | conversion.rs, executor/mod.rs | §3.2 |
| 12 | `health_check` 应执行实际查询 | manager/mod.rs:186-193 | §4.4 |
| 13 | `DatabaseManager.begin_transaction` Dbx 生命周期 | manager/mod.rs:93-96 | §4.1 |

### P2 - 改进项（提升代码质量）

| # | 问题 | 文件 | 章节 |
|---|------|------|------|
| 14 | DbPool 添加统一执行方法，减少 match 分支 | connection/mod.rs | §3.3 |
| 15 | core.rs 拆分职责 | core.rs | §3.4 |
| 16 | 移除不必要的 `#[allow(non_snake_case)]` | config/mod.rs | §3.5 |
| 17 | DbConfig 添加 Debug | config/mod.rs | §3.6 |
| 18 | 清理注释掉的代码 | connection/mod.rs, manager/mod.rs | §3.7 |
| 19 | 处理未使用的 context.rs 类型 | context.rs | §4.3 |
| 20 | 监控任务添加关闭机制 | monitoring/mod.rs | §4.5 |
| 21 | DatabaseManager.shutdown 实现完整关闭 | manager/mod.rs | §4.6 |
| 22 | 添加 SQLite 集成测试 | tests/ | §5.1 |
| 23 | 测试地址使用环境变量 | integration_test.rs | §5.3 |

### 设计文档需更新项

| # | 更新内容 | 章节 |
|---|---------|------|
| D1 | §4.1 模块目录树更新为"当前结构"+"目标结构"对照 | §2.1 |
| D2 | §2.2 监控回滚 bug 标注为"已修复" | §2.2 注 |
| D3 | 附录 D 补充实际使用的依赖 | §2.9 |
| D4 | §11 Phase 1 任务状态更新 | - |

---

*文档结束*

---

# 问题状态报告

**生成日期**: 2026-03-10

## 一、修复状态汇总

| 类别 | 已修复 | 未修复 | 总计 |
|------|:------:|:------:|:----:|
| P0-BUG (严重问题) | 1 | 4 | 5 |
| P1-BUG (重要问题) | 1 | 1 | 2 |
| 设计文档不一致 | 0 | 9 | 9 |
| 代码质量问题 | 0 | 7 | 7 |
| 架构合理性问题 | 0 | 6 | 6 |
| 测试覆盖问题 | 0 | 3 | 3 |
| **总计** | **2** | **30** | **32** |

---

## 二、已修复问题清单

### 1. [P0-BUG] `resume_suspended_txn` 死锁 (问题 1.4)

- **问题描述**: 方法先获取 `suspended_txns` 的锁，在 `else` 分支中再次尝试获取同一锁，导致死锁
- **修复方式**: 在 [core.rs:190-202](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/core.rs#L190-L202) 中先释放 `suspended_txns` 锁，然后再尝试获取 `txn_holder` 锁，最后在需要放回时重新获取 `suspended_txns` 锁
- **状态**: ✅ 已修复

### 2. [P1-BUG] `remove_db_pool` 在异步上下文中调用 `block_on` 会 panic (问题 1.5)

- **问题描述**: 同步函数内部使用 `tokio::runtime::Handle::current().block_on()` 调用异步函数会 panic
- **修复方式**: 在 [connection/mod.rs:218-220](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/connection/mod.rs#L218-L220) 中将 `remove_db_pool` 改为 async 函数，直接调用异步的 `unregister`
- **状态**: ✅ 已修复

---

## 三、未修复问题清单 (按优先级排序)

### P0 - 必须立即修复 (影响正确性)

| # | 问题描述 | 严重程度 | 建议修复方案 | 代码位置 |
|---|----------|:--------:|--------------|----------|
| 1 | `execute_sql_with_params_by_ids` 事务路径忽略参数 | **严重** | 为 `DbTransaction` 添加 `execute_with_params` 方法，在事务路径中调用该方法传入 params | [api.rs:246-251](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/api.rs#L246-L251) |
| 2 | `query_sql_with_params_by_ids` 事务路径忽略参数 | **严重** | 为 `DbTransaction` 添加 `query_with_params` 方法，在事务路径中调用该方法 | [api.rs:384-389](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/api.rs#L384-L389) |
| 3 | `with_transaction_by_id` 持有 MutexGuard 跨 await | **严重** | 实现"取出-使用-放回"模式：先持锁取出事务，执行闭包后释放锁，最后再持锁放回 | [api.rs:165-179](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/api.rs#L165-L179) |

### P1 - 应尽快修复 (影响可靠性/可维护性)

| # | 问题描述 | 严重程度 | 建议修复方案 | 代码位置 |
|---|----------|:--------:|--------------|----------|
| 4 | 类型转换优先级错误 (String 优先于 i64) | **高** | 先根据列的 `type_info()` 判断类型，再用对应的 Rust 类型提取 | [conversion.rs:207-231](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/conversion.rs#L207-L231), [executor/mod.rs:160-181](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/executor/mod.rs#L160-L181) |
| 5 | QueryBuilder.to_sql() 条件始终 1=1 | **高** | 实现 `ConditionExpr` 到 SQL 的转换逻辑，并正确填充 params 向量 | [types/mod.rs:411-419](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/types/mod.rs#L411-L419) |
| 6 | `futures` crate 仍在使用 | **中** | 移除 `futures` 依赖，将 `BoxFuture` 改为泛型 `Fut` 参数 | [Cargo.toml:33](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/Cargo.toml#L33), [api.rs:8](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/api.rs#L8) |
| 7 | `sea-query`/`sea-query-binder` 仍在依赖中 | **中** | 检查是否有代码引用，若无则移除依赖 | [Cargo.toml:20-22](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/Cargo.toml#L20-L22) |

### P2 - 改进项 (提升代码质量)

| # | 问题描述 | 严重程度 | 建议修复方案 | 代码位置 |
|---|----------|:--------:|--------------|----------|
| 8 | 参数绑定逻辑三次重复 | **中** | 提取为辅助函数或使用宏消除重复 | [api.rs:256-309, 392-447](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/api.rs#L256-L309) |
| 9 | 类型转换逻辑完全重复 | **中** | 保留 `ResultConverter` 为唯一实现，移除 `TransactionConverter` trait | [conversion.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/conversion.rs), [executor/mod.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/executor/mod.rs) |
| 10 | DbPool 三分支 match 多处重复 | **中** | 为 `DbPool` 添加统一执行方法 | 多处 |
| 11 | `core.rs` 职责过重 (675行) | **中** | 按设计文档拆分：handle.rs, propagation.rs | [core.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/core.rs) |
| 12 | 移除不必要的 `#[allow(non_snake_case)]` | **低** | 删除 config/mod.rs:27, 55 的 allow 属性 | [config/mod.rs:27,55](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/config/mod.rs#L27) |
| 13 | DbConfig 缺少 Debug derive | **低** | 添加 `Debug` derive，注意对 URL 脱敏 | [config/mod.rs:56-57](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/config/mod.rs#L56-L57) |
| 14 | 注释掉的代码未清理 | **低** | 删除 connection/mod.rs:234-240, manager/mod.rs:232-236 的注释代码 | - |
| 15 | TransactionContextStack 等未使用 | **低** | 暂时标记 `#[allow(dead_code)]` 或移除 | [context.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/transaction/context.rs) |
| 16 | health_check 只验证存在性 | **中** | 执行实际的 `SELECT 1` 查询验证连通性 | [manager/mod.rs:186-193](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/manager/mod.rs#L186-L193) |
| 17 | 监控任务无关闭机制 | **中** | 使用 `CancellationToken` 实现优雅关闭 | [monitoring/mod.rs:16-32](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/monitoring/mod.rs#L16-L32) |
| 18 | DatabaseManager.shutdown 不完善 | **中** | 实现完整关闭流程：停止监控、回滚活跃事务、关闭连接池 | [manager/mod.rs:117-122](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src/manager/mod.rs#L117-L122) |
| 19 | 集成测试仅覆盖 PostgreSQL | **低** | 添加 SQLite 内存数据库测试 | [tests/integration_test.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/tests/integration_test.rs) |
| 20 | 测试地址硬编码 | **低** | 使用环境变量 `DATABASE_URL` | [integration_test.rs:4](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/tests/integration_test.rs#L4) |

---

## 四、技术评估与解决建议

### 关键风险

1. **P0 问题 1.1-1.3** 是严重的正确性问题，会导致：
   - 事务中参数化 SQL 执行失败
   - 多线程场景下可能的死锁
   - future 非 Send 无法在 tokio 多线程 runtime 使用

2. **P1 问题 1.6** 会导致数值类型被误识别为字符串，可能造成业务逻辑错误

### 建议修复顺序

1. **第一阶段 (立即)**：修复 P0 问题 1.1、1.2、1.3
2. **第二阶段 (本周)**：修复 P1 问题 1.6、1.7、问题 6、7
3. **第三阶段 (本月)**：处理 P2 问题 8-18，优化代码质量
4. **第四阶段 (下月)**：完善测试覆盖 (问题 19、20)

### 架构建议

1. **问题 1.1 和 1.2** 可以一起修复：先为 `DbTransaction` 添加带参数的方法，然后在 API 函数中调用
2. **问题 3.1 和 3.2** 可以一起处理：提取公共辅助函数，统一类型转换实现
3. **问题 2.6** (移除 futures 依赖) 需要配合问题 1.3 一起修复

---

*报告生成完毕*
