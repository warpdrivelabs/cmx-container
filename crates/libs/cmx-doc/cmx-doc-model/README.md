# cmx-doc-model

> 业务单据（DOC）模块的语义中立层（DB-free）：把单据定义 JSON 解析为强类型 `DocMetaView`（层序/各层列/父子关系），提供富查询模型（`DocQuery`/`Filter`/游标）、公式求值（`formula`）、校验规则（`rule`）、层级 SELECT 生成（`sql_builder`），生成的 SQL 对 tokio-postgres 与 sqlx 双驱动通用。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-doc-model` 是 DOC 域三件套中的**领域模型层**，设计原则是「**全部 DB-free**」——本 crate 不依赖任何数据库驱动、不执行任何 SQL，只做纯逻辑：定义解析、查询建模、表达式求值、SQL 文本生成。执行与事务编排由下游 `cmx-doc-store-pg` 完成。

### 核心业务概念

- **单据（DOC）**：业务单据 = 多层主从结构。典型如财务凭证：凭证批（L1）→ 凭证头（L2）→ 科目行（L3），每层对应一张物理表（`cv_*` 前缀）。
- **层（Layer）与层序**：`voucherSchema.schema` 声明层序 L1..LN；**同层多表**（并列表）完整保真于 `layer_groups`，主链路 `layer_order` 每层取首表。
- **父子关系（Relation）**：`relations` 声明 parent/child 与键（`parentKey` 默认 `id`、`childKey` 默认 `upper_id`），子层装载时按 `child_key = ANY($n)` 驱动。
- **base 字段集**：`documentFieldSets` 引用公共字段集（id/upper_id/line_no 等），解析时与本表 `fields` 去重合并。
- **富查询**：`DocQuery` 每层独立一棵过滤树 + 多列排序 + offset/keyset 游标分页；列名经 schema 白名单校验，值按列 `data_type` 类型化成 `DataValue` 走参数绑定（`$N` 占位），零字符串拼接注入面。

### 「通用性铁律」

[sql_builder](src/sql_builder.rs) 模块**不认识任何具体单据**（不出现 `cv_batch`/`local_dr` 等词）：列名、类型、表名全部来自 `LayerView`/`ColumnView`，任何用到的列都先经白名单校验。这使同一套装载/查询内核服务任意单据定义。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-core` | 核心模型：`DataValue` / `Field` / `FieldType` / `Schema`（强类型列模型） |
| `cmx-biz` | `BizError` / `Result` + 落库前列级校验规范 `validation::TableSpec`（DOC/DCT 共享） |
| `cmx-utils` | JSON 辅助：`base_fieldset` 解析 base 字段集 |
| `serde` / `serde_json` | 定义 JSON 与 DTO 序列化 |
| `base64` | keyset 游标编解码（`Cursor`） |
| `chrono` | 日期解析（filter 值 RFC3339 归一） |
| `rust_decimal` | 高精度十进制（`FValue::Decimal` 列转换，保留财务精度） |
| `thiserror` | 错误类型派生（`ModelError` / `FormulaError`） |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-doc-store-pg` | `cmx-doc-model = { workspace = true }` | 装载器消费 `DocMetaView` + `build_layer_select` 生成 SQL；saver 消费 `json_to_dv_typed`；revision 消费 `dv_to_json` |
| `cmx-doc-api` | `cmx-doc-model = { workspace = true }` | handler 直接引用 `Filter` / `ColumnView` 等类型；`/doc/meta` 端点调 `project_doc_meta` |
| `cmx-portalservice` / `cmx-flowengine`（跨 workspace） | **不直接依赖** | 不直接使用；DOC 能力经 `cmx-doc-api` HTTP 端点间接暴露 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| `DocMetaView` 定义投影 | 弱类型定义 JSON → 强类型层级模型：层序/层级组/relations/各层列/汇总表/校验规则/状态机/版本化开关 |
| 同层多表保真 | `layer_groups` 保留每层全部并列表；`child_layers` 按 parentTable 声明精确过滤或回退全组同父 |
| 汇总表（sum 表）解析 | `voucherTables[i].summaries[]`：度量列（agg）+ 维度列（dimType）独立 Schema |
| 前端录入控件透传 | `ColumnView` 原样保留 `refDict`/`displayField`/`refField`/`edit`/`editSettings`/`display`，供前端动态列模型（cmx-dict-select 等）消费 |
| 富查询模型 `DocQuery` | 每层独立 filter 树（AND/OR 任意嵌套 + 14 种算子）、多列排序、limit/offset、keyset 游标 |
| keyset 游标 | `Cursor` base64(JSON) 编码「上一页末行排序列值 + id」，展开式谓词兼容混合升降序，稳定无漂移 |
| 层级 SELECT 生成 | `build_layer_select` / `build_layer_count`：`$N` 占位 + `DataValue` 绑定，tokio-postgres 与 sqlx 通用 |
| 注入防护 | filter/orderBy 列名先经 `schema.get_index` 白名单校验；标识符双引号包裹转义 |
| 后端公式求值 | 自带词法 + 递归下降解析 + 求值的安全表达式引擎（非图灵完备 DSL），Decimal 优先保精度 |
| 校验规则执行 | `validate` 对行 scope 逐条求值 `validationRules`：error 阻断 / warn 提示 / 不可求值跳过 |
| JSON ↔ DataValue 转换 | `json_to_dv_typed`（按 FieldType 宽松强转）/ `json_to_dv_loose` / `dv_to_json`，消除 saver/query/revision 三份重复 |
| 日期时间归一 | `parse_datetime_utc` 兼容 RFC3339 与无时区格式（按 UTC 解释）；`parse_naive_date` 短日期 |
| 前端元数据投影 | `project_doc_meta`：强类型视图 → 前端通用单据页 JSON（层/列/汇总/父子键） |

---

## 模块结构

```text
cmx-doc-model
├── src
│   ├── lib.rs           # 模块导出与公共 API re-export
│   ├── meta.rs          # DocMetaView/LayerView/ColumnView/RelationView/SummaryView/LevelGroup + project_doc_meta
│   ├── query.rs         # DocQuery/LayerQuery/Filter/Cond/Op/OrderBy/Cursor + json_to_datavalue
│   ├── formula.rs       # FValue/Scope + eval_formula/eval_bool（词法+递归下降+求值）
│   ├── rule.rs          # validate：validationRules 后端二次校验（Violation/ValidateResult）
│   ├── sql_builder.rs   # build_layer_select/build_layer_count：层级参数化 SELECT 生成
│   ├── codec.rs         # JSON ↔ DataValue 类型化转换（宽松策略，收口三份重复实现）
│   ├── datetime_util.rs # parse_datetime_utc/parse_naive_date：日期时间解析归一
│   └── error.rs         # ModelError/FormulaError + From → BizError 桥接
└── Cargo.toml
```

---

## 关键类型与 API

### meta.rs —— 定义投影

```rust
pub struct DocMetaView {
    pub module_code: String,                    // moduleMeta.moduleCode
    pub version: u64,
    pub layer_order: Vec<String>,               // 主链路层序（每层取首表）
    pub layers: Vec<LayerView>,                 // 含每层全部表（不止主链路）
    pub layer_groups: Vec<LevelGroup>,          // 同 level 下全部并列表
    pub relations: Vec<RelationView>,           // 父子关系
    pub validation_rules: Vec<serde_json::Value>, // §14.2 校验规则透传
    pub status_flow: Option<serde_json::Value>, // §14.1 状态机
    pub versioning: Option<serde_json::Value>,  // §6A 版本化开关
}

