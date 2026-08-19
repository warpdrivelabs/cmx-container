# cmx-model-deploy

> 模型中心 · 数据库初始化与模块部署层：把 DCT/DOC/RPT/SEED/MENU 定义编译成 `TableDefine` 真实建到目标库，并维护 5 张台账系统表（模块部署台账 / 部署历史 / 源 JSON 留档）。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-model-deploy` 承接模型中心「真实落库」的一半职责（另一半是 `cmx-model-meta` 的设计期元数据），原为 `cmx-api::handlers::portal::model_center` 内联代码，按 v6 评估建议抽出为独立 crate（对标 cmx-rpt / cmx-flow / cmx-dct / cmx-doc 的能力下沉），让 cmx-api 回归纯适配层、「无手写 SQL 例外」。

三条主流程（见 `docs/模型中心-数据库初始化与模块部署台账设计.md`）：

1. **db_state**：读目标库台账（`cmx_model_meta` / `cmx_model_module`）+ 扫描磁盘定义，组合出每模块每 kind 的 scenario（create / upgrade / current / retry / drift / downgrade / none），是前端部署工作台「模块 × 资源类型」矩阵的数据源。
2. **init_db**：在目标库建 5 张台账系统表 + 写 `cmx_model_meta` + 记 INIT 历史（真实建表；重复调用视为升级）。
3. **deploy**：把选中的定义编译成 `TableDefine`，用 `PgTableDefineExecutor` 建到目标库（additive-only：只加列/加索引，不 DROP），随后写对象台账 + `cmx_model_module` + `cmx_model_source`（源 JSON 留档）+ 部署历史。

本 crate 是「服务/落库」层，**不含 HTTP 提取器**：7 个公共函数 + `InitEvent` 由 `cmx-model-api` 的 portal handler 直接调用转发。错误类型直接用 `cmx-api-types::{Result, Error}`（与 cmx-api 同源，避免反向依赖成环）。

**关键约束**：建表现状对比只用数据库内省（`PgTableDefineExecutor` 内部走 `information_schema`，不读台账）；DDL 用 `txn_id=None`（PG DDL 自动提交），台账 DML 在事务内；失败经 `cmx_model_deploy_history` 状态可对账。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-core` | 建表模型：`TableDefine` / `ColumnDefine` / `IndexDefine` / `FieldType` / `IndexKind` / `DataValue` |
| `cmx-database` | `get_default_db_manager`：`query_sql` / `execute_sql` / `query_sql_with_datavalues` + 事务上下文 |
| `cmx-metadata` | DDL 执行与差异：`PgTableDefineExecutor` / `DdlDiff` / `PostgresDdlDialect` / information_schema 内省 |
| `cmx-model-meta` | 定义装载：`definitions::store::list_definitions` / `get_definition` 列出并读取 DCT/DOC/RPT 定义 JSON |
| `cmx-biz` | 种子菜单装载/写入（菜单部署复用业务库能力） |
| `cmx-traits` | `MenuDefinitionImporter` trait（`LocalMenuDefinitionImporter` 实现其 `apply_menu_definitions`） |
| `cmx-utils` | 雪花号（台账行 id）、`ConfigManager`（app_id） |
| `cmx-api-types` | 错误码与 `Result`（与 cmx-api 同源） |
| `sha2` / `chrono` / `serde` / `serde_json` / `tokio` / `tracing` / `anyhow` | 种子内容指纹、时间戳、JSON 台账组装、SSE mpsc 通道、日志 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-model-api` | workspace 依赖 | 7 个 portal handler（db-state / init / deploy × 普通+SSE）直接转发调用本 crate 公共函数 |

---

## 核心功能与特性

| 功能 | 入口函数 | 说明 |
|------|---------|------|
| 库状态矩阵 | `db_state(db_id)` | 库门闸（UNINITIALIZED / META_UPGRADE_REQUIRED / CURRENT）+ 模块发现（DB 台账 ∪ 磁盘定义 ∪ menu-only 补全）+ 每 kind cell 的 scenario 计算 |
| 数据库初始化 | `init_db(db_id, operator_id, operator_name)` | 建 5 张台账系统表 + UPSERT `cmx_model_meta` + INIT 历史，返回最新 db_state |
| 初始化计划预览 | `init_plan_stream(db_id, tx)` | 只读探测生成系统表执行计划（SSE 流式，不执行 DDL） |
| 流式初始化 | `init_db_stream(db_id, ..., tx)` | init_db 的 SSE 变体（connect/step/progress/done/error 事件） |
| 模块部署 | `deploy(db_id, items, operator_id, operator_name)` | 编译 → 建表/升级表 → 写台账/源留档/历史；items 按 DCT→DOC→RPT→SEED→MENU 优先级稳定排序 |
| 流式部署 | `deploy_stream` / `deploy_plan_stream` | deploy 的 SSE 变体 / 只读生成部署执行计划（预览顺序 == 执行顺序） |
| SEED 部署 | `deploy_seed_with_events(...)` | 扫描 `seed/*.json` → 前置校验表已建 → `PgSeedDataExecutor` 事务写种子 → 写台账（无文件则 skipped） |
| MENU 部署 | `deploy_menu_with_events(...)` | 扫描 `data/menu-pages/**` → 经 `LocalMenuDefinitionImporter` 写平台库（按 module_code 先删后插）→ 写台账 |
| 模块全量编译 | `compile_all_definitions_for_module(domain, app, module)` | 列出模块全部定义并逐个编译为 `TableDefine` 聚合返回 |
| 种子/菜单扫描 | `seed_scanner::scan_seed_files` 等 | SHA256 checksum + mtime 日期版本 + 行数统计（drift 判断依据） |
| 表差异报告 | `diff_report`（私有） | 基于 `DdlDiff` 引擎生成 no_change / create_table / upgrade_table + 列/索引/注释变更明细 |

### 台账系统表（LEDGER_TABLES，5 张）

| 表 | 职责 |
|----|------|
| `cmx_model_meta` | 库级门闸：meta_version / engine_version / status（一行 per db_id+app_id） |
| `cmx_model_module` | 模块级台账：每模块（domain/app/module）一行 |
| `cmx_model_module_kind` | 模块 × kind 明细：version / status / def_checksum / def_source |
| `cmx_model_deploy_history` | 部署历史：kind=INIT/DCT/DOC/RPT/SEED/MENU，action，status（executing/success/failed 可对账） |
| `cmx_model_source` | 源 JSON 留档：部署时的定义文件原文 |

### scenario 判定规则

- **DCT/DOC/RPT**（`scenario_of`，按版本号）：`status=failed` → `retry`；未装+有定义 → `create`；已装+无定义 → `current`；applied<latest → `upgrade`；applied>latest → `downgrade`；版本一致但 status=drift → `drift`；否则 `current`。
- **SEED/MENU**（`compute_seed_menu_cell`，无版本概念）：按文件聚合 SHA256 与库中 `def_checksum` 对比——checksum 一致 → `current`，不一致 → `drift`；用户可见版本取文件 mtime 日期（YYYY-MM-DD）。

---

## 模块结构

```text
cmx-model-deploy
├── src
│   ├── lib.rs                    # 公共 API 再导出 + 共享常量（META_VERSION/LEDGER_TABLES 等）+ 编译器单元测试
│   ├── compile.rs                # DCT/DOC/RPT 定义 JSON → TableDefine 编译器（私有，701 行）
│   ├── ledger.rs                 # 台账系统表 DDL、schema 检查、DB 读取辅助（私有，523 行）
│   ├── diff_report.rs            # 表差异报告（基于 DdlDiff 引擎，私有，302 行）
│   ├── db_state.rs               # db_state API + 模块发现 + cell/scenario 计算（私有，1003 行）
│   ├── init.rs                   # 数据库初始化流程 + InitEvent（pub，420 行）
│   ├── deploy.rs                 # 部署流程 deploy/deploy_stream/deploy_plan_stream（pub，905 行）
│   ├── seed_scanner.rs           # SEED/MENU 文件扫描 + SHA256 聚合（pub，251 行）
│   ├── menu_pages_adapter.rs     # 菜单页面 JSON → MenuDefinition 适配（pub，109 行）
│   └── deploy_seed_menu.rs       # SEED/MENU 部署编排（pub，457 行）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// lib.rs —— 公共 API（cmx-model-api 经 `cmx_model_deploy::xxx` 调用）
pub use db_state::db_state;
pub use deploy::{deploy, deploy_plan_stream, deploy_stream};
pub use init::{init_db, init_db_stream, init_plan_stream, InitEvent};
pub mod seed_scanner; pub mod menu_pages_adapter; pub mod deploy_seed_menu;

pub struct InitEvent {           // init.rs —— SSE 进度事件
    pub kind: String,            // "connect" / "step" / "progress" / "done" / "error"
    pub data: serde_json::Value, // 事件数据（字段随 kind 变化）
}

// db_state.rs
pub async fn db_state(db_id: &str) -> cmx_api_types::Result<serde_json::Value>;
// 返回顶层：db_id / initialized / meta_version / expected_meta_version / db_status /
//         page_mode / scenario_counts / installed_modules / modules
// modules 内每模块含 dct/doc/rpt/seed/menu 五个 kind cell（applied/latest/status/
// scenario/file/versions 等），前端据此渲染部署矩阵

// init.rs
pub async fn init_db(db_id: &str, operator_id: &str, operator_name: &str)
    -> cmx_api_types::Result<serde_json::Value>;
pub async fn init_plan_stream(db_id: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>);
pub async fn init_db_stream(db_id: &str, operator_id: &str, operator_name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>);

// deploy.rs（items: [{ kind, domain, application, module, file }]）
pub async fn deploy(db_id: &str, items: &[serde_json::Value],
    operator_id: &str, operator_name: &str) -> cmx_api_types::Result<serde_json::Value>;
pub async fn deploy_stream(db_id: &str, items: &[serde_json::Value],
    operator_id: &str, operator_name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>);
pub async fn deploy_plan_stream(db_id: &str, items: &[serde_json::Value],
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>);

// deploy_seed_menu.rs
pub fn infer_conflict_columns(def: &cmx_core::model::cell::TableDefine) -> Vec<String>;
pub async fn compile_all_definitions_for_module(domain: &str, app: &str, module: &str)
    -> cmx_api_types::Result<Vec<cmx_core::model::cell::TableDefine>>;
pub async fn deploy_seed_with_events(db_id: &str, domain: &str, app: &str, module: &str,
    operator_id: &str, operator_name: &str,
    tx: Option<&tokio::sync::mpsc::UnboundedSender<InitEvent>>)
    -> cmx_api_types::Result<serde_json::Value>;
pub async fn deploy_menu_with_events(/* 同上参数 */)
    -> cmx_api_types::Result<serde_json::Value>;

