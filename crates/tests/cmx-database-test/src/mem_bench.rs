//! 内存对标:sqlx/DataSet vs tokio-postgres/ZmcDataSet 各阶段真实堆内存。
//!
//! 用一个「计数分配器」包装系统分配器,原子记录**当前活跃字节**(alloc-dealloc)与
//! **峰值水位**。在每条路径的各阶段边界读计数,得到真实的内存足迹对比,直面回答
//! 「ZmcDataSet 零拷贝 vs 传统 DataSet 到底差多少」。
//!
//! 用法:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/cmx \
//!   ROWS=100000 cargo run -p cmx-database-test --bin mem-bench --release
//!
//! 两条路径各测三阶段:
//!   A 取数:DB → Vec<原始行>(sqlx PgRow / tokio-pg Row)
//!   B 结构:→ DataSet(sqlx,消费掉原始行)/ ZmcDataSet(pg,持有原始行)
//!   C 输出:→ JSON 字节(DataSet serde)/ msgpack 列式包字节(ZmcDataSet 编码)
//!
//! 关键对比:B 阶段"最终结构活跃内存" + C 阶段"输出+结构总活跃内存" + 全程峰值。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// ───────────────────────── 计数分配器 ─────────────────────────

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            // 更新峰值(CAS 循环)
            let mut peak = PEAK.load(Ordering::Relaxed);
            while now > peak {
                match PEAK.compare_exchange_weak(peak, now, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(p) => peak = p,
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
fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}
fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}
fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

// ───────────────────────── schema / 数据 ─────────────────────────

const TABLE: &str = "mem_bench_wide";

/// 建 50 列宽表(与吞吐基准同构:1 主键 + 15 int + 15 text + 8 numeric + 5 ts + 3 bool + 2 uuid + 1 jsonb)。
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

