# cmx-master-slave

> 后端主从协调器：前端 `CmxMasterSlave.js`（cmx-data-comp 组件库）的 Rust 对等物——一个**业务无感知、任意层级**的主从（主表-明细）数据结构引擎，只认 schema 路径树 + relations + aggregations，不认任何 `cv_*`/`cf_*` 业务术语。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-master-slave` 把前端 `CmxMasterSlave`（packages/cmx-data-comp/src/lib/cmx-master-slave.js）的数据类职责——层级 schema、树装配、按路径收集、层间汇总上卷、变更集落库——在服务端重新实现一遍，作为**跨端一致的权威实现**：同一份 schema 定义与变更集 JSON，前端算出什么，后端重算就是什么。

### 解决什么问题

- **前后端汇总一致性**：主从单据（凭证/订单）保存前要把明细金额上卷到主表承接字段。前端有一套 `AGG_FUNCS` + `_cascade` 实现，若后端各写各的，跨端对拍必然漂移。本 crate 的聚合语义（含 JS `Number(v) || 0` 数值强转）与前端**逐字对齐**。
- **业务与结构解耦**：协调器只认「路径、父子关系、汇总规则」这套中立视图，由 DOC/DCT 的定义 JSON 解析而来。换一种单据不需要改协调器，换一种存储不需要改协调器。
- **服务可换（依赖反转）**：trait `HierService` 定义在本 crate，实现在服务侧（`cmx-doc-store-pg` / `cmx-dct-store-pg`）。**是服务依赖协调器，不是协调器依赖服务**——正如前端换数据源是换 JS 适配器，后端换服务是换 `impl HierService`，协调器一字不改。

### 设计要点

- **近叶子依赖**：仅依赖 `cmx-rowsource`（零拷贝数据集 `ZmcDataSet`）+ serde 系，不依赖任何 DB 驱动 / 业务 crate，可被任意服务引入。
- **两种拓扑形状**：形状 A `PathTree`（DOC：异构多表，父子经跨表 FK `upper_id`，childRows 嵌套装载）；形状 B `SelfRef`（DCT：单表自引用 `parent_id`，扁平装载成树）。
- **写时上卷为服务端权威**：`save_via` 落库前先 `rollup_changeset` 重算父层承接字段，服务侧只管校验/铸号/事务，汇总由协调器裁定。
- **arena 树**：`MsTree` 用 `Vec<Node>` + 索引避免 `Rc<RefCell>`，行数据转可变 JSON（`Map<String, Value>`）以便汇总回写。

---

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-rowsource` | 数据模型核心：`ZmcDataSet<R>` / `ZmcChildGroup<R>` / `ZmcRowSource` trait（零拷贝列式，与前端 `CmxDataSet.fromJSON` 同 wire） |
| `serde` / `serde_json` | schema 解析、变更集 JSON、行数据 |
| `thiserror` | `MsError` 错误定义 |
| `async-trait` | `HierService` 异步契约 |
| （dev）`tokio` / `uuid` / `chrono` / `rust_decimal` | 测试 mock 需实现 `ZmcRowSource` 全签名 |

### 下游使用方（谁 impl / 谁引用）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-doc-store-pg`（crates/libs/cmx-doc/） | `cmx-master-slave = { workspace = true }` | `DocHierService`：`impl HierService` 把 DOC 单据存储接为协调器可换后端；`tests/parity_ms.rs` 跨端对拍 |
| `cmx-dct-store-pg`（crates/libs/cmx-dct/） | `cmx-master-slave = { workspace = true }` | `DctHierService`：DCT 字典层级存储的同款适配；`tests/hier_service_pg.rs` 走真库验证 |
| cmx-container 之外 | — | 无跨 workspace 消费者（经 doc/dct 间接触达） |

### 在前端-后端对等架构中的位置

```text
前端 cmx-data-comp: CmxMasterSlave.js ──同语义──▶ 本 crate CmxMasterSlave
        │  ChangeSetCollector.export()                │  ChangeSet::from_json（同 JSON 结构）
        ▼                                            ▼
   doc-source.js / dct-source.js          HierService（trait，本 crate 定义）
                                                 ▲
                    cmx-doc-store-pg / cmx-dct-store-pg（impl 方，依赖反转）
```

---

## 核心功能与特性

