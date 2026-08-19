# cmx-database

> 数据库操作模块，支持 WebAssembly 调用 host 实现数据库操作。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

## 项目简介

cmx-database 是 cmx-container 项目的数据库操作层（sqlx 链路），提供多数据源连接池管理、
事务注册表（守卫式 / 闭包式 / 编程式三种事务形态）、通用 CRUD（DbBmc + GenericCrudService）、
SQL 迁移引擎、长事务监控以及零拷贝列式结果集（zmc）等能力。

> 另有 `cmx-database-pg`（tokio-postgres + deadpool 并行实现）与本 crate 的 sqlx 链路并存，
> 两者共享 `cmx-rowsource` 的中立行来源抽象；本 crate 对外统一重导出
> `ZmcRowSource` / `ZmcColType`，保证上层以同一抽象消费两种驱动链路的结果。

## 快速开始

### 安装

内部 crate，通过 workspace 依赖引入：

```toml
[dependencies]
cmx-database = { workspace = true }
```

### 核心示例

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig, config::DbConfig};

// 1. 创建管理器（同步构造，无 IO）
let manager = DatabaseManager::new(DatabaseManagerConfig::default());

// 2. 注册数据源（default = true 的数据源成为默认库）
manager.register_data_source(DbConfig {
    db_type: cmx_database::DbType::Postgres,
    db_url: "postgresql://user:pass@localhost:5432/cmx".to_string(),
    db_id: "default".to_string(),
    default: true,
    ..Default::default()
}).await?;

// 3. 查询（返回 cmx_core 的 DataSet）
let ds = manager
    .query_sql("default", None, "SELECT id, name FROM users", "users_ds")
    .await?;
```

应用内通常直接使用全局单例：`cmx_database::get_default_db_manager()` 返回
`&'static Arc<DatabaseManager>`（懒初始化，配置默认值）。

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 多数据源管理 | `register_data_source` 动态注册多个库（`db_id` 寻址），支持 default / biz（业务库）/ other 三类 `source_type` |
| 连接池 | 基于 sqlx 的异步连接池（Postgres / MySql / Sqlite），`PoolConfig` 可调 max/min/超时/生命周期 |
| 事务处理 | 全局事务注册表 + 引用计数；`TransactionGuard` RAII 守卫、`with_transaction_by_id` 闭包、`Dbx` 编程式三种形态 |
| CRUD | `DbBmc` trait 元信息 + `GenericCrudService` 通用增删改查（modql 过滤器、加密字段、archived 软删过滤） |
| SQL 执行 | `execute_sql` / `query_sql` 系列自由函数与 manager 方法，支持 Json / DataValues / SqlxValues / Typed 四类参数 |
| 轻量 SQL 构建 | `types` 模块 `QueryBuilder` / `ConditionExpr` / `TypedRow` 条件组合与类型化取值 |
| 零拷贝结果集 | `zmc` 模块（`ZmcDataSet` / `SqlxPgRowSource`），查询直出列式编码，`query_sql_zmc` 入口 |
| SQL 迁移 | `MigrationRunner`：版本化迁移文件、checksum 校验、分布式锁防并发、支持回滚 |
| 监控 | `get_active_transactions` / `check_long_running_transactions` 长事务检测，`start_monitoring` 后台巡检（30s 健康检查 + 60s 事务超时） |
| Wasm host | `DatabaseHostFunctions` 供插件（WebAssembly）经 host 调用数据库能力 |

## 模块结构

```
cmx-database
├── src/
│   ├── lib.rs              # 库入口与公共 API re-export
│   ├── config/             # DbConfig / PoolConfig / DbType（TOML [database] 配置加载）
│   ├── connection/         # DbPool 连接池封装（Postgres/MySql/Sqlite 枚举）
│   ├── crud/               # DbBmc trait + GenericCrudService + CustomQueryService + count 优化
│   ├── error.rs            # Error 错误类型（thiserror + serde）
│   ├── executor/           # ParamValue / ResultConverter（参数绑定与结果转换）
│   ├── host_functions.rs   # DatabaseHostFunctions（Wasm host 函数）
│   ├── manager/            # DatabaseManager（多数据源统一入口 + 全局单例）
│   ├── migration/          # MigrationRunner / MigrationLoader / MigrationRecord
│   ├── monitoring/         # start_monitoring 后台监控
│   ├── transaction/        # 事务注册表：api/core/metadata/registry/txcontext + TransactionManager trait
│   ├── types/              # QueryBuilder / TypedRow / TypedResult / CompareOp / OrderDirection
│   └── zmc.rs              # 零拷贝列式结果集（SqlxPgRowSource / ZmcDataSet / ZmcSchema）
└── Cargo.toml
```

