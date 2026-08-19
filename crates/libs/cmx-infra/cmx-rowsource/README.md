# cmx-rowsource

> 驱动无关的行来源抽象（`ZmcRowSource` trait + 中立列类型 `ZmcColType`）与零拷贝列式二进制（msgpack）编码器，被 cmx-database（sqlx）与 cmx-database-pg（tokio-postgres）共同依赖的近叶子基础 crate。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-rowsource` 把「从一行取某列的值」和「列的类型」抽象成 [`ZmcRowSource`] trait + 中立枚举 [`ZmcColType`]，使同一套零拷贝 msgpack 列式编码器能同时服务 sqlx 的 `PgRow` 和 tokio-postgres 的 `Row` —— 两个数据库 crate 各自 `impl ZmcRowSource`，本 crate 自身不依赖任何具体驱动。

编码产出「列式包」结构（与老 `ColumnarCodec` 的 JSON 逐字段同构，前端 `CmxDataSet.fromJSON` 可无改动复用）：

```text
{datasetId, columns: [名...], rows: [[值...]...], childRows: {父id: {childKey: 子包}}}
```

### 设计要点：为什么不复用 cmx-core 的 `DataValue`

本 crate 刻意**不依赖任何 cmx-\* crate**（包括 cmx-core 的 `DataValue`），以保持三个特性：

1. **wasm 安全** —— 无 `tokio`/`sqlx` 等重运行时，可被插件 SDK 在 wasm 端复用同一编码契约；
2. **近叶子** —— 位于依赖图底层，被 `cmx-database`（sqlx）与 `cmx-database-pg`（tokio-postgres）同时依赖；若反向依赖 cmx-core 会形成循环或把核心类型绑死到驱动层；
3. **驱动中立** —— `ZmcColType` 是 `DataValue` 编码契约的驱动中立投影，只描述「如何把一列编码成 msgpack/JSON」，不承载业务语义。

代价：值编码策略（字符串化/前缀约定）与 `DataValue` 的实现各维护一份，由「值编码对齐老 DataValue 契约」注释 + `json_encoding_matches_binary` 回归测试共同约束不漂移。

### 值编码契约（对齐老 DataValue）

| ZmcColType | 编码结果 |
|------------|----------|
| `Bool` | msgpack bool |
| `Int2` / `Int4` / `Int8` | msgpack int |
| `Float4` / `Float8` | msgpack float（f64） |
| `Numeric` | 字符串（Decimal to_string） |
| `Text` / `Unknown` | 字符串 |
| `Json` | 原字符串（JSON 文本 wire 可零拷贝借 `&str`） |
| `Jsonb` | serde_json 序列化字符串（二进制 wire 带版本头，不能借 `&str`） |
| `Uuid` / `Date` / `Timestamp` / `Timestamptz` | 字符串（时间手写 RFC3339，与 chrono `to_rfc3339` 逐字节一致） |
| `Bytea` | `"B64:" + base64` 前缀字符串 |
| SQL NULL / 解码失败 | nil（**失败即 nil，不 panic**） |

---

## 与其他 crate 的关系

### 上游依赖

无任何 `cmx-*` 依赖，全部为第三方轻量库：`rmp`（msgpack 裸写，零拷贝直写借来的 `&str`/`&[u8]`）、`base64`、`chrono`、`rust_decimal`、`uuid`、`serde_json`、`tracing`（未知类型兜底告警）；dev 依赖 `rmp-serde`（测试解码断言）。

### 下游使用者（Cargo.toml 反查）

| crate | 用法 |
|-------|------|
| `cmx-database`（sqlx） | `zmc` 模块定义 `SqlxPgRowSource` 并重导出本 crate 的 `ZmcColType`/`ZmcRowSource`（对外统一入口） |
| `cmx-database-pg`（tokio-postgres） | `zmcdataset` 模块定义 `TokioPgRowSource`（`#[repr(transparent)]`）并同样重导出 |
| `cmx-dct-store-pg` / `cmx-doc-store-pg` | 业务 store 层直接消费 `ZmcDataSet` 编码出口 |
| `cmx-master-slave` | 主从数据集装载 |
| `cmx-database-test` | 跨 crate 集成测试 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 行取值抽象 | `ZmcRowSource`：`col_count` / `col_name` / `col_type` + 15 个类型化取值方法；SQL NULL 或解码失败一律返回 `None` |
| 零拷贝借出 | `get_str` / `get_bytes` 输出绑定 `&self`，直接借出底层引用计数 `Bytes` |
| 中立列类型 | `ZmcColType` 17 个变体；粒度对齐编码分派（Int 宽度 / Json vs Jsonb wire / Timestamp vs Timestamptz） |
| 列元信息 | `ZmcSchema`（列名 + 类型 + 名→索引 HashMap，`Arc` 共享），支持从首行推导或显式构造 |
| 泛型数据集 | `ZmcDataSet<R>` 持有原始行 `Vec<R>`，惰性列式编码；支持 `children` 子集分组（按父 id 分桶防孙层组合级膨胀）与可选 `total` |
| 三种出口 | 一次性 msgpack（`encode_columnar_binary`）/ JSON 同构（`encode_columnar_json`）/ 流式分帧 |
| 流式骨架 | `encode_stream_open/close`（array32 占位回填行数）+ `encode_frame_header/row/end`（`[u32 大端 len][payload]`，终止帧 len=0） |
| 分桶键 | `stringify_key` / `row_key_string` 把 int/text/uuid 列统一成父 id 字符串键 |

