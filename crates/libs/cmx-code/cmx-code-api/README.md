# cmx-code-api

> 通用业务编码引擎的 HTTP 层 + DB 访问层 + 引擎装配：`CodeModule` 聚合规则库 / 预览 / 生成 / 校验 / 断号端点（`/api/code/*`），`CodeEngine` 实现 `cmx-traits` 的 `CodeMinter` trait 供 DCT/DOC/MDM 钩子全局铸号。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

---

## 项目简介

`cmx-code-api` 是 CMX 平台**通用业务编码引擎**（cmx-code 域）的落地层。业务编码指
单据号、主数据 code 等企业级标识（如 `FV202608040002` = 固定前缀 + 日期 + 按日重置流水）。
本 crate 承担三件事：

1. **HTTP 协议皮肤**：薄 axum handler——提取参数 → 调 store/engine → `ApiResp` 信封。
   `CodeModule` 实现 `cmx-api-core` 的 `ModuleRoutes`，由组装层合并进主路由。
2. **DB 访问层（store）**：`cmx_code_rule` 规则表 CRUD、`cmx_code_gap` 断号表、
   `cmx_code_seq` 发号序列表、反查 max SQL（`PgAdvance`，实现 model 层的 `Advance` trait）。
3. **引擎装配（engine）**：组合 `cmx-code-model` 的段求值算法 + DB 推进器，对外暴露
   `mint`（单条铸号）/ `mint_batch`（批量取号）/ `preview`（预览不占号）；
   `CodeEngine` 实现 `cmx_traits::code::CodeMinter`，经 `GlobalCodeMinter::set` 全局注入，
   DCT 字典直存、DOC 单据保存、MDM 激活器三条链路都通过它取号——**钩子层不直接依赖
   本 crate**，避免环依赖。

### 核心业务概念

- **规则算法（`RuleSpec` → `cmx_code_rule` 表）**：纯段序列（const/serial/date/…），
  一条规则可被任意多个字典/单据复用。
- **挂载点声明（`CodeRule` → DCT/DOC 定义）**：引用规则码 + 声明 target 行为
  （写回列 field、auto/manual 模式、局部覆盖、级联回填）。
- **铸号两条路径**：`use_sequence=true` 走 `cmx_code_seq` 发号序列表（`FOR UPDATE`
  行锁，集群安全）；默认反查业务表 max + `minted_buffer` union（同事务多行连续不重）。
- **断号补偿（`enable_gap`，连号域才开）**：删行记断号（`record_gap`），下次铸号优先
  取断号填补（`take_gap`，原子 `DELETE ... RETURNING FOR UPDATE SKIP LOCKED`）。

> 设计文档：`.trae/documents/20260804_cmx-code_通用业务编码引擎设计方案.html`。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-code-model` | 纯逻辑层：`RuleSpec`/`Target`/`ResolveContext`/`Advance` trait/`rule_algo` 段求值/`pad` 补位 |
| `cmx-api-core` | API 框架：`ModuleRoutes` trait + `CmxAppState` + `CmxSvrContext` + `resolve_db_id_from_headers` |
| `cmx-api-types` | 统一响应 `ApiResp` |
| `cmx-database-pg` | PG 管理器（反查 max / 规则表 CRUD）+ `pg_detail` 错误明细抽取 |
| `cmx-traits` | `code::CodeMinter` trait 定义 + `GlobalCodeMinter` 全局注入点 |
| `cmx-core` | `DataValue`：SQL 参数强类型绑定 |
| `cmx-utils` | `next_pk_id`（规则表 id 生成） |
| `axum` / `serde` / `serde_json` / `tokio` / `async-trait` / `tracing` / `regex` | Web 框架 / 序列化 / 异步运行时 / manual pattern 校验 |

### 下游使用方（谁消费本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-code-api = { workspace = true }` | `routes.rs` 中 `.merge(CodeModule.routes())` 合并 11 个端点；`config/code.rs` 的 `init_code_engine()` 把 `Arc::new(CodeEngine)` 注入 `GlobalCodeMinter` |
| `cmx-dct-store-pg`（DCT 字典直存钩子） | **不直接依赖**（仅依赖 `cmx-traits`） | `write.rs` 经 `GlobalCodeMinter::get()` 取 minter 为字典 code 列铸号；UNIQUE 冲突时清空 code 重铸重试（上限 3 次） |
| `cmx-doc-store-pg`（DOC 单据保存钩子） | **不直接依赖**（仅依赖 `cmx-traits`） | `saver.rs` 经 `GlobalCodeMinter::get()` 为单据号（doc_no）铸号 |
| `cmx-mdm-store-pg`（MDM 激活器） | **不直接依赖**（仅依赖 `cmx-traits`） | `activation_service/activate.rs` 经 `GlobalCodeMinter::get()` 按字典 `dictMeta.codeRule` 铸主数据 code |
| `cmx-portalservice`（跨 workspace） | 经 `cmx-platform-app` 间接依赖 | 门户进程启动链完成注入并承载 `/api/code/*` 端点 |

