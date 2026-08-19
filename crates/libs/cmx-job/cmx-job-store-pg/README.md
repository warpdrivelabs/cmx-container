# cmx-job-store-pg

> 异步任务中心的 PostgreSQL 持久化层：实现 `cmx-job-core::JobStore` trait——作业主表 / 日志 / 断点 / 历史表的自 DDL、写穿与查询，含 `FOR UPDATE SKIP LOCKED` 原子抢占与 RU/HI 归档事务。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-job-store-pg` 是任务中心三件套的持久化落点：`cmx-job-core` 只定义 [`JobStore`] trait（零 DB 依赖，破环方向与 cmx-rpt-store-pg 一致），本 crate 提供 PG 实现。`JobManager` 在关键点（提交 / 状态跃迁 / 进度去抖 / 终态）经 trait 写穿到主库 `primary`；分布式模式（M3）下的 pending 抢占、属主心跳、失联回收、跨节点控制意图也都落在本 crate 的 SQL 里。

硬约束（母版 `cmx-flow-store-pg::ddl`）：表名 `cmx_` 前缀、禁外键、DDL 幂等（`CREATE TABLE IF NOT EXISTS` + `ALTER TABLE ADD COLUMN IF NOT EXISTS` 补列兜底既有库升级）。多实例并发启动时并发 DDL 的单句失败视为「对端已建」的良性竞争——逐条容错执行，最后以 `to_regclass('public.cmx_job')` 校验主表确已存在才算成功。

所有写方法容错：失败只 `warn` 不 panic（**进度是内存权威，DB 是备份**，方案 §14.1）——DB 抖动不阻塞 handler 执行，代价是极端情况下库内状态短暂滞后，由终态 `finish` 与 reaper 循环收敛。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-job-core` | `JobStore` trait + 模型（`Job` / `JobStatus` / `ProgressSnapshot` / `JobOrigin` / `JobError`） |
| `cmx-core` | `DataValue` / `SqlTypeMarker` / `dv!` 宏（SQL 参数绑定） |
| `cmx-database-pg` | tokio-postgres 并行 DB 层：`get_default_pg_db_manager` 的 `execute_sql_with_datavalues` / `query_sql_with_datavalues` + 事务上下文 |
| `cmx-api-types` | 业务错误翻译（PG 错误 → 优雅提示） |
| `cmx-utils` | `next_pk_id` 日志行主键铸号 |
| `async-trait` / `serde` / `serde_json` / `tracing` | trait 实现 / JSONB 交换 / 日志 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | workspace 依赖 | `config/jobs.rs` 的 `init_job_center()`：`Arc::new(PgJobStore::default_db())` 注入 `init_job_subsystem_full` |

---

## 核心功能与特性

| 功能 | 方法 | 说明 |
|------|------|------|
| 自 DDL | `ensure_schema` | 逐条执行幂等 DDL（失败仅 warn），`to_regclass` 校验主表存在 |
| 作业写穿 | `insert` / `update_status` / `update_progress` / `finish` | 状态跃迁必落；进度去抖落；insert 用 `ON CONFLICT DO NOTHING` 防与并发 `update_status` 竞争回写 |
| 日志流水 | `append_log` | 直插 `cmx_job_log`（id 走 `next_pk_id` 铸号） |
| RU/HI 归档 | `archive` | 事务内五步：`INSERT...SELECT` 全列复制作业→`cmx_job_hi`、日志→`cmx_job_hi_log`，再删 log / checkpoint / 活跃行（非真删，供审计） |
| 活跃/历史查询 | `list` / `get` / `load_active` / `list_history` / `get_history` / `count_history` | `load_active` 供启动崩溃恢复；历史按 `archived_at DESC` 分页，`count_history` 与 `list_history` 同过滤保证 total 一致 |
| 原子抢占 | `claim_pending` | `UPDATE...WHERE id IN (SELECT...FOR UPDATE SKIP LOCKED) RETURNING`：pending 按 `(priority DESC, created_at)` 出队，置 `running + node_id + claimed_at + heartbeat_at`，多实例各领不相交子集 |
| 属主心跳 | `heartbeat` | 刷新本节点属主作业的 `heartbeat_at` |
| 失联回收 | `reap_dead_owners` | `heartbeat_at < now - timeout` 的 running/paused/cancelling 重置 pending（清属主），供他节点重领 |
| 跨节点控制 | `set_control_intent` / `take_control_intents` | 控制意图写 `control_intent` 列；pending 态 cancel 直接落终态 cancelled(499)；take 用 CTE 先快照旧值再清空（RETURNING 返回更新后值之坑） |

### 表结构（DDL_STATEMENTS，5 张）

| 表 | 职责 |
|----|------|
| `cmx_job` | 作业主表：status / params·progress·result·error JSONB / origin / org_id / 分布式列（node_id / heartbeat_at / control_intent / claimed_at / parent_job_id 预留 M4） |
| `cmx_job_log` | 日志流水（job_id + seq 序） |
| `cmx_job_checkpoint` | 断点（M3 断点续跑预留，建表占位；归档时直接清） |
| `cmx_job_hi` | 历史作业表：与 `cmx_job` 同构 + `archived_at` |
| `cmx_job_hi_log` | 历史日志表（归档随迁） |