## 使用指南

### 一、管理器初始化与多数据源

#### 1.1 基础配置

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // DatabaseManagerConfig 仅含默认池参数与健康检查间隔/超时
    let manager = DatabaseManager::new(DatabaseManagerConfig::default());

    // 生命周期收尾：清理已完成事务注册表项
    manager.shutdown().await?;
    Ok(())
}
```

#### 1.2 注册数据源与连接池参数

连接池参数在 `DbConfig.pool_config`（`PoolConfig`）中按数据源配置：

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `max_connections` | 10 | 最大连接数（test 构建下为 1） |
| `min_connections` | 2 | 最小空闲连接数 |
| `connect_timeout` | 30 | 连接超时（秒） |
| `acquire_timeout` | 30 | 取连接超时（秒），池耗尽时超出即报错而非无限等待 |
| `idle_timeout` | 600 | 空闲超时（秒） |
| `max_lifetime` | 1800 | 最大生命周期（秒） |

```rust
use cmx_database::config::{DbConfig, PoolConfig, DbType};

let db = DbConfig {
    db_type: DbType::Postgres,
    db_url: "postgresql://user:pass@localhost:5432/cmx".to_string(),
    db_id: "main".to_string(),
    db_name: None,                 // 显示名，可选（未配置时按规则推导）
    db_schema: Some("public".to_string()),
    default: true,
    pool_config: PoolConfig { max_connections: 20, ..Default::default() },
    domain_code: None,             // 归属域/应用/模块编码（可选）
    application_code: None,
    module_code: None,
    source_type: Some("biz".to_string()), // default-默认库 / biz-业务库 / other-其他
    health_check_interval: 60,
    health_check_timeout: 5,
};

manager.register_data_source(db).await?;
manager.unregister_data_source("old").await?;   // 默认数据源不可删除
```

也可从应用配置直接加载：`DbConfig::from_config(&cmx_utils::Config)` /
`DbConfig::list_from_config(...)`（读取 `[[databases]]` 数组节）；
`manager.list_data_sources()` 列出已注册 db_id，`manager.health_check(db_id)`
做一次连通性检查。

#### 1.2.1 配置文件方式（[[databases]]）

应用 TOML 配置（`dev.toml` 等）中以 `[[databases]]` 数组声明多个数据源，
启动时遍历注册：

```toml
[[databases]]
# 数据源唯一标识符（必需）
db_id = "primary"
db_name = "主控数据库"
# 数据库类型：postgres | mysql | sqlite（必需）
db_type = "postgres"
# 数据库连接 URL（必需）
db_url = "postgres://user:pass@localhost:5432/cmx"
# 是否为默认数据库（可选，默认 false）
default = true
# 数据源类型（可选，default=true 时为 default，否则为 other）
# 取值：default-默认库，biz-业务库，other-其他
# source_type = "default"

[[databases]]
db_id = "fico-db"
db_name = "总账数据库"
db_type = "postgres"
db_url = "postgres://user:pass@localhost:5432/cmx_fico"
default = false
source_type = "biz"        # 业务库：get_biz_db_id() 命中此库

# 可选连接池微调（节选，未列字段见 PoolConfig 表）
# [databases.pool_config]
# max_connections = 20
```

#### 1.3 业务库寻址

```rust
// 第一个 source_type = "biz" 的 db_id；没有则回退默认库
let biz_db = manager.get_biz_db_id().await;
// 不回退版本：未配置业务库时返回 None
// （迁移引擎据此决定是否对业务库执行迁移，避免把业务库建到主库）
let biz_db_opt = manager.get_biz_db_id_opt().await;
```

### 二、执行与查询 SQL

#### 2.1 无参数 SQL（manager 方法或自由函数）

```rust
// 返回 DataSet（cmx_core::model::data::dataset::DataSet）
let ds = manager
    .query_sql("main", None, "SELECT id, name FROM users", "users_ds")
    .await?;

