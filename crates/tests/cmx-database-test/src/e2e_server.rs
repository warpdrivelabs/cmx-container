//! 端到端对比服务器 —— 老 DataSet/JSON 链路 vs Zmc 零拷贝二进制链路(sqlx / tokio-pg)。
//!
//! 三个 endpoint,同一张 50 列宽表、同样行数,服务端逐环节测「用时 + 活跃/峰值堆内存」,
//! 指标放响应头,前端侧(e2e_bench.mjs,Node/V8)测下载、解析、展示构建:
//!
//!   GET /old/json       sqlx 流式取行(fetch,逐行) → **DataSet 全量物化**(结构使然,
//!                       无法流式编码) → ColumnarCodec 列式 JSON → serde_json 字节
//!   GET /sqlx/zmc.bin   sqlx 流式取行 → 逐行 Zmc 编码即弃(真流式) → msgpack 字节
//!   GET /tokio/zmc.bin  tokio-pg query_raw 流式 → 逐行 Zmc 编码即弃 → msgpack 字节
//!
//! 响应头:x-t-fetch-ms / x-t-encode-ms / x-mem-struct-b / x-mem-total-b / x-mem-peak-b / x-rows
//!
//! 用法:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/cmx ROWS=100000 PORT=18099 \
//!     cargo run -p cmx-database-test --bin e2e-server --release
//!   然后跑 e2e_bench.mjs(见 crate 根)。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use axum::Router;
use axum::http::HeaderMap;
use axum::routing::get;
use futures::TryStreamExt;

// ───────────────────────── 计数分配器(与 mem_bench 相同) ─────────────────────────

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            let mut peak = PEAK.load(Ordering::Relaxed);
            while now > peak {
                match PEAK.compare_exchange_weak(peak, now, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(p2) => peak = p2,
                }
            }
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}
fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}
fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

// ───────────────────────── 表结构/装载(与 mem_bench 同构) ─────────────────────────

const TABLE: &str = "e2e_bench_wide";

fn create_ddl() -> String {
    let mut cols = vec!["id BIGINT PRIMARY KEY".to_string()];
    for i in 0..15 {
        cols.push(format!("int_{i} BIGINT"));
    }
    for i in 0..15 {
        cols.push(format!("txt_{i} TEXT"));
    }
    for i in 0..8 {
        cols.push(format!("num_{i} NUMERIC(18,4)"));
    }
    for i in 0..5 {
        cols.push(format!("ts_{i} TIMESTAMPTZ"));
    }
    for i in 0..3 {
        cols.push(format!("flag_{i} BOOLEAN"));
    }
    for i in 0..2 {
        cols.push(format!("uid_{i} UUID"));
    }
    cols.push("payload JSONB".to_string());
    format!("CREATE TABLE {TABLE} (\n  {}\n)", cols.join(",\n  "))
}

fn columns() -> Vec<String> {
    let mut c = vec!["id".to_string()];
    for i in 0..15 {
        c.push(format!("int_{i}"));
    }
    for i in 0..15 {
        c.push(format!("txt_{i}"));
    }
    for i in 0..8 {
        c.push(format!("num_{i}"));
    }
    for i in 0..5 {
        c.push(format!("ts_{i}"));
    }
    for i in 0..3 {
        c.push(format!("flag_{i}"));
    }
    for i in 0..2 {
        c.push(format!("uid_{i}"));
    }
    c.push("payload".to_string());
    c
}

async fn seed_rows(client: &tokio_postgres::Client, n: u64) -> anyhow::Result<()> {
    use bytes::Bytes;
    use futures::{SinkExt, pin_mut};
    let cols = columns().join(",");
    let sink = client
        .copy_in::<_, Bytes>(&format!("COPY {TABLE} ({cols}) FROM STDIN WITH (FORMAT text)"))
        .await?;
    pin_mut!(sink);
    let mut buf = String::with_capacity(1 << 20);
    for id in 0..n as i64 {
        buf.push_str(&id.to_string());
        for _ in 0..15 {
            buf.push_str("\t42");
        }
        for _ in 0..15 {
            buf.push_str("\t业务字段文本内容适中长度模拟真实");
        }
        for _ in 0..8 {
            buf.push_str("\t1130000.5000");
        }
        for _ in 0..5 {
            buf.push_str("\t2026-07-05T12:00:00Z");
        }
        for _ in 0..3 {
            buf.push_str("\tt");
        }
        for _ in 0..2 {
            buf.push_str("\t11111111-1111-1111-1111-111111111111");
        }
        buf.push_str("\t{\"k\":\"v\",\"n\":1}");
        buf.push('\n');
        if buf.len() >= (1 << 20) {
            sink.send(Bytes::from(std::mem::take(&mut buf))).await?;
        }
    }
    if !buf.is_empty() {
        sink.send(Bytes::from(buf)).await?;
    }
    sink.finish().await?;
    Ok(())
}

