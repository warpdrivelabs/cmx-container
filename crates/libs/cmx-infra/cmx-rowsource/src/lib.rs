//! cmx-rowsource —— 驱动无关的行来源抽象 + 零拷贝列式二进制(msgpack)编码器。
//!
//! 把「从一行取某列的值」和「列的类型」抽象成 [`ZmcRowSource`] trait + 中立枚举
//! [`ZmcColType`],使同一套零拷贝 msgpack 列式编码器能同时服务 sqlx 的 `PgRow` 和
//! tokio-postgres 的 `Row` —— 两个 driver crate 各自 `impl ZmcRowSource`,本 crate 不
//! 依赖任何具体驱动。
//!
//! 编码产出「列式包」结构(与老 `ColumnarCodec` 的 JSON 逐字段同构,前端 `CmxDataSet.fromJSON`
//! 可无改动复用):`{datasetId, columns:[名...], rows:[[值...]...], childRows:{父id:{childKey:子包}}}`。
//! 值编码对齐老 `DataValue` 契约:`Binary→"B64:"+base64`、`Decimal/DateTime/Date/Uuid→字符串`、
//! `Json→原字符串`、`Null→nil`、`Int→msgpack int`、`Float→msgpack float`、`Bool→msgpack bool`。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rmp::encode as mp;

/// 中立列类型(driver 把自己的 PG 类型映射过来,编码器只认这个)。
///
/// 粒度对齐编码分派:Int2/4/8 决定取值宽度;Json/Jsonb 决定能否零拷贝借 `&str`;
/// Timestamp/Timestamptz 决定 chrono 目标类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZmcColType {
    Bool,
    Int2,
    Int4,
    Int8,
    Float4,
    Float8,
    Numeric,
    /// TEXT/VARCHAR/BPCHAR/NAME/CHAR 归并
    Text,
    /// JSON 文本 wire,可零拷贝借 &str
    Json,
    /// JSONB 二进制 wire 带版本头,不能借 &str,走 serde_json::Value
    Jsonb,
    Uuid,
    Bytea,
    Date,
    Timestamp,
    Timestamptz,
    /// 兜底当字符串
    Unknown,
}

/// 行取值抽象 —— driver 的一行(sqlx PgRow / tokio Row)实现此 trait。
///
/// 取值语义:SQL NULL 或解码失败一律返回 `None`(对齐编码器「失败即 nil 不 panic」)。
/// 借用类(`get_str`/`get_bytes`)输出绑 `&self`,零拷贝借出底层 `Bytes`。
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
    /// 零拷贝借出 UTF-8 文本(文本/JSON 列)
    fn get_str(&self, i: usize) -> Option<&str>;
    /// 零拷贝借出二进制(BYTEA)
    fn get_bytes(&self, i: usize) -> Option<&[u8]>;
    fn get_uuid(&self, i: usize) -> Option<uuid::Uuid>;
    fn get_date(&self, i: usize) -> Option<chrono::NaiveDate>;
    fn get_naive_datetime(&self, i: usize) -> Option<chrono::NaiveDateTime>;
    fn get_datetime_utc(&self, i: usize) -> Option<chrono::DateTime<chrono::Utc>>;
    /// JSONB 解码取值(带版本头,无法借 &str)
    fn get_json_value(&self, i: usize) -> Option<serde_json::Value>;
}

/// 轻量列元信息(列名 + 中立列类型),`Arc` 共享。
#[derive(Debug, Clone)]
pub struct ZmcSchema {
    pub columns: Vec<String>,
    pub types: Vec<ZmcColType>,
    index: HashMap<String, usize>,
}

impl ZmcSchema {
    /// 从一行的列元信息构造(driver 的行实现 `col_name`/`col_type`)。
    pub fn from_row<R: ZmcRowSource>(row: &R) -> Self {
        let n = row.col_count();
        let mut columns = Vec::with_capacity(n);
        let mut types = Vec::with_capacity(n);
        let mut index = HashMap::with_capacity(n);
        for i in 0..n {
            let name = row.col_name(i).to_string();
            index.insert(name.clone(), i);
            columns.push(name);
            types.push(row.col_type(i));
        }
        Self {
            columns,
            types,
            index,
        }
    }

    /// 用显式列名 + 类型构造(空结果集兜底 / 权威 schema 覆盖)。
    pub fn from_parts(columns: Vec<String>, types: Vec<ZmcColType>) -> Self {
        let index = columns
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        Self {
            columns,
            types,
            index,
        }
    }

    pub fn col_count(&self) -> usize {
        self.columns.len()
    }

    pub fn col_index(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }
}

/// 子集分组:父数据集下按某 childKey 挂载的子数据集 + 每子行的父 id 字符串。
pub struct ZmcChildGroup<R: ZmcRowSource> {
    /// 前端 `childRows[父id][childKey]` 的 key
    pub child_key: String,
    pub child: ZmcDataSet<R>,
    /// 与 child.rows 等长,预算好的父 id 字符串
    pub parent_ids: Vec<String>,
}

/// 零拷贝数据集:持有原始行 `Vec<R>`(内含引用计数 Bytes),惰性列式二进制编码。
pub struct ZmcDataSet<R: ZmcRowSource> {
    pub id: String,
    pub schema: Arc<ZmcSchema>,
    pub rows: Vec<R>,
    pub children: Vec<ZmcChildGroup<R>>,
}