// seed_scanner.rs
pub struct ScannedFile {
    pub table_name: String,     // SEED: 物理表名（文件名 stem）；MENU: 空串
    pub rel_path: String,       // 相对路径（写入台账 def_source）
    pub content: String,        // 文件原始内容（免二次读盘）
    pub checksum: String,       // 内容 SHA256 hex（drift 判断依据）
    pub row_count: usize,       // SEED: 数组元素数；MENU: 树节点数
    pub modified_date: Option<String>, // YYYY-MM-DD（用户可见版本）
}
pub fn scan_seed_files(domain: &str, app: &str, module: &str) -> Vec<ScannedFile>;
pub fn scan_menu_files(domain: &str, app: &str, module: &str) -> Vec<ScannedFile>;
pub fn scan_all_menu_module_keys() -> Vec<(String, String, String)>; // menu-only 模块发现
pub fn aggregate_sha256(files: &[ScannedFile]) -> String;            // 模块级聚合指纹

// 共享常量（pub(crate)，经 lib.rs 文档化）
// META_VERSION: i32 = 2            台账结构版本（低于则 db_state 报 META_UPGRADE_REQUIRED）
// ENGINE_VERSION: &str = "1.0.0"
// VARCHAR_DEFAULT_LENGTH: u32 = 255  VARCHAR 未指定 fieldLength 时默认长度（避免建成 TEXT）
```

---

## 使用示例

### 一、初始化目标库并读取部署矩阵

```rust
use cmx_model_deploy::{init_db, db_state};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1) 初始化：建 5 张台账系统表 + 写 cmx_model_meta + INIT 历史
    //    内部：DDL 逐条自动提交（txn_id=None）→ 台账 DML 单事务 → 返回最新 db_state
    let state = init_db("primary", "u_001", "张三").await?;

    // 2) 或单独查询部署矩阵（库门闸 + 每模块每 kind 的 scenario）
    let state = db_state("primary").await?;
    println!("库状态: {}", state["db_status"].as_str().unwrap_or("?")); // CURRENT
    println!("场景统计: {}", state["scenario_counts"]);
    // modules 内每模块的 dct/doc/rpt/seed/menu cell 含 scenario 字段：
    //   create（待装）/ upgrade（可升级）/ current / retry（失败重试）/ drift（漂移重应用）
    Ok(())
}
```

### 二、部署一批定义到目标库

```rust
use cmx_model_deploy::deploy;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // items 内部按 kind_order 排序：DCT(0)→DOC(1)→RPT(2)→SEED(3)→MENU(4)
    // （保证 SEED 在 DCT/DOC 建表之后执行，MENU 最后同步）
    let items = vec![
        json!({ "kind": "DCT", "domain": "fi", "application": "cmxfico",
                "module": "gl", "file": "cmxfico_dct_meta_v1.json" }),
        json!({ "kind": "DOC", "domain": "fi", "application": "cmxfico",
                "module": "gl", "file": "cmxfico_doc_meta_v1.json" }),
        json!({ "kind": "MENU", "domain": "fi", "application": "cmxfico", "module": "gl" }),
    ];
    let result = deploy("primary", &items, "u_001", "张三").await?;
    // result 含 results（逐项部署结果）+ batch_id + db_state（最新矩阵）
    println!("batch_id: {:?}", result["batch_id"]);
    Ok(())
}
```

### 三、SSE 流式初始化/部署（cmx-model-api 转发模式）

```rust
use cmx_model_deploy::{deploy_stream, InitEvent};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // 与非流式 deploy 同源（都走 deploy_with_events），仅多一个 SSE 通道参数
    let (tx, mut rx) = mpsc::unbounded_channel::<InitEvent>();
    let items = vec![/* 同上 */];
    tokio::spawn(deploy_stream("primary", &items, "u_001", "张三", &tx));

    while let Some(ev) = rx.recv().await {
        match ev.kind.as_str() {
            "step" => println!("步骤: {}", ev.data["message"].as_str().unwrap_or("")),
            "progress" => println!("进度: {}", ev.data),
            "done" => { println!("完成，结果数: {}", ev.data["results"].as_array().map(|a| a.len()).unwrap_or(0)); break; }
            "error" => { eprintln!("失败: {}", ev.data["message"]); break; }
            _ => {}
        }
    }
}
```

### 四、扫描模块种子数据（drift 检测基础）

```rust
use cmx_model_deploy::seed_scanner::{scan_seed_files, aggregate_sha256};