// ───────────────────────── 应用状态 ─────────────────────────

#[derive(Clone)]
struct App {
    sqlx_pool: sqlx::PgPool,
    url: String,
    select: String,
}

fn metric_headers(
    fetch_ms: f64,
    encode_ms: f64,
    mem_struct: usize,
    mem_total: usize,
    mem_peak: usize,
    rows: u64,
) -> HeaderMap {
    let mut h = HeaderMap::new();
    let ins = |h: &mut HeaderMap, k: &'static str, v: String| {
        h.insert(k, v.parse().unwrap());
    };
    ins(&mut h, "x-t-fetch-ms", format!("{fetch_ms:.1}"));
    ins(&mut h, "x-t-encode-ms", format!("{encode_ms:.1}"));
    ins(&mut h, "x-mem-struct-b", mem_struct.to_string());
    ins(&mut h, "x-mem-total-b", mem_total.to_string());
    ins(&mut h, "x-mem-peak-b", mem_peak.to_string());
    ins(&mut h, "x-rows", rows.to_string());
    h
}

/// 可选 gzip:压缩 body,置 `Content-Encoding: gzip`(浏览器/fetch 透明解压,前端解码路径不变),
/// 并在 `x-raw-bytes` / `x-wire-bytes` 头暴露压缩前后体积,供基准精确测量压缩率。
/// `?gzip=1` 开启。压缩耗时计入 `x-t-gzip-ms`。
fn maybe_gzip(h: &mut HeaderMap, body: Vec<u8>, want: bool) -> Vec<u8> {
    let raw = body.len();
    h.insert("x-raw-bytes", raw.to_string().parse().unwrap());
    if !want {
        h.insert("x-wire-bytes", raw.to_string().parse().unwrap());
        h.insert("x-t-gzip-ms", "0".parse().unwrap());
        return body;
    }
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    let t = Instant::now();
    let mut enc = GzEncoder::new(Vec::with_capacity(raw / 3), Compression::default());
    enc.write_all(&body).unwrap();
    let gz = enc.finish().unwrap();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    h.insert("content-encoding", "gzip".parse().unwrap());
    h.insert("x-wire-bytes", gz.len().to_string().parse().unwrap());
    h.insert("x-t-gzip-ms", format!("{ms:.1}").parse().unwrap());
    gz
}

/// 查询参数:`?gzip=1` 开启响应 gzip。
#[derive(serde::Deserialize, Default)]
struct Params {
    #[serde(default)]
    gzip: Option<u8>,
}
impl Params {
    fn want_gzip(&self) -> bool {
        self.gzip == Some(1)
    }
}

// ───────────────────────── 三个 endpoint ─────────────────────────