---

## 模块结构

单文件 crate（`src/lib.rs`，约 1130 行），按区块组织：

```text
src/lib.rs
├── 模块文档              # 设计动机（三特性）+ 值编码契约
├── MsgPackWrite 写入糖    # 对 &mut Vec<u8> 的 rmp 写入做 infallible 包装（Vec 写只 OOM panic）
├── ZmcColType            # 中立列类型枚举（17 变体）
├── ZmcRowSource trait    # 行取值抽象（15 个类型化取值方法）
├── ZmcSchema             # 列元信息（from_row / from_parts / col_index）
├── ZmcChildGroup<R>      # 子集分组（child_key + 子数据集 + parent_ids）
├── ZmcDataSet<R>         # 零拷贝数据集（new / with_schema / add_child_group / col_str / row_key_string）
│   └── 编码实现           # encode_columnar_binary / encode_columnar_json（含子集分桶）
├── stringify_key         # 分桶键字符串化
├── 单行/单格编码          # encode_row_json / encode_cell_json / encode_row_into / encode_cell
└── 流式编码              # encode_stream_open / encode_stream_close / encode_frame_header / encode_frame_row / encode_frame_end
```

---

## 关键类型 / API

```rust
/// 中立列类型（driver 把自己的 PG 类型映射过来，编码器只认这个）
pub enum ZmcColType { Bool, Int2, Int4, Int8, Float4, Float8, Numeric, Text,
                      Json, Jsonb, Uuid, Bytea, Date, Timestamp, Timestamptz, Unknown }

/// 行取值抽象 —— driver 的一行（sqlx PgRow / tokio Row）实现此 trait。
/// 取值语义：SQL NULL 或解码失败一律返回 None。
pub trait ZmcRowSource {
    fn col_count(&self) -> usize;
    fn col_name(&self, i: usize) -> &str;
    fn col_type(&self, i: usize) -> ZmcColType;
    fn get_bool(&self, i: usize) -> Option<bool>;
    fn get_i16(&self, i: usize) -> Option<i16>;
    fn get_i32(&self, i: usize) -> Option<i32>;
    fn get_i64(&self, i: usize) -> Option<i64>;
    fn get_f32(&self, i: usize) -> Option<f32>;
    fn get_f64(&self, i: usize) -> Option<f64>;
    fn get_decimal(&self, i: usize) -> Option<rust_decimal::Decimal>;
    fn get_str(&self, i: usize) -> Option<&str>;            // 零拷贝
    fn get_bytes(&self, i: usize) -> Option<&[u8]>;         // 零拷贝
    fn get_uuid(&self, i: usize) -> Option<uuid::Uuid>;
    fn get_date(&self, i: usize) -> Option<chrono::NaiveDate>;
    fn get_naive_datetime(&self, i: usize) -> Option<chrono::NaiveDateTime>;
    fn get_datetime_utc(&self, i: usize) -> Option<chrono::DateTime<chrono::Utc>>;
    fn get_json_value(&self, i: usize) -> Option<serde_json::Value>;  // JSONB 解码
}

/// 轻量列元信息（Arc 共享）
pub struct ZmcSchema { pub columns: Vec<String>, pub types: Vec<ZmcColType>, /* name→idx */ }

/// 子集分组：父数据集下按某 childKey 挂载的子数据集 + 每子行的父 id 字符串
pub struct ZmcChildGroup<R: ZmcRowSource> {
    pub child_key: String,          // 前端 childRows[父id][childKey] 的 key
    pub child: ZmcDataSet<R>,
    pub parent_ids: Vec<String>,    // 与 child.rows 等长
}

/// 零拷贝数据集：持有原始行 Vec<R>，惰性列式二进制编码
pub struct ZmcDataSet<R: ZmcRowSource> {
    pub id: String,
    pub schema: Arc<ZmcSchema>,
    pub rows: Vec<R>,
    pub children: Vec<ZmcChildGroup<R>>,
    pub total: Option<i64>,         // 仅 count_total=true 时由 loader 填入
}
impl<R: ZmcRowSource> ZmcDataSet<R> {
    pub fn new(id: impl Into<String>, rows: Vec<R>) -> Self;             // schema 从首行推导
    pub fn with_schema(id: impl Into<String>, schema: Arc<ZmcSchema>, rows: Vec<R>) -> Self;
    pub fn empty(id: impl Into<String>, schema: Arc<ZmcSchema>) -> Self; // 空表兜底保留列信息
    pub fn add_child_group(&mut self, group: ZmcChildGroup<R>);
    pub fn col_str(&self, row: usize, col: usize) -> Option<&str>;
    pub fn row_key_string(&self, row: usize, col: usize) -> Option<String>;
    pub fn encode_columnar_binary(&self, buf: &mut Vec<u8>);             // msgpack 列式包
    pub fn encode_columnar_json(&self) -> serde_json::Value;             // JSON 同构出口
}

/// 单行/单格与流式编码
pub fn encode_row_into<R: ZmcRowSource>(body: &mut Vec<u8>, row: &R, schema: &ZmcSchema);
pub fn encode_cell<R: ZmcRowSource>(/* ... */);
pub fn encode_row_json<R: ZmcRowSource>(row: &R, schema: &ZmcSchema) -> serde_json::Value;
pub fn encode_stream_open(out: &mut Vec<u8>, dataset_id: &str, schema: &ZmcSchema) -> usize;
pub fn encode_stream_close(out: &mut [u8], marker_pos: usize, row_count: u32);
pub fn encode_frame_header(out: &mut Vec<u8>, dataset_id: &str, schema: &ZmcSchema);
pub fn encode_frame_row<R: ZmcRowSource>(out: &mut Vec<u8>, row: &R, schema: &ZmcSchema);
pub fn encode_frame_end(out: &mut Vec<u8>);   // 终止帧（len=0）
pub fn stringify_key<R: ZmcRowSource>(row: &R, col: usize, ty: ZmcColType) -> Option<String>;
```

