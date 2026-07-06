# ZMCDataSet:零拷贝二进制数据集 + tokio-postgres 新链路方案

## Context(为什么做)

**现状**:业务数据从 PG 到前端要经过 `PG → sqlx PgRow → DataSet(枚举 owned) → serde_json 文本 → 浏览器 JSON.parse`。每个字段值被复制/重新编码多次(见前期分析:一个文本字段从 DB 到浏览器被复制 3~4 次),大结果集时峰值内存 = O(PgRow 全集) + O(DataSet 全集) + O(JSON 字节串) 三份叠加。

**目标**(用户明确):
1. 老功能**代码/使用方式全不动**,继续用 sqlx + 老 DataSet。
2. **新功能**统一走 tokio-postgres,链路 `db → tokio-pg → ZMCDataSet → 前端`,追求**进程内零冗余复制**。
3. `ZMCDataSet` 是一个**全新的零拷贝数据集组件**,新功能不再用老 DataSet。
4. **前端不再用 JSON,改用二进制数据**。

**诚实的物理边界**(方案的前提,先讲清楚):
- 真正字面的"0 copy 到浏览器"做不到 —— 出口 socket 的 syscall 是物理下限,且任何二进制编码(msgpack / 列式)本身要把非文本值(int/decimal/timestamp)写成字节。
- 能做到的是:**① 进程内不再产生"PgRow→DataValue枚举→JSON"这三段冗余;② 文本/二进制列零拷贝借用 PgRow 的底层 `Bytes`;③ 只做一次编码,直接写进二进制响应体。** 这已是 JSON 方案之上的数量级改善。

---

## 技术地基(已核实源码,零拷贝可行)

| 事实 | 出处 | 意义 |
|---|---|---|
| tokio-postgres `Row{ body: DataRowBody, ranges: Vec<Option<Range>> }`,`DataRowBody{ storage: Bytes, len }` | postgres-protocol 0.6.12 `message/backend.rs` | 整行原始字节是 `Bytes`(引用计数、Clone 廉价、可 `'static` 跨 await) |
| `Row::col_buffer(i) -> Option<&[u8]>` = `&body.buffer()[range]` | tokio-postgres `row.rs:204` | 按列**零拷贝借出**原始字节切片 |
| `impl FromSql<'a> for &'a str / &'a [u8]` 直接返回借用 | postgres-types `lib.rs:545/689` | 文本/二进制列零拷贝借出 `&str`/`&[u8]`,不分配 |
| `query_raw() -> RowStream: Stream<Item=Row>` | tokio-postgres `query.rs:337` | 流式逐行,峰值内存 O(单行) |
| 数值/时间列 wire format 非文本(int=大端二进制、timestamp=8字节整数、numeric=专有结构) | — | 这些列**无法零拷贝到文本/JSON**,必须解码+编码,是零拷贝的天然边界 |

**结论**:tokio-postgres 的 Row 与 sqlx 的 PgRow 在零拷贝能力上同构。文本类可零拷贝直通二进制,数值/时间类逃不掉一次转换——这是所有方案的物理天花板。

---

## 老 DataSet 对外契约(ZMCDataSet 要对齐/替代什么)

老 DataSet 承担两种序列化契约,都走 `DataValue` 编码规则:

1. **行式 JSON**:`{id, schema:{id,fields}, rows:[{字段名:值, 子集名:[...]}]}` —— 通用 CRUD 出口。
2. **列式包**:`{datasetId, columns:[...], rows:[[值]], childRows:{父id:{childKey:子包}}}` —— **业务单据出口**(`ColumnarCodec::encode`,前端 `CmxDataSet.fromJSON` 消费)。

DataValue 编码要点(前端硬依赖):`Binary→"B64:<b64>"`、`NullTyped→"$null:Int"`、`Decimal/DateTime/Date/Uuid→字符串`、`Json→保持字符串`、`Float→透明数字`、`Null→null 不跳过`。

**可弃**:`inserted/updated/deleted` 三池(全仓零读取,写路径走独立 JSON changeset);`validate.rs`;可变写入 API(除 loader 建树需要,见下)。

**第二硬约束**:老 DataSet 经 `rmp_serde` 跨 wasm 插件边界(`DbResponse.dataset`)。ZMCDataSet 若要服务 wasm 插件路径需保这条往返 —— 但**本期新链路是 REST→浏览器,不碰 wasm**,故 ZMCDataSet 只需管好浏览器二进制出口,wasm 路径继续用老 DataSet。