impl DocMetaView {
    pub fn parse(doc: &Value, base: &Value) -> Result<Self>;  // 定义 + base 字段集 → 强类型视图
    pub fn layer(&self, id_or_table: &str) -> Option<&LayerView>; // 按 schema id 或表名找层
    pub fn root_layer(&self) -> Option<&LayerView>;
    pub fn child_relations(&self, parent_id: &str) -> Vec<&RelationView>;
    pub fn child_layers(&self, parent_id: &str) -> Vec<&LayerView>; // 同父兄弟（parentTable 精确/回退）
    pub fn is_primary_in_group(&self, table_id: &str) -> bool;     // 层组主表（table_ids[0]）
    pub fn child_key_for(&self, parent_id: &str) -> String;        // 默认 "upper_id"
    pub fn child_key_for_child(&self, child_id: &str) -> Option<String>; // 懒下钻用
    pub fn is_state_editable(&self, state: &str) -> bool;  // §14.1：无状态机默认可编辑
    pub fn state_field(&self) -> Option<&str>;
    pub fn versioning_enabled(&self) -> bool;
}

pub struct LayerView {
    pub id: String,                    // 逻辑层 id（= tableName）
    pub table_name: String,            // 物理表名
    pub level: String,                 // L1/L2/...
    pub level_name: String,            // 层级显示名
    pub parent_table: String,          // 父表 id（空=回退上一层默认表）
    pub columns: Vec<ColumnView>,      // 本表 fields + documentFieldSets 展开（去重有序）
    pub summaries: Vec<SummaryView>,   // 汇总表
    pub agg_fields: Vec<String>,       // measure 且 agg 非空的列（可上卷列）
    pub schema: Arc<Schema>,           // 物理 Schema（Arc 共享，装载零拷贝复用）
    pub spec: Arc<cmx_biz::validation::TableSpec>, // 落库前列级校验规范
    pub code_rule: Option<serde_json::Value>,      // 编码规则挂载点声明
}

