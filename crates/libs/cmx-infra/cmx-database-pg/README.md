# cmx-database-pg

> 基于 tokio-postgres + deadpool-postgres 的 PostgreSQL-only 数据库操作层，与 cmx-database（sqlx 抽象层）并行存在的独立门面实现，提供多数据源管理、声明式事务、零拷贝列式查询与通用 CRUD。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-database-pg` 是 cmx-container 基础设施层的 PG 直连数据库 crate，基于 **tokio-postgres 原生驱动 + deadpool-postgres 连接池** 实现（区别于 `cmx-database` 的 sqlx 驱动）。它复刻并补全了 `cmx-database` 的模块与 API 面：多数据源注册、连接池管理、事务编排、SQL 执行/查询、零拷贝 `ZmcDataSet` 出口、通用 CRUD（sea-query + modql）以及 WASM 宿主函数。

### 与 cmx-database（sqlx）的配对关系

本 crate 与 `cmx-database` 是「抽象参考 → PG-only 并行实现」的配对关系，模块名逐一对照（`config` / `connection` / `executor` / `manager` / `transaction` / `types` / `crud` / `monitoring` / `host_functions`），关键差异有三点：

| 差异点 | cmx-database（sqlx） | cmx-database-pg（本 crate） |
|--------|----------------------|-----------------------------|
| 全局门面 | `get_default_db_manager()` | `get_default_pg_db_manager()`（独立 `OnceLock` 单例与注册表，两者完全隔离） |
| SQL 参数桥接 | `SqlParams::SqlxValues(sqlx::PgArguments)` | `SqlParams::SeaValues(sea_query::Values)`，对应 `*_with_seavalues` 系列方法 |
| 事务底层 | `sqlx::Transaction<'static>` | deadpool 独占连接 + 文本命令 `BEGIN`/`COMMIT`/`ROLLBACK`（事务期内独占一条池化连接） |

`cmx-database` 中的 `migration`（迁移管理）模块在本 crate 源码中已有移植但**尚未启用**（`lib.rs` 中 `pub mod migration;` 处于注释状态）。

### 设计要点

- **PG-only**：`DbType` 枚举保留了 `MySql`/`Sqlite` 占位，但实际只有 `Postgres` 可用，注册其他类型直接报错；
- **schema 注入**：建池时通过 deadpool `post_create` 钩子对每条新连接执行 `SET search_path TO <schema>, public`；
- **建池即首连验证**：deadpool 建池是惰性的，`new_db_pool` 建池后立即拨号一条连接（顺带执行 post_create 的 SET search_path）并归还，网络不可达 / 库不存在 / 认证失败在注册期即报 `Error::PoolFirstConnect`——fail-fast，不拖到首次 `get`；
- **事务安全**：`TransactionGuard` RAII 守卫（Drop 经 mpsc 通道异步回滚）+ 全局事务注册表 + `start_monitoring()` 后台扫描（默认 300s 超时自动回滚）；
- **零拷贝出口**：查询可返回 `ZmcDataSet<TokioPgRowSource>`（持有原始 `tokio_postgres::Row`，惰性列式 msgpack 编码），超大结果可走「真·分帧流式」`query_sql_zmc_stream_chunks`，峰值内存 O(单行)；
- **类型自适应**：`PgInt` 按 INT2/INT4/INT8 宽度自适应编码，`PgDateTime` 按 TIMESTAMP/TIMESTAMPTZ 自适应，结果转换失败一律置 `Null` 不 panic。