impl<R: ZmcRowSource> ZmcDataSet<R> {
    /// 用已有行构造(schema 从首行推导;空行 → 空 schema)。
    pub fn new(id: impl Into<String>, rows: Vec<R>) -> Self {
        let schema = match rows.first() {
            Some(r) => Arc::new(ZmcSchema::from_row(r)),
            None => Arc::new(ZmcSchema::from_parts(vec![], vec![])),
        };
        Self {
            id: id.into(),
            schema,
            rows,
            children: Vec::new(),
        }
    }

    /// 用显式 schema 构造(空表兜底:即使 0 行也保留列信息)。
    pub fn with_schema(id: impl Into<String>, schema: Arc<ZmcSchema>, rows: Vec<R>) -> Self {
        Self {
            id: id.into(),
            schema,
            rows,
            children: Vec::new(),
        }
    }

    pub fn empty(id: impl Into<String>, schema: Arc<ZmcSchema>) -> Self {
        Self {
            id: id.into(),
            schema,
            rows: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn add_child_group(&mut self, group: ZmcChildGroup<R>) {
        self.children.push(group);
    }

    /// 按列名取某行的借用字符串(仅文本/JSON 列,零拷贝)。
    pub fn col_str(&self, row: usize, col: usize) -> Option<&str> {
        self.rows.get(row)?.get_str(col)
    }

    /// 取某行某列的「父 id 分桶用字符串键」(int/text/uuid 统一成字符串)。
    pub fn row_key_string(&self, row: usize, col: usize) -> Option<String> {
        let r = self.rows.get(row)?;
        let ty = *self.schema.types.get(col)?;
        stringify_key(r, col, ty)
    }
}

/// 把一列的值转成「分桶键字符串」(int/text/uuid/其它)。失败 → None。
pub fn stringify_key<R: ZmcRowSource>(row: &R, col: usize, ty: ZmcColType) -> Option<String> {
    match ty {
        ZmcColType::Int2 => row.get_i16(col).map(|v| v.to_string()),
        ZmcColType::Int4 => row.get_i32(col).map(|v| v.to_string()),
        ZmcColType::Int8 => row.get_i64(col).map(|v| v.to_string()),
        ZmcColType::Text | ZmcColType::Json => row.get_str(col).map(|s| s.to_string()),
        ZmcColType::Uuid => row.get_uuid(col).map(|u| u.to_string()),
        _ => row.get_str(col).map(|s| s.to_string()),
    }
}

// ============================================================================
// 列式二进制编码(msgpack,路线 A —— 对齐老 columnar JSON 契约)
// ============================================================================

impl<R: ZmcRowSource> ZmcDataSet<R> {
    /// 编码为列式包 msgpack,写入 `buf`。输出 `{datasetId, columns, rows, childRows?}`。
    pub fn encode_columnar_binary(&self, buf: &mut Vec<u8>) {
        let has_children = !self.children.is_empty();
        let map_len = if has_children { 4 } else { 3 };
        mp::write_map_len(buf, map_len).unwrap();

        mp::write_str(buf, "datasetId").unwrap();
        mp::write_str(buf, &self.id).unwrap();

        mp::write_str(buf, "columns").unwrap();
        mp::write_array_len(buf, self.schema.col_count() as u32).unwrap();
        for name in &self.schema.columns {
            mp::write_str(buf, name).unwrap();
        }

        mp::write_str(buf, "rows").unwrap();
        mp::write_array_len(buf, self.rows.len() as u32).unwrap();
        for row in &self.rows {
            encode_row_into(buf, row, &self.schema);
        }

        if has_children {
            mp::write_str(buf, "childRows").unwrap();
            self.encode_child_rows(buf);
        }
    }

    fn encode_child_rows(&self, buf: &mut Vec<u8>) {
        self.encode_child_rows_scoped(buf, None);
    }

    /// 按父 id 把各子层的行分桶,返回**稳定顺序**(父 id 首次出现序)的
    /// `[(父id, [(childKey, 子数据集, 该父下的子行下标)...])...]`。
    ///
    /// `scope` 为 `Some(set)` 时**只**收父 id ∈ set 的子行(下钻时传「当前父行子集自身的
    /// id 集合」,把孙层限定在这批父行下——否则每个更深层会按祖先行数被重复整层产出,
    /// 导致载荷组合级膨胀)。`None` = 根层全量。二进制/JSON 两个编码器共用此分桶,避免逻辑漂移。
    ///
    /// `scope` 里放的是**父 id 字符串**(与子行的 `parent_ids[i]` 同一取值口径),不是行下标。
    #[allow(clippy::type_complexity)]
    fn bucket_children<'a>(
        &'a self,
        scope: Option<&HashSet<String>>,
    ) -> Vec<(String, Vec<(&'a str, &'a ZmcDataSet<R>, Vec<usize>)>)> {
        let mut by_parent: HashMap<String, Vec<(&'a str, &'a ZmcDataSet<R>, Vec<usize>)>> =
            HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for group in &self.children {
            let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
            for (child_row_idx, pid) in group.parent_ids.iter().enumerate() {
                if let Some(set) = scope
                    && !set.contains(pid) {
                        continue; // 该子行的父不在当前子集内,跳过(核心:限定下钻范围)
                    }
                buckets.entry(pid.clone()).or_default().push(child_row_idx);
            }
            for (pid, idxs) in buckets {
                if !by_parent.contains_key(&pid) {
                    order.push(pid.clone());
                }
                by_parent
                    .entry(pid)
                    .or_default()
                    .push((group.child_key.as_str(), &group.child, idxs));
            }
        }
        order
            .into_iter()
            .map(|pid| {
                let groups = by_parent.remove(&pid).unwrap();
                (pid, groups)
            })
            .collect()
    }

    /// 按父 id 分桶写 `childRows`(msgpack)。分桶复用 [`bucket_children`](Self::bucket_children)。
    fn encode_child_rows_scoped(&self, buf: &mut Vec<u8>, scope: Option<&HashSet<String>>) {
        let buckets = self.bucket_children(scope);
        mp::write_map_len(buf, buckets.len() as u32).unwrap();
        for (pid, child_groups) in &buckets {
            mp::write_str(buf, pid).unwrap();
            mp::write_map_len(buf, child_groups.len() as u32).unwrap();
            for (child_key, child_ds, idxs) in child_groups {
                mp::write_str(buf, child_key).unwrap();
                child_ds.encode_child_subset(buf, idxs);
            }
        }
    }

    /// 收集本数据集 `row_idxs` 这批行的 `id` 列值(字符串键),作为下钻孙层的父 id 作用域。
    /// 无 `id` 列时返回空集(与老 JSON 编码一致:无 id 列则不产 childRows)。
    fn subset_id_scope(&self, row_idxs: &[usize]) -> HashSet<String> {
        let mut set = HashSet::with_capacity(row_idxs.len());
        if let Some(id_idx) = self.schema.col_index("id") {
            for &ri in row_idxs {
                if let Some(key) = self.row_key_string(ri, id_idx) {
                    set.insert(key);
                }
            }
        }
        set
    }

    fn encode_child_subset(&self, buf: &mut Vec<u8>, row_idxs: &[usize]) {
        let has_children = !self.children.is_empty();
        let map_len = if has_children { 4 } else { 3 };
        mp::write_map_len(buf, map_len).unwrap();

        mp::write_str(buf, "datasetId").unwrap();
        mp::write_str(buf, &self.id).unwrap();

        mp::write_str(buf, "columns").unwrap();
        mp::write_array_len(buf, self.schema.col_count() as u32).unwrap();
        for name in &self.schema.columns {
            mp::write_str(buf, name).unwrap();
        }

        mp::write_str(buf, "rows").unwrap();
        mp::write_array_len(buf, row_idxs.len() as u32).unwrap();
        for &ri in row_idxs {
            if let Some(row) = self.rows.get(ri) {
                encode_row_into(buf, row, &self.schema);
            } else {
                mp::write_array_len(buf, 0).unwrap();
            }
        }

        if has_children {
            mp::write_str(buf, "childRows").unwrap();
            // 只对本子集这批父行下钻孙层:作用域 = 本子集行的 id 集合,
            // 避免把整层孙数据按祖先重复产出。
            let scope = self.subset_id_scope(row_idxs);
            self.encode_child_rows_scoped(buf, Some(&scope));
        }
    }

    // ========================================================================
    // 列式 JSON 编码(路线 B —— 同一零拷贝 ZmcDataSet,出口换成纯 JSON Value)
    //
    // 与 encode_columnar_binary 逐字段同构(datasetId/columns/rows/childRows),但产
    // serde_json::Value 走普通 ApiResp 信封,前端无需 msgpack 解码,直接 CmxDataSet.fromJSON。
    // 值编码复用 encode_cell 的同一套 DataValue 契约(见 encode_cell_json)。子层分桶复用
    // bucket_children(与二进制同一作用域逻辑,不会漂移)。
    // ========================================================================

    /// 编码为列式包 `serde_json::Value`。输出 `{datasetId, columns, rows, childRows?}`,
    /// 与 [`encode_columnar_binary`](Self::encode_columnar_binary) / 老 `ColumnarCodec` 同构。
    pub fn encode_columnar_json(&self) -> serde_json::Value {
        let row_idxs: Vec<usize> = (0..self.rows.len()).collect();
        self.encode_columnar_json_subset(&row_idxs, None)
    }

    /// 内部:按给定行下标 + 可选父作用域产出本层 JSON(根层传全量行 + `scope=None`)。
    fn encode_columnar_json_subset(
        &self,
        row_idxs: &[usize],
        scope: Option<&HashSet<String>>,
    ) -> serde_json::Value {
        use serde_json::{Map, Value};

        let mut obj = Map::new();
        obj.insert("datasetId".into(), Value::String(self.id.clone()));

        let cols: Vec<Value> = self
            .schema
            .columns
            .iter()
            .map(|c| Value::String(c.clone()))
            .collect();
        obj.insert("columns".into(), Value::Array(cols));

        let rows: Vec<Value> = row_idxs
            .iter()
            .map(|&ri| match self.rows.get(ri) {
                Some(row) => encode_row_json(row, &self.schema),
                None => Value::Array(Vec::new()),
            })
            .collect();
        obj.insert("rows".into(), Value::Array(rows));

        if !self.children.is_empty() {
            // 根层 scope=None → 全量;子层 scope=Some(本子集 id 集) → 只下钻这批父。
            let buckets = self.bucket_children(scope);
            if !buckets.is_empty() {
                let mut child_rows = Map::new();
                for (pid, child_groups) in &buckets {
                    let mut per_child = Map::new();
                    for (child_key, child_ds, idxs) in child_groups {
                        let sub_scope = child_ds.subset_id_scope(idxs);
                        let child_pkg =
                            child_ds.encode_columnar_json_subset(idxs, Some(&sub_scope));
                        per_child.insert((*child_key).to_string(), child_pkg);
                    }
                    child_rows.insert(pid.clone(), Value::Object(per_child));
                }
                obj.insert("childRows".into(), Value::Object(child_rows));
            }
        }

        Value::Object(obj)
    }
}

/// 把单行按 schema 编成一个 JSON 值数组(与 [`encode_row_into`] 的 msgpack 逐值同构)。
pub fn encode_row_json<R: ZmcRowSource>(row: &R, schema: &ZmcSchema) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = (0..schema.col_count())
        .map(|col| encode_cell_json(row, col, schema.types[col]))
        .collect();
    serde_json::Value::Array(arr)
}