impl LayerView {
    pub fn column(&self, name: &str) -> Option<&ColumnView>;
    pub fn has_column(&self, name: &str) -> bool;  // schema 白名单
}

/// 一列的最小视图：建表/装载/回存必需属性 + 前端显示（caption/dimType/agg）
/// + 录入控件透传（ref_dict/display_field/ref_field/edit/edit_settings/display）。
pub struct ColumnView { /* name/data_type/nullable/is_primary_key/caption/dim_type/agg/... */ }

/// 前端元数据投影：DocMetaView → JSON（/doc/meta 端点数据源）。
pub fn project_doc_meta(meta: &DocMetaView) -> serde_json::Value;
```

### query.rs —— 富查询模型

```rust
pub struct DocQuery {
    pub layers: HashMap<String, LayerQuery>,  // 按层 id 指定（未指定层用默认）
    pub depth: Option<usize>,                 // 装载深度（None=全部层）
    pub include_siblings: bool,               // 是否装同父兄弟表（默认 true）
    pub only_parents: Option<Vec<Value>>,     // 懒下钻：只装这些父 id 的子树
    pub count_total: bool,                    // 根层多跑 COUNT(*) 挂到 total
}

impl DocQuery {
    pub fn from_json(v: &Value) -> Result<DocQuery>;                 // HTTP body 解析
    pub fn simple(root_layer_id: &str, limit: Option<u64>, depth: Option<usize>) -> DocQuery;
    pub fn layer(&self, layer_id: &str) -> LayerQuery;               // 缺省返回空指令
    pub fn validate(&self, meta: &DocMetaView) -> Result<()>;        // 全层列名白名单
}

pub struct LayerQuery {
    pub filter: Option<Filter>,
    pub order_by: Vec<OrderBy>,      // 元素 "col" 升序 / "!col" 降序
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub cursor: Option<Cursor>,      // keyset 游标（优先于 offset）
}

impl LayerQuery {
    pub fn validate_against(&self, layer: &LayerView) -> Result<()>; // 列名白名单（防注入）
}

/// 过滤树：AND 组、OR 组、或叶子（列 op 值），任意嵌套。
pub enum Filter { And(Vec<Filter>), Or(Vec<Filter>), Leaf(Cond) }
impl Filter {
    pub fn from_json(v: &Value) -> Result<Option<Filter>>; // 隐式 AND/等值简写/$or/$and
}

/// 14 种比较算子：Eq/Ne/Gt/Gte/Lt/Lte/In/NotIn/Like/Ilike/Contains/StartsWith/EndsWith/IsNull
pub enum Op { /* JSON 键 $eq/$ne/$gt/$gte/$lt/$lte/$in/$notIn/$like/$ilike/$contains/$startsWith/$endsWith/$null */ }

pub struct Cursor { pub vals: Vec<Value>, pub id: Value }
impl Cursor {
    pub fn decode(s: &str) -> Result<Cursor>;        // base64(JSON {vals,id})
    pub fn encode(vals: Vec<Value>, id: Value) -> String;
}