---

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-core` | `DataValue` / `SqlParam` / `DataSet` / `Schema` 等核心数据模型（无 sqlx 依赖） |
| `cmx-rowsource` | 驱动无关行来源抽象 `ZmcRowSource` + 零拷贝列式编码器（本 crate 重导出 `ZmcColType` 等） |
| `cmx-buffer` / `cmx-utils` / `cmx-traits` | 缓存 / 工具 / HostFunctionProvider trait |
| `tokio-postgres` + `deadpool-postgres` + `postgres-types` | PG 原生驱动、连接池、类型系统（ToSql/FromSql + OID） |
| `sea-query` + `modql`（with-sea-query） | SQL 构建 / 过滤器（ListOptions、FilterGroups） |
| `rust_decimal`（db-tokio-postgres） | NUMERIC 列的 ToSql/FromSql |

### 下游使用者（Cargo.toml 反查）

| 仓库 | crate | 场景 |
|------|-------|------|
| cmx-container | `cmx-service-base` | `register_pg_datasources()` 统一注册 flow + portal 共享数据源 |
| cmx-container | `cmx-biz`、`cmx-api-core`、`cmx-common-api`、`cmx-platform-app` | 业务层 / 协议层 / 应用壳 |
| cmx-container | `cmx-dct-store-pg`、`cmx-doc-store-pg`、`cmx-job-store-pg`、`cmx-mdm-store-pg` | 各业务域 PG store 层 |
| cmx-container | `cmx-code-api`、`cmx-doc-api`、`cmx-mdm-api`、`cmx-web-monitor` | API / 监控 |
| cmx-container | `cmx-database-test` | 跨 crate 集成测试 |
| cmx-flowengine | `cmx-flow-app` / `cmx-flow-def` / `cmx-flow-demo` / `cmx-flow-identity` / `cmx-flow-server` / `cmx-flow-store-pg` / `cmx-flow-tests` | 流程微服务全量（跨 workspace path 依赖） |
| cmx-report | `cmx-rpt-server` / `cmx-rpt-store-pg` | 报表微服务 |
| cmx-rulesengine | `cmx-rule-app` / `cmx-rule-server` / `cmx-rule-store-pg` | 规则引擎微服务 |
| cmx-portalservice | （传递依赖） | 经 `cmx-portal-app` 牵入完整平台依赖树（根 Cargo.toml 注释说明） |

---

## 核心功能与特性

| 功能 | 说明 | 关键入口 |
|------|------|----------|
| 多数据源管理 | 动态注册/注销数据源，按 `db_id` 路由；`source_type == "biz"` 回退 default | `DatabaseManager::register_data_source` |
| 连接池 | deadpool-postgres，`post_create` 注入 search_path，建池即首连验证（库不可达注册期报 `PoolFirstConnect` fail-fast），优雅关闭（活跃计数等待） | `DbPool` / `new_db_pool` |
| SQL 执行 | 四种参数形态：无参 / `DataValue` / `SqlParam`（带类型 NULL）/ `sea_query::Values` | `execute_sql*` 系列 |
| SQL 查询 | 返回老 `DataSet`（JSON 链路）或零拷贝 `ZmcDataSet`（二进制链路） | `query_sql*` 系列 |
| 声明式事务 | `transaction!` 宏 + `Propagation`（目前仅实现 `Required`）+ `with_transaction_by_id`（take/放回不持锁执行闭包） | `cmx_database_pg::transaction!` |
| RAII 事务守卫 | `TransactionGuard` Drop 自动回滚（mpsc + OnceLock 清理通道），commit 失败自动回滚兜底 | `begin_transaction_guard_by_db_id` |
| 事务元数据 | 全局注册表、状态跟踪（Active/Committed/RolledBack）、超时扫描 | `get_active_transactions` / `check_long_running_transactions` |
| 零拷贝/流式 | `ZmcDataSet<TokioPgRowSource>` 一次性编码；`query_sql_zmc_stream_chunks` 长度分帧流式（峰值内存 O(单行)） | `query_sql_zmc*` 系列 |
| 通用 CRUD | `DbBmc` trait（表名/PK/加密字段/archived）+ `GenericCrudService<MC, F>`（sea_query + modql 过滤分页） | `crud` 模块 |
| 健康监控 | `SELECT 1` 探活；`start_monitoring()` 30s 健康检查 + 60s 事务超时扫描 | `DatabaseManager::health_check` / `start_monitoring` |
| WASM 宿主函数 | 命名空间 `cmx:database`（`db_query` / `db_execute`，MsgPack 编解码） | `DatabaseHostFunctions` |
| 错误诊断 | `pg_detail()` 从 cmx-biz 下沉至此，渲染 PG 错误链 | `cmx_database_pg::pg_detail` |

---

## 模块结构

```text
src/
├── lib.rs               # 导出面；模块名与 cmx-database 逐一对照
├── config/              # DbType / PoolConfig（max10·min2·connect30s·acquire30s·idle600s·lifetime1800s）/ DbConfig / PoolStatus
├── connection/          # DbPool（池化执行）、DatabasePoolImpl（活跃计数/优雅关闭）、DbRegistry 全局注册表、new_db_pool（search_path 注入 + 首连验证）
├── executor/            # PgInt（INT2/4/8 宽度自适应）、PgDateTime（TZ 自适应）、ParamValue::from_json、
│                        #   bind_data_values_pg、PgResultConverter::convert_rows（按 OID 分派，失败即 Null 不 panic）、sea_values_to_tosql
├── manager/             # DatabaseManager（数据源路由 + execute/query 委托）、PoolManager、TransactionContext、
│                        #   get_default_pg_db_manager（OnceLock 独立单例）
├── transaction/         # 事务核心：core（Dbx/DbTransaction/TxnHolder/Propagation）、api（TransactionGuard/SqlParams/
│                        #   with_transaction_by_id/execute_sql*/query_sql*）、metadata（状态注册表）、txcontext、registry、
│                        #   mod（transaction! 声明式宏）
├── zmcdataset/          # TokioPgRowSource（#[repr(transparent)] 包装 tokio_postgres::Row）→ ZmcDataSet/ZmcChildGroup 类型别名
├── crud/                # DbBmc trait、GenericCrudService（create/create_many/get/update/update_many/delete/list/page）、
│                        #   CustomQueryService::page_custom、CountOptimizer、字段加密/解密 utils、ServiceError
├── types/               # QueryBuilder DSL（CompareOp / OrderDirection / TypedRow / TypedResult / WhereClause）
├── host_functions.rs    # DatabaseHostFunctions（cmx:database 命名空间，db_query/db_execute，MsgPack）
├── monitoring/          # start_monitoring()：30s 健康检查 + 60s 事务超时扫描（默认 300s 超时自动回滚）
├── error.rs             # Error/Result + pg_detail（PG 错误链渲染，供 cmx-biz/dct/doc/mdm 等复用）
└── migration/           # （源码在，lib.rs 中已注释，未启用）
tests/
└── integration_test.rs  # #[ignore] 集成测试：TEST_PG_URL 环境变量指定库，覆盖建表/事务提交回滚/参数绑定
```

---

## 关键类型 / API

以下均为 `src/lib.rs` 的真实导出：

```rust
// 错误
pub use error::{pg_detail, Error, Result};