/// 老链路:sqlx 流式取行 → DataSet 全量物化 → ColumnarCodec 列式 JSON。
///
/// 注意:「流式」只到驱动取行为止 —— DataSet/ColumnarCodec 结构上要求全量物化,
/// 这正是与 Zmc 流式的本质差异(对比要点,不是实现偷懒)。
async fn old_json(
    axum::extract::State(app): axum::extract::State<App>,
    axum::extract::Query(params): axum::extract::Query<Params>,
) -> (HeaderMap, Vec<u8>) {
    use cmx_core::model::data::dataset::{ColumnarCodec, DataSet};
    use cmx_database::executor::ResultConverter;

    let base = live();
    reset_peak();

    // 环节 1:流式取行(驱动层逐行拉取)→ DataSet 全量物化。
    // 「流式」只到取行为止:DataSet/ColumnarCodec 结构上要求全量,这正是与 Zmc 的本质差异。
    let t0 = Instant::now();
    let mut stream = sqlx::query(sqlx::AssertSqlSafe(app.select.clone())).fetch(&app.sqlx_pool);
    let mut raw_rows: Vec<sqlx::postgres::PgRow> = Vec::new();
    while let Some(r) = stream.try_next().await.unwrap() {
        raw_rows.push(r);
    }
    drop(stream);
    let rows_n = raw_rows.len() as u64;
    let ds: DataSet = ResultConverter::convert_postgres_rows(raw_rows, "e2e");
    let fetch_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mem_struct = live().saturating_sub(base);

    // 环节 2:列式 JSON 编码(ColumnarCodec → serde_json 字节)
    let t1 = Instant::now();
    let pkg = ColumnarCodec::encode(&ds);
    let body = serde_json::to_vec(&serde_json::json!({"code":0,"msg":"success","data":pkg})).unwrap();
    let encode_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let mem_total = live().saturating_sub(base);
    let mem_peak = peak().saturating_sub(base);

    let mut h = metric_headers(fetch_ms, encode_ms, mem_struct, mem_total, mem_peak, rows_n);
    h.insert("content-type", "application/json".parse().unwrap());
    let body = maybe_gzip(&mut h, body, params.want_gzip());
    (h, body)
}

/// sqlx 零拷贝流式:逐行 Zmc 编码即弃 → msgpack(信封 {code,msg,data})。
async fn sqlx_zmc_bin(
    axum::extract::State(app): axum::extract::State<App>,
    axum::extract::Query(params): axum::extract::Query<Params>,
) -> (HeaderMap, Vec<u8>) {
    use cmx_database::zmc::SqlxPgRowSource;
    use cmx_rowsource::{ZmcSchema, encode_row_into, encode_stream_close, encode_stream_open};

    let base = live();
    reset_peak();

    // 环节 1+2 交织:流式取行、边取边编码(每行编完即弃)——分别计时:取行耗时按流侧累计
    let t0 = Instant::now();
    let mut stream = sqlx::query(sqlx::AssertSqlSafe(app.select.clone())).fetch(&app.sqlx_pool);
    let first = stream.try_next().await.unwrap().map(SqlxPgRowSource::from);
    let schema = match &first {
        Some(r) => ZmcSchema::from_row(r),
        None => ZmcSchema::from_parts(vec![], vec![]),
    };

    // 单缓冲:信封头 + 列式包头(预留 rows 长度)先入 body,各行直接编进 body(免 rows_body)
    let te = Instant::now();
    let mut body: Vec<u8> = Vec::new();
    rmp_envelope_header(&mut body);
    let marker = encode_stream_open(&mut body, "e2e", &schema);
    let mut encode_ns: u64 = te.elapsed().as_nanos() as u64;

    let mut count: u64 = 0;
    if let Some(r) = &first {
        let te = Instant::now();
        encode_row_into(&mut body, r, &schema);
        encode_ns += te.elapsed().as_nanos() as u64;
        count += 1;
    }
    drop(first);
    while let Some(r) = stream.try_next().await.unwrap() {
        let r = SqlxPgRowSource::from(r);
        let te = Instant::now();
        encode_row_into(&mut body, &r, &schema);
        encode_ns += te.elapsed().as_nanos() as u64;
        count += 1;
    }
    drop(stream);

    // 回填 rows 数组长度
    let te = Instant::now();
    encode_stream_close(&mut body, marker, count as u32);
    encode_ns += te.elapsed().as_nanos() as u64;

    let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let encode_ms = encode_ns as f64 / 1e6;
    let fetch_ms = total_ms - encode_ms; // 取行(网络+解帧)≈ 总 − 编码
    let mem_total = live().saturating_sub(base);
    let mem_peak = peak().saturating_sub(base);

    let mut h = metric_headers(fetch_ms, encode_ms, 0, mem_total, mem_peak, count);
    h.insert("content-type", "application/x-msgpack".parse().unwrap());
    let body = maybe_gzip(&mut h, body, params.want_gzip());
    (h, body)
}

