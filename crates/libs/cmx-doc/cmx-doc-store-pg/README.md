# cmx-doc-store-pg

> 业务单据（DOC）模块的 PostgreSQL 持久化/服务层：`cv_*` 单据物理表的装载（`DocLoader` 全拷贝 + `ZmcDocLoader` 零拷贝双驱动）、回存（`DocSaver` merge/replace 双模式 + 铸号 + 审计 + 乐观锁）、版本化（`DocRevision` 列式快照台账）与 `DocMetaView` 进程内缓存。SQL 由 `cmx-doc-model` 生成，本层负责执行与事务编排。

![version](https://img.shields.io/badge/version-0.1.12-blue) ![rust-edition](https://img.shields.io/badge/rust--edition-2024-orange)

## 项目简介

`cmx-doc-store-pg` 是 DOC 域三件套中的**持久化/服务层**。它把 `cmx-doc-model` 生成的参数化 SQL（`$N` 占位 + `DataValue` 绑定）在真实的 PostgreSQL 连接上执行，并承担三件套中所有"有状态"的职责：定义解析与缓存、多层主从树的逐层装载、changeset 事务回存、版本快照台账、层级懒下钻。SQL 文本生成完全委托上游 `cmx-doc-model`（零单据专属假设），本层只做执行、事务编排与结果装配，因此同一套装载/回存代码对 tokio-postgres 与 sqlx 两条驱动路径通用（[`ZmcExecutor`](src/zmc_util.rs) trait 抽象）。

核心业务概念是**业务单据的多层主从结构**：一张单据由根层（如凭证批 `cv_batch`）逐层挂子表（凭证头、科目行…），层间以 `parentKey=id / childKey=upper_id`（或命名外键）关联。装载端沿 `layer_order` BFS 逐层下钻、按 `childKey = ANY($parentIds)` 取子集并挂到父行 `_children`；回存端按同一拓扑序"插更父先、删除子先"落库，全程一个事务。

回存链路内置了四个平台级横切机制：**铸号**（前端临时 id 换 52 位 JS 安全真号 + `idMap` 回传，编码引擎可为 code 字段铸业务编码）、**审计方案 C**（服务端权威填充 create_by/create_time（不可变）/update_by/update_time，actor 缺失兜底 0=系统）、**乐观锁 B2**（根层 UPDATE 携带 `update_time` 基线检测并发冲突，成功后回传新基线 `updatedAt`）、**版本快照 B1**（保存事务内重新装载整单、`ColumnarCodec` 列式快照写入 `cmx_doc_revision` 台账）。此外还有**严格对账**（H1：期望写行数 ≠ 实际落行数即报错回滚，杜绝"假成功"）与**SAVEPOINT 兄弟表容错**（事务内非主兄弟表查询失败 `ROLLBACK TO SAVEPOINT` 复活事务）。

### 三件套分工

| 层 | crate | 职责 |
|----|-------|------|
| 协议皮肤 | `cmx-doc-api` | axum handler、参数提取、`ApiResp`/msgpack 信封、OpenAPI 切片 |
| 领域模型 | `cmx-doc-model` | `DocMetaView` 强类型定义投影、`DocQuery` 富查询、公式/规则、SQL 文本生成（DB-free） |
| **持久化** | **`cmx-doc-store-pg`（本 crate）** | **装载/回存/版本化执行、事务编排、定义缓存、层级服务适配** |

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-doc-model` | 消费 `DocMetaView` / `DocQuery` / `build_layer_select` / `json_to_dv_typed` / `dv_to_json` 等生成与转换逻辑 |
| `cmx-database` | sqlx 驱动门面 `DatabaseManager`：老 DataSet 链路 + 事务 guard（保存链路） |
| `cmx-database-pg` | tokio-postgres 并行 DB 层：`ZmcDataSet` 零拷贝新链路（装载链路） |
| `cmx-rowsource` | `ZmcDataSet<R>` / `ZmcChildGroup<R>` / `ZmcRowSource` 泛型行源抽象 |
| `cmx-traits` | `GlobalCodeMinter`：编码引擎钩子注入点（铸业务编码，避免直接依赖 cmx-code-api 成环） |
| `cmx-model-meta` | `definitions::store` 读定义 JSON + `resolve_doc_file` 定位 |
| `cmx-core` | `DataValue` / `DataSet` / `Schema` / `ColumnarCodec`（版本快照列式编解码） |
| `cmx-biz` | `BizError`、落库前列级校验（validation/errcode） |
| `cmx-utils` | `snowflake_id`（版本快照主键）、`next_pk_id`（铸号） |
| `cmx-master-slave` | 本 crate `impl` 其 `HierService` trait（依赖反转，见下） |
| `cmx-api-types` | resolve 链路的统一 Result/Error 返回类型 |

### 下游使用者

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-doc-api` | `cmx-doc-store-pg = { workspace = true }` | HTTP handler 调 `resolve_doc_meta` / `DocLoader` / `ZmcDocLoader` / `DocSaver` / `DocRevision` |
| `cmx-platform-app` | 间接（经 `cmx-doc-api`） | web-server 合并 `DocModule.routes()` 与 OpenApi 切片 |
| `cmx-master-slave` | trait 关系（非 Cargo 依赖） | 本 crate 的 `DocHierService` `impl HierService`，把 DOC 接为主从协调器的可换后端 |

> 跨 workspace 的 `cmx-portalservice` / `cmx-flowengine` 不直接依赖本 crate（经运行时 HTTP / 平台应用间接使用）。

## 核心功能与特性

| 功能 | 说明 | 关键入口 |
|------|------|---------|
| 定义解析咽喉点 | DAM（domain/application/module）三段可选，按 `doc`(moduleCode) > `file` 全局反查补全；脏值（空串/`"undefined"`/`"null"`）视为缺失；base 字段集自动合并 | `resolve::resolve_doc_meta` |
| DocMetaView 进程内缓存 | DashMap 无锁缓存 + TTL 600s 兜底最终一致 + 代数守卫（definitions 目录 generation 变化自动逐出，手动改定义无需重启） | `cache::get/put/invalidate` |
| 整树装载（全拷贝） | BFS 逐层下钻、`childKey = ANY($parentIds)`、子集挂 `_children`；`count_total` 时共享 filter 多跑一条 COUNT；空表也返回定义 schema 表头 | `DocLoader::load` |
| 整树装载（零拷贝） | 泛型 `<E: ZmcExecutor>` 双驱动（sqlx / tokio-postgres），`ZmcDataSet` 列式零拷贝；非主兄弟表失败仅跳过 warn | `ZmcDocLoader::load` |
| 层级懒下钻 | 只装某父 id 集下的子树，parent_ids 按 childKey 列类型化绑定 | `ZmcDocLoader::load_subtree` |
| merge 回存 | changeset 精确 UPSERT/UPDATE/DELETE；`assert_all_keys_matched` 静默零写防护 + 列级校验（422 结构化 violations）+ 严格对账 | `DocSaver::save(Merge)` |
| replace 回存 | 按 rootId 子树子查询链 DELETE（零预 SELECT）后父先全量 INSERT，前端免 diff | `DocSaver::save(Replace)` |
| 铸号 | inserted 临时 id → 52 位 JS 安全真号，子层外键（`upper_id` + 命名 childKey）全局两遍重指向；`idMap` 回传 | `saver::mint_ids_for_changeset` |
| 业务编码 | 挂 `codeRule(mode=auto)` 的层为空 code 行铸业务编码（`GlobalCodeMinter`），支持 `codeRuleOverrides` 激活配置覆盖（MDM cr-form 场景） | `saver::mint_codes_for_changeset` |
| 审计填充（方案 C） | 服务端权威覆盖审计列；`create_by/create_time` 在 ON CONFLICT SET 中排除（不可变）；actor 兜底 0=系统 | `saver::AuditCtx` |
| 乐观锁（B2） | 根层 UPDATE 携带前端回传的 `update_time` 基线，冲突即报错；成功回传 `updatedAt` 新基线支持连续保存 | `SaveResult::updated_at` |
| 版本台账（B1） | 事务内 `FOR UPDATE` 锁版本号行、旧版翻 `is_current=0`、列式快照 INSERT；时间线倒序 + 按版取快照 | `DocRevision` |
| 批量保存（方案 F） | `atomic=true` N 单共一个大事务（过账/导入语义）；`false` 每单独立事务逐单成败 | `DocSaver::save_batch` |
| 主从协调适配 | `impl HierService`：load/expand/save 全委托现成装载与保存，零新存储逻辑 | `DocHierService` |

## 模块结构

```text
src/
├── lib.rs          # 模块声明与 re-export 门面（含 ZmcDocLoaderSqlx/ZmcDocLoaderPg 别名 + cmx-doc-model re-export）
├── resolve.rs      # 定义解析咽喉点：DAM 归一化 → file 落定 → base 合并 → DocMetaView::parse（带缓存）
├── cache.rs        # DocMetaView 进程内缓存：DashMap + TTL 600s + 代数守卫
├── loader.rs       # DocLoader：老 DataSet 全拷贝装载（BFS 逐层 + count_total + SAVEPOINT 兄弟表容错）
├── zmc_loader.rs   # ZmcDocLoader：零拷贝装载（泛型双驱动）+ load_subtree 懒下钻
├── zmc_util.rs     # ZmcExecutor trait（双驱动抽象）+ rebind_schema/typecast_ids/collect_ids 等泛型辅助
├── saver.rs        # DocSaver：merge/replace 回存、铸号、审计、乐观锁、批量、严格对账
├── revision.rs     # DocRevision：cmx_doc_revision 版本台账（列式快照 / 时间线 / 取快照）
└── hier_service.rs # DocHierService：impl cmx-master-slave::HierService 依赖反转适配
tests/
└── parity/         # 前后端语义对拍测试（ms-driver.mjs + parity_ms.rs，缺 node 自动跳过）
```

## 关键类型与 API

### 定义解析与缓存（`resolve.rs` / `cache.rs`）

```rust
/// 读单据定义 + base 字段集，解析为 DocMetaView（命中缓存直接返回）。
/// 返回 (meta, file)：file 为最终落定的定义文件名。DAM 三段可选，全缺返回 400。
pub async fn resolve_doc_meta(
    domain: Option<&str>, app: Option<&str>, module: Option<&str>,
    file: Option<&str>, doc: Option<&str>,
) -> Result<(Arc<DocMetaView>, String)>;

/// 智能定位 DOC 定义文件名（doc(moduleCode) 精确定位 > file 显式指定 > 盲选默认/最高版本）。
pub async fn resolve_doc_file_smart(
    domain: &str, app: &str, module: &str,
    file: Option<&str>, doc: Option<&str>,
) -> Result<String>;

/// 从定义的 baseDocMetaRef.file 读 base 字段集（域=base）；失败返回 Null。
pub async fn load_base(doc: &Value) -> Value;

// cache.rs（模块函数，非方法）：缓存键与读写
pub fn doc_key(domain: &str, app: &str, module: &str, file: &str) -> String;
pub fn get(key: &str) -> Option<Arc<DocMetaView>>;
pub fn put(key: String, view: Arc<DocMetaView>);
pub fn invalidate(key: &str);
pub fn clear();
```

### 装载（`loader.rs` / `zmc_loader.rs`）

```rust
/// 按定义 + 查询指令装载整棵单据树，返回根层嵌套 DataSet（txn_id=None 走连接池）。
pub async fn DocLoader::load(
    mm: &cmx_database::DatabaseManager, db_id: &str,
    meta: &DocMetaView, query: &DocQuery,
) -> Result<DataSet>;

/// 同 load，但可在指定事务连接上装载（保存事务内重装载做版本快照，能看到未提交写）。
pub async fn DocLoader::load_txn(
    mm: &cmx_database::DatabaseManager, db_id: &str,
    meta: &DocMetaView, query: &DocQuery, txn_id: Option<&str>,
) -> Result<DataSet>;

/// 零拷贝装载：泛型于 ZmcExecutor（sqlx / tokio-postgres 双驱动），返回 ZmcDataSet<E::Row>。
pub async fn ZmcDocLoader::load<E: ZmcExecutor>(
    mm: E, db_id: &str, meta: &DocMetaView, query: &DocQuery,
) -> Result<ZmcDataSet<E::Row>>;

/// 懒下钻：只装载某层 parent_ids 下的子树（parent_ids 为 JSON 值，按 childKey 列类型化绑定）。
pub async fn ZmcDocLoader::load_subtree<E: ZmcExecutor>(
    mm: E, db_id: &str, meta: &DocMetaView, layer_id: &str,
    parent_ids: &[Value], query: &DocQuery,
) -> Result<ZmcDataSet<E::Row>>;

// 两个驱动别名的语境：同一算法，E 分别绑定为 cmx_database::DatabaseManager（sqlx）
// 或 cmx_database_pg::DatabaseManager（tokio-postgres）。
pub type ZmcDocLoaderSqlx = ZmcDocLoader;
pub type ZmcDocLoaderPg = ZmcDocLoader;
```

### 回存（`saver.rs`）

```rust
/// 保存上下文：操作者身份（审计）+ 单据定位 + 操作类型 + 编码规则覆盖。
pub struct SaveCtx {
    pub actor_id: i64,                       // 审计列用；缺失兜底 0=系统
    pub actor_name: String,                  // 版本台账展示名
    pub doc_file: String,                    // 版本台账定位「哪种单据」
    pub op_override: Option<String>,         // 如 restore 传 Some("restore")
    pub code_rule_overrides: HashMap<String, String>, // {field: ruleCode}（MDM 激活配置）
}

pub enum SaveMode { Merge, Replace }         // SaveMode::parse("merge"/"replace")

/// 单单保存：guard 事务内「铸号 → 写入 → 记版本 → 算新基线」一体，任一步失败回滚。
pub async fn DocSaver::save(
    mm: &cmx_database::DatabaseManager, db_id: &str,
    meta: &DocMetaView, mode: SaveMode, changes: &Value, sctx: &SaveCtx,
) -> Result<SaveResult>;

/// 批量保存（一批可混多种单据）：atomic=true 一个大事务；false 每单独立事务。
pub async fn DocSaver::save_batch(
    mm: &cmx_database::DatabaseManager, db_id: &str,
    items: &[BatchItem<'_>], atomic: bool,
) -> Result<Vec<BatchOutcome>>;

#[derive(serde::Serialize)]
pub struct SaveResult {
    pub ok: bool,
    pub mode: String,
    pub affected: u64,
    pub updated_at: Vec<UpdatedBaseline>,    // 序列化为 updatedAt（乐观锁新基线）
    pub id_map: Map<String, Value>,          // 序列化为 idMap（临时 id → 真号）
}

// HTTP body 解析辅助（handler 用）
pub fn parse_save_body(body: &Value) -> (SaveMode, Value);
pub fn parse_code_rule_overrides(body: &Value) -> HashMap<String, String>;
```

### 版本化（`revision.rs`）与层级服务（`hier_service.rs`）

```rust
pub struct DocRevision;

/// 版本业务字段（事务坐标 txn_id 单独传）。root_ds 为「保存后重新装配的整单」快照源。
pub struct RevisionRecord<'a> {
    pub doc_file: &'a str,        // 如 "cv_batch.json"
    pub root_table: &'a str,      // 如 "cv_batch"
    pub root_id: &'a str,
    pub op: &'a str,              // create / update / delete / restore
    pub root_ds: &'a DataSet,
    pub actor_id: Option<&'a str>,
    pub actor_name: Option<&'a str>,
    pub reason: Option<&'a str>,  // reason_required 时由 saver 校验非空
    pub biz_status: Option<&'a str>,
}

pub async fn DocRevision::record(mm: &DatabaseManager, db_id: &str,
    txn_id: &str, rec: &RevisionRecord<'_>) -> Result<i64>;   // 返回版本雪花 id
pub async fn DocRevision::list(mm: &DatabaseManager, db_id: &str,
    doc_file: &str, root_id: &str) -> Result<Value>;          // 倒序时间线 rows
pub async fn DocRevision::get_snapshot(mm: &DatabaseManager, db_id: &str,
    doc_file: &str, root_id: &str, rev: Option<i32>) -> Result<Value>; // 列式包（前端 fromJSON 直接用）

/// 主从协调器适配：load→ZmcDocLoader、expand→load_subtree、save→DocSaver(Merge)。
pub struct DocHierService { pub domain/application/module/file/doc: Option<String>, pub db_id: String }
```

## 数据表

**`cv_*` 单据物理表**：由模型中心（cmx-model）按 voucherSchema/voucherTables 定义 deploy 建 DDL，本层**不建表、不写 DDL**，只在已存在的表上执行装载/回存。

**`cmx_doc_revision` 版本台账**（本层唯一自管表）：

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | BIGINT | 雪花 id 主键（`snowflake_id()`） |
| `doc_file` | TEXT | 单据定义文件名（与 root_id 联合定位一单的版本序列） |
| `root_table` | TEXT | 根层物理表名 |
| `root_id` | TEXT | 单据根行 id |
| `rev_no` | INT | 版本号（同 doc_file+root_id 下 MAX+1） |
| `is_current` | INT | 1=当前版（同单仅一行；写新版前先翻旧版为 0） |
| `op` | TEXT | create / update / delete / restore |
| `snapshot` | JSONB | `ColumnarCodec` 列式包快照（与装载同序列化器） |
| `reason` / `actor_id` / `actor_name` / `biz_status` | TEXT | 变更原因 / 操作者 / 业务状态冗余 |
| `created_at` | TIMESTAMPTZ | 记录时间 |

并发防护：`SELECT … FOR UPDATE` 子查询锁版本号行串行化分配；唯一索引 `uk_doc_rev` 兜底（首次保存无行可锁时防并发重复）。

## 使用示例

### 场景 1：解析单据定义并零拷贝装载整树

```rust
use cmx_database_pg::get_default_pg_db_manager;        // tokio-postgres 驱动
use cmx_doc_model::DocQuery;
use cmx_doc_store_pg::{ZmcDocLoader, resolve_doc_meta};

// DAM/file 全部省略，只按 doc（moduleCode）全局反查——与 /doc/* HTTP 端点同一咽喉点
let (meta, _file) = resolve_doc_meta(None, None, None, None, Some("cv-voucher")).await?;

// 根层 id + limit=50 + 下钻 2 层
let root_id = meta.root_layer().expect("单据定义无根层").id.clone();
let dq = DocQuery::simple(&root_id, Some(50), Some(2));

let mm = get_default_pg_db_manager();
let zmc = ZmcDocLoader::load(mm, "main", &meta, &dq).await?;
// zmc: ZmcDataSet<TokioPgRowSource> —— 可 encode_columnar_binary 直出 msgpack，
//      子层已按 ZmcChildGroup 挂好，空表也有定义 schema 表头（rebind_schema 兜底）。
```

### 场景 2：merge 模式保存 changeset（铸号 + 乐观锁基线回传）

```rust
use std::collections::HashMap;
use cmx_database::get_default_db_manager;              // sqlx 驱动（保存链路）
use cmx_doc_store_pg::{DocSaver, SaveCtx, SaveMode, resolve_doc_meta};
use serde_json::json;

let (meta, doc_file) = resolve_doc_meta(None, None, None, None, Some("cv-voucher")).await?;
let sctx = SaveCtx {
    actor_id: 1001,
    actor_name: "张三".into(),
    doc_file: doc_file.clone(),
    op_override: None,
    code_rule_overrides: HashMap::new(),   // MDM cr-form 激活配置才需要
};

// 前端 ChangeSetCollector 收拢的 changeset：临时 id 由后端铸 52 位真号并回传 idMap
let changes = json!({
    "cv_batch": {
        "updated": [ { "id": "88", "fields": { "total_dr": 1200 } } ]   // 根层带乐观锁基线检测
    },
    "cv_entry": {
        "inserted": [ { "id": "tmp-1", "upper_id": "88", "fields": { "subject": "1001", "dr": 600 } } ],
        "deleted": [ "901" ]
    }
});

let mm = get_default_db_manager();
let res = DocSaver::save(mm, "main", &meta, SaveMode::Merge, &changes, &sctx).await?;
// res.id_map: {"tmp-1": 2345678901234567}  —— 前端把临时行换成真号
// res.updated_at: [{id:"88", updateTime:"..."}] —— 前端刷新乐观锁基线，可连续保存不刷新页
// 任一层 key 对不上定义（H1）或对账不等 → 整个事务回滚报错，绝不假成功。
```

### 场景 3：版本时间线与按版取快照

```rust
use cmx_database::get_default_db_manager;
use cmx_doc_store_pg::DocRevision;

let mm = get_default_db_manager();
// 台账倒序时间线（id/rev_no/is_current/op/reason/actor_name/biz_status/created_at）
let rows = DocRevision::list(mm, "main", "cv_batch.json", "88").await?;
// 取第 3 版快照（None 则取当前版）：列式包，前端 CmxDataSet.fromJSON 直接用
let snap = DocRevision::get_snapshot(mm, "main", "cv_batch.json", "88", Some(3)).await?;
// 回滚 = 用快照经 DocSaver 以 Replace 模式重写整单（op_override 传 Some("restore")）
```

### 场景 4：批量保存（过账语义，一大事务）

```rust
use cmx_doc_store_pg::{BatchItem, DocSaver, SaveMode};

// 一批可混多种单据：各 item 自带 meta/mode/changes/sctx
let items = [
    BatchItem { meta: &meta_a, mode: SaveMode::Merge, changes: &cs_a, sctx: &sctx_a },
    BatchItem { meta: &meta_b, mode: SaveMode::Merge, changes: &cs_b, sctx: &sctx_b },
];
// atomic=true：N 单共一个大事务，任一单失败整批回滚（Err 带出错单 index），无部分提交
let outcomes = DocSaver::save_batch(mm, "main", &items, true).await?;
// atomic=false 时逐单独立事务，失败单 ok=false + error，全部收进 outcomes，永不整体 Err。
```

### 场景 5：懒下钻子树（前端展开某父行时按需装载）

```rust
use cmx_database_pg::get_default_pg_db_manager;
use cmx_doc_model::DocQuery;
use cmx_doc_store_pg::{ZmcDocLoader, resolve_doc_meta};
use serde_json::json;

let (meta, _file) = resolve_doc_meta(None, None, None, None, Some("cv-voucher")).await?;
let parent_ids: Vec<serde_json::Value> = vec![json!("88"), json!("89")]; // 待展开的父行 id
let dq = DocQuery::simple("cv_entry", None, None);                       // 目标层 + 不再下钻

let mm = get_default_pg_db_manager();
// 只装载 cv_entry 层中 childKey 指向 88/89 的行（id 按 childKey 列类型化绑定）
let subtree = ZmcDocLoader::load_subtree(mm, "main", &meta, "cv_entry", &parent_ids, &dq).await?;
```

## 设计要点

- **SQL 生成与执行分离**：本层零 SQL 字符串拼接（防注入由 model 层列名白名单保证），只以 `DataValue` 参数绑定执行；因此双驱动（sqlx / tokio-postgres）共用一份装载算法。
- **装载与保存走不同驱动**：装载（读多）走 tokio-postgres 零拷贝，保存（事务）走 sqlx 事务 guard——`DocHierService` 内注释明确记录了这一约定。
- **缓存一致性**：进程内 DashMap 缓存以 TTL 600s 兜底最终一致；`definitions_generation` 代数守卫在定义文件变更时自动逐出（无需重启）。
- **对拍测试**：`tests/parity/` 内置 Node 驱动的前后端语义对拍（缺 node 自动跳过），是「后端移植是否忠实复现前端语义」的最强保障。
