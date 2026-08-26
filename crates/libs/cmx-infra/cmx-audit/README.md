# cmx-audit

> 通用审计日志基础设施：领域无关的审计记录（`AuditRecord`）+ 记录/查询双层 trait（`AuditLogger` / `AuditStore`），内置 PostgreSQL 持久化与内存两套存储实现。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-audit` 是 cmx-container 基础设施层的审计日志 crate，为平台中所有关键操作（认证、IAM、插件、业务）提供统一的记录与查询能力。它**不耦合任何具体业务领域**：各域通过 `AuditRecord.domain`（`Auth` / `Iam` / `Plugin` / `Biz`）区分，记录结构由通用字段（操作者、目标、结果、耗时、请求 ID、IP）组成。

### 设计要点

- **双层抽象**：`AuditLogger`（面向业务代码的记录/查询门面）+ `AuditStore`（面向存储后端的持久化抽象），`DefaultAuditLogger` 是二者的默认粘合；
- **双实现**：`DatabaseAuditStore`（PostgreSQL，表 `cmx_audit_log`，经 **cmx-database（sqlx 版）** 的 `DatabaseManager` 执行 sea-query 生成的 SQL）与 `MemoryAuditStore`（`RwLock<Vec<AuditRecord>>`，测试 / 单体场景）；
- **app_id 多租户隔离**：`DatabaseAuditStore` 构造时绑定 `app_id`，写入时绑定到记录、查询时默认按其过滤；按 `DeployMode` 分支 —— Mono（单体）模式查询**不拼** `WHERE app_id`（审计全局可见），Micro（微服务）模式默认按 app_id 过滤（调用方可用 `AuditFilter.app_id` 显式覆盖）；
- **物理删除安全约束**：`delete_hard(filter)` 要求 filter 至少含 `ids` / `from`+`to` 时间窗 / `actor_id` / `target_id` / `request_id` 之一，且强制限定 app_id，否则直接拒绝并告警 —— 防止误删全表；
- **软删除约定**：查询自动追加 `archived = 0` 过滤；批量写入按 1000 条/批拆分（本表每行 14 字段，1000 条 = 14000 参数，远低于 PG 单条 INSERT 约 65535 参数上限）。

建表 DDL 位于 `docs/sql/migrations/20260624_001_cmx_audit_log.up.sql`（cmx-container 仓库）。

> 注意：本 crate 依赖的是 **`cmx-database`（sqlx 抽象层）**，而非 `cmx-database-pg`（tokio-postgres 版）。

---

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-database`（sqlx 版） | `DatabaseManager` + `execute_sql_with_sqlxvalues` / `query_sql_with_sqlxvalues` 执行 SQL |
| `cmx-core` | `DataValue` / `DataSet` 参数与结果模型 |
| `cmx-utils` | `DeployMode`（Mono/Micro）部署模式判定 |
| `sea-query` + `sea-query-sqlx` | SQL 生成与参数绑定 |
| `async-trait` / `chrono` / `uuid` / `serde_json` / `thiserror` / `tracing` | 异步 trait、时间、ID、详情 JSON、错误、日志 |

### 下游使用者（Cargo.toml 反查）

| crate | 用法 |
|-------|------|
| `cmx-auth` | 认证域审计（登录/登出/Token 刷新 → `AuditDomain::Auth`） |
| `cmx-iam` | IAM 域审计（角色分配/权限变更 → `AuditDomain::Iam`） |
| `cmx-plugin` | 插件域审计（安装/卸载/升级 → `AuditDomain::Plugin`） |
| `cmx-platform-app` | 应用启动时装配审计组件（构造 `DefaultAuditLogger` 注入各业务模块） |

---

## 核心功能与特性

| 功能 | 说明 | 关键入口 |
|------|------|----------|
| 审计记录 | 领域无关记录结构 + builder 链式补充（`with_actor` / `with_target` / `with_details` / `with_request_id` / `with_ip` / `with_duration`），自动生成 UUID 与时间戳 | `AuditRecord::new` |
| 记录门面 | trait 抽象 `log` / `query`，默认实现包装任意 `AuditStore` | `DefaultAuditLogger::new` / `with_db` |
| PG 持久化 | sea-query 生成 SQL，save / save_batch（1000/批）/ query，自动 `archived = 0`，`ORDER BY started_at DESC` | `DatabaseAuditStore` |
| 内存存储 | `RwLock<Vec<AuditRecord>>`，进程内单租户（有意不过滤 app_id） | `MemoryAuditStore` |
| 多租户 | app_id 绑定 + DeployMode 分支（Mono 全局可见 / Micro 隔离过滤） | `DatabaseAuditStore::new` |
| 安全删除 | `delete_hard` 强制安全约束，无约束条件直接拒绝 | `DatabaseAuditStore::delete_hard` |
| 组合过滤 | 域 / 操作者 / 目标 / 请求 ID / 结果 / 时间范围 / ID 列表 | `AuditFilter` |