/// tokio-pg 零拷贝流式:query_raw 逐行 Zmc 编码即弃 → msgpack(信封同上)。
async fn tokio_zmc_bin(
    axum::extract::State(app): axum::extract::State<App>,
    axum::extract::Query(params): axum::extract::Query<Params>,
) -> (HeaderMap, Vec<u8>) {
    use cmx_database_pg::zmcdataset::TokioPgRowSource;
    use cmx_rowsource::{ZmcSchema, encode_row_into, encode_stream_close, encode_stream_open};

    let (client, conn) = tokio_postgres::connect(&app.url, tokio_postgres::NoTls)
        .await
        .unwrap();
    let handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    let base = live();
    reset_peak();

    let t0 = Instant::now();
    let qparams: Vec<i32> = Vec::new();
    let stream = client.query_raw(app.select.as_str(), qparams).await.unwrap();
    futures::pin_mut!(stream);
    let first = stream.try_next().await.unwrap().map(TokioPgRowSource::from);
    let schema = match &first {
        Some(r) => ZmcSchema::from_row(r),
        None => ZmcSchema::from_parts(vec![], vec![]),
    };

    // 单缓冲:信封头 + 列式包头(预留 rows 长度)入 body,各行直接编进 body(免 rows_body)
    let te = Instant::now();
    let mut body: Vec<u8> = Vec::new();
    rmp_envelope_header(&mut body);
    let marker = encode_stream_open(&mut body, "e2e", &schema);
    let mut encode_ns: u64 = te.elapsed().as_nanos() as u64;

    let mut count: u64 = 0;
    if let Some(r) = &first {
        let te = Instant::now();
        encode_row_into(&mut body, r, &schema);
        encode_ns += te.elapsed().as_nanos() as u64;
        count += 1;
    }
    drop(first);
    while let Some(r) = stream.try_next().await.unwrap() {
        let r = TokioPgRowSource::from(r);
        let te = Instant::now();
        encode_row_into(&mut body, &r, &schema);
        encode_ns += te.elapsed().as_nanos() as u64;
        count += 1;
    }

    let te = Instant::now();
    encode_stream_close(&mut body, marker, count as u32);
    encode_ns += te.elapsed().as_nanos() as u64;

    let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let encode_ms = encode_ns as f64 / 1e6;
    let fetch_ms = total_ms - encode_ms;
    let mem_total = live().saturating_sub(base);
    let mem_peak = peak().saturating_sub(base);
    handle.abort();

    let mut h = metric_headers(fetch_ms, encode_ms, 0, mem_total, mem_peak, count);
    h.insert("content-type", "application/x-msgpack".parse().unwrap());
    let body = maybe_gzip(&mut h, body, params.want_gzip());
    (h, body)
}

/// msgpack 信封头:{code:0, msg:"success", data:<紧随其后的列式包>}(3 键 map,先写前两键)。
fn rmp_envelope_header(buf: &mut Vec<u8>) {
    use rmp::encode as mp;
    mp::write_map_len(buf, 3).unwrap();
    mp::write_str(buf, "code").unwrap();
    mp::write_uint(buf, 0).unwrap();
    mp::write_str(buf, "msg").unwrap();
    mp::write_str(buf, "success").unwrap();
    mp::write_str(buf, "data").unwrap();
    // data 值(列式包)由调用方紧接着写入
}

// ───────────────────────── main ─────────────────────────

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    rt.block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/cmx".to_string());
    let rows: u64 = std::env::var("ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let port: u16 = std::env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(18099);

    // 建表 + 装载
    let (client, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.batch_execute(&format!("DROP TABLE IF EXISTS {TABLE}")).await?;
    client.batch_execute(&create_ddl()).await?;
    eprintln!(">> 装载 {rows} 行...");
    seed_rows(&client, rows).await?;
    eprintln!(">> 装载完成");

    let sqlx_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await?;

    let app = App {
        sqlx_pool,
        url: url.clone(),
        select: format!("SELECT * FROM {TABLE}"),
    };

    let router = Router::new()
        .route("/old/json", get(old_json))
        .route("/sqlx/zmc.bin", get(sqlx_zmc_bin))
        .route("/tokio/zmc.bin", get(tokio_zmc_bin))
        .with_state(app);

    let addr = format!("127.0.0.1:{port}");
    eprintln!(">> e2e-server 就绪: http://{addr}  (rows={rows})");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
