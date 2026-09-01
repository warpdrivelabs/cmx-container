# cmx-dct-store-pg

> 数据字典（DCT）的 PostgreSQL 持久化/服务层 —— SQL 文本全部由 [cmx-dct-model](../cmx-dct-model) 构造，本层负责执行、事务编排、编码铸号与流式导入导出。

![version](https://img.shields.io/badge/version-0.1.12-blue)
![rust-edition](https://img.shields.io/badge/rust--edition-2024-orange)

## 项目简介

`cmx-dct-store-pg` 是 DCT 域三件套的持久化层，与 [cmx-dct-api](../cmx-dct-api)（HTTP 协议皮肤）、[cmx-dct-model](../cmx-dct-model)（DB-free 纯逻辑与 SQL 文本生成）配套。层间分工严格遵守"生成与执行分离"：`WHERE`/`INSERT`/`UPDATE` 等 SQL 语句由 cmx-dct-model 的 `build_*` 函数产出，本层拿到 `(sql, params)` 后通过 `cmx-database-pg`（tokio-postgres 路线）执行，并叠加事务生命周期管理。物理表（`cf_*`）由模型中心（model_center）部署创建，本层不含任何 DDL——这与 cmx-doc-store-pg（sqlx + tokio-postgres 双驱动）形成对比：DCT 只走 tokio-postgres 单通道。

对外只暴露**场景入口**（一步到位，内部 `resolve_dict` 解析视图）：读侧 `dict_meta`（元数据文档）/ `dict_search`（分页读）/ `dict_search_zmc`（零拷贝列式读）；写侧 `dict_upsert`（单行/批量 merge）/ `dict_delete` / `dict_save`（changeset 事务回存）；另提供流式导入导出 `export_stream` / `import_stream` 与分级字典层级补偿入口 `recompute_dict_hierarchy`。内部模块（`resolve_dict`、`upsert`、`save`、`DictView` 等）均为 `pub(crate)`，调用方无需手动解析视图。

核心业务概念沿袭 cmx-dct-model：字典定位器 `DctQuery`（域/应用/模块/文件均可缺省，按 `dict` code 自动反查）；两种主键形态——整型自增 PK（服务端铸 52 位 JS 安全真号并回传 `idMap`）与 `code` 字符串 PK（NoID，不铸号）；自分级字典（`parentField` 指向本表，维护 `level_no`/`full_path`/`is_leaf` 层级列）。

写路径的关键机制：主键铸号（`mint_ids_for_inserts`）在列校验**之前**执行（空 `code` 先铸再过 NOT NULL）；配置了 `codeRule` 的字典经编码引擎 `mint_codes_batch` 批量铸业务编码，遇 UNIQUE 冲突自动清 `code` 重铸重试（上限 3 次）；`dict_save` 的 changeset 按删除→插入→更新三段序落库，updated 分支带 `update_time` 基线乐观锁（冲突返回 `SaveOutcome::Conflict` → HTTP 409）；分级字典在事务末尾用**递归 CTE** 一次重算受影响子树的层级列。

## 与其他 crate 的关系

**上游依赖**（本 crate 的 Cargo.toml）：

| 依赖 | 用途 |
| --- | --- |
| cmx-dct-model | DctQuery/DictView 领域模型 + 全部 SQL 文本构造（build_search_sql / build_upsert_sql_dv / build_batch_insert_sql 等）与值类型 coerce |
| cmx-database-pg | tokio-postgres 数据库管理器（查询/执行/事务路由，get_default_pg_db_manager） |
| cmx-model-meta | 定义仓库（DefRef 定位 DCT 定义 JSON、resolve_dict_file 文件自动解析、代数 version/generation） |
| cmx-biz | 列级校验（validate_insert_row / ValidateOptions）与 Violation 结构、DB 错误中文翻译（BizError::from_db_error） |
| cmx-traits | GlobalCodeMinter 编码引擎全局注入点（mint_codes_batch / record_gap_for_code 断号记录） |
| cmx-core | DataValue 值类型（PG 参数绑定） |
| cmx-api-types | Result 统一错误信封 |
| cmx-master-slave | HierService trait 定义（本 crate 为其提供 DCT 形状实现） |
| cmx-rowsource | ZmcDataSet 行源（dict_search_zmc 返回值） |
| csv / tokio / futures / bytes | 流式导入导出的 CSV 解析、异步 IO 与字节流 |

**下游使用者**（grep 各仓库 Cargo.toml 反查）：

| 使用者 | 引用方式 |
| --- | --- |
| cmx-dct-api | 直接依赖——HTTP handler 全部转发到本 crate 场景入口 |
| cmx-mdm/cmx-mdm-api、cmx-mdm/cmx-mdm-store-pg | 依赖本 crate 复用字典读写（MDM 主数据 CRUD 走 DCT 通道，激活器经 `Txn::External` 把字典写入纳入主事务） |
| cmx-master-slave | trait 反转关系：其定义 HierService trait，本 crate 的 DctHierService 实现"形状 B"（自引用单表） |
| cmx-portalservice / cmx-flowengine | 不直接依赖（经 cmx-platform-app 的 cmx-dct-api 间接触达） |

## 核心功能与特性

| 功能 | 说明 |
| --- | --- |
| 元数据解析 | `dict_meta` 返回可直接下发的字典元数据文档（含列清单/主键/编码规则/唯一键，camelCase） |
| 视图解析链 | resolve_dict 六步：DAM 归一化 → 代数比对 → 定义加载 → dictionaryTables 定位 → merge_columns（own fields + 动态 `*FieldSet` 引用 + fieldSetOrder 自定义段序）→ resolve_pk → TableSpec 构建 |
| TableSpec 缓存 | 校验规范缓存键含 `version#g{generation}`，定义代数推进自动失效重建 |
| 分页查询 | `dict_search`：filters 等值过滤 + q 模糊 + 排序 + 分页，parent_id 三态语义（None 不过滤 / Some(Null) 查根 / Some(v) 查 children） |
| 零拷贝列式读 | `dict_search_zmc` 返回 ZmcDataSet，COUNT 挂到 zmc.total，供主从联动直查 |
| 单行/批量 upsert | merge 语义，主键铸号 → 编码铸号 → 列校验 → 落库，回传 idMap 供前端换真号 |
| changeset 事务回存 | `dict_save` 按 deleted→inserted→updated 三段序单事务落库；桶匹配 dict_code → tableName → 单桶兜底 |
| 乐观锁 | updated 分支 `AND "update_time" = $n` 基线比对；UPDATE RETURNING ut 消 N+1；冲突返回 Conflict（handler 转 409） |
| 编码铸号 | codeRule 配置时经 GlobalCodeMinter 批量铸业务编码；UNIQUE 冲突清 code 重铸重试 ≤ 3 次 |
| 断号记录 | 删除行前查旧 code，经 `record_gap_for_code` 记断号（连号域 enable_gap 生效） |
| 层级级联重算 | 分级字典改动后递归 CTE（anchor 推自身 + subtree 展开后代）一次重算 level_no / full_path / is_leaf |
| 外部层级补偿 | `recompute_dict_hierarchy` 供外部直写路径（如 MDM 激活器）事后补算层级 |
| 事务归属 | `Txn::Auto`（函数自管事务）/ `Txn::External(id)`（路由到外部事务，供编排器把 DCT 写入纳入主事务） |
| 流式导出 | keyset 分页（`pk > $last` ORDER BY pk LIMIT N）+ mpsc 字节流；NDJSON / CSV（首批发表头） |
| 流式导入 | NDJSON 逐行 / CSV 一次性读入；每批 1000 行单事务；列校验失败与主键冲突跳过不中断，错误清单 ≤ 100 条 |
| CSV NULL 语义 | 无引号空 = NULL / 带引号空 = 空字符串（对齐 PG COPY）；Replace 模式前置 TRUNCATE RESTART IDENTITY |
| 服务端托管列 | SERVER_FILLED_COLS（NOT NULL 兕底列 backfill）/ SERVER_REPLACED_COLS（id/create_time/update_time）；批量导入尊重用户提供的时间戳（迁移语义），与单行 upsert 的过滤策略形成行为分叉 |
| 解析链兼容性 | fieldSetOrder 悬空段忽略、清单外段按默认相对序补尾；无 fieldSetOrder 时保持旧行为（本表 fields 在前 + Common 头/Audit 尾重排） |
| 错误治理 | map_db_err：error 级只留 phase/dict_code/table/row_index/pg_detail，debug 级才记 SQL 全文；批量导入错误经 BizError 中文翻译 |
| HTTP 解耦 | 内部模块 pub(crate) 不对外暴露；HTTP 信封与参数提取由 cmx-dct-api 薄 handler 包装，错误助手 api_err / api_err_db 亦从本层导出 |

## 模块结构

```text
src/
├── lib.rs           # 场景入口 re-export；内部模块声明（pub(crate) 不对外）
├── error.rs         # DB 错误映射 map_db_err（脱敏日志）+ UNIQUE 冲突识别 + api_err/api_err_db
├── resolve.rs       # resolve_dict 六步解析链：DAM 归一化→定义加载→merge_columns→resolve_pk→TableSpec
├── meta.rs          # 对外类型：DictMeta 元数据文档 / SearchQuery（含 from_body、default_query）/ Sort / SearchResult
├── query.rs         # dict_search（分页读）与 dict_search_zmc（ZmcDataSet 列式读）执行
├── write.rs         # upsert / delete / save：铸号两段、列校验、乐观锁、Txn 事务编排、断号记录
├── hierarchy.rs     # 分级字典递归 CTE 层级重算 + 外部补偿入口 recompute_dict_hierarchy
├── import_export.rs # export_stream / import_stream：keyset 分页、mpsc 流、NDJSON/CSV、批次事务
└── hier_service.rs  # DctHierService：impl cmx-master-slave 的 HierService（形状 B 自引用单表）
```

源码规模分布（共约 3638 行）：write.rs（996）/ import_export.rs（780）/ resolve.rs（654）三大主战场占近七成——回存事务缩排、流式导入导出、元数据解析链是本 crate 的重心。

## 关键类型与 API

### 场景入口（lib.rs re-export，均为 pub）

```rust
// —— 读侧 ——
pub async fn dict_meta(q: &DctQuery) -> Result<DictMeta>;
pub async fn dict_search(q: &DctQuery, search: &SearchQuery, db_id: &str) -> Result<SearchResult>;
pub async fn dict_search_zmc(q: &DctQuery, search: &SearchQuery, db_id: &str) -> Result<ZmcDataSet>;

// —— 写侧 ——
pub async fn dict_upsert(q: &DctQuery, body: Value, db_id: &str, txn: Txn) -> Result<UpsertOutcome>;
pub async fn dict_delete(q: &DctQuery, id: &str, db_id: &str, txn: Txn) -> Result<Value>; // {ok, deleted}
pub async fn dict_save(q: &DctQuery, body: &Value, db_id: &str, txn: Txn) -> Result<SaveOutcome>;

// —— 层级补偿 / 层级服务 ——
pub async fn recompute_dict_hierarchy(q: &DctQuery, ids: &[i64], db_id: &str, txn_id: &str) -> Result<bool>;
pub use hier_service::DctHierService; // impl cmx_master_slave::HierService

// —— 流式导入导出 ——
pub async fn export_stream(q: &DctQuery, db_id: String, fmt: ImportFormat,
    batch_size: i64, buffer: usize) -> Result<tokio::sync::mpsc::Receiver<bytes::Bytes>>;
pub async fn import_stream<R: tokio::io::AsyncRead + Unpin>(q: &DctQuery, db_id: String,
    fmt: ImportFormat, mode: BatchConflictMode, batch_size: usize, data: R) -> Result<ImportSummary>;
```

### 核心类型

```rust
// 事务归属：Auto 自管事务；External(id) 路由到外部事务（guard 由外部管理，本函数不自开不提交）
pub enum Txn { Auto, External(String) }

pub enum UpsertOutcome {
    Invalid(Vec<cmx_biz::errcode::Violation>),            // 列校验未过，一次回报全部
    Ok { affected: u64, id_map: Map<String, Value> },     // 临时 id → 铸得的真号
}

pub enum SaveOutcome {
    Invalid(Vec<cmx_biz::errcode::Violation>),            // 列校验未过
    Conflict,                                             // 乐观锁冲突（handler 转 409）
    Ok { affected: u64, updated_at: Vec<Value>, id_map: Map<String, Value> },
}

pub enum ImportFormat { Json /* NDJSON */, Csv }           // as_str / content_type / ext
pub enum BatchConflictMode { Upsert, InsertOnly, Replace } // re-export 自 cmx-dct-model

pub struct ImportSummary { pub total: u64, pub affected: u64, pub skipped: u64, pub errors: Vec<ImportError> }
pub struct ImportError { pub row: usize, pub col: Option<String>, pub message: String }

```

### 对外数据类型字段一览

`DictMeta`（camelCase 序列化，可直接下发前端）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| dictCode / dictName | String | 字典编码与显示名 |
| tableName | String | 物理表名（`cf_*`，由模型中心建表） |
| pk / idField / codeField / labelField | String | 主键列 / 整型 id 列 / 编码列 / 显示列 |
| parentField | String | 自分级父引用列（非分级为空） |
| selfHierarchy | bool | 是否自分级字典 |
| codeRule | Option<Value> | 编码规则（mode=auto 时写路径铸业务编码） |
| columns | Vec<Value> | 合并后的列清单（含 edit/display 等透传属性） |
| uniqueKeys | Vec<Value> | 唯一键约束清单 |

`SearchQuery`（查询参数，支持从 HTTP body 一次构造）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| filters | Map<String, Value> | 等值过滤（列名不在视图列白名单内时忽略，防注入） |
| q | Option<String> | 全局模糊匹配 |
| sort | Option<Sort> | 单列排序（field + desc） |
| page / page_size | u64 | 分页，default_query 为 1 / 500 |
| parent_id | Option<Value> | 三态：None 不过滤；Some(Null) 查根节点；Some(v) 查指定父的 children |

## 设计要点

- **SQL 生成与执行分离**：所有 SQL 文本由 cmx-dct-model 的纯函数构造（便于单测与复用），本层只做参数绑定、执行与事务编排——两层各自可独立演进。
- **单驱动通道**：仅依赖 cmx-database-pg（tokio-postgres）；对照 cmx-doc-store-pg 的 sqlx + tokio-postgres 双驱动（装载走 tokio 零拷贝、保存走 sqlx 事务 guard），DCT 读写都在同一通道内完成，值类型统一经 cmx-dct-model 的 `to_dv_by_col` coerce（NULL→NullTyped、整型列字符串数字→Int、TIMESTAMP/DATE 字符串解析），避免 PG prepare 的 OID 不匹配。
- **铸号时机前置**：主键铸号与编码铸号都发生在列校验**之前**——空 `code` 先铸再过 NOT NULL 校验；主键整型自增的行铸 52 位 JS 安全真号并回传 `idMap`，前端据此把临时行 id 换成真号。
- **三段序 + 级联收尾**：save_apply 严格按 deleted→inserted→updated 落库；删除记旧 parent（供层级重算取旧父分支），更新改 parent 时同时收集自身+旧父+新父；无冲突时事务末尾一次递归 CTE 重算受影响子树，保证幂等。
- **导入导出性能约束**（源码注释承诺）：导出每批 5000 行、导入每批 1000 行、峰值内存 ≤ 64MB；导出走 keyset 分页（`pk > $last`）避免深分页 OFFSET 扫描，mpsc 容量建议 8 防止内存堆积。
- **错误脱敏治理**：`map_db_err` 在 error 级日志只保留 phase/dict_code/table/row_index/pg_detail，SQL 全文仅在 debug 级输出；批量导入的 DB 错误经 `BizError::from_db_error` 翻译为中文提示后整批计入 skipped，不中断后续批次。

## 使用示例

### 1. 分页查询字典数据（dict_search）

```rust
use cmx_dct_store_pg::{DctQuery, SearchQuery, dict_search};

// 只按 dict code 定位（域/应用/模块/文件全缺省，resolve_dict 自动反查补全）
let q = DctQuery::by_code("currency");

// 从 HTTP body 构造查询（body 为 None 时等价 default_query：page=1, page_size=500）
let mut search = SearchQuery::from_body(Some(body));
search.filters.insert("status".into(), serde_json::json!("1")); // 等值过滤
search.q = Some("人民".into());                                  // 模糊匹配
search.sort = Some(cmx_dct_store_pg::Sort { field: "code".into(), desc: false });

let result = dict_search(&q, &search, "default").await?;
// result.rows: Vec<Value>；result.total / page / page_size 随行返回
```

### 2. changeset 事务回存 + 乐观锁处理（dict_save）

```rust
use cmx_dct_store_pg::{DctQuery, SaveOutcome, Txn, dict_save};

let q = DctQuery::by_code("gl_account");
// 前端 ChangeSetCollector 收拢的 changeset；桶 key 允许 dictCode / tableName / 单桶别名
let body = serde_json::json!({
    "saveMode": "merge",
    "changes": {
        "gl_account": {
            "inserted": [{ "code": null, "name": "银行存款" }], // 空 code 由编码引擎先铸再校验
            "updated":  [{ "id": 1001, "name": "库存现金",
                           "update_time": "2026-08-01T10:00:00" }], // 乐观锁基线
            "deleted":  [2002]
        }
    }
});

match dict_save(&q, &body, "default", Txn::Auto).await? {
    SaveOutcome::Ok { affected, updated_at, id_map } => {
        // id_map：前端临时行 id → 服务端铸得的 52 位真号
        tracing::info!(affected, ?updated_at.len(), "字典保存成功");
    }
    SaveOutcome::Conflict => {
        // update_time 基线不匹配 → handler 层映射 HTTP 409，提示用户刷新重试
    }
    SaveOutcome::Invalid(violations) => {
        // 列级校验失败（类型/长度/非空），一次回报全部违规
    }
}
```

### 3. 流式导出与批量导入（export_stream / import_stream）

```rust
use cmx_dct_store_pg::{
    BatchConflictMode, DctQuery, ImportFormat, export_stream, import_stream,
};

let q = DctQuery::by_code("bus_partner");

// 导出：keyset 分页（每批 5000 行）→ mpsc 字节流 → axum Body::from_stream 包装为响应
let rx = export_stream(&q, "default".into(), ImportFormat::Csv, 5000, 8).await?;
let body = axum::body::Body::from_stream(
    tokio_stream::wrappers::ReceiverStream::new(rx));

// 导入：NDJSON 逐行流式解析，每批 1000 行单事务；主键冲突跳过不中断
let data = tokio::fs::File::open("bus_partner.ndjson").await?;
let summary = import_stream(
    &q, "default".into(), ImportFormat::Json,
    BatchConflictMode::InsertOnly, 1000, data,
).await?;
// summary.total / affected / skipped / errors（错误清单最多前 100 条，含行号与列名）
```

### 4. 外部事务写入 + 分级字典层级补偿（Txn::External / recompute_dict_hierarchy）

```rust
use cmx_dct_store_pg::{DctQuery, Txn, dict_upsert, recompute_dict_hierarchy};

let q = DctQuery::by_code("dept");

// 编排器（如 MDM 激活器）持有主事务 guard：DCT 写入路由进同一事务，一荣俱荣一损俱损
let txn_id = main_txn.id().to_string();
let rows = serde_json::json!([{ "code": "D001", "name": "华东大区", "parent_id": 1 }]);
let outcome = dict_upsert(&q, rows, "default", Txn::External(txn_id.clone())).await?;

// 激活器绕过本 crate 直写 cf_* 表后，事后补算受影响子树的层级列
// （level_no / full_path / is_leaf 递归 CTE 重算；非分级字典返回 Ok(false)）
let touched: Vec<i64> = vec![1001, 1002];
let recomputed = recompute_dict_hierarchy(&q, &touched, "default", &txn_id).await?;
```

### 5. DctHierService 接入主从联动（impl cmx-master-slave::HierService）

```rust
use cmx_dct_store_pg::DctHierService;
use cmx_master_slave::HierService;

// 形状 B：自引用单表（parent_id 指回本表），DctHierService 实现三层服务
// - load：按 zmc 直查字典表构树（from_zmc_self_ref）
// - expand：expand_raw 展开下级，只取首父 id 并驼峰为 parentId（有回归测试锁键名）
// - save：内部走 write::save(merge)，乐观锁冲突向上抛"乐观锁冲突(409)"
let svc = DctHierService::new("basic", "dataplatform", "mdm", "default");
// 协调器（cmx-master-slave）按 HierService trait 驱动主从联动，无需感知 DCT 细节
```

## Features

本 crate 无 `[features]`，所有能力默认开启。
