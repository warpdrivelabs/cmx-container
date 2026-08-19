# cmx-model-meta

> 模型中心元数据层：定义中心（DCT 数据字典 / DOC 业务单据 / BASE 字段集模板）、弹性组合规则引擎与字典检索引擎的设计期元数据建模服务——纯 JSON 文件存储，不落库。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-model-meta` 承接 CMX 模型中心的「meta 相关」元数据建模能力（迁移自 CMXPortalManager 的 Node 后端），覆盖平台的五项元数据：**BASE-DCT / BASE-DOC（公共字段集模板）、DCT（数据字典定义）、DOC（业务单据定义）、FC（弹性组合）**。它是「表定义 JSON → DDL 生成 → 数据库初始化 → 模块部署台账」链路的最上游：设计器产出定义 JSON，本 crate 负责存取、定位、校验与运行时派生。

三大子模块分工明确：

- **`definitions`（定义中心）**：DCT/DOC/BASE 定义文件的设计期 CRUD（`store`）、业务编码 → 定义文件解析（`resolve`）、DAM 坐标全局反查与定义树代数（`coord`）。
- **`flexible_combination`（弹性组合）**：FC 档案的文件 CRUD（`store`）、DRN 引用寻址（`drn`）、锚点评分合并规则引擎（`engine`）、use/pick overlay 展开（`overlay`）、domain-neutral 校验（`validator`）与端点编排（`api`）。
- **`dict`（字典检索引擎）**：字典 schema 注册表（`schema`）、条目内存检索（全文/模糊 CJK/Levenshtein/树形，`repo`）、SCD 停旧启新写入（`write`），被 `flexible_combination` 的维度元数据补全依赖。

**存储形态**：数据全部为 JSON 文件（`data/meta/definitions/**`、`data/meta/flexible-combination/<d>/<a>/<m>/<scenario>.json`、`data/dict/registry.json` + `data/dict/entries/**`），不写任何数据库；`data_root` 解析优先级为 portal 配置 → `CMX_PORTAL_DATA_ROOT` 环境变量 → `./data`。基础设施（`config`/`error`/`fsutil`/`util`）从 `cmx-jsonstore` 再导出，保持被迁移代码中 `crate::config` 等既有路径无需改动。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-jsonstore` | 共享基础设施（config / error / fsutil / util），并再导出 |
| `cmx-api-types` | `definitions::resolve` / `coord` 返回 `cmx_api_types::Result`（叶子 crate，无反向依赖，无环） |
| `tokio` / `serde` / `serde_json` | 异步文件 IO 与 JSON 序列化（`preserve_order` 保列序） |
| `chrono` | 版本时间戳 / 有效期 |
| `regex` | 字典/定义 ID 与编码校验 |
| `tracing` | 脏状态检测 warn（多 isDefault、代数变化等） |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-model-api` | workspace 依赖 | 模型中心 HTTP 层直接调用 definitions / flexible_combination 的 store 与 api 函数 |
| `cmx-model-deploy` | workspace 依赖 | `list_definitions` 列 DCT/DOC/RPT 定义 + `get_definition` 装载定义 JSON 供编译 |
| `cmx-dct-store-pg` | workspace 依赖 | 字典数据落 PG 时经 `definitions::coord` 做 DAM 坐标反查 |
| `cmx-doc-api` / `cmx-doc-store-pg` | workspace 依赖 | `/doc/*` 装载/回存前经 `coord` + `resolve_doc_file` + `store::get_definition` 定位单据定义 |
| `cmx-portalservice`（跨仓库） | path 依赖 `../cmx-container/crates/libs/cmx-model/cmx-model-meta` | `cmx-portal` crate `pub use cmx_model_meta::{definitions, dict, flexible_combination};` 整体再导出 |

> 本 crate 是把「定义定位逻辑」从 cmx-api / cmx-doc / cmx-dct 三方抽出共享的落点——三方都已依赖 cmx-model，解析器放这里可避免 `cmx-api ⇄ cmx-doc/cmx-dct` 依赖环。

---

## 核心功能与特性

| 功能 | 模块 | 说明 |
|------|------|------|
| 定义文件 CRUD | `definitions::store` | list（递归扫描 + summarize）/ get / save（补 updatedAt）/ delete（base 不可删）/ batch / set_default_version |
| 业务编码定位 | `definitions::resolve` | dictCode / moduleCode / base moduleCode → 定义文件名，(isDefault, version, file) 确定性排序 + RwLock 缓存 |
| DAM 坐标反查 | `definitions::coord` | 三段坐标可选缺失时按编码/文件名全局反查：唯一命中→坐标，多冲突→`Error::Conflict`(409) |
| 定义树代数 | `definitions::coord` | stat 指纹节流扫描 + 写路径即时 bump，派生缓存（file cache / CodeIndex）自动感知带外文件变更 |
| FC 档案 CRUD | `flexible_combination::store` | 按 DAM + scenario 四段定位（scenario 可带 `_v<N>` 版本后缀），多版本聚合摘要 |
| DRN 引用寻址 | `flexible_combination::drn` | `drn://kind/domain/app/module/name` 解析/归一/格式化/可见性判定/路径映射 |
| 规则评分合并 | `flexible_combination::engine` | `resolve_merged_rule`（锚点评分）、`build_columns`（_fieldToColumn 全派生 → CmxColumn.toJSON）、`build_members`（分组） |
| overlay 展开 | `flexible_combination::overlay` | 读时把 use/pick 规则按 DOC 物理列展开为 inline；deep_merge 覆盖合并 |
| schema 校验 | `flexible_combination::validator` | domain-neutral 结构校验（fields/groups/props），返回 diagnostics JSON |
| 字典 schema 注册 | `dict::schema` | `dict/registry.json` 注册表：load / get / try_get / register |
| 字典条目检索 | `dict::repo` | 内存引擎：全文 / 模糊 CJK / Levenshtein / 树形（parentId/ancestorId）/ 有效性过滤 / 分页排序 |
| 字典写入 | `dict::write` | 写入时计算 level/path/pathStr/sortKey/fullText + SCD 停旧启新（supersede / deactivate） |
| 多字典联查 | `dict::multi` | 父子 join + 分批并发（已废弃：唯一调用者下线，模块保留但不再导出） |

---

## 模块结构

```text
cmx-model-meta
├── src
│   ├── lib.rs                                # 模块导出 + 基础设施再导出（cmx_jsonstore::{config,error,fsutil,util}）
│   ├── definitions/                          # ── 定义中心 ──
│   │   ├── mod.rs                            #   子模块导出
│   │   ├── store.rs                          #   DefRef + 定义文件 CRUD + 批量读 + 设默认版本（797 行）
│   │   ├── resolve.rs                        #   业务编码 → 定义文件解析 + 三类 file cache（647 行）
│   │   └── coord.rs                          #   DAM 坐标反查 + CodeIndex + 定义树代数 generation（581 行）
│   ├── flexible_combination/                 # ── 弹性组合 ──
│   │   ├── mod.rs                            #   子模块导出
│   │   ├── store.rs                          #   FcRef + FC 档案文件 CRUD（392 行）
│   │   ├── api.rs                            #   resolve / rule / preview / validate 端点编排（409 行）
│   │   ├── defs.rs                           #   DRN → 定义装载 / 档案列表 / 依赖抽取（288 行）
│   │   ├── drn.rs                            #   DRN 解析 / FromDam / AbsDrn / 可见性（536 行）
│   │   ├── engine/                           #   运行时引擎（复刻 flexible-combination-engine.js）
│   │   │   ├── mod.rs                        #     Engine + resolve_merged_rule / build_members（595 行）
│   │   │   ├── column.rs                     #     _fieldToColumn 全派生 → CmxColumn.toJSON（618 行）
│   │   │   └── group.rs                      #     分组 CmxColumnGroup 构建
│   │   ├── overlay.rs                        #   use/pick overlay 展开 + deep_merge（431 行）
│   │   ├── dict_meta.rs                      #   维度 dict 元数据补全 enrich（223 行）
│   │   └── validator/                        #   domain-neutral 校验
│   │       ├── mod.rs                        #     validate_flexible_combination 主入口（389 行）
│   │       ├── fields.rs / groups.rs / props.rs  #  字段 / 分组 / 属性规则校验
│   └── dict/                                 # ── 字典检索引擎 ──
│       ├── mod.rs                            #   子模块导出（multi 已注释废弃）
│       ├── schema.rs                         #   DictSchema + registry.json 读写
│       ├── repo.rs                           #   SearchQuery + 条目内存检索引擎（451 行）
│       ├── tree.rs                           #   平铺 hits → 树形 toTreeResult / 分页 toPagedResult
│       ├── write.rs                          #   upsert / deactivate / supersede（SCD 停旧启新）
│       ├── api.rs                            #   search / suggest / batch_data / upsert_entries 端点编排
│       └── util.rs                           #   field_str 等取值助手
└── Cargo.toml
```

> `#![recursion_limit = "256"]`：derive 嵌套较深（HashMap/BTreeSet/serde_json::Value 多层泛型 + `#[serde(flatten)]`），默认 128 会触发编译期 `recursion limit reached`。

---

## 关键类型 / API

### definitions（定义中心）

```rust
// store.rs —— 定义文件引用与 CRUD
pub struct DefRef {
    pub domain: Option<String>,       // 业务域（如 fi / hr）；base 域特例
    pub application: Option<String>,  // 应用标识（与 app 等价，优先取 application）
    pub app: Option<String>,
    pub module: Option<String>,
    pub file: Option<String>,         // 文件名（与 id 等价，优先取 file）
    pub id: Option<String>,
    pub kind: Option<String>,         // "DOC" / "DCT" / "BASE"
}
pub async fn list_definitions(kind: Option<&str>, domain: Option<&str>,
    application: Option<&str>, module: Option<&str>) -> PortalResult<Vec<Value>>;
pub async fn get_definition(r: &DefRef) -> PortalResult<Value>;          // 缺失 → NotFound（含定位路径）
pub async fn save_definition(r: &DefRef, doc: &Value) -> PortalResult<Value>;
pub async fn delete_definition(r: &DefRef) -> PortalResult<Value>;       // base 域不可删
pub async fn get_definitions_batch(input: &Value) -> PortalResult<Value>; // 批量读 + 附 base 字段集
pub async fn set_default_version(r: &DefRef) -> PortalResult<Value>;     // 同 stem 切换 isDefault

// resolve.rs —— 业务编码 → 文件解析（错误统一 cmx_api_types::Error::business_error）
pub async fn resolve_doc_file(domain: &str, app: &str, module: &str,
    doc: Option<&str>) -> Result<String>;   // doc=moduleCode；None 时盲选默认/最高版本
pub async fn resolve_dict_file(domain: &str, app: &str, module: &str,
    dict: &str) -> Result<String>;          // 按 dictCode/tableName 逐候选匹配
pub async fn resolve_base_file(domain: &str, code: &str) -> Result<String>;
pub fn dict_matches(t: &Value, target: &str) -> bool;  // dictCode 或 tableName 命中
pub fn doc_matches(doc: &Value, target: &str) -> bool; // moduleMeta.moduleCode 命中
pub fn sort_candidates_by_default(candidates: &mut [String],
    entries: &[(String, String, bool, u64)]);          // isDefault↓ version↓ file↑

// coord.rs —— DAM 坐标反查与定义树代数
pub struct DamPartial { pub domain: Option<String>, pub application: Option<String>, pub module: Option<String> }
pub struct DamCoord { pub domain: String, pub application: String, pub module: String } // Ord 字典序稳定
pub struct CodeIndex { pub generation: u64, pub dct_by_code: HashMap<String, Vec<DamCoord>>,
    pub dct_by_table: HashMap<String, Vec<DamCoord>>, pub doc_by_code: HashMap<String, Vec<DamCoord>>,
    pub files: HashMap<(String, String), Vec<DamCoord>> }
pub async fn resolve_dam_by_code(kind: &str, code: &str, partial: &DamPartial) -> Result<DamCoord>;
pub async fn resolve_dam_by_file(kind: &str, file: &str, partial: &DamPartial) -> Result<DamCoord>;
pub async fn definitions_generation() -> u64;  // 当前定义树代数（2s stat 指纹节流扫描）
pub async fn bump_generation();                // 写路径即时 bump，触发派生缓存失效
```

### flexible_combination（弹性组合）

```rust
// store.rs
pub struct FcRef { pub domain: Option<String>, pub app: Option<String>,
    pub module: Option<String>, pub scenario: Option<String> } // scenario 可带 _v<N> 后缀
pub async fn get_flexible_combination(r: &FcRef) -> PortalResult<Value>;
pub async fn list_flexible_combinations(domain: Option<&str>, app: Option<&str>,
    module: Option<&str>) -> PortalResult<Vec<Value>>;
pub async fn save_flexible_combination(r: &FcRef, body: &Value) -> PortalResult<Value>;
pub async fn delete_flexible_combination(r: &FcRef) -> PortalResult<Value>;

// api.rs —— 端点编排（cmx-model-api 的 fc_* handler 委托到这里）
pub async fn resolve(r: &FcRef, query: &Map<String, Value>) -> PortalResult<Value>;
pub async fn rule(r: &FcRef, query: &Map<String, Value>) -> PortalResult<Value>;
pub async fn preview(body: &Value, r: &FcRef) -> PortalResult<Value>;
pub async fn validate(body: &Value, r: &FcRef) -> PortalResult<Value>;

// drn.rs —— DRN 引用寻址
pub const DRN_KINDS: [&str; 4] = ["DCT", "DOC", "FLC", "BASE"];
pub const DRN_VISIBILITY: [&str; 4] = ["private", "app", "domain", "public"];
pub struct Drn { /* kind/domain/app/module/name/version/visibility 等字段 */ }
pub fn parse_drn(input: &str) -> Result<Drn, String>;
pub fn normalize_drn(/* ... */) -> AbsDrn;     // 相对 DRN 按 FromDam 补全继承段
pub fn drn_to_path(abs: &AbsDrn, with_version: bool) -> String;
pub fn drn_visible_from(target_visibility: Option<&str>, target: &AbsDrn, from: &FromDam) -> bool;