| 功能 | 入口 | 一句话 |
|------|------|--------|
| schema 中立视图 | `HierSchema::from_json` | 解析点分路径树 + relations + aggregations，`validate` 校验自洽（路径唯一、引用存在、规则合法） |
| 双形状树装配 | `MsTree::from_zmc` / `from_zmc_self_ref` | 形状 A 嵌套 childRows / 形状 B 扁平自引用；孤儿行兜底为根（防丢数据，对齐前端 `buildTreeFromFlat`） |
| 平铺装载 | `CmxMasterSlave::set_flat_data` | `路径 → 行数组` 平铺多层装载，用 relations 的 child_key 自动建父子 |
| 按路径收集 | `MsTree::collect_path` / `descend_from` | 对齐前端 `_collectRows` / `_descendFromRow` |
| 读时上卷 | `rollup_read` / `rollup_in_place` | 对内存树现算汇总规则，前者返回结果不改树，后者原地回写承接字段 |
| 写时上卷 | `rollup_changeset`（经 `save_via` 自动调用） | 对变更集 inserted+updated 建树、上卷、承接字段写回各行的 `fields`（服务端权威） |
| 汇总拓扑排序 | `topo_sort` | `(from→to)` 规则建 DAG 排执行序，成环拒 `AggCycle`（对标前端 16 层安全阀） |
| 可换服务 | `HierService`（load/expand/save） | 泛型驱动任意后端；`load_via` / `expand_via` / `save_via` 编排 |
| JS 数值语义 | `value::as_f64` / `to_value` | `Number(v) \|\| 0` 强转、整数写回不带 `.0`，保跨端 golden 对拍 |

---

## 模块结构

```text
cmx-master-slave
├── src
│   ├── lib.rs          # 导出汇总 + VERSION 常量
│   ├── schema.rs       # HierSchema/Shape/LayerDef/RelationDef/AggRule/AggFn/Scope（中立视图）
│   ├── tree.rs         # MsTree：arena 树、from_zmc（A）/from_zmc_self_ref（B）/from_flat、路径收集
│   ├── coordinator.rs  # CmxMasterSlave 协调器本体：装载/下钻/保存/上卷编排
│   ├── agg.rs          # 层间汇总引擎（= cmx-agg）：topo_sort + rollup/rollup_read + 作用域解析
│   ├── changeset.rs    # ChangeSet/LayerChanges/SaveOutcome + rollup_changeset（写时上卷）
│   ├── service.rs      # HierService trait + LoadQuery（依赖反转插口）
│   ├── value.rs        # as_f64/to_value：JS 数值语义对齐
│   └── error.rs        # MsError（6 变体）/ Result
└── Cargo.toml
```

---

## 关键类型 / API

### schema（`src/schema.rs`）

```rust
/// 拓扑形状：serde tag = "kind"
pub enum Shape {
    PathTree,                              // 形状 A：异构，每层一表，跨表 FK（DOC upper_id）
    SelfRef { parent_field: String },      // 形状 B：同构，单表自引用（DCT parent_id）
}

pub struct LayerDef {
    pub path: String,                      // 完整点分路径，如 "head.items.taxes"
    pub table: String,                     // 物理表名（协调器只透传，绝不硬编码）
    pub pk: String,                        // 主键列名（默认 "id"）
    pub child_key: Option<String>,         // 指向父的 FK 列；根层为 None
    pub order_key: Option<String>,         // 排序列，如 line_no / sort_no
    pub derived: DerivedCols,              // 服务端派生列名（形状 B：full_path/level_no/is_leaf）
    pub agg_fields: Vec<String>,           // 承接汇总结果的列名清单（可空）
}

pub struct AggRule {                       // 与前端 AggregationRule 逐字对齐
    pub from: String,                      // 源层路径
    pub to: String,                        // 目标层路径
    pub field: Option<String>,             // 聚合字段（Count 可省）
    pub to_field: String,                  // 承接字段（serde rename "toField"）
    pub agg: AggFn,                        // Sum/Avg/Min/Max/Count
    pub scope: Scope,                      // Siblings（默认）/ All
}

impl HierSchema {
    pub fn from_json(v: &serde_json::Value) -> Result<Self>;  // 解析 + 自动 validate
    pub fn validate(&self) -> Result<()>;                      // 路径唯一 / 引用存在 / 规则合法
    pub fn layer(&self, path: &str) -> Option<&LayerDef>;
    pub fn roots(&self) -> Vec<&LayerDef>;                     // 不含 '.' 的层
    pub fn layer_order(&self) -> Vec<String>;                  // 拓扑序（按 path 段数升序，父在前）
}
```