---

## 模块结构

```text
src/
├── lib.rs        # 导出面：AuditError/Result、AuditLogger/DefaultAuditLogger、
│                 #   AuditDomain/AuditRecord/OperationResult、DatabaseAuditStore
├── record.rs     # AuditRecord（14 通用字段）+ AuditDomain / OperationResult 枚举
├── logger.rs     # AuditLogger trait + DefaultAuditLogger（new / with_db 快捷构造）
├── error.rs      # AuditError（Database / SerdeJson / Internal）+ Result 别名
└── store/
    ├── mod.rs    # AuditFilter（组合过滤条件）+ AuditStore trait（save / save_batch / query）
    ├── database.rs  # DatabaseAuditStore：PG 实现（表 cmx_audit_log，sea-query + sqlx 桥接，
    │                #   app_id 隔离、DeployMode 分支、delete_hard 安全约束、BATCH_CHUNK=1000）
    ├── cmx_audit_log.testfixture.sql  # 测试专用建表夹具（database.rs 单测 include_str!，
    │                #   不依赖 docs/sql/ 迁移目录布局；生产 DDL 仍见 docs/sql/migrations）
    └── memory.rs    # MemoryAuditStore：内存实现（测试 / 单体场景）
```

---

## 关键类型 / API

```rust
// —— 记录（src/record.rs）——
pub enum AuditDomain { Auth, Iam, Plugin, Biz }        // serde lowercase
pub enum OperationResult { Success, Failure }

pub struct AuditRecord {
    pub id: String,                     // UUID v4
    pub domain: AuditDomain,
    pub operation: String,              // 如 "login", "role_assign", "plugin_install"
    pub result: OperationResult,
    pub actor_id: Option<String>,       // 操作者
    pub actor_name: Option<String>,
    pub target_type: Option<String>,    // 如 "user", "role", "plugin"
    pub target_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub request_id: Option<String>,     // 链路追踪
    pub ip_address: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub duration_ms: Option<i64>,
}
impl AuditRecord {
    pub fn new(domain: AuditDomain, operation: impl Into<String>, result: OperationResult) -> Self;
    pub fn with_actor(mut self, actor_id: impl Into<String>, actor_name: impl Into<String>) -> Self;
    pub fn with_target(mut self, target_type: impl Into<String>, target_id: impl Into<String>) -> Self;
    pub fn with_details(mut self, details: serde_json::Value) -> Self;
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self;
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self;
    pub fn with_duration(mut self, duration_ms: i64) -> Self;
}

// —— 门面（src/logger.rs）——
#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn log(&self, record: AuditRecord) -> Result<()>;
    async fn query(&self, filter: &AuditFilter, limit: u64, offset: u64) -> Result<Vec<AuditRecord>>;
}
pub struct DefaultAuditLogger { /* Arc<dyn AuditStore> */ }
impl DefaultAuditLogger {
    pub fn new(store: Arc<dyn AuditStore>) -> Self;
    /// 从 cmx-database 管理器快捷构造（app_id 通常取 ConfigManager::global()
    /// .get_string("application.id")，缺省回退 "default"）
    pub fn with_db(mm: Arc<cmx_database::DatabaseManager>,
                   db_id: impl Into<String>, app_id: impl Into<String>) -> Self;
}

// —— 存储（src/store）——
pub struct AuditFilter {                 // 全部 Option，Default 可全置空
    pub domain: Option<AuditDomain>, pub actor_id: Option<String>,
    pub target_type: Option<String>, pub target_id: Option<String>,
    pub request_id: Option<String>, pub result: Option<OperationResult>,
    pub from: Option<DateTime<Utc>>, pub to: Option<DateTime<Utc>>,
    pub ids: Option<Vec<String>>,        // delete_hard 安全调用 / 精确查询
    pub app_id: Option<String>,          // 覆盖 DatabaseAuditStore 构造时的默认
}
#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn save(&self, record: &AuditRecord) -> Result<()>;
    async fn save_batch(&self, records: &[AuditRecord]) -> Result<()>;  // 默认逐条，PG 版批量
    async fn query(&self, filter: &AuditFilter, limit: u64, offset: u64) -> Result<Vec<AuditRecord>>;
}

pub struct DatabaseAuditStore { /* cmx-database(sqlx) 版 DatabaseManager */ }
impl DatabaseAuditStore {
    pub fn new(db_manager: Arc<cmx_database::DatabaseManager>,
               default_db_id: impl Into<String>, app_id: impl Into<String>) -> Self;
    /// 物理删除（需安全约束：ids / from+to / actor_id / target_id / request_id 至少一项）
    pub async fn delete_hard(&self, filter: &AuditFilter) -> Result<u64>;
}
```