// engine/mod.rs —— 运行时引擎
pub struct Engine<'a> { /* dimensions + rules + DRN 引用上下文 */ }
// 核心方法：resolve_merged_rule（锚点评分合并）、build_columns（CmxColumn.toJSON 形状）、
//          build_members（分组 CmxColumnGroup）、build_column_model_props

// overlay.rs
pub fn deep_merge(base: &Value, over: &Value) -> Value;
pub fn expand_rules_value<F>(rules: &Value, table_cols: Option<&F>) -> Value; // use/pick → inline

// dict_meta.rs
pub async fn enrich_flexible_combination_dict_meta(cfg: &Value) -> PortalResult<Value>;
```

### dict（字典检索引擎）

```rust
// schema.rs
pub struct DictSchema { /* id/name/domain/app/module/fields 等 */ }
pub async fn load_schemas() -> PortalResult<Vec<DictSchema>>;
pub async fn get_schema(dict_id: &str) -> PortalResult<DictSchema>;
pub async fn try_get_schema(dict_id: &str) -> PortalResult<Option<DictSchema>>;
pub async fn register_schema(body: &Value) -> PortalResult<Value>;

// repo.rs —— 内存检索引擎
pub struct SearchQuery { pub q: Option<String>, pub filters: Map<String, Value>,
    pub parent_id: Option<Value>, pub ancestor_id: Option<Value>, pub page: i64,
    pub page_size: i64, pub sort_field: Option<String>, pub sort_desc: bool,
    pub include_inactive: bool, pub as_of: Option<String> }