/// 编码单个单元格为 JSON —— 与 [`encode_cell`] 的 msgpack 分派值语义完全一致
/// (Decimal/Date/DateTime/Uuid→字符串、Bytea→"B64:"、Jsonb→字符串、Null→null),失败一律 `null`。
pub fn encode_cell_json<R: ZmcRowSource>(
    row: &R,
    col: usize,
    ty: ZmcColType,
) -> serde_json::Value {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use serde_json::Value;

    match ty {
        ZmcColType::Bool => row.get_bool(col).map_or(Value::Null, Value::Bool),
        ZmcColType::Int2 => row
            .get_i16(col)
            .map_or(Value::Null, |v| Value::from(v as i64)),
        ZmcColType::Int4 => row
            .get_i32(col)
            .map_or(Value::Null, |v| Value::from(v as i64)),
        ZmcColType::Int8 => row.get_i64(col).map_or(Value::Null, Value::from),
        ZmcColType::Float4 => row
            .get_f32(col)
            .map_or(Value::Null, |v| Value::from(v as f64)),
        ZmcColType::Float8 => row.get_f64(col).map_or(Value::Null, Value::from),
        // NUMERIC → Decimal → 字符串(保精度)
        ZmcColType::Numeric => row
            .get_decimal(col)
            .map_or(Value::Null, |d| Value::String(d.to_string())),
        // 文本/JSON:JSON 里当普通字符串
        ZmcColType::Text | ZmcColType::Json => row
            .get_str(col)
            .map_or(Value::Null, |s| Value::String(s.to_string())),
        // JSONB:解码后序列化成字符串(对齐老契约:childRows 里 JSONB 存字符串)
        ZmcColType::Jsonb => row
            .get_json_value(col)
            .map_or(Value::Null, |j| Value::String(j.to_string())),
        ZmcColType::Uuid => row
            .get_uuid(col)
            .map_or(Value::Null, |u| Value::String(u.to_string())),
        ZmcColType::Bytea => row
            .get_bytes(col)
            .map_or(Value::Null, |b| Value::String(format!("B64:{}", BASE64.encode(b)))),
        ZmcColType::Date => row
            .get_date(col)
            .map_or(Value::Null, |d| Value::String(d.to_string())),
        ZmcColType::Timestamp => row.get_naive_datetime(col).map_or(Value::Null, |ndt| {
            let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
            Value::String(dt.to_rfc3339())
        }),
        ZmcColType::Timestamptz => row
            .get_datetime_utc(col)
            .map_or(Value::Null, |dt| Value::String(dt.to_rfc3339())),
        ZmcColType::Unknown => row
            .get_str(col)
            .map_or(Value::Null, |s| Value::String(s.to_string())),
    }
}