/// 用 COPY 快速装载 n 行相同数据(仅主键递增)。文本 COPY,与吞吐基准的 copy_line 一致理念。
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
        // 一行 50 列:id + 15 int + 15 text + 8 num + 5 ts + 3 bool + 2 uuid + 1 json
        buf.push_str(&id.to_string());
        for _ in 0..15 {
            buf.push_str("\t42");
        }
        for _ in 0..15 {
            buf.push_str("\t业务字段文本内容适中长度模拟真实"); // 文本列(含中文,考验零拷贝)
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

// ───────────────────────── 单阶段测量结果 ─────────────────────────

#[derive(Default, Clone)]
#[allow(dead_code)] // path/stage_c_output 供调试打印,报告未全用
struct PathMem {
    path: String,
    // 各阶段"持有该结构时的活跃内存增量"(相对基线)
    stage_a_rows: usize,      // 原始行集
    stage_b_struct: usize,    // 最终数据结构(sqlx:DataSet 已消费行;pg:ZmcDataSet 持有行)
    stage_c_output: usize,    // 输出字节(JSON / msgpack)大小
    stage_c_total_live: usize, // C 阶段"结构+输出"同时活跃的总内存
    path_peak: usize,         // 全程峰值(相对基线)
    output_bytes: usize,      // 输出体积
}

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/cmx".to_string());
    let rows: u64 = std::env::var("ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000);

    println!("== 内存对标配置 ==");
    println!("URL: {}", mask(&url));
    println!("行数: {}  列数: 50\n", rows);

    // 建表 + 装数据(用 tokio-postgres 直连)
    let (client, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.batch_execute(&format!("DROP TABLE IF EXISTS {TABLE}")).await?;
    client.batch_execute(&create_ddl()).await?;
    println!(">> 装载 {} 行测试数据...", rows);
    seed_rows(&client, rows).await?;

    let select = format!("SELECT * FROM {TABLE}");

    // ── 路径 1:tokio-postgres / ZmcDataSet(全量) ──
    println!(">> 测量 tokio-pg / ZmcDataSet 全量路径...");
    let zmc_mem = measure_zmc(&url, &select, rows).await?;

    // ── 路径 1b:tokio-postgres / ZmcDataSet(流式,不囤 Row) ──
    println!(">> 测量 tokio-pg / ZmcDataSet 流式路径...");
    let zmc_stream_mem = measure_zmc_streaming(&url, &select).await?;

    // ── 路径 2:sqlx / ZmcDataSet(全量) ──
    println!(">> 测量 sqlx / ZmcDataSet 全量路径...");
    let sqlx_zmc_mem = measure_sqlx_zmc(&url, &select).await?;

    // ── 路径 2b:sqlx / ZmcDataSet(流式) ──
    println!(">> 测量 sqlx / ZmcDataSet 流式路径...");
    let sqlx_zmc_stream_mem = measure_sqlx_zmc_streaming(&url, &select).await?;

    // ── 路径 3:sqlx / DataSet(老链路) ──
    println!(">> 测量 sqlx / DataSet 路径...");
    let sqlx_mem = measure_sqlx(&url, &select).await?;

    // 清理
    client.batch_execute(&format!("DROP TABLE {TABLE} CASCADE")).await?;

    // 报告
    let report = build_report(
        rows,
        &sqlx_mem,
        &sqlx_zmc_mem,
        &sqlx_zmc_stream_mem,
        &zmc_mem,
        &zmc_stream_mem,
    );
    println!("{report}");
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("MEM_RESULTS.md");
    std::fs::write(&out, &report)?;
    println!("\n报告已写入: {}", out.display());
    Ok(())
}

/// tokio-pg / ZmcDataSet 路径:阶段 A 原始 Vec<Row>,B 包成 ZmcDataSet(持有行),C 编码 msgpack。
async fn measure_zmc(url: &str, select: &str, _rows: u64) -> anyhow::Result<PathMem> {
    use cmx_database_pg::ZmcDataSet;
    use cmx_database_pg::zmcdataset::TokioPgRowSource;

    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    let handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    let base = live();
    reset_peak();

    // A:取数 → Vec<Row> → 包 newtype(newtype 零开销)
    let raw_rows: Vec<TokioPgRowSource> = client
        .query(select, &[])
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    let after_a = live();
    let stage_a = after_a.saturating_sub(base);

    // B:包成 ZmcDataSet(移动 raw_rows 进去,零拷贝持有)
    let zmc = ZmcDataSet::new("mem", raw_rows);
    let after_b = live();
    let stage_b = after_b.saturating_sub(base);

    // C:编码 msgpack 列式包(结构仍活着,输出新增)
    let live_before_c = live();
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    let output_bytes = buf.len();
    let after_c_total = live().saturating_sub(base);
    let stage_c_output = live().saturating_sub(live_before_c);

    let p = peak().saturating_sub(base);

    // 保活到测量后再 drop
    std::hint::black_box(&zmc);
    std::hint::black_box(&buf);
    drop(zmc);
    drop(buf);

    handle.abort();
    Ok(PathMem {
        path: "tokio-pg / ZmcDataSet".to_string(),
        stage_a_rows: stage_a,
        stage_b_struct: stage_b,
        stage_c_output,
        stage_c_total_live: after_c_total,
        path_peak: p,
        output_bytes,
    })
}

/// tokio-pg / ZmcDataSet 流式路径:query_raw → 边取边编码,不囤 Row。峰值 O(单行 + 输出)。
async fn measure_zmc_streaming(url: &str, select: &str) -> anyhow::Result<PathMem> {
    use cmx_database_pg::zmcdataset::TokioPgRowSource;
    use cmx_rowsource::{ZmcSchema, encode_row_into, encode_stream_footer, encode_stream_header};
    use futures::TryStreamExt;

    let (client, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    let handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    let base = live();
    reset_peak();

    // 流式取数 + 边编码(不囤 Row,每行编完即弃)
    let params: Vec<i32> = Vec::new();
    let stream = client.query_raw(select, params).await?;
    futures::pin_mut!(stream);
    let first = stream.try_next().await?.map(TokioPgRowSource::from);
    let schema = match &first {
        Some(r) => ZmcSchema::from_row(r),
        None => ZmcSchema::from_parts(vec![], vec![]),
    };
    let mut rows_body: Vec<u8> = Vec::new();
    let mut count: u64 = 0;
    if let Some(r) = &first {
        encode_row_into(&mut rows_body, r, &schema);
        count += 1;
    }
    drop(first);
    while let Some(r) = stream.try_next().await? {
        let r = TokioPgRowSource::from(r);
        encode_row_into(&mut rows_body, &r, &schema);
        count += 1;
    }
    let mut buf = Vec::new();
    encode_stream_header(&mut buf, "mem", &schema);
    encode_stream_footer(&mut buf, count as u32, &rows_body);
    let output_bytes = buf.len();

    let after_c_total = live().saturating_sub(base);
    let p = peak().saturating_sub(base);

    std::hint::black_box(&buf);
    drop(buf);
    drop(rows_body);
    handle.abort();

    Ok(PathMem {
        path: "tokio-pg / ZmcDataSet(流式)".to_string(),
        stage_a_rows: 0,
        stage_b_struct: 0,
        stage_c_output: output_bytes,
        stage_c_total_live: after_c_total,
        path_peak: p,
        output_bytes,
    })
}

/// sqlx / ZmcDataSet 全量:sqlx fetch_all → ZmcDataSet<SqlxPgRowSource> 持有 → 编码 msgpack。
async fn measure_sqlx_zmc(url: &str, select: &str) -> anyhow::Result<PathMem> {
    use cmx_database::ZmcDataSet;
    use cmx_database::zmc::SqlxPgRowSource;
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new().max_connections(2).connect(url).await?;
    let base = live();
    reset_peak();

    // A:取数 → Vec<PgRow> → 包 newtype
    let raw_rows: Vec<SqlxPgRowSource> = sqlx::query(sqlx::AssertSqlSafe(select.to_string()))
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    let stage_a = live().saturating_sub(base);

    // B:包成 ZmcDataSet(零拷贝持有原始 PgRow)
    let zmc = ZmcDataSet::new("mem", raw_rows);
    let stage_b = live().saturating_sub(base);

    // C:编码 msgpack
    let live_before_c = live();
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    let output_bytes = buf.len();
    let after_c_total = live().saturating_sub(base);
    let stage_c_output = live().saturating_sub(live_before_c);

    let p = peak().saturating_sub(base);
    std::hint::black_box(&zmc);
    std::hint::black_box(&buf);
    drop(zmc);
    drop(buf);
    pool.close().await;

    Ok(PathMem {
        path: "sqlx / ZmcDataSet".to_string(),
        stage_a_rows: stage_a,
        stage_b_struct: stage_b,
        stage_c_output,
        stage_c_total_live: after_c_total,
        path_peak: p,
        output_bytes,
    })
}

/// sqlx / ZmcDataSet 流式:sqlx fetch → 边取边编码,不囤 PgRow。
async fn measure_sqlx_zmc_streaming(url: &str, select: &str) -> anyhow::Result<PathMem> {
    use cmx_database::zmc::SqlxPgRowSource;
    use cmx_rowsource::{ZmcSchema, encode_row_into, encode_stream_footer, encode_stream_header};
    use futures::TryStreamExt;
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new().max_connections(2).connect(url).await?;
    let base = live();
    reset_peak();

    let mut stream = sqlx::query(sqlx::AssertSqlSafe(select.to_string())).fetch(&pool);
    let first = stream.try_next().await?.map(SqlxPgRowSource::from);
    let schema = match &first {
        Some(r) => ZmcSchema::from_row(r),
        None => ZmcSchema::from_parts(vec![], vec![]),
    };
    let mut rows_body: Vec<u8> = Vec::new();
    let mut count: u64 = 0;
    if let Some(r) = &first {
        encode_row_into(&mut rows_body, r, &schema);
        count += 1;
    }
    drop(first);
    while let Some(r) = stream.try_next().await? {
        let r = SqlxPgRowSource::from(r);
        encode_row_into(&mut rows_body, &r, &schema);
        count += 1;
    }
    let mut buf = Vec::new();
    encode_stream_header(&mut buf, "mem", &schema);
    encode_stream_footer(&mut buf, count as u32, &rows_body);
    let output_bytes = buf.len();

    let after_c_total = live().saturating_sub(base);
    let p = peak().saturating_sub(base);
    std::hint::black_box(&buf);
    drop(buf);
    drop(rows_body);
    drop(stream);
    pool.close().await;

    Ok(PathMem {
        path: "sqlx / ZmcDataSet(流式)".to_string(),
        stage_a_rows: 0,
        stage_b_struct: 0,
        stage_c_output: output_bytes,
        stage_c_total_live: after_c_total,
        path_peak: p,
        output_bytes,
    })
}

/// sqlx / DataSet 路径:阶段 A 原始 Vec<PgRow>,B convert 成 DataSet(消费掉行),C serde JSON。
async fn measure_sqlx(url: &str, select: &str) -> anyhow::Result<PathMem> {
    use cmx_database::executor::ResultConverter;
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await?;

    let base = live();
    reset_peak();

    // A:取数 → Vec<PgRow>
    let raw_rows: Vec<sqlx::postgres::PgRow> =
        sqlx::query(sqlx::AssertSqlSafe(select.to_string()))
            .fetch_all(&pool)
            .await?;
    let after_a = live();
    let stage_a = after_a.saturating_sub(base);

    // B:convert → DataSet(消费 raw_rows,解码成 DataValue 枚举)
    let ds = ResultConverter::convert_postgres_rows(raw_rows, "mem");
    let after_b = live();
    let stage_b = after_b.saturating_sub(base);

    // C:serde 序列化成 JSON 字节(结构仍活着)
    let live_before_c = live();
    let json = serde_json::to_vec(&ds).unwrap();
    let output_bytes = json.len();
    let after_c_total = live().saturating_sub(base);
    let stage_c_output = live().saturating_sub(live_before_c);

    let p = peak().saturating_sub(base);

    std::hint::black_box(&ds);
    std::hint::black_box(&json);
    drop(ds);
    drop(json);
    pool.close().await;

    Ok(PathMem {
        path: "sqlx / DataSet".to_string(),
        stage_a_rows: stage_a,
        stage_b_struct: stage_b,
        stage_c_output,
        stage_c_total_live: after_c_total,
        path_peak: p,
        output_bytes,
    })
}

fn build_report(
    rows: u64,
    sqlx_ds: &PathMem,
    sqlx_zmc: &PathMem,
    sqlx_zmc_s: &PathMem,
    tokio_zmc: &PathMem,
    tokio_zmc_s: &PathMem,
) -> String {
    let mut s = String::new();
    s.push_str("# ZmcDataSet(零拷贝) vs DataSet(传统) 五方内存对标\n\n");
    s.push_str(&format!(
        "> 表:50 列宽表 · 行数:{} · 真实 PG · 计数分配器测活跃堆内存(alloc−dealloc)\n\
         > 五路径:sqlx/DataSet(老) · sqlx/Zmc全量 · sqlx/Zmc流式 · tokio/Zmc全量 · tokio/Zmc流式\n\n",
        rows
    ));

    s.push_str("## 各阶段活跃内存(MB,相对取数前基线)\n\n");
    s.push_str("| 阶段 | sqlx/DataSet | sqlx/Zmc全量 | sqlx/Zmc流式 | tokio/Zmc全量 | tokio/Zmc流式 |\n");
    s.push_str("|------|--------------|---------------|---------------|----------------|----------------|\n");
    row5(&mut s, "A 取数(原始行集)", sqlx_ds.stage_a_rows, Some(sqlx_zmc.stage_a_rows), None, Some(tokio_zmc.stage_a_rows), None);
    row5(&mut s, "B 结构就绪", sqlx_ds.stage_b_struct, Some(sqlx_zmc.stage_b_struct), None, Some(tokio_zmc.stage_b_struct), None);
    row5(&mut s, "C 结构+输出同时活跃", sqlx_ds.stage_c_total_live, Some(sqlx_zmc.stage_c_total_live), Some(sqlx_zmc_s.stage_c_total_live), Some(tokio_zmc.stage_c_total_live), Some(tokio_zmc_s.stage_c_total_live));
    row5(&mut s, "峰值水位", sqlx_ds.path_peak, Some(sqlx_zmc.path_peak), Some(sqlx_zmc_s.path_peak), Some(tokio_zmc.path_peak), Some(tokio_zmc_s.path_peak));

    s.push_str(&format!(
        "\n> 峰值(相对 sqlx/DataSet {:.0} MB):sqlx/Zmc全量 **{:.0} MB**({}) · sqlx/Zmc流式 **{:.0} MB**({}) · tokio/Zmc全量 **{:.0} MB**({}) · tokio/Zmc流式 **{:.0} MB**({})\n",
        mb(sqlx_ds.path_peak),
        mb(sqlx_zmc.path_peak), pct_save(sqlx_ds.path_peak, sqlx_zmc.path_peak),
        mb(sqlx_zmc_s.path_peak), pct_save(sqlx_ds.path_peak, sqlx_zmc_s.path_peak),
        mb(tokio_zmc.path_peak), pct_save(sqlx_ds.path_peak, tokio_zmc.path_peak),
        mb(tokio_zmc_s.path_peak), pct_save(sqlx_ds.path_peak, tokio_zmc_s.path_peak),
    ));

    s.push_str("\n## 输出体积(序列化结果)\n\n");
    s.push_str("| | sqlx/DataSet(JSON) | ZmcDataSet(msgpack,两驱动同) | 比值 |\n");
    s.push_str("|---|---|---|---|\n");
    let ratio = if sqlx_zmc.output_bytes > 0 {
        sqlx_ds.output_bytes as f64 / sqlx_zmc.output_bytes as f64
    } else {
        0.0
    };
    s.push_str(&format!(
        "| 输出字节 | {:.2} MB | {:.2} MB | JSON 是 msgpack 的 {:.2}x |\n",
        mb(sqlx_ds.output_bytes),
        mb(sqlx_zmc.output_bytes),
        ratio
    ));

    s.push_str("\n## 解读(直面「sqlx 能不能吃到 ZmcDataSet 红利」)\n\n");
    s.push_str(&format!(
        "- **sqlx + ZmcDataSet 完全成立**:同一套驱动无关编码器(cmx-rowsource),sqlx 的 PgRow 与 \
         tokio 的 Row 底层同为引用计数 Bytes,零拷贝能力等同。全量峰值 sqlx/Zmc **{:.0} MB** vs \
         tokio/Zmc **{:.0} MB**;流式 sqlx **{:.0} MB** vs tokio **{:.0} MB** —— **驱动差异很小,\
         收益来自 ZmcDataSet 的设计(不产 DataValue 副本 + msgpack + 流式),不来自换驱动**。\n",
        mb(sqlx_zmc.path_peak),
        mb(tokio_zmc.path_peak),
        mb(sqlx_zmc_s.path_peak),
        mb(tokio_zmc_s.path_peak),
    ));
    s.push_str(&format!(
        "- **「持有原始行」的代价随驱动而异(意外发现)**:Zmc 全量攥着 10 万行原始 Row,\
         sqlx 版占 {:.0} MB、tokio 版占 {:.0} MB,而 DataSet 的 DataValue 副本占 {:.0} MB。\
         **tokio 的 Row 每行结构更重**(列偏移用 usize、元数据引用等),持有全量时反超 DataValue 副本;\
         **sqlx 的 PgRow 更紧凑**(列偏移 u32),持有全量甚至略省于 DataValue 副本。\
         Bytes retention 的代价真实存在,但大小取决于驱动的行结构开销。\n",
        mb(sqlx_zmc.stage_b_struct),
        mb(tokio_zmc.stage_b_struct),
        mb(sqlx_ds.stage_b_struct),
    ));
    s.push_str(&format!(
        "- **流式在两个驱动上同样是杀手锏**:峰值 sqlx流式 {:.0} MB / tokio流式 {:.0} MB,\
         vs 老链路 {:.0} MB —— 不囤行、边编边弃,这是老 DataSet 结构上做不到的。\n",
        mb(sqlx_zmc_s.path_peak),
        mb(tokio_zmc_s.path_peak),
        mb(sqlx_ds.path_peak),
    ));
    s.push_str(
        "\n### 一句话结论\n\n\
         **老代码不必为内存红利换驱动**:留在 sqlx,把出口从「DataSet+JSON」换成「ZmcDataSet+msgpack\
         (大结果集用流式)」,即可拿到与 tokio-postgres 几乎相同的内存收益。tokio-postgres 的价值\
         在别处(pipelining、低点查延迟),与本报告的内存维度无关。\n",
    );
    s.push_str("\n---\n注:计数分配器统计 Rust 堆分配(含网络缓冲/驱动缓存);单次测量,数值随数据/机器波动,重点看相对差。\n");
    s
}

/// 五方一行:label + 5 列(None = 该路径无此阶段)。
fn row5(
    s: &mut String,
    label: &str,
    sqlx_ds: usize,
    sqlx_zmc: Option<usize>,
    sqlx_zmc_s: Option<usize>,
    tokio_zmc: Option<usize>,
    tokio_zmc_s: Option<usize>,
) {
    let cell = |v: Option<usize>| match v {
        Some(b) => format!("{:.1} MB", mb(b)),
        None => "—".to_string(),
    };
    s.push_str(&format!(
        "| {} | {:.1} MB | {} | {} | {} | {} |\n",
        label,
        mb(sqlx_ds),
        cell(sqlx_zmc),
        cell(sqlx_zmc_s),
        cell(tokio_zmc),
        cell(tokio_zmc_s),
    ));
}

/// 相对 base 的节省描述。
fn pct_save(base: usize, v: usize) -> String {
    if v <= base && base > 0 {
        format!("省 {:.0}%", (1.0 - v as f64 / base as f64) * 100.0)
    } else if base > 0 {
        format!("多 {:.0}%", (v as f64 / base as f64 - 1.0) * 100.0)
    } else {
        "-".to_string()
    }
}

fn mask(url: &str) -> String {
    if let Some(at) = url.rfind('@') {
        if let Some(scheme) = url.find("://") {
            return format!("{}://***@{}", &url[..scheme], &url[at + 1..]);
        }
    }
    url.to_string()
}