```text
cmx-platform-app ──merge──► CodeModule.routes()   （HTTP 端点）
cmx-platform-app ──set────► GlobalCodeMinter ──get──► DCT/DOC/MDM 钩子
                                   │
                                   ▼
                          CodeEngine（本 crate engine.rs）
                                   │ 组合
              ┌────────────────────┴────────────────────┐
              ▼                                         ▼
      cmx-code-model（纯算法：段求值/补位）      store/（PgAdvance：反查 max/
                                                  规则表 / 断号表 / 序列表）
```

---

## 核心功能与特性

| 功能 | 端点 / API | 说明 |
|------|-----------|------|
| 规则库 CRUD | `GET/POST /code/rules`、`GET/PUT/DELETE /code/rules/{ruleCode}` | `cmx_code_rule` 表读写；delete 为软删除；请求头 DAM（域/应用/模块）补进规则与过滤 |
| 规则选优 | `store::rule_store::select_best` | 多条候选按 priority 取大；`query_rules` 供钩子层按挂载点解析 |
| 预览编码 | `POST /code/preview`、`/code/preview/batch` | 与定稿共用前缀构造 + `next_after`（预览码 = 定稿码），不落库不占号 |
| 权威生成 | `POST /code/generate`、`/code/generate/batch` | 事务内铸号；批量按 prefix 分组一次反查 max 取 N 个连续号（buffer 推进） |
| 批量取号双路径 | `engine::mint_batch` | `use_sequence=true` 走 `cmx_code_seq` 行锁原子取号段（首启从业务表探测基线）；默认反查 max + minted_buffer union |
| manual 校验 | `POST /code/validate` | 手敲码 pattern 正则校验（regex 编译失败 400） |
| 断号查询 / 手动取号 | `GET /code/gaps`、`POST /code/gaps/take` | 断号列表 + 手动填补（C6） |
| 删行记断号 | `CodeMinter::record_gap_for_code` | 反解析 code → (prefix, serial, width) 落 `cmx_code_gap`；仅连号域生效，失败仅 warn 不阻断删行 |
| 统一错误映射 | `handlers::err_resp` | 参数错 400 / 未找到 404 / 唯一冲突 409（从 PG DETAIL 抽冲突键给中文提示）/ 内部错 500 |
| OpenAPI | （暂无独立 ApiDoc 切片） | 端点由平台主文档聚合 |

---

## 模块结构

```text
cmx-code-api
├── src
│   ├── lib.rs              # CodeModule（impl ModuleRoutes）：11 条路由聚合（prefix = "code"）
│   ├── engine.rs           # 引擎装配：CodeEngine(impl CodeMinter) + mint/mint_batch/preview
│   │                       #   + record_gap_for_code_impl + mint_via_minter(_batch) 桥接
│   ├── handlers.rs         # axum handler：规则 CRUD 5 个 + preview/generate×4 + validate
│   │                       #   + gap_list/gap_take；Dam 提取 + err_resp 错误映射
│   └── store/              # DB 访问层
│       ├── mod.rs          #   模块导出（PgAdvance）
│       ├── rule_store.rs   #   cmx_code_rule CRUD + dam_where 维度过滤 + query_rules/select_best 选优
│       ├── serial_pg.rs    #   PgAdvance：反查 max SQL（LIKE + 长度 + 尾部纯数字三重约束）
│       │                   #     + minted_buffer union + take_gap 透传；try_insert 恒 Ok（只算号不落库）
│       ├── gap_store.rs    #   cmx_code_gap 断号表：record_gap / take_gap（原子 DELETE…RETURNING）/ query_gaps
│       └── seq_store.rs    #   cmx_code_seq 发号序列表：alloc_serial_segment（FOR UPDATE 取连续号段）
└── Cargo.toml
```

---

## 关键类型 / API

### 路由聚合与引擎（lib.rs / engine.rs）