/// 把单行按 schema 编成一个 msgpack 数组,追加到 `body`。
///
/// 性能:一个 `scratch` String 在行内 50 列间复用,数值/时间/uuid/bytea 用 `write!` 写进它、
/// `write_str` 后 `clear()` —— 避免每格 `to_string()`/`to_rfc3339()`/`format!` 各分配一个临时
/// String(10 万行 × 多列 = 上百万次分配)。文本列仍零拷贝借出、不经 scratch。
pub fn encode_row_into<R: ZmcRowSource>(body: &mut Vec<u8>, row: &R, schema: &ZmcSchema) {
    let mut scratch = String::with_capacity(48);
    mp::write_array_len(body, schema.col_count() as u32).unwrap();
    for col in 0..schema.col_count() {
        encode_cell(body, row, col, schema.types[col], &mut scratch);
    }
}

/// 手写 RFC3339(UTC)写入器 —— 输出与 `DateTime<Utc>::to_rfc3339()` 逐字节一致,
/// 零分配、零格式机(strftime 解释器逐 Item 派发比直写慢)。常规范围(0..=9999 年、
/// 无闰秒)走快路径;极端值回退 chrono 标准实现(分配一次,可忽略)。
fn write_rfc3339_utc(out: &mut String, dt: &chrono::DateTime<chrono::Utc>) {
    use chrono::{Datelike, Timelike};
    let year = dt.year();
    let nanos = dt.nanosecond();
    if !(0..=9999).contains(&year) || nanos >= 1_000_000_000 {
        out.push_str(&dt.to_rfc3339());
        return;
    }
    #[inline]
    fn push2(out: &mut String, v: u32) {
        out.push((b'0' + (v / 10) as u8) as char);
        out.push((b'0' + (v % 10) as u8) as char);
    }
    let y = year as u32;
    push2(out, y / 100);
    push2(out, y % 100);
    out.push('-');
    push2(out, dt.month());
    out.push('-');
    push2(out, dt.day());
    out.push('T');
    push2(out, dt.hour());
    out.push(':');
    push2(out, dt.minute());
    out.push(':');
    push2(out, dt.second());
    // 小数秒对齐 to_rfc3339 的 AutoSi:0 → 省略;毫秒整 → 3 位;微秒整 → 6 位;否则 9 位
    if nanos != 0 {
        out.push('.');
        if nanos.is_multiple_of(1_000_000) {
            let ms = nanos / 1_000_000;
            out.push((b'0' + (ms / 100) as u8) as char);
            push2(out, ms % 100);
        } else if nanos.is_multiple_of(1_000) {
            let us = nanos / 1_000;
            let mut div = 100_000;
            for _ in 0..6 {
                out.push((b'0' + (us / div % 10) as u8) as char);
                div /= 10;
            }
        } else {
            let mut div = 100_000_000;
            for _ in 0..9 {
                out.push((b'0' + (nanos / div % 10) as u8) as char);
                div /= 10;
            }
        }
    }
    out.push_str("+00:00");
}