### 协调器（`src/coordinator.rs`）

```rust
pub struct CmxMasterSlave { schema: HierSchema, tree: MsTree }

impl CmxMasterSlave {
    pub fn new(schema: HierSchema) -> Result<Self>;            // 校验 schema 自洽
    pub fn schema(&self) -> &HierSchema;
    pub fn tree(&self) -> &MsTree;

    /// 以 ZmcDataSet 装载（对齐前端 setDataSet）：按 schema 形状自动分派 A/B
    pub fn set_data_set<R: cmx_rowsource::ZmcRowSource>(&mut self, zmc: &ZmcDataSet<R>);
    /// 平铺多层行装载（对齐前端 setFlatData）：路径 → 行数组，relations 自动建父子
    pub fn set_flat_data(&mut self, rows_by_path: &HashMap<String, Vec<serde_json::Map<String, Value>>>);

    /// 经服务装载整棵树（对齐前端 loadDoc/loadDict）
    pub async fn load_via<S: HierService>(&mut self, svc: &S, query: &LoadQuery) -> Result<(), String>;
    /// 经服务懒下钻某层（对齐前端 loadDictChildren），返回子树由调用方决定并入
    pub async fn expand_via<S: HierService>(&self, svc: &S, layer_path: &str, parent_ids: &[String])
        -> Result<ZmcDataSet<S::Row>, String>;
    /// 保存变更集：先写时上卷（权威）再交服务落库
    pub async fn save_via<S: HierService>(&self, svc: &S, changes: ChangeSet) -> Result<SaveOutcome, String>;

    pub fn rollup_read(&self) -> Result<Vec<ReadResult>>;      // 读时上卷（不改树）
    pub fn rollup_in_place(&mut self) -> Result<()>;           // 内存上卷，原地回写承接字段
    pub fn flat_data(&self) -> HashMap<String, Vec<Map<String, Value>>>;  // 对齐前端 getFlatData
}
```

### 可换服务插口（`src/service.rs`）

```rust
pub struct LoadQuery {                       // 对齐前端 DocQuery 的中立子集
    pub root_filter: Map<String, Value>,     // 根层过滤（列名 → 值）
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub depth: Option<usize>,                // 装载深度（None = 全部）
    pub count_total: bool,
}

#[async_trait]
pub trait HierService: Send + Sync {
    type Row: ZmcRowSource;                  // 服务驱动的零拷贝行（如 TokioPgRowSource）
    async fn load(&self, schema: &HierSchema, query: &LoadQuery) -> Result<ZmcDataSet<Self::Row>, String>;
    async fn expand(&self, schema: &HierSchema, layer_path: &str, parent_ids: &[String])
        -> Result<ZmcDataSet<Self::Row>, String>;
    async fn save(&self, schema: &HierSchema, changes: &ChangeSet) -> Result<SaveOutcome, String>;
    // 注意：写时上卷已由协调器在 save_via 内完成，服务只管校验/铸号/事务/落库
}
```

### 变更集（`src/changeset.rs`）

```rust
pub struct LayerChanges {                    // 行对象 JSON 原样透传，不强解字段
    pub inserted: Vec<Value>,                // [{id, upper_id?, fields:{...}}]
    pub updated: Vec<Value>,                 // [{id, fields:{...}, baseline?}]
    pub deleted: Vec<Value>,                 // 主键值列表
}

pub struct ChangeSet {
    #[serde(flatten)]                        // JSON 顶层就是 {path: {...}}
    pub layers: HashMap<String, LayerChanges>,
}

impl ChangeSet {
    pub fn from_json(v: &Value) -> serde_json::Result<Self>;   // 前端 ChangeSetCollector.export() 的产出
    pub fn to_json(&self) -> Value;
}

pub struct SaveOutcome {                     // 前端 refreshBaselines 所需
    pub affected: u64,
    pub id_map: Map<String, Value>,          // temp id → real id（rename "idMap"）
    pub updated_at: Vec<Value>,              // 新乐观锁基线（rename "updatedAt"）
}

/// 写时上卷（saver 落库前的权威重算入口）：建树 → agg::rollup → 承接字段写回 fields + 顶层
pub fn rollup_changeset(schema: &HierSchema, cs: &mut ChangeSet) -> crate::Result<()>;
```