索引：`ix_cmx_job_status` / `ix_cmx_job_kind` / `ix_cmx_job_org` / `ix_cmx_job_claim`（抢占出队）/ `ix_cmx_job_owner`（属主巡检）/ `ix_cmx_job_log` + 历史表三索引。

---

## 模块结构

```text
cmx-job-store-pg
├── src
│   ├── lib.rs   # PgJobStore：JobStore 全方法 PG 实现 + 行↔Job 映射 + 容错原则（572 行）
│   └── ddl.rs   # DDL_STATEMENTS：5 表幂等建表 + 补列 + 索引（110 行）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// lib.rs
pub const JOB_DB_ID: &str = "primary";   // 任务中心主库 id（对齐 dev-local.toml 的 primary）

pub struct PgJobStore { /* db_id: String，无状态 */ }
impl PgJobStore {
    pub fn new(db_id: impl Into<String>) -> Self;
    pub fn default_db() -> Self;         // PgJobStore::new("primary")
}

// JobStore trait 的 PG 实现（trait 定义见 cmx-job-core::store，共 18 个方法）：
//   ensure_schema / insert / update_status / update_progress / finish / append_log /
//   archive / list / get / load_active /
//   list_history / get_history / count_history /
//   claim_pending / heartbeat / reap_dead_owners / set_control_intent / take_control_intents

// ddl.rs
pub const DDL_STATEMENTS: &[&str];  // 按顺序执行的幂等 DDL（5 表 + 补列 + 索引）

// —— 关键 SQL（claim_pending：分布式不重跑的核心）——
// UPDATE cmx_job SET status='running', node_id=$1, claimed_at=$2, heartbeat_at=$2,
//        started_at=COALESCE(started_at,$2)
// WHERE id IN (SELECT id FROM cmx_job WHERE status='pending'
//              ORDER BY priority DESC, created_at ASC
//              FOR UPDATE SKIP LOCKED LIMIT {n})
// RETURNING id, kind, title, status, params, progress, ...
```

DB 访问统一走 `cmx_database_pg::get_default_pg_db_manager()` 的 DataValue 门面；JSONB 列以 `DataValue::Json` + `$N::jsonb` 绑定，可空列用 `DataValue::NullTyped(SqlTypeMarker::…)` 类型化 NULL。

---

## 使用示例

### 一、web-server 装配（cmx-platform-app/src/config/jobs.rs 真实用法）

```rust
// 注入 PG 持久化后端 + 失败告警终态回调；distributed=true 时由 core 启动
// claim / heartbeat+reaper / control 三循环（本 crate 提供 SQL 支撑）。
cmx_job_core::init_job_subsystem_full(
    job_cfg,                                            // JobConfig { distributed, node_id, owner_timeout_ms: 30_000, .. }
    std::sync::Arc::new(cmx_job_store_pg::PgJobStore::default_db()),
    Some(hook),                                         // TerminalHook：失败作业 → GlobalEventBus("job.failed")
)
.await;
```

### 二、自定义库 id（多库部署）

```rust
// 默认走 primary 主库；作业表独立部署在其它库时显式指定 db_id
let store = PgJobStore::new("job-db");
cmx_job_core::init_job_subsystem_with_store(cfg, std::sync::Arc::new(store)).await;
```

### 三、手动预建表（部署脚本 / 运维排查）

```rust
// ensure_schema 幂等：重复调用安全；多实例并发建表时单句失败仅 warn，
// 最后校验 to_regclass('public.cmx_job') 存在即成功。
let store = PgJobStore::default_db();
if let Err(e) = store.ensure_schema().await {
    tracing::warn!(error = %e, "schema 不可用，任务中心将降级为内存态");
}
```

### 四、归档语义验证（SQL 视角）

```sql
-- DELETE /api/jobs/{id}（归档）在库内的最终效果：活跃表无此行，历史表留全档
SELECT id, status, archived_at FROM cmx_job_hi ORDER BY archived_at DESC LIMIT 10;
SELECT count(*) AS n FROM cmx_job WHERE id = 7359230048614400;  -- → 0
```

---

## 设计说明

- **容错优先**：写失败仅 warn（「内存权威，DB 备份」）；查询失败返回空集——DB 短暂不可用不拖垮任务执行，靠终态写与 reaper 收敛。
- **insert 用 ON CONFLICT DO NOTHING**：insert 与 update_status 是并发 fire-and-forget，若 insert 用 DO UPDATE 回写 status 会把 running 覆盖回 pending。
- **CTE 取控制意图**：`UPDATE...SET control_intent=NULL...RETURNING control_intent` 返回的是更新后的 NULL，必须 CTE 先 SELECT 快照旧值再 UPDATE 清空。
- **pending 态 cancel 直落终态**：排队中作业无属主消费意图，`set_control_intent("cancel")` 直接置 cancelled 并写 `error = {"code":499,"message":"作业已被停止"}`。