// 配置与连接
pub use config::{DbConfig, DbType, PoolConfig, PoolStatus};
pub use connection::DbPool;

// 管理器门面（独立单例，与 cmx-database 的 get_default_db_manager 完全隔离）
pub use manager::{
    DatabaseManager, DatabaseManagerConfig, TransactionContext, TransactionOptions,
    get_default_pg_db_manager,
};

// 事务（自由函数，按 db_id 全局路由）
pub use transaction::{
    Dbx, Propagation, SqlParams, TransactionMetadata, TransactionStatus,
    check_long_running_transactions, cleanup_completed_transactions, commit_txn_by_id, execute_sql,
    execute_sql_with_params, get_active_transactions, get_dbx_by_db_id, get_txn_holder_by_id,
    get_txn_metadata, query_sql, query_sql_with_params, rollback_txn_by_id, with_transaction_by_id,
};

// 执行器与零拷贝数据集
pub use executor::{ParamValue, PgResultConverter};
pub use zmcdataset::{ZmcChildGroup, ZmcDataSet, ZmcSchema};
pub use cmx_rowsource::{ZmcColType, ZmcRowSource};

// 类型 DSL / 监控 / WASM 宿主函数
pub use types::{CompareOp, OrderDirection, QueryBuilder, TypedResult, TypedRow};
pub use monitoring::start_monitoring;
pub use host_functions::DatabaseHostFunctions;

// 老链路核心模型（来自 cmx-core）
pub use cmx_core::model::data::dataset::DataSet;
```

`DatabaseManager` 主要方法签名（节选，均 `&self` 异步方法）：

```rust
pub async fn register_data_source(&self, db_config: DbConfig) -> Result<()>;
pub async fn get_biz_db_id(&self) -> String;          // source_type=="biz" 回退 default
pub async fn get_dbx(&self, db_id: &str) -> Result<Dbx>;
pub async fn health_check(&self, db_id: &str) -> Result<bool>;  // SELECT 1
pub async fn execute_sql_with_datavalues(&self, db_id: &str, txn_id: Option<&str>,
    sql: &str, params: Vec<DataValue>) -> Result<u64>;
pub async fn execute_sql_with_seavalues(&self, db_id: &str, txn_id: Option<&str>,
    sql: &str, params: sea_query::Values) -> Result<u64>;