/// JSON 标量按列 data_type 类型化（严格路径，Result 返回，SQL 绑定前强校验）。
pub fn json_to_datavalue(col: &ColumnView, v: &Value) -> Result<DataValue>;
```

### sql_builder.rs —— SQL 生成

```rust
/// 构建一层 SELECT。parent_scope = Some((child_key, parent_ids)) 时
/// 生成 WHERE child_key = ANY($n)（子层驱动）。默认排序：子层 [child_key, line_no?, id]，根层 [id]。
pub fn build_layer_select(
    layer: &LayerView,
    lq: &LayerQuery,
    parent_scope: Option<(&str, &[DataValue])>,
) -> Result<(String, Vec<DataValue>)>;

/// 根层 COUNT 查询：只复用 filter 下推（无 ORDER BY/LIMIT/OFFSET/cursor）。
pub fn build_layer_count(layer: &LayerView, lq: &LayerQuery)
    -> Result<(String, Vec<DataValue>)>;
```

### formula.rs / rule.rs —— 公式与校验

```rust
pub type Scope = HashMap<String, FValue>;
pub enum FValue { Num(f64), Decimal(Decimal), Str(String), Bool(bool), Null }

pub fn scope_from_json(row: &serde_json::Value) -> Scope;  // 数值字符串优先 Decimal
pub fn eval_formula(expr: &str, scope: &Scope) -> Result<FValue, ModelError>;
pub fn eval_bool(expr: &str, scope: &Scope, fallback: bool) -> bool;

pub struct Violation { pub code: String, pub message: String, pub severity: String, pub level: Option<String> }
pub struct ValidateResult { pub ok: bool, pub violations: Vec<Violation>, pub skipped: usize }
impl ValidateResult { pub fn has_error(&self) -> bool; }

pub fn validate(rules: &[Value], scope: &Scope) -> ValidateResult;
```

### codec.rs / datetime_util.rs / error.rs

```rust
pub fn json_to_dv_typed(ft: &FieldType, v: &Value) -> DataValue; // 按目标列类型宽松强转（非 Result）
pub fn json_to_dv_loose(v: &Value) -> DataValue;                 // 无类型信息兜底
pub fn dv_to_json(dv: &DataValue) -> Value;

pub fn parse_datetime_utc(s: &str) -> Option<DateTime<Utc>>; // RFC3339 优先 + 无时区按 UTC
pub fn parse_naive_date(s: &str) -> Option<NaiveDate>;

pub enum ModelError { Formula(FormulaError), Parse(String) }  // From → BizError 桥接
pub enum FormulaError { DivByZero, Arity { name, expected, actual }, UnknownFunction(String), UnknownOperator(String), Eval(String) }
```

---

## 使用示例

### 场景一：解析单据定义 + 遍历层级

```rust
use cmx_doc_model::{DocMetaView, project_doc_meta};
use serde_json::json;

// doc：单据定义 JSON（含 moduleMeta / voucherSchema / voucherTables）
// base：base 字段集 JSON（含 fieldSets，供 documentFieldSets 展开；无则传 Value::Null）
let doc = json!({
    "moduleMeta": { "moduleCode": "cmxfico", "version": 1 },
    "voucherSchema": {
        "schema": [
            [ { "id": "cv_batch",  "level": "L1", "levelName": "凭证批" } ],
            [ { "id": "cv_header", "level": "L2", "levelName": "凭证头" } ]
        ],
        "relations": [
            { "parent": "cv_batch", "child": "cv_header", "parentKey": "id", "childKey": "upper_id" }
        ]
    },
    "voucherTables": [ /* ... */ ]
});

// 一次解析得到强类型视图（列 = 本表 fields + base 字段集合并去重）
let meta = DocMetaView::parse(&doc, &serde_json::Value::Null)?;

// 主链路层序 + 根层 + 子层推导
assert_eq!(meta.layer_order, vec!["cv_batch", "cv_header"]);
let root = meta.root_layer().unwrap();          // cv_batch
let kids = meta.child_layers("cv_batch");       // 同父兄弟表（parentTable 精确/回退全组）
let child_key = meta.child_key_for("cv_batch"); // "upper_id"（默认）

// 投影成前端通用单据页 JSON（/doc/meta 端点数据源）
let front_meta = project_doc_meta(&meta);
```

### 场景二：富查询 JSON → 参数化 SQL（DB-free）

```rust
use cmx_doc_model::{DocQuery, build_layer_select};
use serde_json::json;