---

## 前端现状(决定二进制通道怎么搭)

- 前端**当前 100% 吃 JSON,无任何二进制解析库**(无 msgpack/arrow/protobuf)。
- 业务单据出口:`handlers/portal/doc.rs:76` `DocLoader::load → DataSet → ColumnarCodec::encode → Json(ApiResp::ok(pkg))`;前端 `cmx-data-comp/src/lib/cmx-doc-source.js:37` `res.json() → CmxDataSet.fromJSON`。
- REST 出口被 `Json<ApiResp<T>>` 焊死为 JSON;`rmp-serde` 依赖已在但仅 wasm 边界用。
- `mw_trace` 中间件会 `serde_json::from_slice` 预览 body —— 二进制响应需豁免。

**结论**:没有现成的 HTTP→浏览器二进制通道,需新搭;但 axum 原生支持二进制 body,`rmp-serde` 已在,前端只需加一个 msgpack decode 库。改动面:**后端小、前端小、设计决策中等**。

---

## 方案总览

```
┌─────────────┐   query_raw    ┌──────────────┐   借用+惰性编码   ┌────────────────┐   axum bin body   ┌──────────┐
│ PostgreSQL  │ ─────────────► │ RowStream    │ ───────────────► │ ZMCDataSet     │ ────────────────► │ 浏览器    │
│             │   (tokio-pg)   │ Vec<Row>     │  文本列零拷贝借   │ = 借用视图 +   │   Content-Type:   │ msgpack   │
│             │                │ (持有 Bytes) │  数值列就地编码   │   列式二进制编码 │   application/    │ decode    │
└─────────────┘                └──────────────┘                  └────────────────┘   x-msgpack        └──────────┘
                                                                  一次编码直出二进制
```

**核心设计:ZMCDataSet 不是"又一个 owned 数据结构",而是"持有 PG Row 原始字节的借用视图 + 惰性列式二进制编码器"。** 它不预先把每个单元格解成枚举,而是保留 `Vec<Row>`(内含 `Bytes`),在序列化那一刻按列 `Type` 决定:文本/二进制列直接 `slice` 原始字节写进输出;数值/时间列就地解码+编码。全程只有一次"写进输出缓冲",没有中间 DataValue 枚举、没有中间 JSON 串。

---

## 落地范围与模块

新代码集中在 `cmx-database-pg`(已存在的 tokio-pg 并行 crate)+ 一个新的核心组件,再加 API 出口和前端 decode。老 `cmx-database` / 老 `DataSet` **零改动**。

### 1. `ZMCDataSet` 组件(新增,建议放 `crates/libs/cmx-core/src/model/data/zmcdataset/`)

**定位**:只读投影 + 零拷贝二进制编码器。不做变更追踪、不做可变写入。

**数据结构**(核心思路,非最终签名):
```
pub struct ZmcDataSet {
    id: String,
    schema: Arc<ZmcSchema>,          // 列名 + 列 PG Type(OID),复用 Arc 共享
    rows: Vec<tokio_postgres::Row>,  // ★ 持有原始 Row,内含 Bytes,零拷贝保有
    children: Vec<ZmcChildGroup>,    // 树形:每组 = (childKey列, 子ZmcDataSet, 按父id分桶索引)
}
```
- `rows` 直接持有 tokio-pg `Row` —— 每个 Row 内是 `DataRowBody{storage: Bytes}`,引用计数,不复制。
- `schema` 从 `RowStream` 的第一行 `columns()` 拿 `&[Column]`(列名 + `Type`),转成轻量 `ZmcSchema`(name + OID)。
- 树形嵌套:沿用 DocLoader 的 `upper_id = ANY($ids)` BFS 建树,但子集也是 `ZmcDataSet`,按父行 id 分桶(索引用 `HashMap<childKeyBytes, Vec<rowIdx>>`,不复制行)。

**从 RowStream 构造**(替代 `PgResultConverter::convert_rows`,但**不产 DataValue、不产 DataSet**):
```
ZmcDataSet::collect(stream: RowStream, id) -> ZmcDataSet   // 全量:drain 成 Vec<Row>
ZmcDataSet::encode_streaming(stream, writer)                // 流式:边取边编码进 writer,峰值 O(单行)
```