/// 编码单个单元格 —— 核心分派:按中立类型取值 + 写 msgpack,失败一律 `nil` 不 panic。
///
/// `scratch`:调用方提供的复用 String(见 [`encode_row_into`]),字符串化的值写进它再
/// `write_str`,行内复用免每格分配。
pub fn encode_cell<R: ZmcRowSource>(
    buf: &mut Vec<u8>,
    row: &R,
    col: usize,
    ty: ZmcColType,
    scratch: &mut String,
) {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use std::fmt::Write as _;

    macro_rules! nil {
        () => {{
            mp::write_nil(buf).unwrap();
            return;
        }};
    }
    /// 把 Display 值经 scratch 写成 msgpack str(免临时 String 分配)。
    macro_rules! str_via_scratch {
        ($v:expr) => {{
            scratch.clear();
            let _ = write!(scratch, "{}", $v);
            mp::write_str(buf, scratch).unwrap();
        }};
    }

    match ty {
        ZmcColType::Bool => match row.get_bool(col) {
            Some(v) => mp::write_bool(buf, v).unwrap(),
            None => nil!(),
        },
        ZmcColType::Int2 => match row.get_i16(col) {
            Some(v) => {
                mp::write_sint(buf, v as i64).unwrap();
            }
            None => nil!(),
        },
        ZmcColType::Int4 => match row.get_i32(col) {
            Some(v) => {
                mp::write_sint(buf, v as i64).unwrap();
            }
            None => nil!(),
        },
        ZmcColType::Int8 => match row.get_i64(col) {
            Some(v) => {
                mp::write_sint(buf, v).unwrap();
            }
            None => nil!(),
        },
        ZmcColType::Float4 => match row.get_f32(col) {
            Some(v) => mp::write_f64(buf, v as f64).unwrap(),
            None => nil!(),
        },
        ZmcColType::Float8 => match row.get_f64(col) {
            Some(v) => mp::write_f64(buf, v).unwrap(),
            None => nil!(),
        },
        // NUMERIC → Decimal → 字符串(保精度;经 scratch 免临时分配)
        ZmcColType::Numeric => match row.get_decimal(col) {
            Some(d) => str_via_scratch!(d),
            None => nil!(),
        },
        // 文本/JSON:零拷贝借出 &str
        ZmcColType::Text | ZmcColType::Json => match row.get_str(col) {
            Some(s) => mp::write_str(buf, s).unwrap(),
            None => nil!(),
        },
        // JSONB:解码后序列化成字符串(serde_json 序列化必然分配,经 scratch 统一)
        ZmcColType::Jsonb => match row.get_json_value(col) {
            Some(j) => str_via_scratch!(j),
            None => nil!(),
        },
        ZmcColType::Uuid => match row.get_uuid(col) {
            Some(u) => str_via_scratch!(u),
            None => nil!(),
        },
        // BYTEA → "B64:"+base64(base64 直接编进 scratch,免 format! 拼接)
        ZmcColType::Bytea => match row.get_bytes(col) {
            Some(b) => {
                scratch.clear();
                scratch.push_str("B64:");
                BASE64.encode_string(b, scratch);
                mp::write_str(buf, scratch).unwrap();
            }
            None => nil!(),
        },
        ZmcColType::Date => match row.get_date(col) {
            Some(d) => str_via_scratch!(d),
            None => nil!(),
        },
        ZmcColType::Timestamp => match row.get_naive_datetime(col) {
            Some(ndt) => {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                scratch.clear();
                write_rfc3339_utc(scratch, &dt);
                mp::write_str(buf, scratch).unwrap();
            }
            None => nil!(),
        },
        ZmcColType::Timestamptz => match row.get_datetime_utc(col) {
            Some(dt) => {
                scratch.clear();
                write_rfc3339_utc(scratch, &dt);
                mp::write_str(buf, scratch).unwrap();
            }
            None => nil!(),
        },
        ZmcColType::Unknown => match row.get_str(col) {
            Some(s) => mp::write_str(buf, s).unwrap(),
            None => nil!(),
        },
    }
}

// ============================================================================
// 流式编码骨架(driver 的流式管道逐行调 encode_row_into,单缓冲直写 + 回填长度)
// ============================================================================