// 影响行数；txn_id 传 Some 时在对应事务内执行
let affected: u64 = manager
    .execute_sql("main", None, "UPDATE users SET status = 'active'")
    .await?;

// 等价的模块级自由函数（lib.rs 重导出）
let ds = cmx_database::query_sql("main", None, "SELECT 1", "probe").await?;
```

#### 2.2 参数化查询（SqlParams 四形态）

```rust
use cmx_database::{query_sql_with_params, execute_sql_with_params, transaction::SqlParams};

// 1) JSON 数组参数（Wasm/HTTP 场景，内部自动转换为 DataValue）
let params = SqlParams::Json(serde_json::json!([1_i64, "active"]));
let ds = query_sql_with_params(
    "main", None,
    "SELECT id, name FROM users WHERE id = $1 AND status = $2",
    params, "users_ds",
).await?;

// 2) SqlParams::DataValues(Vec<DataValue>)：cmx-core 单元格类型，直接绑定
// 3) SqlParams::SqlxValues(SqlxValues)：sea-query 构建的 SQL 配套
// 4) SqlParams::Typed(Vec<SqlParam>)：强类型参数，带 NULL 类型支持
```

> `query_sql_with_json` / `execute_sql_with_json` 自 0.1.10 起废弃，
> 请改用 `*_with_params(SqlParams::Json(..))` 或 typed / datavalues 变体。

#### 2.3 类型化查询与轻量 SQL 构建（types 模块）

- `TypedRow`：按列下标取值（`get_string` / `get_i64` / `get_f64` / `get_bool`），
  `manager.query_sql_typed(...)` 返回 `TypedResult`；
- `QueryBuilder` + `ConditionExpr` / `CompareOp` / `LogicalOp` / `JoinClause` /
  `OrderDirection`：在拼 SQL 字符串之外提供轻量的条件组合与占位符编号管理
  （`to_sql_string(start_index)` / `param_count()`）。

### 三、事务处理

事务由全局注册表管理（`txn_id` 寻址，`TransactionMetadata` 含状态与创建时间），
基于引用计数：`commit_txn_by_id` / `rollback_txn_by_id` 在引用归零时才真正提交/回滚。

#### 3.1 TransactionGuard（RAII 守卫，推荐）

```rust
use cmx_database::TransactionOptions;
// 注意：guard 入口经 transaction 模块访问（lib.rs 顶层未重导出）
use cmx_database::transaction::begin_transaction_guard_by_db_id;

let guard = begin_transaction_guard_by_db_id(
    "main",
    TransactionOptions { propagation: cmx_database::Propagation::Required },
).await?;

cmx_database::execute_sql("main", Some(guard.txn_id()),
    "INSERT INTO logs(msg) VALUES ('a')").await?;
cmx_database::execute_sql("main", Some(guard.txn_id()),
    "UPDATE counters SET n = n + 1").await?;

// 显式提交；未调用 commit 而析构时自动发送回滚命令
guard.commit().await?;
// guard.rollback().await?;  // 或显式回滚
```

#### 3.2 闭包式

```rust
use cmx_database::with_transaction_by_id;

// 以事务 ID 为上下文执行闭包（闭包内拿到 &mut DbTransaction）
let count = with_transaction_by_id(&txn_id, |txn| {
    Box::pin(async move {
        let ds = txn.query("SELECT count(*) FROM users", "cnt").await?;
        Ok(ds)
    })
}).await?;

// TransactionGuard 也提供等价的 guard.with_transaction(...) 方法
```

#### 3.3 编程式 Dbx

```rust
use cmx_database::Propagation;

let dbx = manager.get_dbx("main").await?;
let txn_id = dbx.begin_txn("main", Propagation::Required).await?;
// ... execute_sql / query_sql 传 Some(&txn_id) ...
cmx_database::commit_txn_by_id(&txn_id).await?;      // 或 rollback_txn_by_id
```

`Dbx::with_transaction()` 可将普通连接切换为事务模式；`Dbx::db()` 返回底层
`&DbPool` 供直接使用 sqlx 能力；`dbx.is_txn_timeout(duration)` 判定事务超时。

### 四、通用 CRUD（DbBmc + GenericCrudService）

为实体的 Bmc（模型控制器）实现 `DbBmc` trait 声明表元信息：

```rust
use cmx_database::crud::DbBmc;