**列式二进制编码**(核心,替代 `ColumnarCodec::encode` 的 JSON 版):
```
ZmcDataSet::encode_columnar_binary(&self, buf: &mut Vec<u8>)
```
产出**与老列式包同构、但 msgpack 编码**的结构:`{datasetId, columns:[名+类型], rows:[[值]], childRows:{...}}`。逐列按 `Type` 分派:
- `TEXT/VARCHAR/JSON/JSONB` → `row.col_buffer(i)` 拿 `&[u8]`,UTF-8 直接作为 msgpack str 写入(**零拷贝借用,不经 String**)。
- `INT2/4/8` → 从 wire 字节解出整数,msgpack int 原生写入(**不再字符串化,比 JSON 更省**)。
- `FLOAT4/8` → msgpack float。
- `NUMERIC` → 解 `rust_decimal::Decimal` → 字符串(保精度,对齐前端契约)。
- `BOOL` → msgpack bool。
- `UUID` → 16 字节 → 标准字符串(对齐契约)或 msgpack bin(前端约定)。
- `BYTEA` → msgpack bin(**原生二进制,不再 `B64:` 前缀** —— 二进制通道的红利)。
- `TIMESTAMP/TIMESTAMPTZ/DATE` → RFC3339 / `YYYY-MM-DD` 字符串(对齐契约)。
- `NULL` → msgpack nil。

> **关键红利**:二进制通道下,int 不再转十进制文本、bytea 不再 base64、null 直接 nil —— 这些都是 JSON 方案省不掉的开销,二进制天然消掉。文本列则零拷贝借用。

**读侧访问 API**(供 DocLoader 等消费方沿用,类比老 Row 的 get/get_by_name):
`row_count/is_empty`、`col_index(name)`、按行按列取"借用值"`col_bytes(row, col) -> Option<&[u8]>`、`col_as_i64/str/...`、`children(key)`。

### 2. `cmx-database-pg` 查询出口(新增流式/ZMC 方法)

在 `cmx-database-pg` 的 connection/transaction/manager 上新增**产 ZmcDataSet** 的查询方法(与现有产 DataSet 的方法并存,不改老的):
- `PgDbPool::query_zmc(sql, params, id) -> ZmcDataSet`(用 `query_raw` + `ZmcDataSet::collect`)。
- `PgDbPool::query_zmc_streaming(sql, params, writer)`(边取边编码,大结果集用)。
- manager 门面加对应 `query_sql_zmc(...)`,入口 `get_default_pg_db_manager()`(已存在)。

### 3. 业务单据装载切到 ZMC(新增 DocLoader 变体,老的不动)

老 `cmx-biz/src/doc/loader.rs::DocLoader` 依赖老 DataSet + sqlx manager,**不动**。为新功能提供 `ZmcDocLoader`(或 loader 内加 zmc 变体):
- 复用 BFS 逐层 + `upper_id = ANY($ids)` 算法(前面确认过这是最优,pipelining 帮不上纵向下钻)。
- 每层查询走 `cmx-database-pg` 的 `query_zmc`,产 `ZmcDataSet`;按父 id 分桶挂子(不复制行,只建索引)。
- 空表兜底:用单据定义的权威 schema 覆盖(对齐老 `rebind_schema`)。

### 4. API 二进制出口(新增 endpoint / 内容协商)

- 新增二进制响应封装:给 `ApiResp`（或新建 `BinResp`）实现 axum `IntoResponse`,`Content-Type: application/x-msgpack`,body = `ZmcDataSet::encode_columnar_binary` 的字节。
- 新增业务单据二进制 endpoint(如 `/api/doc/data.bin`),或用 `Accept: application/x-msgpack` 内容协商复用现有路由;handler 走 `ZmcDocLoader` → `encode_columnar_binary` → 二进制 body。
- **`mw_trace` 中间件豁免二进制 Content-Type**(避免它 `from_slice` 解析二进制 body)。

### 5. 前端二进制解码(核心澄清:现有组件如何与二进制交互)

**结论先行:前端组件完全不接触二进制。** 二进制只活在"网络响应 → CmxDataSet 对象"这一小段里,解码在**一个加载函数**内完成,组件读到的还是和现在一模一样的 `CmxDataSet` 对象。

#### 5.1 现有前端已有的隔离层(这是关键)