---

## 使用示例

### 安装

```toml
[dependencies]
# 内部依赖 - 驱动无关行来源抽象（workspace path 统一版本）
cmx-rowsource = { workspace = true }
```

### 场景 1：为自己的驱动行实现 ZmcRowSource（数据库 crate 视角）

```rust
use cmx_rowsource::{ZmcColType, ZmcRowSource};

/// 零开销包装（cmx-database-pg 的 TokioPgRowSource 即此模式，#[repr(transparent)]）
#[repr(transparent)]
pub struct MyRowSource(pub my_driver::Row);

impl ZmcRowSource for MyRowSource {
    fn col_count(&self) -> usize { self.0.len() }
    fn col_name(&self, i: usize) -> &str { self.0.name(i) }
    fn col_type(&self, i: usize) -> ZmcColType {
        match self.0.type_oid(i) {
            16 => ZmcColType::Bool,
            21 | 23 | 20 => ZmcColType::Int8,     // 按实际宽度映射 Int2/Int4/Int8
            25 => ZmcColType::Text,
            17 => ZmcColType::Bytea,
            _ => ZmcColType::Unknown,             // 兜底当字符串
        }
    }
    fn get_bool(&self, i: usize) -> Option<bool> { self.0.try_get(i).ok() }
    fn get_str(&self, i: usize) -> Option<&str> { self.0.try_borrow_str(i).ok().flatten() } // 零拷贝
    // ...其余 12 个取值方法（get_i16/i32/i64、get_f32/f64、get_decimal、get_bytes、get_uuid、
    // get_date、get_naive_datetime、get_datetime_utc、get_json_value）同构实现：
    // NULL / 解码失败一律返回 None，不 panic
}
```