```rust
pub struct CodeModule;
impl ModuleRoutes for CodeModule {
    fn routes(self) -> Router<CmxAppState> { /* 11 条 .route(...) */ }
    fn prefix() -> &'static str { "code" }
}

/// 编码引擎实例（实现 CodeMinter trait，供全局注入）。
pub struct CodeEngine;

// cmx_traits::code::CodeMinter 的实现（弱类型 Value 桥接）：
//   mint(code_rule, target, attrs, db_id, txn_id) -> Result<String, String>
//   mint_batch(code_rule, target, rows, db_id, txn_id) -> Result<Vec<String>, String>
//   record_gap_for_code(code_rule, code, attrs, db_id) -> bool

/// 单条铸号（方案 §4.4 mint_single）——委托 rule_algo::evaluate_segments。
pub async fn mint(rule: &RuleSpec, target: &Target,
                  ctx: &ResolveContext, advance: &dyn Advance) -> Result<String>;

/// 批量取号（方案 §4.5）：use_sequence 决定发号路径；补位/步长/reset_key 与单条一致。
pub async fn mint_batch(rule: &RuleSpec, target: &Target, ctx: &ResolveContext,
                        advance: &dyn Advance, count: usize,
                        txn_id: Option<&str>) -> Result<Vec<String>>;

/// 预览编码（不落库不占号，与定稿共用前缀构造保证一致）。
pub async fn preview(rule: &RuleSpec, target: &Target,
                     ctx: &ResolveContext, advance: &dyn Advance) -> Result<String>;

/// 创建 PgAdvance（handler 调用入口）。
pub fn pg_advance(db_id: &str, txn_id: Option<&str>) -> PgAdvance;
```

### store 层（部分签名）

```rust
// rule_store.rs
pub async fn create_rule(rule: &RuleSpec, db_id: &str) -> Result<()>;
pub async fn get_rule(rule_code: &str, db_id: &str, dam: &Dam) -> Result<RuleSpec>;
pub async fn list_rules(db_id: &str, dam: &Dam) -> Result<Vec<serde_json::Value>>;
pub async fn update_rule(rule_code: &str, rule: &RuleSpec, db_id: &str) -> Result<()>;
pub async fn delete_rule(rule_code: &str, db_id: &str, dam: &Dam) -> Result<()>;  // 软删除
pub async fn query_rules(rule_code: &str, db_id: &str, dam: &Dam) -> Result<Vec<RuleSpec>>;
pub fn select_best<'a>(candidates: &'a [RuleSpec], /* … */) -> Option<&'a RuleSpec>; // priority 取大

// gap_store.rs
pub async fn take_gap(prefix: &str, width: usize, db_id: &str,
                      txn_id: Option<&str>) -> Result<Option<i64>>;   // 原子取走
pub async fn record_gap(prefix: &str, serial: i64, width: usize, db_id: &str) -> Result<()>;
pub async fn query_gaps(prefix: Option<&str>, db_id: &str) -> Result<Vec<serde_json::Value>>;

// seq_store.rs
pub async fn alloc_serial_segment(rule_code: &str, prefix: &str, count: usize,
    start: i64, step: i64, probed_max: i64, width: usize,
    db_id: &str, txn_id: Option<&str>) -> Result<Vec<i64>>;  // FOR UPDATE 连续号段

// handlers.rs
pub struct Dam { pub domain_code: String, pub application_code: String, pub module_code: String }
```

### 反查 max SQL（serial_pg.rs，核心口径）

```sql
SELECT COALESCE(MAX(CAST(SUBSTRING("{field}" FROM {prefix_len} + 1 FOR {width}) AS BIGINT)), 0)
FROM "{table}"
WHERE "{field}" LIKE $1                        -- 'prefix%'
  AND LENGTH("{field}") = {prefix_len} + {width}  -- 长度精确匹配
  AND SUBSTRING("{field}" FROM {prefix_len} + 1) ~ '^[0-9]+$'  -- 尾部纯数字
```

---

## 使用示例

### 一、启动链全局注入（cmx-platform-app 场景，摘自 config/code.rs）

```rust
/// 初始化编码引擎全局注入：把 CodeEngine 注册为全局铸号器。
/// 未注入时所有 DCT/DOC/MDM 钩子跳过铸号（等价现状零影响）。
pub fn init_code_engine() {
    let engine = std::sync::Arc::new(cmx_code_api::engine::CodeEngine);
    if let Err(e) = cmx_traits::code::GlobalCodeMinter::set(engine) {
        tracing::warn!("编码引擎全局注入失败（可能重复初始化）：{e}");
    }
}
// 此后任何钩子层（不依赖 cmx-code-api）：
// if let Some(minter) = cmx_traits::code::GlobalCodeMinter::get() {
//     let code = minter.mint(&code_rule_json, &target_json, &attrs, db_id, None).await?;
// }
```

### 二、HTTP 调用序列（规则管理 → 预览 → 权威生成）