pub struct UserBmc;

impl DbBmc for UserBmc {
    const TABLE: &'static str = "cmx_user";
    const PK_COLUMN: &'static str = "id";     // 默认 "code"
    // has_timestamps() 默认 true：自动维护时间戳列
    // has_owner_id()  默认 false：创建时自动填充 owner_id
    // encrypted_fields() 声明需加密存储的字段，如 &["db_url", "password"]
    // archived 过滤开启后 get/list 自动追加 archived = 0 软删过滤
}
```

随后零实现地获得通用 CRUD（`GenericCrudService<MC, F>`，`F` 为 modql 过滤器类型）：

```rust
use cmx_database::crud::GenericCrudService;
use serde_json::json;

// create / update 的 data 实现 serde::Serialize；均返回 DataSet
GenericCrudService::<UserBmc>::create(&manager, "main", None, &new_user).await?;
GenericCrudService::<UserBmc>::create_many(&manager, "main", None, &users).await?;
GenericCrudService::<UserBmc>::update(&manager, "main", Some(&txn_id), &user).await?;

// get 按（复合）主键取单条：id 为 serde_json::Value
let ds = GenericCrudService::<UserBmc>::get(&manager, "main", None, json!("42")).await?;

// delete 按 id 列表
GenericCrudService::<UserBmc>::delete(&manager, "main", None, vec![json!("42")]).await?;

// list / page / count 支持泛型过滤器 F: Into<FilterGroups>（modql FilterNodes）
let list = GenericCrudService::<UserBmc, F>::list(
    &manager, "main", None, Some(filters), None,   // ListOptions 可选
).await?;
let page = GenericCrudService::<UserBmc, F>::page(
    &manager, "main", None, Some(filters), page_no, page_size,
).await?;
let total = GenericCrudService::<UserBmc, F>::count(&manager, "main", None, Some(filters)).await?;
```

查询构建依赖 sea-query + modql（`with-sea-query` feature）：

- `count_optimizer`（`CountOptimizerConfig` / `generate_count_sql`）负责大表 COUNT 优化；
- `CustomQueryService::page_custom` 支持自定义 SQL 分页；
- `crud::utils` 提供实体 ↔ sea-query 值转换等工具。

### 五、SQL 迁移（migration）

```rust
use std::{path::PathBuf, sync::Arc};
use cmx_database::migration::MigrationRunner;

let runner = MigrationRunner::new(
    manager.clone(),                    // Arc<DatabaseManager>
    "default".to_string(),              // 迁移记录所在库
    PathBuf::from("./migrations"),
)
.with_lock_manager(Arc::new(lock_manager))   // 可选：cmx-buffer 分布式锁防多实例并发迁移
.with_lock_key("cmx_migration_lock")         // 锁 key / 等待超时 / 轮询间隔均可调
.with_validate_checksum(true)                // checksum 变更检测
.with_enabled(true);

let summary = runner.run_pending_migrations().await?;   // MigrationSummary
runner.rollback_migration("20260801_120000").await?;    // 按版本回滚
```

`MigrationLoader::load_migrations()` 扫描迁移目录并按文件名解析版本与描述、计算
checksum；执行记录持久化为 `MigrationRecord`（状态机 `MigrationStatus`，支持
Baseline / Pending / Applied / Failed 等状态与 `PendingMigration` / `MigrationSummary`
结果结构）。配合 `get_biz_db_id_opt()` 可将业务库迁移与主库迁移分开执行。

### 六、长事务监控

```rust
use cmx_database::{get_active_transactions, check_long_running_transactions};
use std::time::Duration;

// 全部进行中的事务元信息（txn_id / db_id / create_time / status）
for meta in get_active_transactions().await {
    tracing::info!(txn_id = %meta.txn_id, ?meta.status, "active txn");
}

// 超过 60s 仍活跃的事务
let long = check_long_running_transactions(Duration::from_secs(60)).await;

// 收尾：清理已完成事务的注册表项
cmx_database::cleanup_completed_transactions().await;