pub async fn query_sql(&self, db_id: &str, txn_id: Option<&str>,
    sql: &str, dataset_id: &str) -> Result<DataSet>;
pub async fn query_sql_zmc(&self, db_id: &str, sql: &str,
    dataset_id: &str) -> Result<ZmcDataSet>;                       // 零拷贝，只读不走事务
pub async fn query_sql_zmc_stream_chunks(&self, db_id: &str, sql: &str,
    params: Vec<DataValue>, dataset_id: &str, col_names: Vec<String>,
    chunk_tx: mpsc::Sender<bytes::Bytes>) -> Result<u64>;          // 真·分帧流式
pub async fn commit_transaction(&self, txn_id: &str) -> Result<()>;
pub async fn rollback_transaction(&self, txn_id: &str) -> Result<()>;
pub fn get_transaction_context(&self) -> TransactionContext;
```

---

## 使用示例

### 安装

```toml
[dependencies]
# 内部依赖 - PG 数据库层（workspace path 统一版本）
cmx-database-pg = { workspace = true }
```

### 场景 1：注册数据源并执行查询（参考 `tests/integration_test.rs` / `cmx-service-base`）

```rust
use cmx_database_pg::{
    DatabaseManager, DatabaseManagerConfig, DataSet, DbConfig, DbType, PoolConfig,
};

async fn demo() -> cmx_database_pg::Result<()> {
    // 连接池配置（生产默认：max 10 / min 2 / acquire 30s / idle 600s / lifetime 1800s）
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        acquire_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };
    // 数据源配置：db_schema 会在建池时注入每条连接的 search_path
    let db_config = DbConfig {
        db_type: DbType::Postgres,                 // 仅 Postgres 实际可用
        db_url: "postgresql://postgres:postgres@127.0.0.1:5432/postgres".into(),
        db_id: "main_db".into(),
        db_schema: Some("public".into()),
        db_name: None,
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
        domain_code: None,
        application_code: None,
        module_code: None,
        default: true,                             // 作为 default 数据源
        source_type: None,
    };

    let manager = DatabaseManager::new(DatabaseManagerConfig::default());
    manager.register_data_source(db_config).await?;
    assert!(manager.health_check("main_db").await?);

    // 无参查询 → 老 DataSet（JSON 链路）
    let ds: DataSet = manager
        .query_sql("main_db", None, "SELECT 1 AS one", "demo_query")
        .await?;
    Ok(())
}
```

全局单例用法（与 `cmx-database` 的门面完全隔离，可并存于同一进程）：

```rust
use cmx_database_pg::{DbConfig, get_default_pg_db_manager};

async fn register_globally(config: DbConfig) -> cmx_database_pg::Result<()> {
    // cmx-service-base::register_pg_datasources 即此模式：flow 与 portal 共享同一份数据源
    get_default_pg_db_manager().register_data_source(config).await
}
```

### 场景 2：声明式事务（TransactionContext + TransactionGuard 自动回滚）

```rust
use cmx_database_pg::transaction::begin_transaction_guard_by_db_id;
use cmx_database_pg::{get_default_pg_db_manager, DataSet};

async fn transfer_points(db_id: &str) -> anyhow::Result<()> {
    let mm = get_default_pg_db_manager();

    // 方式 A：显式 begin/commit（guard 在 Drop 时若未提交则自动回滚）
    let guard = begin_transaction_guard_by_db_id(db_id).await?;
    let txn_id = guard.txn_id();
    mm.execute_sql(db_id, Some(txn_id), "UPDATE account SET pts = pts - 10 WHERE id = 1").await?;
    mm.execute_sql(db_id, Some(txn_id), "UPDATE account SET pts = pts + 10 WHERE id = 2").await?;
    mm.commit_transaction(txn_id).await?;   // 提交后 guard Drop 不再回滚
    Ok(())
}