let meta = DocMetaView::parse(&doc, &base)?;
// POST body 富查询：每层独立条件/排序/分页
let dq = DocQuery::from_json(&json!({
    "depth": 2,
    "layers": {
        "cv_batch": { "filter": { "period_code": "2026",
                                   "$or": [ { "status": "posted" }, { "status": "draft" } ] },
                      "orderBy": ["!posting_date"], "limit": 50 }
    }
}))?;

// 校验：列名必须在该层 schema 内（白名单，防注入；不在 meta 的层宽松忽略）
dq.validate(&meta)?;

let layer = meta.layer("cv_batch").unwrap();
let lq = dq.layer("cv_batch");
// 生成参数化 SQL：$N 占位 + DataValue 绑定值（tokio-postgres 与 sqlx 通用）
let (sql, params) = build_layer_select(layer, &lq, None)?;
// sql ≈ SELECT "id", ... FROM "cv_batch" WHERE (...) ORDER BY "posting_date" DESC, "id" ASC LIMIT 50
```

### 场景三：子层下钻 SQL（parent_scope 驱动）

```rust
use cmx_doc_model::{DocQuery, build_layer_select};
use cmx_core::model::cell::DataValue;

let meta = DocMetaView::parse(&doc, &base)?;
let header = meta.layer("cv_header").unwrap();
let lq = DocQuery::default().layer("cv_header");

// 父作用域：装载 upper_id ∈ {1, 2} 的子行（子层驱动）
let parent_ids = vec![DataValue::Int(1), DataValue::Int(2)];
let (sql, params) =
    build_layer_select(header, &lq, Some(("upper_id", &parent_ids)))?;
// sql ≈ SELECT ... FROM "cv_header" WHERE "upper_id" = ANY($1)
//        ORDER BY "upper_id" ASC, "line_no" ASC, "id" ASC
```

### 场景四：后端二次校验（借贷平衡）

```rust
use cmx_doc_model::{validate, scope_from_json};
use serde_json::json;

// validationRules 来自单据定义（severity=error 阻断保存，warn 仅提示）
let rules = vec![json!({
    "code": "balance", "expr": "total_dr == total_cr",
    "message": "借贷不平", "severity": "error", "level": "L2"
})];

// 行对象 → 求值 scope（数值字符串 "100" 优先按 Decimal 解析保精度）
let scope = scope_from_json(&json!({ "total_dr": 100, "total_cr": "100" }));
let result = validate(&rules, &scope);
assert!(result.ok);                  // 无 error 违规
assert_eq!(result.skipped, 0);       // 表达式全部可求值

// 不可求值（描述性文本/跨层聚合未铺平）→ 跳过不误判
let bad_rules = vec![json!({ "code": "x", "expr": "sum(lines.amount) == 0" })];
let r2 = validate(&bad_rules, &scope);
assert!(r2.ok && r2.skipped == 1);
```

### 场景五：changeset 值类型化（回存绑定）

```rust
use cmx_doc_model::{json_to_dv_typed, dv_to_json};
use cmx_core::model::cell::{DataValue, FieldType, SqlTypeMarker};
use serde_json::json;

// 前端 changeset 中 id/数值常以 JSON 字符串出现，目标列可能是 BIGINT/DECIMAL：
// 按目标列 FieldType 强转，避免 PG "bigint = text" 类型不匹配。
assert_eq!(json_to_dv_typed(&FieldType::Int, &json!("1000000001")), DataValue::Int(1000000001));
// 空白字符串/JSON null → 带类型的 NULL（配合 $p::bigint 等强转占位符）
assert_eq!(json_to_dv_typed(&FieldType::Int, &json!(null)),
           DataValue::NullTyped(SqlTypeMarker::Int));
// Decimal 列走 rust_decimal 保精度
assert!(matches!(json_to_dv_typed(&FieldType::Decimal, &json!(3.14)), DataValue::Decimal(_)));

// 反向：版本快照回放 DataValue → JSON
assert_eq!(dv_to_json(&DataValue::Int(42)), json!(42));
```

---

## Features 说明

本 crate 无 `[features]` 配置（`[dev-dependencies]` 仅含 tokio 测试工具）。对比：DCT 侧的 `cmx-dct-model` 有 `openapi` feature，DOC 侧的 handler 参数（`DocDataQuery` 等）在 `cmx-doc-api` 内直接派生 `utoipa::IntoParams`，故本 crate 无需 OpenAPI 开关。