### 场景 2：构造 ZmcDataSet 并编码（cmx-dct-store-pg / cmx-doc-store-pg 的消费模式）

```rust
use cmx_rowsource::{ZmcChildGroup, ZmcDataSet};

fn export(rows: Vec<MyRowSource>, children: Vec<(String, Vec<MyRowSource>)>) -> Vec<u8> {
    // schema 从首行推导（空结果 → 空 schema，可改用 with_schema 显式给列）
    let mut ds = ZmcDataSet::new("order_list", rows);

    // 挂载子集分组：childRows[父id]["order_item"]，parent_ids 与子行等长
    // （父 id 键由 row_key_string/stringify_key 把 int/text/uuid 统一字符串化）
    for (child_key, child_rows) in children {
        let parent_ids = vec!["1001".to_string(); child_rows.len()];
        ds.add_child_group(ZmcChildGroup {
            child_key,
            child: ZmcDataSet::new("items", child_rows),
            parent_ids,
        });
    }

    // 一次性列式 msgpack 编码（前端 CmxDataSet.fromJSON 同构契约）
    let mut buf = Vec::new();
    ds.encode_columnar_binary(&mut buf);
    buf
}
```

### 场景 3：JSON 同构出口（调试 / 非 msgpack 链路）

```rust
use cmx_rowsource::ZmcDataSet;

fn to_json(ds: &ZmcDataSet<MyRowSource>) -> serde_json::Value {
    // 同构输出 {datasetId, columns, rows, childRows?, total?}（json_encoding_matches_binary 测试约束不漂移）
    ds.encode_columnar_json()
}
```

### 场景 4：流式分帧（超大结果集，峰值内存 O(单行)）

```rust
use cmx_rowsource::{ZmcSchema, encode_frame_end, encode_frame_header, encode_frame_row};

fn stream_out(schema: &ZmcSchema, rows: impl Iterator<Item = MyRowSource>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    // 头帧：datasetId + columns（不依赖首行 → 空结果也能收尾）
    let mut header = Vec::new();
    encode_frame_header(&mut header, "big_ds", schema);
    frames.push(header);
    // 行帧：每帧 [u32 大端长度][单行 payload]
    for row in rows {
        let mut f = Vec::new();
        encode_frame_row(&mut f, &row, schema);
        frames.push(f);
    }
    // 终止帧：len = 0
    let mut end = Vec::new();
    encode_frame_end(&mut end);
    frames.push(end);
    frames
}
```

---

## Features 说明

本 crate 的 `Cargo.toml` 未定义 `[features]` 段，无可选特性；依赖面刻意保持最小（无任何 `cmx-*`、无 tokio/sqlx），以保证 wasm 安全与近叶子位置。