async fn ctx_style(db_id: &str) -> anyhow::Result<()> {
    let mm = get_default_pg_db_manager();
    // 方式 B：TransactionContext 三段式（与 cmx-database 同构）
    let ctx = mm.get_transaction_context();
    let txn_id = ctx.begin(db_id).await?;
    mm.execute_sql(db_id, Some(&txn_id), "INSERT INTO t(a) VALUES (1)").await?;
    mm.commit_transaction(&txn_id).await?;
    Ok(())
}
```

> 事务期内独占一条池化连接（deadpool `Object`），`BEGIN`/`COMMIT`/`ROLLBACK` 以文本命令驱动；`with_transaction_by_id` 采用 take/放回手法在**不持锁**状态下执行闭包，适合跨 await 长事务。

### 场景 3：零拷贝 / 流式查询（ZmcDataSet 二进制出口）

```rust
use cmx_database_pg::get_default_pg_db_manager;

async fn load_dataset(db_id: &str) -> cmx_database_pg::Result<()> {
    let mm = get_default_pg_db_manager();

    // 零拷贝：返回持有原始 Row 的 ZmcDataSet，惰性列式 msgpack 编码（只读，不走事务）
    let zmc = mm
        .query_sql_zmc(db_id, "SELECT code, name FROM cm_dict", "dict_ds")
        .await?;
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);   // 编码产出前端 CmxDataSet.fromJSON 同构的列式包

    // 真·分帧流式：逐行编码经 chunk_tx 发送，峰值内存 O(单行)，适合超大扁平结果的网络直出
    let (tx, mut rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(64);
    let cols = vec!["code".to_string(), "name".to_string()];
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await { /* 写入响应流 */ }
    });
    let rows = mm
        .query_sql_zmc_stream_chunks(db_id, "SELECT code, name FROM big_table",
            vec![], "big_ds", cols, tx)
        .await?;
    Ok(())
}
```

### 场景 4：通用 CRUD（DbBmc + GenericCrudService，sea-query + modql）

```rust
use cmx_database_pg::crud::{DbBmc, GenericCrudService};
use modql::filter::ListOptions;

// 1. 为实体 Bmc 声明元信息
pub struct UserBmc;
impl DbBmc for UserBmc {
    const TABLE: &'static str = "user";
    const PK_COLUMN: &'static str = "code";
    fn encrypted_fields() -> &'static [&'static str] { &["phone"] } // 写入加密、读出解密
}

// 2. 直接使用静态泛型服务（E: HasSeaFields，F: Into<FilterGroups>）
async fn crud_demo(mm: &cmx_database_pg::DatabaseManager, db_id: &str)
    -> Result<(), cmx_database_pg::crud::ServiceError>
{
    // create：UNIQUE_VIOLATION 自动映射为业务错误「数据已存在」；get 自动追加 archived = 0
    // let created = GenericCrudService::<UserBmc>::create(mm, db_id, None, user_for_create).await?;

    // list / page：modql FilterGroups + ListOptions（分页排序），sea_query PostgresQueryBuilder 生成 SQL
    let (page_ds, total): (_, i64) = GenericCrudService::<UserBmc, user::Filter>::page(
        mm, db_id, None, None, ListOptions::default(),
    ).await?;
    Ok(())
}
```

### 场景 5：错误诊断与后台监控

```rust
use cmx_database_pg::{get_active_transactions, get_default_pg_db_manager, pg_detail, start_monitoring};

async fn diag(db_id: &str) {
    let mm = get_default_pg_db_manager();

    // 启动后台监控：30s 一次 SELECT 1 健康检查 + 60s 一次事务超时扫描
    // （默认 300s 未结束的活跃事务会被自动回滚，防止连接被长事务占用）
    start_monitoring(mm.clone(), std::time::Duration::from_secs(300));

    // 查看全局事务状态（Active / Committed / RolledBack + 元数据）
    let active = get_active_transactions().await;
    for meta in &active {
        println!("txn={}, db={}, status={:?}", meta.txn_id, meta.db_id, meta.status);
    }

    // 执行失败时用 pg_detail 渲染 PG 错误链（从 cmx-biz 下沉的统一出口）
    if let Err(e) = mm.execute_sql(db_id, None, "SELECT * FROM not_exist_table").await {
        tracing::error!("执行失败: {}", pg_detail(&e));
    }
}
```

---

## Features 说明

本 crate 的 `Cargo.toml` 未定义 `[features]` 段，无可选特性。注意依赖侧约定：`sea-query` 保持 workspace 默认 feature 集（with-chrono / with-time / with-json / with-uuid），**不得**额外启用 `with-rust_decimal` / `postgres-array`，否则 feature 统一会破坏 `sea-query-sqlx` 的穷尽 match（见 Cargo.toml 注释）。