前端组件从不直接吃网络数据,而是吃 `CmxDataSet` 对象。数据流:

```
网络响应 ──► loadDocData() ──► CmxDataSet.fromJSON(pkg) ──► CmxDataSet 对象 ──► 组件读 dataset.rows
              │                    │                          │                     │
       唯一接触传输格式的点   把列式包摊平成内部行对象      内部是 _rows 行对象     组件只认这个,永不变
```

代码事实(`packages/cmx-data-comp/src/lib/`):
- 组件读的是 `dataset.rows` getter(`cmx-data-set.js:143`),拿到的是**已还原好的行对象** `{id, doc_no, ...}`。
- `CmxDataSet.fromJSON(pkg)`(`cmx-data-set.js:362`)把列式包 `{datasetId, columns, rows:[[值]], childRows}` 摊平成内部 `_rows` 行对象数组,并递归还原 `_children` 子层。
- `fromJSON` **只被 `loadDocData`(`cmx-doc-source.js`)调用一次**——就是接收网络响应那一处。
- 组件 / CmxDataSet 内部结构 / `rows` getter / `getRow` / 子层 `_children` **全都不知道数据来自 JSON 还是二进制**。

→ **"前端组件如何与二进制交互"的答案是:不交互。** 二进制在到达组件前,已在 `loadDocData` 里被解回成与现状完全相同的对象。

#### 5.2 唯一改动:一个加载函数的两行

`loadDocData`(`cmx-doc-source.js`)里换收取方式,`fromJSON` 及之后**一个字不改**:

```js
// 老(JSON):
const res  = await _fetch(host, url, { headers: { Accept: 'application/json' } })
const body = await res.json()

// 新(二进制 msgpack):
const res  = await _fetch(host, url, { headers: { Accept: 'application/x-msgpack' } })
const buf  = await res.arrayBuffer()
const body = msgpack.decode(new Uint8Array(buf))   // 解出的 body 与 JSON.parse 结构一致

// 这一行两条路线都不变:
const dsMap = /* ... */ CmxDataSet.fromJSON(body.data)
```
成立前提:msgpack `decode` 出来的 `{datasetId, columns, rows, childRows}` 与 `JSON.parse` 出来的是**同一个 JS 对象**。前端只需新增 `@msgpack/msgpack`(目前无 msgpack 库),信封 `ApiResp` 拦截器加一个二进制分支。

#### 5.3 关键分叉:结构必须同构 → 两条路线,先走 A

上面"组件零改动"成立,依赖一个硬前提:**msgpack 承载的结构和值编码必须与老列式包逐字段同构**。而"二进制红利"(bytea→原生 bin、int→原生 int、null→nil)会打破这个前提。这里有两条路线,**本期先走 A**:

| | **路线 A:二进制=更快的 JSON(推荐先做)** | **路线 B:二进制用原生类型(后续优化)** |
|---|---|---|
| msgpack 承载 | 老列式包**同结构同值编码**:bytea 仍 `"B64:"`、Decimal 仍字符串、int 仍 int | 原生类型:bytea→msgpack bin(前端得 `Uint8Array`)、null→nil |
| 相比 JSON 省什么 | 解析更快、体积略小、数字免文本 parse | 上述 + 免 base64 编解码 |
| 前端 `fromJSON` | **零改动** | **要改**:bytea 字段从"解 `B64:` 前缀字符串"改成"直接收 `Uint8Array` |
| 组件层 | **零改动** | 处理二进制字段(附件/图片)的组件要适配 |
| 契约 | 与现状完全一致,前端无感 | 新契约,前后端要重新对齐 |
| 风险/改动面 | 最低 | 扩散到组件层 |

**先走路线 A 的理由**:先把 `db→tokio-pg→ZMCDataSet→msgpack→前端` 链路跑通、测出内存/延迟收益,而组件层零改动、零风险。等链路稳了、且确认 base64/字符串化确实是瓶颈,再**针对性**按路线 B 逐个字段类型吃红利——那时是局部优化,不是一开始就把改动面铺满组件层。

> 注意:这意味着**方案早期,后端 `ZMCDataSet::encode_columnar_binary` 的值编码要对齐老 DataValue 规则**(`B64:`/`$null:`/Decimal 字符串化),只是把外层容器从 JSON 换成 msgpack。零拷贝红利(文本列借用、int 免文本)在后端进程内照样吃到;前端契约层面保持不变。路线 B 是后端编码器 + 前端 `fromJSON` 协同的第二步。