```text
1. POST /api/code/rules          # 建规则：固定段 FV + 日期流水段（按日重置、宽 4）
   body: { "ruleCode": "fv_doc_no", "ruleName": "凭证号",
           "segments": [ {"type":"const","value":"FV"},
                         {"type":"dateSerial","format":"YYYYMMDD","width":4} ] }
   → { "ruleCode": "fv_doc_no" }
   # 注：创建时请求头 DAM（domain_code 等）自动补进规则（body 未带时以请求头为准）

2. POST /api/code/preview        # 预览下一个号（不占号）
   body: { "ruleCode": "fv_doc_no", "target": { "kind": "doc", "code": "cv_header", "field": "doc_no" } }
   → { "code": "FV202608190001", "warning": "预览码非定稿，最终以保存时为准" }

3. POST /api/code/generate       # 权威生成（saver 钩子内实际落库时同款算法）
   → { "code": "FV202608190001", "ruleCode": "fv_doc_no" }

4. POST /api/code/generate/batch # 批量：rows 内不同 attrs 产生不同 prefix，各自独立取连续号段
   body: { "ruleCode": "fv_doc_no", "target": { … }, "rows": [ {}, {}, {} ] }
   → { "codes": ["FV202608190002", "FV202608190003", "FV202608190004"] }
```

### 三、在事务内批量铸号（引擎直调，DOC saver 场景）

```rust
use cmx_code_api::engine::{self, CodeEngine};
use cmx_code_model::context::ResolveContext;
use cmx_code_model::spec::Target;

async fn mint_doc_nos(db_id: &str, txn_id: &str) -> Result<Vec<String>, String> {
    // 钩子层弱类型入口：codeRule（挂载点声明）+ target + 每行 attrs
    let code_rule = serde_json::json!({
        "ruleCode": "fv_doc_no", "mode": "auto", "field": "doc_no"
    });
    let target = serde_json::json!({ "kind": "doc", "code": "cv_header", "field": "doc_no" });
    let rows = vec![
        serde_json::json!({ "org": "1000" }),
        serde_json::json!({ "org": "1000" }),
    ];

    // mint_via_minter_batch 内部：查规则表一次 → 每行算 prefix 分组
    // → 同组一次反查 max 取 N 连号（minted_buffer union 保证同事务不重）
    let codes = <CodeEngine as cmx_traits::code::CodeMinter>::mint_batch(
        &CodeEngine, &code_rule, &target, &rows, db_id, Some(txn_id),
    ).await.map_err(|e| e)?;
    Ok(codes)
}
```

### 四、删行记断号（连号域补偿闭环）

```rust
use cmx_code_api::engine::CodeEngine;

async fn on_row_deleted(code_rule: &serde_json::Value, code: &str,
                        attrs: &serde_json::Value, db_id: &str) {
    // 删除带流水号的行时调用（DOC/DCT 删除钩子）：
    // 1. 查规则表 + merge 挂载点局部覆盖
    // 2. 仅 enable_gap 生效（连号域）才继续
    // 3. parse_code_serial 反解 code = prefix + 流水值
    // 4. 落 cmx_code_gap（失败仅 warn，不阻断删行主流程）
    let recorded = <CodeEngine as cmx_traits::code::CodeMinter>::record_gap_for_code(
        &CodeEngine, code_rule, code, attrs, db_id).await;
    // recorded=true：下次铸号 take_gap 会优先把这个号补出去
}
```

---

## 常见问题

### Q1: 铸号为什么不落库（`try_insert` 恒 Ok）？

设计决策：DCT/DOC saver 的铸号发生在 apply_merge **之前**（钩子算号写回 changeset），
真正的 INSERT 由 saver 完成，业务表 UNIQUE 约束在那里兜底。因此 `evaluate_segments`
的重试循环在铸号阶段恒不触发，UNIQUE 冲突重试责任上移到 saver 层（捕获冲突 → 清空
code → 重新调 mint 取下一号，上限 3 次）。铸号函数**只算号不落库**。

### Q2: `use_sequence` 什么时候开？

默认 false（反查 max 老路径，向后兼容）。集群多副本部署且并发铸号高时开启：走
`cmx_code_seq` 发号序列表，`FOR UPDATE` 行锁保证取号原子不重（首启 current_val=0 时
从业务表探测真实 max 作基线）。必须在事务内调用（txn_id 非 None）。

### Q3: 规则端点为什么用了 Path Variable（`/code/rules/{ruleCode}`）？

本 crate 早于 MDM 的「禁用 Path Variable」API 约定（AGENTS.md §四 第 5 条）成型，
规则库是纯资源型 CRUD，沿用了 REST 风格路径参数；与 cmx-mdm-api 的 query/body 传参
约定不一致，属历史遗留——新端点建议遵循平台统一约定。