// 后台巡检：每 30s 数据源健康检查 + 每 60s 事务超时检查
cmx_database::start_monitoring().await;
```

### 七、零拷贝结果集（zmc）

`zmc` 模块在 sqlx Postgres 链路上实现驱动无关的零拷贝行来源：
`SqlxPgRowSource`（包装 `sqlx::postgres::PgRow`，impl `ZmcRowSource`，按
`type_info().name()` 分派到中立列类型）→ `ZmcDataSet` / `ZmcSchema` / `ZmcChildGroup`。

```rust
// 入口：manager 方法（另有 query_sql_zmc_with_datavalues）
let zmc = manager.query_sql_zmc("main", None, sql, "big_ds").await?;
```

中立抽象 `ZmcRowSource` / `ZmcColType` 定义于 `cmx-rowsource` 并由本 crate 统一重导出，
供大结果集流式传输（如单据 `tokio-zmc-stream` 端点）使用；`cmx-database-pg` 的
tokio-postgres 链路实现同一抽象。

### 八、Wasm host 函数

`DatabaseHostFunctions::new(Arc<DatabaseManager>)` 实现 cmx 插件体系的 host 函数接口
（`namespace()` / `functions()` / `call(name, input) -> output`，字节进字节出），
注册 `db_query` / `db_execute` 两个函数（namespace `"cmx:database"`，MsgPack 编解码）；
WebAssembly 插件经 cmx-plugin-sdk 声明导入后在沙箱内调用，SQL 与参数以序列化字节流传递。

## 公共 API 速览（lib.rs 重导出）

| API | 来源 | 说明 |
|-----|------|------|
| `DatabaseManager` / `DatabaseManagerConfig` / `TransactionContext` / `TransactionOptions` / `get_default_db_manager` | manager | 管理器与全局单例 |
| `DbConfig` / `DbType` / `PoolConfig` | config | 数据源与连接池配置 |
| `DbPool` | connection | 连接池枚举（Postgres/MySql/Sqlite） |
| `DataSet` | cmx-core（重导出） | 查询结果的通用数据集 |
| `execute_sql` / `execute_sql_with_params` / `query_sql` / `query_sql_with_params` | transaction::api | SQL 执行/查询自由函数 |
| `commit_txn_by_id` / `rollback_txn_by_id` / `with_transaction_by_id` / `get_dbx_by_db_id` / `get_txn_holder_by_id` / `get_txn_metadata` | transaction::api | 事务生命周期（lib.rs 顶层重导出） |
| `begin_transaction_guard_by_db_id` / `TransactionGuard` / `register_txn` | transaction::api | 事务入口（经 `cmx_database::transaction::` 模块访问，顶层未重导出） |
| `get_active_transactions` / `cleanup_completed_transactions` / `check_long_running_transactions` | transaction::metadata | 事务注册表观测 |
| `Dbx` / `Propagation` / `SqlParams` / `TransactionMetadata` / `TransactionStatus` | transaction | 事务核心类型 |
| `CompareOp` / `OrderDirection` / `QueryBuilder` / `TypedResult` / `TypedRow` | types | 轻量 SQL 构建与类型化行 |
| `ParamValue` / `ResultConverter` | executor | 参数绑定与结果转换 |
| `start_monitoring` | monitoring | 后台健康检查/超时巡检 |
| `DatabaseHostFunctions` | host_functions | Wasm host 函数 |
| `MigrationError` / `MigrationLoader` / `MigrationRecord` / `MigrationResult` / `MigrationRunner` / `DbMigrationStatus` / `MigrationSummary` / `PendingMigration` | migration | 迁移引擎 |
| `SqlxPgRowSource` / `ZmcChildGroup` / `ZmcDataSet` / `ZmcSchema` | zmc | 零拷贝列式结果集 |
| `ZmcRowSource` / `ZmcColType` | cmx-rowsource（重导出） | 驱动中立行来源抽象 |
| `Error` / `Result` | error | 错误类型 |

### 九、错误处理

```rust
use cmx_database::Error;   // 注意：错误类型名为 Error（thiserror），非 DbError