---

## 关键设计决策(我的推荐 + 理由)

| 决策点 | 推荐 | 理由 |
|---|---|---|
| 二进制格式 | **msgpack 承载列式包结构** | rmp-serde 已在依赖、DataSet 已验证 msgpack-ready;前端只需加 `@msgpack/msgpack`;比 Arrow/自研快落地。列式结构复用前端 `fromJSON`,前端改动最小 |
| ZMCDataSet 承载 | **持有 `Vec<tokio_postgres::Row>` 的借用视图** | Row 内 `Bytes` 引用计数,零拷贝保有;文本列 `col_buffer` 借出。不预解成枚举 = 消掉 DataValue 中间层 |
| 编码时机 | **惰性,序列化那一刻按 Type 分派** | 只编码一次直出二进制,无中间 JSON 串 |
| 值编码规则 | **本期路线 A:对齐老 DataValue 契约(`B64:`/`$null:`/Decimal 字符串化),仅外层换 msgpack** | 前端 `fromJSON` 与组件**零改动**(见 §5.3);零拷贝红利在后端进程内照吃。原生 bin/int/nil 是后续路线 B 的针对性优化 |
| 前端组件 | **零改动** | 组件只吃 `CmxDataSet` 对象,不碰传输格式;仅 `loadDocData` 一处换 decode(§5.2) |
| 树形建树 | **复用 DocLoader 的 BFS + `ANY($ids)`** | 已是最优(O(层数) 往返);pipelining 只对横向多单据有用,纵向下钻用不上 |
| 老 DataSet / 老 loader / wasm 路径 | **全不动** | 满足"老功能零改动";wasm 边界继续用老 DataSet 的 msgpack |
| 大结果集 | **`encode_streaming` 边取边写** | 峰值内存 O(单行),呼应零内存目标 |

---

## 内存/复制账(方案达成度)

以一个文本字段为例,新链路 vs 老链路的进程内触碰:

| 阶段 | 老链路(sqlx+DataSet+JSON) | 新链路(ZMCDataSet+msgpack) |
|---|---|---|
| DB→用户态 | Bytes(网络缓冲) | Bytes(网络缓冲)—— syscall 下限,不可免 |
| Row→中间表示 | `try_get::<String>` **malloc 复制**成 String | **零拷贝**:保留在 Row 的 Bytes 里 |
| 中间→DataValue 枚举 | move 进 `DataValue::String` | **无此步**(不产枚举) |
| 序列化 | 写进 JSON 字节串(再复制一遍) | **借用 Bytes 直接写进 msgpack 输出** |
| 出口→socket | syscall | syscall —— 下限,不可免 |

文本列:老链路 2 次冗余复制(malloc String + 写 JSON)→ 新链路 **0 次冗余**(只有出口 syscall)。**注意这张表对路线 A / B 都成立**——文本列的零拷贝借用与"不产 DataValue 枚举、不产中间 JSON 串"是**后端进程内**的收益,与前端契约走 A 还是 B 无关。

数值/时间列:老链路解码+字符串化+写 JSON。新链路——**路线 A**:解码+字符串化+写 msgpack(省掉的是"JSON 文本序列化"这层,值仍字符串化以对齐契约);**路线 B**:解码+写 msgpack 原生 int/float(再省掉字符串化中间态)。即数值列的字符串化红利要到路线 B 才吃到,文本列的零拷贝红利路线 A 就有。

---

## 分阶段落地