/// 写列式包头并**预留 rows 数组长度占位**,返回占位起始偏移。
///
/// 单缓冲设计:driver 之后把各行用 [`encode_row_into`] **直接编进同一个 `out`**(追加),
/// 最后用 [`encode_stream_close`] 回填长度 —— 相比旧的 header+footer 双缓冲,省一份
/// `rows_body`(峰值 O(全部行 ≈ 整个输出体积))+ 一次整段 memcpy。
///
/// 关键:msgpack `array 32`(`0xdd` + 4 字节大端 u32)是**定宽**的,与元素个数无关,
/// 因此行数未知时也能先占位、编完后回填。array32 对任意长度(含小数组)都是合法编码。
pub fn encode_stream_open(out: &mut Vec<u8>, dataset_id: &str, schema: &ZmcSchema) -> usize {
    mp::write_map_len(out, 3).unwrap();
    mp::write_str(out, "datasetId").unwrap();
    mp::write_str(out, dataset_id).unwrap();
    mp::write_str(out, "columns").unwrap();
    mp::write_array_len(out, schema.col_count() as u32).unwrap();
    for name in &schema.columns {
        mp::write_str(out, name).unwrap();
    }
    mp::write_str(out, "rows").unwrap();
    // 预留 array32 标记:0xdd + 4 字节大端长度占位(定宽,后续回填)
    let marker_pos = out.len();
    out.push(0xdd);
    out.extend_from_slice(&[0, 0, 0, 0]);
    marker_pos
}

/// 回填 [`encode_stream_open`] 预留的 rows 数组长度(大端 u32)。
///
/// `marker_pos` 为 open 的返回值;`out` 在此期间只能追加(rows 直接编在占位之后),
/// 占位偏移保持有效。
pub fn encode_stream_close(out: &mut [u8], marker_pos: usize, row_count: u32) {
    out[marker_pos + 1..marker_pos + 5].copy_from_slice(&row_count.to_be_bytes());
}

// ============================================================================
// 真·分帧流式协议(chunked,峰值内存 O(单行))—— 行数未知也能边查边发边收
// ============================================================================
//
// 上面的 encode_stream_header/footer 需先知道行数(msgpack 数组长度),故 driver 仍要把
// 全部行编进临时 buf → 峰值 O(全部行)。要真正 O(单行) 网络流式,改用**长度分帧**协议:
//
//   [帧] = [u32 大端 payload 长度][payload 字节]
//   第 1 帧: header  payload = msgpack `{datasetId, columns:[...]}`
//   第 2..N 帧: row  payload = msgpack 行数组 `[值...]`
//   终止帧: len = 0(无 payload)
//
// 每行编完即可作为一帧发出、随即丢弃 → 服务端峰值 O(单行)。前端按帧读取、逐帧解码累积。

/// 写一个长度分帧的头帧:`[u32 len][msgpack {datasetId, columns}]`,追加到 `out`。
pub fn encode_frame_header(out: &mut Vec<u8>, dataset_id: &str, schema: &ZmcSchema) {
    let mut payload = Vec::with_capacity(64);
    mp::write_map_len(&mut payload, 2).unwrap();
    mp::write_str(&mut payload, "datasetId").unwrap();
    mp::write_str(&mut payload, dataset_id).unwrap();
    mp::write_str(&mut payload, "columns").unwrap();
    mp::write_array_len(&mut payload, schema.col_count() as u32).unwrap();
    for name in &schema.columns {
        mp::write_str(&mut payload, name).unwrap();
    }
    push_frame(out, &payload);
}

/// 写一个行帧:`[u32 len][msgpack 行数组]`,追加到 `out`。行编完即可发出、丢弃。
pub fn encode_frame_row<R: ZmcRowSource>(out: &mut Vec<u8>, row: &R, schema: &ZmcSchema) {
    let mut payload = Vec::with_capacity(schema.col_count() * 8);
    encode_row_into(&mut payload, row, schema);
    push_frame(out, &payload);
}

/// 写终止帧:`[u32 = 0]`(无 payload),标识流结束。
pub fn encode_frame_end(out: &mut Vec<u8>) {
    out.extend_from_slice(&0u32.to_be_bytes());
}

/// 帧封装:`[u32 大端 len][payload]`。
fn push_frame(out: &mut Vec<u8>, payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}