fn main() {
    // 扫描 data/meta/definitions/fi/cmxfico/gl/seed/*.json
    let files = scan_seed_files("fi", "cmxfico", "gl");
    for f in &files {
        println!("表 {} · {} 行 · {} (mtime {})",
            f.table_name, f.row_count, f.checksum, f.modified_date.as_deref().unwrap_or("-"));
    }
    // 模块级聚合指纹（按文件路径排序后拼接计算，顺序无关）——与库中
    // cmx_model_module_kind.def_checksum 对比即可判断 SEED 是否 drift
    if !files.is_empty() {
        println!("聚合 SHA256: {}", aggregate_sha256(&files));
    }
}
```

---

## 关键设计决策

### 1. 为什么「建表现状对比」只走数据库内省？

部署时的 create/upgrade 判定不看台账（台账可能滞后或漂移），而是 `PgTableDefineExecutor` 内部实时查 `information_schema` 还原当前表结构，与设计期 `TableDefine` 做 `DdlDiff`。台账只做「装了什么版本」的记录与展示，真相在数据库本身。

### 2. DDL 与台账 DML 的事务边界

PG 的 DDL 是自动提交的，无法与 DML 同事务。因此约定：**DDL 用 `txn_id=None` 直接执行**；**台账写入（history / module / module_kind / source）在同一事务内**，失败可经 `cmx_model_deploy_history.status` 对账补偿（执行前置 executing 锚点，成功改 success，失败改 failed）。

### 3. additive-only：永不 DROP

`create_or_upgrade_table` 只加列 / 加索引，不做 DROP——业务库中的数据安全优先于结构完全一致。被删除的列保留在库中（diff 报告会列出 droppedColumns 供人工决策）。

### 4. 假阳性修复：bigint / timestamptz / 索引名错配

`diff_table_to_report` 复用 `DdlDiff` 引擎后修复了多类误报（lib.rs 内置回归测试覆盖）：PG 内省还原的 bigint 列（带派生 precision=64）与设计期 Int 列（precision=None）对比不报变更；索引名不同但列+类型相同判 no_change；列/表注释差异单独透出（commentChange / modifiedColumnComments）而非混入结构变更。

### 5. VARCHAR 默认长度 255

无 fieldLength 的 VARCHAR 若不指定长度会被 PG 建成 TEXT，与设计期望不一致时无法 ALTER 修正。编译器统一默认 `varchar(255)`（`VARCHAR_DEFAULT_LENGTH`），存量 TEXT 列会在 diff 中报 `TEXT → VARCHAR(255)` 修改项。

---

## 常见问题

### Q1: db_state 里 `installed_modules` 与 `modules` 有什么区别？

**A**: `installed_modules` 遍历 `cmx_model_module` 台账，每条代表「装过 ≥1 个 kind 的模块」，其未装 kind 的 cell 为 `status="none"` / `scenario="create"`；`modules` 遍历磁盘定义目录（外加 menu-only 反向发现的轻量模块，`_source="menu-only"`），每条带 `installed: bool`，供前端展示可创建/可升级面板。

### Q2: 部署 items 里 SEED/MENU 的 file 为什么可以为空？

**A**: DCT/DOC/RPT 靠 `file` 定位单个定义文件；SEED/MENU 的目标文件由扫描器按模块目录全量发现（`scan_seed_files` / `scan_menu_files`），`file` 字段仅作占位，部署时忽略。

### Q3: 初始化可以重复调吗？

**A**: 可以。`init_db` 幂等：已存在 `cmx_model_meta` 行时 reinit=true，action 从 `"create"` 变 `"upgrade"`，DDL 均为 `IF NOT EXISTS`，台账 UPSERT；`ensure_ledger_schema` 会应用台账升级补丁（`LEDGER_UPGRADE_DDL`）。

### Q4: 部署失败后如何对账？

**A**: 每次部署前先写 `cmx_model_deploy_history` 锚点（status=executing），成功改 success、失败改 failed；db_state 对 failed 的 kind cell 给出 `scenario="retry"`（最高优先级），前端引导重试。源 JSON 留档在 `cmx_model_source`，可追溯部署时的定义原文。