pub struct SearchResult { pub hits: Vec<Value>, pub total: usize }
impl SearchQuery { pub fn from_body(body: &Value) -> SearchQuery }
pub async fn search(dict_id: &str, query: &SearchQuery) -> PortalResult<SearchResult>;
pub async fn upsert_entries(dict_id: &str, entries: &[Value]) -> PortalResult<Value>;
pub async fn delete_entry(dict_id: &str, id: &str) -> PortalResult<Value>;

// tree.rs
pub fn to_paged_result(hits: Vec<Value>, total: usize, page: i64, page_size: i64) -> Value;
pub fn to_tree_result(/* hits → children 树形 */);

// write.rs —— SCD 停旧启新
pub async fn upsert(/* 计算 level/path/sortKey/fullText */);
pub async fn deactivate(/* 停用条目 */);
pub async fn supersede(/* 旧码停用 + 新码启用 */);
```

---

## 使用示例

### 一、读取与保存 DCT 定义（定义中心 CRUD）

```rust
use cmx_model_meta::definitions::store::{self, DefRef};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1) 四段定位（domain/application/module/file）读取字典定义
    //    文件落在 data/meta/definitions/fi/cmxfico/gl/cmxfico_dct_meta_v1.json
    let r = DefRef {
        domain: Some("fi".into()),
        application: Some("cmxfico".into()),
        module: Some("gl".into()),
        file: Some("cmxfico_dct_meta_v1.json".into()),
        ..Default::default()
    };
    let doc = store::get_definition(&r).await?;
    println!("字典表数: {}", doc["dictionaryTables"].as_array().map(|a| a.len()).unwrap_or(0));

    // 2) 保存修改（内部自动补 updatedAt；路径段经 is_safe_segment 防穿越）
    let saved = store::save_definition(&r, &doc).await?;
    println!("保存成功: {}", saved["message"].as_str().unwrap_or("ok"));
    Ok(())
}
```

### 二、业务编码定位定义文件（resolve，三方共享入口）

```rust
use cmx_model_meta::definitions::resolve;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 运行时只知道 dictCode，反查应装载的定义文件（isDefault 优先 → version 最大 → file 升序）
    let file = resolve::resolve_dict_file("fi", "cmxfico", "gl", "cf_client").await?;
    println!("cf_client → {file}"); // 如 "cmxfico_dct_meta_v1.json"

    // DOC 按 moduleCode 反查（None 时盲选默认/最高版本）；BASE 只需 domain="base" + moduleCode
    let doc_file = resolve::resolve_doc_file("fi", "cmxfico", "gl", Some("cmxfico_gl")).await?;
    let base_file = resolve::resolve_base_file("base", "base_dct_meta").await?;
    println!("{doc_file} / {base_file}");
    Ok(())
}
```

### 三、DAM 坐标可选化反查（coord，`/api/dct/*` 咽喉点用）

```rust
use cmx_model_meta::definitions::coord::{self, DamPartial};

#[tokio::main]
async fn main() {
    // 前端未传 domain/application/module 时，按 dictCode 全局反查补全坐标
    let partial = DamPartial::default(); // 三段均可选：传了就缩小范围
    match coord::resolve_dam_by_code("DCT", "cf_client", &partial).await {
        Ok(c) => println!("唯一定位: {}", c.display()),      // 如 "fi/cmxfico/gl"
        Err(e) => println!("零命中或多 DAM 冲突: {e}"),      // 多命中 → Error::Conflict(409) 枚举候选
    }
}
```

### 四、弹性组合 resolve（锚点评分合并规则）

```rust
use cmx_model_meta::flexible_combination::{api, store::FcRef};
use serde_json::Map;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 四段定位 FC 档案（scenario 可带 _v2 版本后缀）
    let r = FcRef {
        domain: Some("fi".into()),
        app: Some("cmxfico".into()),
        module: Some("gl".into()),
        scenario: Some("account".into()),
    };
    // 锚点维度键（如 gl_account=1001）作为查询条件，引擎按锚点评分合并多规则字段
    let mut query = Map::new();
    query.insert("gl_account".into(), serde_json::json!("1001"));
    let merged = api::resolve(&r, &query).await?;
    println!("合并列数: {}", merged["columns"].as_array().map(|a| a.len()).unwrap_or(0));
    Ok(())
}
```

### 五、字典条目检索（dict 引擎）

```rust
use cmx_model_meta::dict::repo::{self, SearchQuery};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从前端请求体构造查询（q=全文/模糊 CJK、filters=字段过滤、分页排序）
    let body = json!({ "q": "应收", "page": 1, "pageSize": 20 });
    let query = SearchQuery::from_body(&body);
    let result = repo::search("cf_gl_account", &query).await?;
    println!("命中 {}/{} 条", result.hits.len(), result.total);
    Ok(())
}
```

---

## 关键设计决策

### 1. 为什么 resolve/coord 放在 cmx-model-meta 而不是协议层？

`cmx-api`（业务编码定位 handler）、`cmx-doc-api`（单据定义装载）、`cmx-dct-api`（字典定义定位）三方都需要「编码 → 文件」解析。若放在任一协议 crate，其余两方要么重复实现，要么互相依赖成环。三方均已依赖 cmx-model，故解析器下沉到这里共享。

### 2. 确定性选版本：为什么候选文件要排序？

原实现从 `HashMap` 直接 collect 候选，迭代顺序不定导致多副本部署下不同节点可能选中不同文件、进程重启后选中不同文件。现统一按 **(isDefault 降序 → version 降序 → file 升序)** 排序，任意副本 / 任意重启收敛到同一份；同时 `warn_stem_multi_default` 检测「同 stem 多 isDefault=true」脏状态（多见于手工编辑 JSON）并 warn 提醒清理。

### 3. 定义树「代数」（generation）解决什么问题？

手动改定义 JSON（带外变更）无需重启、无需手动调接口即可被派生缓存感知：读路径每 2s（`GEN_SCAN_INTERVAL`）stat 指纹节流扫描比对，写路径即时 `bump_generation`；file cache / CodeIndex 等派生缓存发现代数变化即失效重建。

### 4. 为什么错误类型混用 PortalError 与 cmx_api_types::Error？

- `store`（纯文件 CRUD）：`PortalResult` / `PortalError`（来自 `cmx-jsonstore` 再导出，与迁移前 Node 后端语义对齐）。
- `resolve` / `coord`（三方共享的定位逻辑）：`cmx_api_types::{Result, Error}`——它是叶子 crate，`cmx-api` / `cmx-doc` / `cmx-dct` 都能直接 `?` 传播而无需转换；多 DAM 冲突用 `Error::Conflict` 映射 HTTP 409。

### 5. `dict::multi` 为什么废弃？

唯一调用者 `dict_multi_search` handler 已注释下线、暂无前端使用，模块代码保留但 `mod.rs` 中不再导出，待确认无回归价值后移除。