match result {
    Ok(_) => {}
    Err(Error::DbNotFound(id)) => { /* 数据源未注册 */ }
    Err(Error::NoTxn) => { /* 事务 ID 不存在或已清理 */ }
    Err(Error::NoDb) => { /* 数据库不存在 */ }
    Err(Error::PoolExhausted) => { /* 连接池耗尽（acquire 超时） */ }
    Err(Error::ConnectionTimeout) => { /* 连接超时 */ }
    Err(Error::InvalidParams(msg)) => { /* 参数类型/数量与占位符不匹配 */ }
    Err(Error::DefaultDbSourceCantDelete(id)) => { /* 默认数据源禁止删除 */ }
    Err(Error::Sqlx(e)) => { /* 底层 sqlx 错误（透传，DisplayFromStr 序列化） */ }
    Err(e) => {
        // TxnCantCommitNoOpenTxn / CannotBeginTxnWithTxnFalse / CannotCommitTxnWithTxnFalse
        // NoTxn / TransactionRequired / TransactionNotAllowed / UnsupportedDbType
        // CantCreateModelManagerProvider 等
    }
}
```

## 在 cmx 体系中的位置

- **本 crate = sqlx 链路**：Postgres / MySql / Sqlite 三驱动，连接池由 sqlx 提供；
  通用 CRUD（DbBmc / GenericCrudService）、迁移引擎、Wasm host 函数都在本链路上。
- **cmx-database-pg = tokio-postgres + deadpool 并行链路**：仅 Postgres，性能优先，
  与本 crate 并存演进；两者共享 `cmx-rowsource` 的 `ZmcRowSource` / `ZmcColType`
  中立抽象，上层结果消费代码无需感知驱动差异。
- **数据源注册由应用侧完成**：应用（portal / 平台启动流程）从 `[[databases]]`
  配置读入并遍历 `register_data_source`，本 crate 不直接读配置文件启动。
- 跨 workspace 复用：cmx-portalservice 直接以 path 依赖引用本 crate（0.1.12）；
  cmx-flowengine 的数据库链路以 `cmx-database-pg` 为主（直接 path 依赖为
  cmx-database-pg / cmx-service-base 等）；cmx-container 内则经 cmx-service-base
  的 `db-sqlx` / `storage` 等 feature 拉入本 crate。

## 常见问题（FAQ）

### Q: query_sql 返回的 DataSet 是什么？

**A**: `DataSet` 定义在 `cmx-core`（`cmx_core::model::data::dataset::DataSet`），
由本 crate 重导出。它是 cmx 平台通用的查询结果集（含 schema 与行数据），
跨 Wasm / HTTP / RPC 边界序列化传输均以此为载体；`dataset_id` 参数用于
构建返回 DataSet 的 schema 标识。

### Q: 事务为什么用注册表 + txn_id，而不是直接持有 sqlx Transaction？

**A**: 事务句柄存在全局注册表（`txn_id` 寻址 + 引用计数），使跨函数/跨模块/
跨 Wasm 调用边界都能以字符串 ID 参与同一事务；`commit_txn_by_id` /
`rollback_txn_by_id` 在引用归零时才真正提交/回滚，配合 `TransactionGuard`
的 RAII Drop 自动回滚，兼顾灵活与安全。

### Q: 什么场景用 zmc（query_sql_zmc）而不是 query_sql？

**A**: 大结果集流式传输场景（如单据 tokio-zmc-stream 端点）。`query_sql`
以 DataSet 装载全部数据；`query_sql_zmc` 通过 `SqlxPgRowSource` 零拷贝包装
sqlx 行，直出列式编码的 `ZmcDataSet`，避免整表物化，目前仅支持 Postgres 链路。

### Q: 加密字段（encrypted_fields）如何生效？

**A**: `DbBmc::encrypted_fields()` 声明的字段在 `GenericCrudService` 写入时
经 `CryptoService::encrypt()` 加密、读出（get/list/page）时
`decrypt_dataset_fields()` 解密，用于数据源密码等敏感列；
未声明的表零开销。

## 附录：在连接串中指定 PostgreSQL Schema

sqlx 底层使用 libpq 协议，指定搜索路径的参数是 `options` 而非 `currentSchema`
（后者是 JDBC 惯用参数）：

```text
postgresql://user:pass@host:5432/cmx?options=-c%20search_path%3Dmyschema
```

- `options=`：libpq 传递 PostgreSQL 后端启动参数的标准选项；
- `-c`：表示设置一个配置参数，`search_path=myschema` 是真实配置项；
- URL 编码：空格 → `%20`，等号 → `%3D`，拼接为 `-c%20search_path%3Dmyschema`。

也可以在建立连接池后执行 `SET search_path TO myschema`，或使用 `DbConfig.db_schema`
字段（pg 库默认 `public`）由本 crate 统一处理。