### 汇总引擎与错误（`src/agg.rs` / `src/error.rs` / `src/value.rs`）

```rust
pub fn rollup(tree: &mut MsTree, rules: &[AggRule]) -> Result<()>;          // 原地回写
pub fn rollup_read(tree: &MsTree, rules: &[AggRule]) -> Result<Vec<ReadResult>>;
pub fn topo_sort(rules: &[AggRule]) -> Result<Vec<usize>>;                  // 成环 → AggCycle

pub struct ReadResult { pub to: String, pub to_field: String, pub values: Vec<(String, Value)> }

pub enum MsError { DuplicatePath(String), UnknownPath(String), AggCycle(String),
                   InvalidRule(String), InvalidSchema(String), InvalidTree(String) }

pub fn as_f64(v: &Value) -> Number;          // JS Number(v) || 0 语义（NaN/非数/空串 → 0）
pub fn to_value(f: Number) -> Value;         // 整数写回 84 而非 84.0（对齐 JSON.stringify）
```

---

## 使用示例

### 一、定义凭证 schema + 平铺装载 + 读时上卷

```rust
use cmx_master_slave::{CmxMasterSlave, HierSchema};

// 1. schema：主表 head + 明细 items + 税费 taxes，明细借方上卷到主表（对齐 schema.rs 测试）
let schema = HierSchema::from_json(&serde_json::json!({
    "shape": { "kind": "path_tree" },
    "layers": [
        { "path": "head", "table": "cv_header" },
        { "path": "head.items", "table": "cv_acc_line", "child_key": "upper_id", "order_key": "line_no" },
        { "path": "head.items.taxes", "table": "cv_aux_line", "child_key": "upper_id" }
    ],
    "relations": [{ "parent": "head", "child": "head.items", "child_key": "upper_id" }],
    "aggregations": [
        { "from": "head.items", "to": "head", "field": "debit", "toField": "totalDebit", "agg": "sum" },
        { "from": "head.items.taxes", "to": "head.items", "field": "tax", "toField": "totalTax", "agg": "sum" }
    ]
}))?;

// 2. 平铺装载：路径 → 该层行数组（上层在前；FK 未命中/缺失的孤儿行兜底为根，不丢数据）
let mut rows = std::collections::HashMap::new();
rows.insert("head".to_string(), vec![
    serde_json::json!({"id": "h1", "voucher_no": "V001"}).as_object().unwrap().clone(),
]);
rows.insert("head.items".to_string(), vec![
    serde_json::json!({"id": "i1", "upper_id": "h1", "debit": 100}).as_object().unwrap().clone(),
    serde_json::json!({"id": "i2", "upper_id": "h1", "debit": 50}).as_object().unwrap().clone(),
]);

let mut ms = CmxMasterSlave::new(schema)?;
ms.set_flat_data(&rows);

// 3. 读时上卷：不落库，现算各 target 的承接值（串联规则经 topo_sort 保证 taxes→items 先于 items→head）
for rr in ms.rollup_read()? {
    // rr.to == "head"：values 含 ("h1", 150)；rr.to == "head.items"：各明细 totalTax
    println!("{}({}) = {:?}", rr.to, rr.to_field, rr.values);
}
```

### 二、impl HierService 把自己的存储接为可换后端