---

## 使用示例

### 安装

```toml
[dependencies]
# 内部依赖 - 审计日志（workspace path 统一版本）
cmx-audit = { workspace = true }
```

### 场景 1：记录一条认证审计（cmx-auth 的模式）

```rust
use cmx_audit::{AuditDomain, AuditLogger, AuditRecord, DefaultAuditLogger, OperationResult};

async fn on_login(logger: &DefaultAuditLogger, user_id: &str, user_name: &str, ip: &str)
    -> cmx_audit::Result<()>
{
    let record = AuditRecord::new(
            AuditDomain::Auth, "login", OperationResult::Success)
        .with_actor(user_id, user_name)     // 操作者
        .with_ip(ip)                        // 来源 IP
        .with_request_id("req-2026-0001")   // 链路追踪
        .with_duration(35);                 // 耗时 ms
    logger.log(record).await
}
```

### 场景 2：接入 PostgreSQL 存储（cmx-platform-app 装配模式）

```rust
use cmx_audit::{DefaultAuditLogger, MemoryAuditStore};
use std::sync::Arc;

fn build_logger() -> DefaultAuditLogger {
    // 生产：绑定 cmx-database（sqlx 版）管理器 + app_id（多租户隔离）
    // let mm = cmx_database::get_default_db_manager().clone();
    // let app_id = cmx_utils::config::ConfigManager::global()
    //     .get_string("application.id")
    //     .unwrap_or_else(|| "default".into());
    // DefaultAuditLogger::with_db(mm, "main_db", app_id)

    // 测试/单体：内存存储
    DefaultAuditLogger::new(Arc::new(MemoryAuditStore::new()))
}
```

### 场景 3：组合条件分页查询

```rust
use cmx_audit::{AuditDomain, AuditFilter, AuditLogger, DefaultAuditLogger};
use chrono::{TimeZone, Utc};

async fn query_failed_iam(logger: &DefaultAuditLogger) -> cmx_audit::Result<Vec<cmx_audit::AuditRecord>> {
    let filter = AuditFilter {
        domain: Some(AuditDomain::Iam),
        result: Some(cmx_audit::OperationResult::Failure),
        from: Some(Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap()),
        to: Some(Utc.with_ymd_and_hms(2026, 8, 19, 0, 0, 0).unwrap()),
        ..Default::default()
    };
    // 自动：archived = 0 过滤 + Micro 模式 app_id 隔离 + ORDER BY started_at DESC
    logger.query(&filter, 20, 0).await
}
```

### 场景 4：受安全约束的物理删除

```rust
use cmx_audit::{AuditFilter, DatabaseAuditStore};

async fn purge(store: &DatabaseAuditStore) -> cmx_audit::Result<u64> {
    // 必须带安全约束（ids / from+to / actor_id / target_id / request_id 至少一项），
    // 否则返回 Err 并 warn，防止误删全表；执行时仍强制限定构造时的 app_id
    let filter = AuditFilter {
        ids: Some(vec!["0197c000-0000-7000-8000-000000000001".into()]),
        ..Default::default()
    };
    store.delete_hard(&filter).await
}
```

---

## Features 说明

本 crate 的 `Cargo.toml` 未定义 `[features]` 段，无可选特性（存储后端选择在代码层通过 `Arc<dyn AuditStore>` 注入完成，无需编译期 feature）。