// ============================================================================
// 测试:多层 childRows 作用域(回归——防止孙层按祖先重复产出的组合级膨胀)
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// 极简 mock 行:只有若干 Int8 列(id / upper_id 足够复现多层分桶)。
    struct MockRow {
        cols: Vec<&'static str>,
        vals: Vec<i64>,
    }
    impl ZmcRowSource for MockRow {
        fn col_count(&self) -> usize {
            self.cols.len()
        }
        fn col_name(&self, i: usize) -> &str {
            self.cols[i]
        }
        fn col_type(&self, _i: usize) -> ZmcColType {
            ZmcColType::Int8
        }
        fn get_bool(&self, _i: usize) -> Option<bool> {
            None
        }
        fn get_i16(&self, _i: usize) -> Option<i16> {
            None
        }
        fn get_i32(&self, _i: usize) -> Option<i32> {
            None
        }
        fn get_i64(&self, i: usize) -> Option<i64> {
            self.vals.get(i).copied()
        }
        fn get_f32(&self, _i: usize) -> Option<f32> {
            None
        }
        fn get_f64(&self, _i: usize) -> Option<f64> {
            None
        }
        fn get_decimal(&self, _i: usize) -> Option<rust_decimal::Decimal> {
            None
        }
        fn get_str(&self, _i: usize) -> Option<&str> {
            None
        }
        fn get_bytes(&self, _i: usize) -> Option<&[u8]> {
            None
        }
        fn get_uuid(&self, _i: usize) -> Option<uuid::Uuid> {
            None
        }
        fn get_date(&self, _i: usize) -> Option<chrono::NaiveDate> {
            None
        }
        fn get_naive_datetime(&self, _i: usize) -> Option<chrono::NaiveDateTime> {
            None
        }
        fn get_datetime_utc(&self, _i: usize) -> Option<chrono::DateTime<chrono::Utc>> {
            None
        }
        fn get_json_value(&self, _i: usize) -> Option<serde_json::Value> {
            None
        }
    }

    fn ds(id: &str, cols: Vec<&'static str>, rows: Vec<Vec<i64>>) -> ZmcDataSet<MockRow> {
        let schema = Arc::new(ZmcSchema::from_parts(
            cols.iter().map(|s| s.to_string()).collect(),
            vec![ZmcColType::Int8; cols.len()],
        ));
        let rows = rows
            .into_iter()
            .map(|vals| MockRow {
                cols: cols.clone(),
                vals,
            })
            .collect();
        ZmcDataSet::with_schema(id.to_string(), schema, rows)
    }

    /// 三层树:batch(2) → header(每 batch 2) → acc(每 header 1)。供多个测试共用。
    fn sample_tree() -> ZmcDataSet<MockRow> {
        // L1: batch id 1,2
        let mut batch = ds("cv_batch", vec!["id"], vec![vec![1], vec![2]]);
        // L2: header id 10..13, upper_id 指向 batch
        let mut header = ds(
            "cv_header",
            vec!["id", "upper_id"],
            vec![vec![10, 1], vec![11, 1], vec![12, 2], vec![13, 2]],
        );
        // L3: acc id 100.., upper_id 指向 header(每 header 1 行)
        let acc = ds(
            "cv_acc_line",
            vec!["id", "upper_id"],
            vec![
                vec![100, 10],
                vec![101, 11],
                vec![102, 12],
                vec![103, 13],
            ],
        );
        // header 挂 acc(parent_ids = 各 acc 行的 upper_id 字符串)
        header.add_child_group(ZmcChildGroup {
            child_key: "cv_acc_line".to_string(),
            child: acc,
            parent_ids: vec!["10".into(), "11".into(), "12".into(), "13".into()],
        });
        // batch 挂 header(parent_ids = 各 header 行的 upper_id 字符串)
        batch.add_child_group(ZmcChildGroup {
            child_key: "cv_header".to_string(),
            child: header,
            parent_ids: vec!["1".into(), "1".into(), "2".into(), "2".into()],
        });
        batch
    }

    /// 三层树:batch(2) → header(每 batch 2) → acc(每 header 1)。
    /// 断言 childRows 按父 id 严格作用域——每层只在其父下出现一次,无组合级重复。
    #[test]
    fn child_rows_scoped_no_duplication() {
        let batch = sample_tree();

        let mut buf = Vec::new();
        batch.encode_columnar_binary(&mut buf);
        let v: serde_json::Value = rmp_serde::from_slice(&buf).expect("decode msgpack");

        // 顶层 childRows: 父 id "1","2"
        let cr = v["childRows"].as_object().expect("childRows map");
        assert_eq!(cr.len(), 2, "两个 batch 各一桶");

        // batch 1 → 2 个 header(id 10,11);每个 header 下只有本 header 的 1 条 acc
        let b1_headers = &cr["1"]["cv_header"];
        assert_eq!(b1_headers["rows"].as_array().unwrap().len(), 2);
        let b1_child = b1_headers["childRows"].as_object().unwrap();
        // 关键:header 10、11 的孙层只挂各自 1 行,且不含 batch 2 的 header(12,13)
        assert_eq!(b1_child.len(), 2, "只含本 batch 的两个 header 作父");
        assert!(b1_child.contains_key("10") && b1_child.contains_key("11"));
        assert!(!b1_child.contains_key("12") && !b1_child.contains_key("13"));
        assert_eq!(b1_child["10"]["cv_acc_line"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(b1_child["10"]["cv_acc_line"]["rows"][0][0], 100);

        // batch 2 → header 12,13,同样各自 1 条 acc,不串到 batch 1
        let b2_child = cr["2"]["cv_header"]["childRows"].as_object().unwrap();
        assert_eq!(b2_child.len(), 2);
        assert!(b2_child.contains_key("12") && b2_child.contains_key("13"));
        assert!(!b2_child.contains_key("10"));
        assert_eq!(b2_child["12"]["cv_acc_line"]["rows"][0][0], 102);

        // 载荷规模健壮性:acc 层总共只有 4 行,编码里 "cv_acc_line" 键应恰好出现 4 次
        // (修复前会随 batch 数重复 → 8 次)。用解码后结构计数更稳。
        let mut acc_row_total = 0;
        for pid in ["1", "2"] {
            for hchild in cr[pid]["cv_header"]["childRows"].as_object().unwrap().values() {
                acc_row_total += hchild["cv_acc_line"]["rows"].as_array().unwrap().len();
            }
        }
        assert_eq!(acc_row_total, 4, "acc 行不因祖先重复放大");
    }

    /// JSON 编码与二进制编码**逐字段同构**:msgpack 解回的 Value 应与 encode_columnar_json 相等。
    /// 一并证明 JSON 路也享有同一套父作用域(无组合级重复)。
    #[test]
    fn json_encoding_matches_binary() {
        let batch = sample_tree();

        let mut buf = Vec::new();
        batch.encode_columnar_binary(&mut buf);
        let from_bin: serde_json::Value = rmp_serde::from_slice(&buf).expect("decode msgpack");

        let from_json = batch.encode_columnar_json();

        assert_eq!(
            from_json, from_bin,
            "JSON 出口应与 msgpack 出口逐字段同构(含 childRows 作用域)"
        );

        // 直接在 JSON 上复核关键作用域断言(不经二进制)
        let cr = from_json["childRows"].as_object().unwrap();
        assert_eq!(cr.len(), 2);
        let b1 = cr["1"]["cv_header"]["childRows"].as_object().unwrap();
        assert!(b1.contains_key("10") && b1.contains_key("11"));
        assert!(!b1.contains_key("12") && !b1.contains_key("13"));
        assert_eq!(from_json["rows"].as_array().unwrap().len(), 2);
    }
}


#[cfg(test)]
mod rfc3339_tests {
    use super::*;

    /// 手写 writer 的输出必须与 to_rfc3339() 逐字节一致(前端契约依赖此格式)。
    #[test]
    fn rfc3339_items_equiv_to_rfc3339() {
        use chrono::TimeZone;
        let cases = [
            chrono::Utc.with_ymd_and_hms(2026, 7, 5, 12, 0, 0).unwrap(),
            chrono::Utc.timestamp_opt(1_751_700_000, 123_456_789).unwrap(), // 纳秒(9位)
            chrono::Utc.timestamp_opt(1_751_700_000, 123_456_000).unwrap(), // 微秒整(6位)
            chrono::Utc.timestamp_opt(1_751_700_000, 123_000_000).unwrap(), // 毫秒整(3位)
            chrono::Utc.timestamp_opt(0, 0).unwrap(),                       // epoch
            chrono::Utc.timestamp_opt(1_751_700_000, 500_000_000).unwrap(), // .5 秒
            chrono::Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap(),        // 极小年份
            chrono::Utc.with_ymd_and_hms(9999, 12, 31, 23, 59, 59).unwrap(),
        ];
        for dt in cases {
            let mut s = String::new();
            write_rfc3339_utc(&mut s, &dt);
            assert_eq!(s, dt.to_rfc3339(), "格式不等价: {dt}");
        }
        // 回退路径:负年份走 chrono 标准实现,仍等价
        let neg = chrono::Utc.with_ymd_and_hms(-1, 6, 1, 1, 2, 3).unwrap();
        let mut s = String::new();
        write_rfc3339_utc(&mut s, &neg);
        assert_eq!(s, neg.to_rfc3339());
    }
}

#[cfg(test)]
mod stream_buffer_tests {
    use super::*;

    /// 单缓冲 open+close 的输出必须与"旧 header + array_len + rows_body"逐字节一致
    /// （前端 decodeMsgpack 依赖此结构:{datasetId, columns, rows:[...]}）。
    #[test]
    fn single_buffer_equiv_to_header_footer() {
        let schema = ZmcSchema::from_parts(
            vec!["id".into(), "name".into()],
            vec![ZmcColType::Int8, ZmcColType::Text],
        );
        // 模拟已编码的两行(内容任意,只验结构拼装等价)
        let mut fake_rows = Vec::new();
        for i in 0..2i64 {
            mp::write_array_len(&mut fake_rows, 2).unwrap();
            mp::write_sint(&mut fake_rows, i).unwrap();
            mp::write_str(&mut fake_rows, "x").unwrap();
        }

        // 旧法:header → array_len(count) → rows_body(等价于原 encode_stream_footer)
        let mut old = Vec::new();
        {
            mp::write_map_len(&mut old, 3).unwrap();
            mp::write_str(&mut old, "datasetId").unwrap();
            mp::write_str(&mut old, "d").unwrap();
            mp::write_str(&mut old, "columns").unwrap();
            mp::write_array_len(&mut old, 2).unwrap();
            mp::write_str(&mut old, "id").unwrap();
            mp::write_str(&mut old, "name").unwrap();
            mp::write_str(&mut old, "rows").unwrap();
            mp::write_array_len(&mut old, 2).unwrap(); // 旧 footer 用最紧凑的 array 编码
            old.extend_from_slice(&fake_rows);
        }

        // 新法:open(预留 array32)→ 直接追加行 → close 回填
        let mut new = Vec::new();
        let marker = encode_stream_open(&mut new, "d", &schema);
        new.extend_from_slice(&fake_rows);
        encode_stream_close(&mut new, marker, 2);

        // 二者解码回同一个 serde_json::Value(array16/array32 只是长度前缀宽度不同,语义一致)
        let old_v: serde_json::Value = rmp_serde::from_slice(&old).unwrap();
        let new_v: serde_json::Value = rmp_serde::from_slice(&new).unwrap();
        assert_eq!(old_v, new_v, "单缓冲输出与旧法应解码等价");
        assert_eq!(new_v["rows"].as_array().unwrap().len(), 2);
        assert_eq!(new_v["datasetId"], "d");
    }

    /// 0 行也要合法(空结果集)。
    #[test]
    fn single_buffer_zero_rows() {
        let schema = ZmcSchema::from_parts(vec!["id".into()], vec![ZmcColType::Int8]);
        let mut out = Vec::new();
        let marker = encode_stream_open(&mut out, "empty", &schema);
        encode_stream_close(&mut out, marker, 0);
        let v: serde_json::Value = rmp_serde::from_slice(&out).unwrap();
        assert_eq!(v["rows"].as_array().unwrap().len(), 0);
        assert_eq!(v["columns"].as_array().unwrap().len(), 1);
    }
}