1. **ZMCDataSet 骨架**:`ZmcSchema`(name+OID)、`ZmcDataSet` 持有 `Vec<Row>`、读侧访问 API、`collect(RowStream)`。单元测试:构造 + 按列取值不 panic。
2. **列式二进制编码(路线 A)**:`encode_columnar_binary`(msgpack),逐 Type 分派,**值编码对齐老 DataValue 契约**(`B64:`/`$null:`/Decimal 字符串化,文本列零拷贝借用)。测试:msgpack 解出的结构与老 `ColumnarCodec::encode` 的 JSON **逐字段同构、值一致**(保证前端 `fromJSON` 可无改动复用)。
3. **cmx-database-pg 出口**:`query_zmc` / `query_zmc_streaming` + manager 门面方法。集成测试(真实 PG):查宽表 → 编码 → msgpack decode 校验。
4. **ZmcDocLoader**:BFS 建树产 ZmcDataSet,按父 id 挂子;空表 schema 兜底。测试:多层单据树 → 二进制包 → 结构正确。
5. **API 二进制 endpoint** + `mw_trace` 豁免 + `ApiResp` 二进制 IntoResponse。
6. **前端(组件零改动)**:加 `@msgpack/msgpack`;`cmx-doc-source.js::loadDocData` 一处换 `arrayBuffer()+msgpack.decode()`,`CmxDataSet.fromJSON` 及组件不动;信封拦截器加二进制分支(§5.2)。
7. **端到端 + 内存对标**:一个真实业务单据,老 JSON 路径 vs 新二进制路径,对比响应体大小、后端峰值内存、端到端延迟,写基准报告。
8. **(可选,后续)路线 B 针对性优化**:确认 base64/字符串化是瓶颈后,把后端编码器对应字段类型改原生(bytea→bin、int→int、null→nil),前端 `fromJSON` 同步适配这些字段。逐类型灰度,不一次铺满。

---

## 风险与验证

| 风险 | 应对 |
|---|---|
| tokio-postgres `Row::get::<T>` 类型不符 **panic** | 编码器一律按 `col_buffer` + Type 手工解,失败降级 nil/空,不用 `get` |
| `Vec<Row>` 保活整块读缓冲(Bytes retention) | 大结果集用 `encode_streaming` 边取边写,不囤 Row;全量模式仅用于中小结果集 |
| 前端两套契约(cmx-data-comp 列式 / cmx-portal 行式) | 本期**只做业务单据列式那套**(cmx-data-comp);cmx-portal 维持 JSON 不动 |
| 前端组件如何吃二进制 | **不吃**——组件只认 `CmxDataSet` 对象,二进制在 `loadDocData` 里解回。仅换一个加载函数(§5.1/5.2) | 唯一注入点;`fromJSON` 及组件零改动 |
| DataValue 契约字节级差异(B64/$null 前缀) | **路线 A(本期):后端编码器保持老契约**(bytea→`B64:`、null→null、Decimal 字符串),仅外层换 msgpack → 前端 `fromJSON` 零改动。路线 B(后续)才改原生 bin/int/nil 并同步前端 | 见 §5.3;先同构后优化,避免一开始铺满改动面 |
| msgpack decode 后结构 vs fromJSON 期望 | 路线 A 下二者逐字段同构,加 encode→decode 往返测试对拍老 columnar JSON | 保证组件零改动的硬前提 |
| wasm 插件路径 | 不改,继续走老 DataSet + msgpack;ZMCDataSet 仅服务 REST→浏览器 |

**验证方式**:
- 单元:ZMCDataSet 构造/取值/编码,与老 columnar JSON 语义等价对拍。
- 集成(真实 PG,`127.0.0.1:5432/cmx`):`query_zmc` → `encode_columnar_binary` → `rmp_serde::from_slice` 解回校验字段与值。
- 端到端:业务单据新 endpoint,浏览器 `@msgpack/msgpack` 解码渲染,与老 JSON endpoint 结果一致。
- 基准:复用 `cmx-database-test` 思路,对比新旧路径的响应体大小 / 峰值内存 / 延迟。

---

## 参考锚点

- 零拷贝地基:tokio-postgres `row.rs`(col_buffer)、postgres-protocol `message/backend.rs`(DataRowBody.storage: Bytes)、postgres-types `lib.rs`(&str/&[u8] FromSql)
- 老 DataSet 契约:`cmx-core/src/model/data/dataset/{rds.rs,columnar.rs,mod.rs}`、`cmx-core/src/model/cell.rs`(DataValue 编码)、`cmx-core/src/model/meta/table.rs`(FieldType)
- tokio-pg 层:`cmx-database-pg/src/{connection,executor,transaction,manager}/`(在此加 zmc 出口)
- 业务单据:`cmx-biz/src/doc/loader.rs`(BFS 建树,ZmcDocLoader 参照)、`cmx-api/src/handlers/portal/doc.rs`(出口)
- 前端:`packages/cmx-data-comp/src/lib/{cmx-doc-source.js,cmx-data-set.js}`(fromJSON 复用点)
- 中间件豁免:`cmx-api/src/middleware/mw_trace.rs`
