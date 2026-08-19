# cmx-dct-model

> 数据字典（DCT）模块的语义中立层（DB-free）：字典表强类型视图（`DictView`/`DictColumn`）、请求坐标 DTO（`DctQuery`），以及全部纯逻辑——列白名单校验、主键铸号/临时 id 识别/自分级 `parent_id` 重指向、search / upsert / 批量导入导出的参数化 SQL 构造。生成的 SQL 用 `$N` 占位 + `DataValue` 绑定，由 `cmx-dct-store-pg` 执行。

![version](https://img.shields.io/badge/version-0.1.12-blue) ![rust-edition](https://img.shields.io/badge/rust--edition-2024-orange)

## 项目简介

`cmx-dct-model` 是 DCT 域三件套中的**领域模型层**，设计原则与 `cmx-doc-model` 同构：「**全部 DB-free**」——本 crate 不依赖任何数据库驱动、不执行任何 SQL，只做纯逻辑：定义投影、值类型 coerce、SQL 文本生成。执行与事务编排由下游 `cmx-dct-store-pg` 完成。

核心业务概念是**字典表视图（`DictView`）**：由 store 层的 `resolve_dict` 从定义 JSON（dictMeta + dictionaryTables + `*FieldSet` 字段集合并去重）构造，聚拢一张 `cf_*` 字典表的全部元数据——表名、主键（有 `id` 用 id，无则用 `code`）、编码/显示字段、自分级 `parent_field`、列清单、落库校验规范（`TableSpec`）、编码规则挂载点与业务唯一键。**两种主键形态**是重要分支：整型 `id` 主键的字典走**服务端铸号**（`pk_is_generated`），以 `code`(VARCHAR) 作主键的 NoID 字典（如 `cf_currency`）业务编码跨系统稳定、原样保留不铸号。**自分级字典**（`self_hierarchy`，如科目树）的子行通过 `parent_id` 引用父行，同批新增时需把指向"临时父 id"的引用重指向为铸号后的真号。

生成的 SQL 一律 `$N` 占位 + 参数绑定，列名全部经 `valid_col` 白名单校验（只允许定义中已知的列），杜绝 SQL 注入。值绑定走 `to_dv_by_col` 按列类型 coerce：NULL 带 `SqlTypeMarker` 类型（避开 tokio-postgres 裸 NULL 的 WrongType）、整型列字符串数字（`"1"`→Int）、TIMESTAMP/DATETIME/DATE 列 ISO8601/RFC3339 字符串解析——这是与 PG 协议层对齐的关键黏合层。

### 三件套分工

| 层 | crate | 职责 |
|----|-------|------|
| 协议皮肤 | `cmx-dct-api` | axum handler、参数提取、ApiResp/msgpack 信封 |
| **领域模型** | **`cmx-dct-model`（本 crate）** | **`DictView` 视图、`DctQuery` DTO、铸号/白名单/coerce、SQL 文本生成（DB-free）** |
| 持久化 | `cmx-dct-store-pg` | `resolve_dict` 构造视图、`cf_*` 表执行、导入导出编排、事务 |

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-core` | `DataValue` / `SqlTypeMarker`（查询参数强类型绑定） |
| `cmx-utils` | `next_pk_id` 主键铸号；re-export `base_fieldset`（base 字段集读取）与 `id::{is_temp_id, id_to_key}`（临时 id 识别） |
| `cmx-biz` | `validation::TableSpec`（落库前列级校验规范，DOC/DCT 共享；`DictView` 持有） |
| serde / serde_json / chrono | DTO 反序列化、弱类型 JSON → 强类型 view、时间字符串 coerce |
| utoipa（optional） | 仅 `openapi` feature 时引入：`DctQuery` 的 `IntoParams` 派生 |

### 下游使用者

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-dct-store-pg` | `cmx-dct-model = { workspace = true }` | `resolve_dict` 构造 `DictView`；search/upsert/delete/导入导出执行时消费本层生成的 SQL 与 `DataValue` |
| `cmx-dct-api` | `cmx-dct-model = { workspace = true, features = ["openapi"] }` | handler 提取 `DctQuery` 查询参数（`Query<DctQuery>`），`IntoParams` 供 Swagger 文档 |

> 跨 workspace 的 `cmx-portalservice` / `cmx-flowengine` 不直接依赖本 crate。

## 核心功能与特性

| 功能 | 说明 | 关键入口 |
|------|------|---------|
| 坐标 DTO | `DctQuery`：DAM 三段可选（缺失全局反查），`dict`(dictCode) 必填，`with_props` 控制字段扁平属性投影 | `DctQuery::by_code` / `.with_props()` |
| 字典表视图 | `DictView`/`DictColumn`：表名/PK/编码·显示字段/自分级/列/校验规范/编码规则/唯一键 | `DictView`（store 层构造） |
| 元数据投影 | `DictColumn` → `/dct/meta` 列对象（固定键 + 条件键 + 扁平属性铺顶） | `project_meta_column` |
| 列白名单防注入 | 列名只允许 `view.columns` 中已知列 | `valid_col` |
| 值类型 coerce | NULL 带类型（NullTyped）、整型列字符串数字、时间列字符串解析 | `to_dv_by_col` / `sql_type_marker_of` |
| 主键铸号 | 临时 id 行铸 `next_pk_id` 真号 + 同批自分级 `parent_id` 重指向 + `idMap` 回传；仅整型 PK | `pk_is_generated` / `mint_ids_for_inserts` |
| 查询 SQL 构造 | parentId 过滤（= / IS NULL）、filters（标量等值/数组 IN/null IS NULL/空数组 false）、q 对 code·label ILIKE 模糊、sort 白名单 + sort_no 回退、LIMIT/OFFSET | `build_search_sql` / `parse_paging` |
| 单行 upsert | 列白名单 + 服务端 backfill（now()/0/1 等）+ `ON CONFLICT(pk) DO UPDATE ... EXCLUDED` + full_path 用 code 兜底 | `build_upsert_sql_dv` |
| 删除 SQL | `DELETE FROM "tbl" WHERE "pk" = $1`（delete 端点与 changeset deleted 分支共用） | `build_delete_sql` |
| 批量导入导出 | 导出 keyset 分页（`pk > $last ORDER BY pk LIMIT N`）；导入多行 INSERT + 三种冲突模式 + replace 前置 TRUNCATE；迁移场景尊重用户时间戳 | `build_export_sql` / `build_batch_insert_sql` / `build_truncate_sql` |
| changeset 行解析 | 兼容 `{id, fields:{...}}` 与裸对象两种形态 | `row_fields` |
| 服务端托管列 | 单行 upsert 跳过客户端 `create_time/update_time`；`SERVER_FILLED_COLS`（NOT NULL 兜底）与 `SERVER_REPLACED_COLS`（始终替换）两张清单 | `is_server_managed_col` |

## 模块结构

```text
src/
├── lib.rs   # DctQuery/DictView/DictColumn、project_meta_column、valid_col/to_dv_by_col、
│            # pk_is_generated/mint_ids_for_inserts、build_search_sql/build_upsert_sql_dv/
│            # build_delete_sql/row_fields、托管列清单、分页解析（含单测）
└── bulk.rs  # 批量导入导出 SQL 构造（DB-free）：build_export_sql(keyset)/extract_pk/
             # build_truncate_sql/build_batch_insert_sql + BatchConflictMode（含回归单测）
```

## 关键类型与 API

```rust
/// /api/dct/* 共用坐标：定位定义文件 + 其中哪张字典表（DAM/file 可缺省，按 dict 全局反查）。
pub struct DctQuery {
    pub domain: Option<String>, pub application: Option<String>,
    pub module: Option<String>, pub file: Option<String>,
    pub dict: String,           // 字典表 dictCode（必填，如 currency / gl_account）
    pub with_props: bool,       // true 时 columns[].extra 携带字段扁平属性
}
impl DctQuery {
    pub fn by_code(dict: impl Into<String>) -> Self;  // 最常用：只按 dict code 定位
    pub fn with_props(mut self) -> Self;              // 链式开启完整字段属性投影
}

/// 解析出的字典表视图（由 cmx-dct-store-pg::resolve_dict 构造）。
pub struct DictView {
    pub dict_code: String, pub dict_name: String, pub table_name: String,
    pub id_field: String, pub code_field: String, pub label_field: String,
    pub parent_field: Option<String>,      // 自分级字典的父引用列（如 parent_id）
    pub self_hierarchy: bool,
    pub columns: Vec<DictColumn>,          // own fields + *FieldSet 合并，去重保序
    pub pk: String,                        // 有 id 用 id；无 id 用 code
    pub spec: Arc<cmx_biz::validation::TableSpec>,  // 落库前列级校验规范
    pub code_rule: Option<Value>,          // dictMeta.codeRule（编码引擎挂载点）
    pub unique_keys: Vec<Vec<String>>,     // 业务唯一键清单（合并去重用）
}

// —— 纯逻辑函数（签名摘录）——
pub fn valid_col(view: &DictView, name: &str) -> bool;
pub fn to_dv_by_col(view: &DictView, col_name: &str, v: &Value) -> DataValue;
pub fn sql_type_marker_of(dt: &str) -> SqlTypeMarker;
pub fn pk_is_generated(view: &DictView) -> bool;   // PK 为整型 → 需服务端铸号
pub fn mint_ids_for_inserts(
    view: &DictView,
    rows: &mut [serde_json::Map<String, Value>],
) -> serde_json::Map<String, Value>;               // 铸号 + parent_id 重指向，返回 idMap
pub fn parse_paging(raw: &Value) -> (i64, i64);     // page≥1；pageSize 默认 500，clamp [1,5000]
pub fn build_search_sql(view: &DictView, raw: &Value)
    -> (String, String, Vec<DataValue>);           // (data_sql, count_sql, params)
pub fn json_to_datavalue(v: &Value) -> DataValue;
pub fn is_server_managed_col(name: &str) -> bool;  // create_time / update_time
pub const SERVER_FILLED_COLS: &[&str];   // create_by/update_by/sort_no/status/is_system/is_leaf/level_no/full_path/delete_flag
pub const SERVER_REPLACED_COLS: &[&str]; // id / create_time / update_time
pub fn build_upsert_sql_dv(
    view: &DictView, obj: &serde_json::Map<String, Value>,
) -> Option<(String, Vec<DataValue>)>;
pub fn build_delete_sql(view: &DictView) -> String;
pub fn row_fields(row: &Value) -> Option<serde_json::Map<String, Value>>;
pub fn project_meta_column(c: &DictColumn) -> Value;

// bulk.rs —— 批量导入导出
pub fn build_export_sql(view: &DictView, last_pk: Option<&DataValue>, limit: i64)
    -> (String, Vec<DataValue>);
pub fn extract_pk(view: &DictView, row: &serde_json::Map<String, Value>) -> DataValue;
pub fn build_truncate_sql(view: &DictView) -> String;   // TRUNCATE ... RESTART IDENTITY
pub enum BatchConflictMode { Upsert, InsertOnly, Replace }
pub fn build_batch_insert_sql(
    view: &DictView, rows: &[serde_json::Map<String, Value>], mode: BatchConflictMode,
) -> Option<(String, Vec<DataValue>)>;

// re-export（调用点零改动地上提到 cmx-utils 后保持兼容）
pub use cmx_utils::json::base_fieldset;
pub use cmx_utils::id::{id_to_key, is_temp_id};
```

## 使用示例

### 场景 1：构造查询坐标与搜索 SQL（前端字典下拉/列表）

```rust
use cmx_dct_model::{DctQuery, build_search_sql, parse_paging};

// 最常用：只按 dictCode 定位（DAM/file 由 store 层全局反查补全）
let q = DctQuery::by_code("currency");          // 需要完整字段属性时 .with_props()

// 由（store 层 resolve_dict 得到的）view + 请求 body 构造 SQL——零手写拼接
let body = serde_json::json!({
    "filters": { "status": "1", "id": ["101", "102"] },  // 标量=等值；数组=IN；null=IS NULL
    "q": "人民",                                            // 对 code/label ILIKE 模糊
    "sort": { "field": "code", "order": "desc" },           // field 经列白名单校验
    "page": 1, "pageSize": 500
});
let (data_sql, count_sql, params) = build_search_sql(&view, &body);
// data_sql: SELECT "id", "code", ... FROM "cf_currency" WHERE "status" = $1
//           AND "id" IN ($2, $3) AND ("code" ILIKE $4 OR "name" ILIKE $4)
//           ORDER BY "code" DESC LIMIT 500 OFFSET 0
// params 已是 Vec<DataValue>（含整型字符串 coerce），由 store 层直接绑定执行
let (page, page_size) = parse_paging(&body);    // (1, 500)
```

### 场景 2：批量新增铸号（自分级 parent_id 重指向）

```rust
use cmx_dct_model::{mint_ids_for_inserts, pk_is_generated};

// 仅整型主键字典才铸号；code 作 PK 的 NoID 字典（如 cf_currency）跳过
if pk_is_generated(&view) {
    let mut rows = vec![
        serde_json::Map::from_iter([
            ("id".into(), "tmp-parent".into()),          // 前端临时 id
            ("code".into(), "1001".into()),
            ("parent_id".into(), serde_json::Value::Null),
        ]),
        serde_json::Map::from_iter([
            ("id".into(), "tmp-child".into()),
            ("code".into(), "100101".into()),
            ("parent_id".into(), "tmp-parent".into()),   // 指向同批父行的临时 id
        ]),
    ];
    // 纯内存改写：临时 id → next_pk_id() 真号；子行 parent_id 重指向父真号
    let id_map = mint_ids_for_inserts(&view, &mut rows);
    // id_map: {"tmp-parent": 1724…, "tmp-child": 1725…} —— 回传前端刷新临时行
}
```

### 场景 3：单行 upsert 与删除 SQL 构造

```rust
use cmx_dct_model::{build_upsert_sql_dv, build_delete_sql};

let mut row = serde_json::Map::new();
row.insert("code".into(), "CNY".into());
row.insert("name".into(), "人民币".into());

let (sql, params) = build_upsert_sql_dv(&view, &row).unwrap();
// INSERT INTO "cf_currency" ("code", "name", "update_time", ...)
// VALUES ($1, $2, now(), ...) ON CONFLICT ("id") DO UPDATE SET
//   "code" = EXCLUDED."code", "name" = EXCLUDED."name", "update_time" = now()
// 服务端 backfill：create_time/update_time=now()、sort_no=0、status=1、is_system=0、
// is_leaf=1、level_no=1；full_path 缺失时用 code 值兜底。
// 注意：客户端的 create_time/update_time 值被 is_server_managed_col 过滤（单行路径）。

let del_sql = build_delete_sql(&view);
// DELETE FROM "cf_currency" WHERE "id" = $1 （pk 参数由调用方 to_dv_by_col 构造）
```

### 场景 4：批量导入（三种冲突模式）

```rust
use cmx_dct_model::{build_batch_insert_sql, build_truncate_sql, BatchConflictMode};

let rows: Vec<serde_json::Map<String, Value>> = parse_ndjson_or_csv();  // 行集合

// upsert（默认）：ON CONFLICT(pk) DO UPDATE SET ...（等同单行 upsert 语义）
let (sql, params) = build_batch_insert_sql(&view, &rows, BatchConflictMode::Upsert).unwrap();
// insert_only：ON CONFLICT(pk) DO NOTHING（主键冲突跳过）
// replace：先 TRUNCATE 再裸 INSERT（不加 ON CONFLICT）：
let truncate = build_truncate_sql(&view);   // TRUNCATE TABLE "cf_currency" RESTART IDENTITY

// 与单行路径的关键差异：批量导入是数据迁移/同步场景，用户提供的
// create_time/update_time 被尊重（保留历史时间戳）；未提供（Null）时才走 backfill 字面量。
```

## Features

| feature | 默认 | 说明 |
|---------|------|------|
| `openapi` | 关 | 引入 utoipa 并为 `DctQuery` 派生 `IntoParams`，供 `cmx-dct-api` 的 Swagger 文档描述查询参数。不开启则**零 utoipa 依赖**（编译隔离，纯逻辑 crate 保持轻量）。 |

消费方式：`cmx-dct-model = { workspace = true, features = ["openapi"] }`（`cmx-dct-api` 已这样引用）。