```rust
use cmx_master_slave::{HierService, HierSchema, LoadQuery, ChangeSet, SaveOutcome};
use cmx_rowsource::ZmcDataSet;
use async_trait::async_trait;

// 服务侧依赖反转落地（真实样例：cmx-doc-store-pg 的 DocHierService / cmx-dct-store-pg 的 DctHierService）
struct MyStore;

#[async_trait]
impl HierService for MyStore {
    type Row = cmx_database_pg::TokioPgRowSource;   // 换驱动 = 换关联类型，协调器不绑定

    async fn load(&self, schema: &HierSchema, query: &LoadQuery)
        -> Result<ZmcDataSet<Self::Row>, String> {
        // 按自己的定义翻译 query.root_filter/limit/offset/depth，返回根层 ZmcDataSet（含 childRows）
        todo!("翻译为你的 SQL 并打包 ZmcDataSet")
    }
    async fn expand(&self, schema: &HierSchema, layer_path: &str, parent_ids: &[String])
        -> Result<ZmcDataSet<Self::Row>, String> {
        todo!("大树懒下钻：只取这些父 id 的子层")
    }
    async fn save(&self, schema: &HierSchema, changes: &ChangeSet)
        -> Result<SaveOutcome, String> {
        // 收到的 changes 里承接字段已被 save_via 上卷为权威值——只管校验/铸号/事务落库
        todo!("执行你的 saver，返回 affected/idMap/updatedAt")
    }
}

// 编排：装载 → 内存上卷预览 → 保存（先 rollup_changeset 再交 save）
// let mut ms = CmxMasterSlave::new(schema)?;
// ms.load_via(&store, &LoadQuery::default()).await?;
// ms.rollup_in_place()?;                       // 试算预览，承接字段原地回写
// let outcome = ms.save_via(&store, changes).await?;
```

### 三、解析前端变更集 + 服务端权威写时上卷

```rust
use cmx_master_slave::{ChangeSet, HierSchema};
use cmx_master_slave::changeset::rollup_changeset;

// 前端 ChangeSetCollector.export() 的产出原样进来（顶层键 = schema 路径，serde flatten）
let mut cs = ChangeSet::from_json(&serde_json::json!({
    "head":       { "inserted": [{"id": "h1", "fields": {}}] },
    "head.items": { "inserted": [
        {"id": "i1", "upper_id": "h1", "fields": {"debit": 100}},
        {"id": "i2", "upper_id": "h1", "fields": {"debit": 50}}
    ]}
}))?;

// 写时上卷：用 inserted+updated 行建树 → 按拓扑序执行规则 → 承接字段写回行的 fields 与顶层
rollup_changeset(&schema, &mut cs)?;

// 落库前即可断言：head 行的 totalDebit 已是权威值 150（整数写回，不带 .0）
assert_eq!(
    cs.layers["head"].inserted[0]["fields"]["totalDebit"],
    serde_json::json!(150)
);
```

### 四、形状 B（DCT 单表自引用树）的 schema

```rust
use cmx_master_slave::HierSchema;

// 同构单表：层内经 parent_field 自引用成树；derived 声明服务端派生列
let schema = HierSchema::from_json(&serde_json::json!({
    "shape": { "kind": "self_ref", "parent_field": "parent_id" },
    "layers": [{
        "path": "dict", "table": "cf_gl_account",
        "child_key": "parent_id", "order_key": "sort_no",
        "derived": { "full_path": "full_path", "level_no": "level_no", "is_leaf": "is_leaf" }
    }]
}))?;
// set_data_set 时自动走 MsTree::from_zmc_self_ref：无父或父不在集内的行兜底为根
```

---

## Features 说明

本 crate 无 `[features]` 门控——作为近叶子结构引擎刻意保持零配置：不开 feature、不拉可选依赖，任何服务引入即得全量能力。

---

## 值得注意的实现细节

- **依赖边必须「层相等」而非「路径前缀」**：`produces_input_for` 判定规则 a 是否为规则 b 生产输入时，要求 `a.to == b.from && a.to_field == b.field`。若用前缀判断，会把「父层写」（acc 是 aux 的路径前缀）误判为「子层读」的输入，导致同一批逐层上卷规则被误判成环。
- **数值语义逐字对齐 JS**：`as_f64` 复现 `Number(v) || 0`（字符串 `"100"` → 100、`"nope"`/空串/null/数组 → 0、`true` → 1）；`to_value` 把整数值写回为 `150` 而非 `150.0`（对齐 `JSON.stringify`）。这是跨端 golden 对拍能过的前提。
- **孤儿兜底为根**：树装配时父键未命中的行不会静默丢弃，而是提升为根（对齐前端 `buildTreeFromFlat` 防丢数据）。
- **`rollup_changeset` 双写承接字段**：既写进行对象的 `fields` 子对象（供落库 saver），也同步行顶层（兼容无 `fields` 包装的行结构）。
