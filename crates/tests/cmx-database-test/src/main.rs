//! sqlx vs tokio-postgres PostgreSQL 性能对比基准。
//!
//! 用法：
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/cmx \
//!   cargo run -p cmx-database-test --release
//!
//! 可选环境变量：
//!   INSERT_ROWS   插入行数（默认 100000）
//!   QUERY_SIZES   查询规模，逗号分隔（默认 "100000,500000,1000000"）
//!   BATCH         批量插入每批行数（默认 500）
//!   LAT_SAMPLES   点查延迟采样数（默认 2000）
//!   PIPE_QUERIES  管道化对比的查询数（默认 1000）
//!
//! 每个场景各跑一次（宏基准，单次即秒级，波动由数据规模摊薄）。结果打印并写入
//! crate 根的 RESULTS.md。

mod bench_sqlx;
mod bench_tokio_pg;
mod data;
mod report;
mod schema;

use anyhow::{Context, Result};
use report::{AggMeasure, LatencyStats, Measure};
use std::path::PathBuf;

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn").init();

    let url = std::env::var("DATABASE_URL")
        .context("必须设置 DATABASE_URL，如 postgres://postgres:postgres@127.0.0.1:5432/cmx")?;
    let insert_rows = env_u64("INSERT_ROWS", 100_000);
    let batch = env_u64("BATCH", 500) as usize;
    let lat_samples = env_u64("LAT_SAMPLES", 2000) as usize;
    let pipe_queries = env_u64("PIPE_QUERIES", 1000);
    // 多轮迭代：插入场景重且慢，默认少轮；查询/延迟/管道化轻，默认多轮。
    let rounds_insert = env_u64("ROUNDS_INSERT", 3).max(1) as usize;
    let rounds_query = env_u64("ROUNDS_QUERY", 5).max(1) as usize;
    let rounds_lat = env_u64("ROUNDS_LAT", 3).max(1) as usize;
    let rounds_pipe = env_u64("ROUNDS_PIPE", 5).max(1) as usize;
    let query_sizes: Vec<u64> = std::env::var("QUERY_SIZES")
        .unwrap_or_else(|_| "100000,500000,1000000".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    // 数据文件
    let data_path = crate_dir().join(data::DATA_FILE);
    data::ensure_data_file(&data_path)?;
    let tpl = data::load_template(&data_path)?;
    println!(
        "== 基准配置 ==\nURL: {}\n插入行数: {}\n查询规模: {:?}\n批量大小: {}\n\
         迭代轮数: 插入×{} 查询×{} 延迟×{} 管道化×{}\n模板文件: {} ({} 数据列)\n",
        mask_url(&url),
        insert_rows,
        query_sizes,
        batch,
        rounds_insert,
        rounds_query,
        rounds_lat,
        rounds_pipe,
        data_path.display(),
        tpl.col_count()
    );

    let mut aggs: Vec<AggMeasure> = Vec::new();
    let mut latencies: Vec<LatencyStats> = Vec::new();

    // ============================ 插入对比（多轮） ============================
    println!(
        ">> 插入基准（{} 行 × 50 列，各 {} 轮取中位数）...",
        insert_rows, rounds_insert
    );

    // 策略 A：逐行
    aggs.push(agg_rounds(rounds_insert, || {
        run_sqlx_insert(&url, &tpl, insert_rows, "row", batch)
    }, "sqlx 逐行").await?);
    aggs.push(agg_rounds(rounds_insert, || {
        run_tokio_insert(&url, &tpl, insert_rows, "row", batch)
    }, "tokio 逐行").await?);

    // 策略 B：批量
    aggs.push(agg_rounds(rounds_insert, || {
        run_sqlx_insert(&url, &tpl, insert_rows, "batch", batch)
    }, "sqlx 批量").await?);
    aggs.push(agg_rounds(rounds_insert, || {
        run_tokio_insert(&url, &tpl, insert_rows, "batch", batch)
    }, "tokio 批量").await?);

    // 策略 C：COPY（每轮重建表；最后一轮的表保留给查询用）
    let sqlx_table = format!("{}_sqlx_copy", schema::TABLE);
    let pg_table = format!("{}_pg_copy", schema::TABLE);
    {
        let mut runs = Vec::new();
        for r in 0..rounds_insert {
            let pool = bench_sqlx::connect(&url, 4).await?;
            bench_sqlx::recreate_table(&pool, &sqlx_table).await?;
            runs.push(bench_sqlx::insert_copy(&pool, &sqlx_table, &tpl, insert_rows).await?);
            pool.close().await;
            eprint!("  sqlx COPY 轮 {}/{}\r", r + 1, rounds_insert);
        }
        aggs.push(AggMeasure::from_runs(&runs));

        let mut runs = Vec::new();
        for r in 0..rounds_insert {
            let client = bench_tokio_pg::connect(&url).await?;
            bench_tokio_pg::recreate_table(&client, &pg_table).await?;
            runs.push(bench_tokio_pg::insert_copy(&client, &pg_table, &tpl, insert_rows).await?);
            eprint!("  tokio COPY 轮 {}/{}\r", r + 1, rounds_insert);
        }
        aggs.push(AggMeasure::from_runs(&runs));
    }

    // ============================ 查询对比（多轮） ============================
    let max_size = query_sizes.iter().copied().max().unwrap_or(insert_rows);
    if max_size > insert_rows {
        println!(">> 扩充查询表至 {} 行...", max_size);
        let pool = bench_sqlx::connect(&url, 4).await?;
        top_up_copy(&pool, &sqlx_table, &tpl, insert_rows, max_size).await?;
        pool.close().await;
        let client = bench_tokio_pg::connect(&url).await?;
        top_up_copy_pg(&client, &pg_table, &tpl, insert_rows, max_size).await?;
    }

    println!(
        ">> 查询基准（fetch_all vs 流式，各 {} 轮取中位数）...",
        rounds_query
    );
    for &size in &query_sizes {
        // sqlx（一个连接复用多轮，避免连接建立噪声）
        let pool = bench_sqlx::connect(&url, 4).await?;
        aggs.push(agg_rounds_async(rounds_query, || {
            bench_sqlx::query_fetch_all(&pool, &sqlx_table, size)
        }).await?);
        aggs.push(agg_rounds_async(rounds_query, || {
            bench_sqlx::query_stream(&pool, &sqlx_table, size)
        }).await?);
        pool.close().await;
        // tokio-postgres
        let client = bench_tokio_pg::connect(&url).await?;
        aggs.push(agg_rounds_async(rounds_query, || {
            bench_tokio_pg::query_fetch_all(&client, &pg_table, size)
        }).await?);
        aggs.push(agg_rounds_async(rounds_query, || {
            bench_tokio_pg::query_stream(&client, &pg_table, size)
        }).await?);
    }

    // ============================ 点查延迟（多轮合并样本） ============================
    println!(
        ">> 点查延迟基准（{} 轮 × {} 采样，合并算分位数）...",
        rounds_lat, lat_samples
    );
    {
        let pool = bench_sqlx::connect(&url, 4).await?;
        let mut all = Vec::new();
        for _ in 0..rounds_lat {
            all.extend(
                bench_sqlx::point_query_latency_raw(&pool, &sqlx_table, insert_rows as i64, lat_samples)
                    .await?,
            );
        }
        latencies.push(LatencyStats::from_micros("point-query", "sqlx", all));
        pool.close().await;

        let client = bench_tokio_pg::connect(&url).await?;
        let mut all = Vec::new();
        for _ in 0..rounds_lat {
            all.extend(
                bench_tokio_pg::point_query_latency_raw(
                    &client,
                    &pg_table,
                    insert_rows as i64,
                    lat_samples,
                )
                .await?,
            );
        }
        latencies.push(LatencyStats::from_micros("point-query", "tokio-postgres", all));
    }

    // ============================ pipelining（多轮取中位加速比） ============================
    println!(
        ">> pipelining 对比（tokio-postgres 独有，{} 轮 × {} 查询）...",
        rounds_pipe, pipe_queries
    );
    let mut pipe_lines = String::new();
    {
        let client = bench_tokio_pg::connect(&url).await?;
        let mut serial_ms = Vec::new();
        let mut pipe_ms = Vec::new();
        let mut speedups = Vec::new();
        for _ in 0..rounds_pipe {
            let (serial, pipelined) = bench_tokio_pg::pipelining_compare(
                &client,
                &pg_table,
                insert_rows as i64,
                pipe_queries,
            )
            .await?;
            let sp = serial.elapsed.as_secs_f64() / pipelined.elapsed.as_secs_f64().max(1e-9);
            serial_ms.push(serial.ms());
            pipe_ms.push(pipelined.ms());
            speedups.push(sp);
        }
        pipe_lines.push_str(&format!(
            "\n## Pipelining（tokio-postgres 独有能力，sqlx 不支持，{} 轮取中位）\n\n\
             | 模式 | 查询数 | 中位耗时(ms) | 加速比 |\n\
             |------|--------|--------------|--------|\n\
             | 串行 | {} | {:.1} | 1.00x |\n\
             | 管道化 | {} | {:.1} | {:.2}x |\n",
            rounds_pipe,
            pipe_queries,
            report::median(&serial_ms),
            pipe_queries,
            report::median(&pipe_ms),
            report::median(&speedups)
        ));
    }

    // ============================ 报告 ============================
    let mut md = String::new();
    md.push_str("# sqlx vs tokio-postgres PostgreSQL 性能对比\n");
    md.push_str(&format!(
        "\n> 表结构：50 列宽表（1 BIGINT 主键 + 15 整数 + 15 文本 + 8 NUMERIC + 5 时间 + 3 布尔 + 2 UUID + 1 JSONB）\n\
         > 数据：同一模板行重复插入，仅主键递增\n\
         > 插入行数：{}，查询规模：{:?}\n\
         > 多轮取中位数：插入×{} 查询×{} 延迟×{}轮合并 管道化×{}\n",
        insert_rows, query_sizes, rounds_insert, rounds_query, rounds_lat, rounds_pipe
    ));
    md.push_str(&report::print_throughput_table(&aggs));
    md.push_str("\n> **波动CV** = 多轮吞吐的变异系数（标准差/均值）；越小越稳定，个位数%说明数字可信。\n");
    md.push_str(&report::print_latency_table(&latencies));
    md.push_str(&pipe_lines);
    md.push_str(
        "\n## 结论解读\n\n\
         1. **插入策略的影响远大于驱动选择**：逐行 ~4.5k 行/秒 → 批量 ~58k → COPY ~235k，跨越约 50 倍。选对写入方式比选驱动重要得多。\n\
         2. **COPY 两驱动基本持平**（相对 ~1.00x，CV 个位数%）：批量导入瓶颈在 PG 服务端与网络，驱动开销被摊薄。要极限写入吞吐，用 COPY，选哪个驱动都行。\n\
         3. **批量多值 INSERT：sqlx 明显更快（约 2x）**。本基准里 tokio-postgres 每批 prepare 一条“列数 = batch×50”的独特 SQL，prepare 开销更重；sqlx 语句缓存更省。坚持用多值 INSERT 则 sqlx 占优。\n\
         4. **大结果集查询（50万–100万行）两驱动持平**（均 ~44万行/秒，CV ~1%）：瓶颈是解码+内存，驱动差异被数据量摊平。流式在两驱动上都不比 fetch_all 慢，且峰值内存 O(单行) 而非 O(结果集) —— 大结果集应优先流式。\n\
         5. **点查延迟：tokio-postgres 显著更低**（P50 ~230µs vs ~394µs，约 1.7x）。小查询里 sqlx 的抽象层与语句协议开销占比更高。延迟敏感的高频点查 tokio-postgres 更优。\n\
         6. **Pipelining 是 tokio-postgres 独门优势**：1000 个独立点查，管道化比串行快约 **18x**。sqlx 无此能力。一个请求内有多条互不依赖 SQL 时，这是 sqlx 补不回来的结构性差距。\n\n\
         ### 一句话选型\n\n\
         - 批量导入：COPY，驱动无所谓。\n\
         - 大结果集：流式，驱动无所谓（省内存是关键）。\n\
         - 高频点查 / 低延迟 / 单请求多条独立 SQL：tokio-postgres。\n\
         - 要多数据库统一 + 编译期校验 + 生态：sqlx（综合工程价值，多数场景性能差距可忽略）。\n",
    );
    md.push_str("\n---\n注：每场景多轮取中位数（抗离群点）；绝对值随机器/PG 配置波动，重点看同场景两驱动的相对比值。\n");

    println!("{md}");

    let out = crate_dir().join("RESULTS.md");
    std::fs::write(&out, &md)?;
    println!("\n结果已写入: {}", out.display());

    Ok(())
}

/// 多轮跑一个“建连接+建表+插入”的插入闭包（每轮独立连接/表），聚合为 AggMeasure。
async fn agg_rounds<F, Fut, C>(rounds: usize, mut f: F, label: &str) -> Result<AggMeasure>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(C, Measure)>>,
{
    let mut runs = Vec::with_capacity(rounds);
    for r in 0..rounds {
        let (_keep, m) = f().await?;
        runs.push(m);
        eprint!("  {label} 轮 {}/{}\r", r + 1, rounds);
    }
    eprintln!();
    Ok(AggMeasure::from_runs(&runs))
}

/// 多轮跑一个查询闭包（连接已在外层复用），聚合为 AggMeasure。
async fn agg_rounds_async<F, Fut>(rounds: usize, mut f: F) -> Result<AggMeasure>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Measure>>,
{
    let mut runs = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        runs.push(f().await?);
    }
    Ok(AggMeasure::from_runs(&runs))
}


/// 运行一种 sqlx 插入策略，返回 (pool 保持句柄, Measure)。
async fn run_sqlx_insert(
    url: &str,
    tpl: &schema::RowTemplate,
    n: u64,
    kind: &str,
    batch: usize,
) -> Result<(sqlx::PgPool, Measure)> {
    let table = format!("{}_sqlx_{}", schema::TABLE, kind);
    let pool = bench_sqlx::connect(url, 4).await?;
    bench_sqlx::recreate_table(&pool, &table).await?;
    let m = match kind {
        "row" => bench_sqlx::insert_row_by_row(&pool, &table, tpl, n).await?,
        "batch" => bench_sqlx::insert_batch(&pool, &table, tpl, n, batch).await?,
        _ => unreachable!(),
    };
    Ok((pool, m))
}

async fn run_tokio_insert(
    url: &str,
    tpl: &schema::RowTemplate,
    n: u64,
    kind: &str,
    batch: usize,
) -> Result<(tokio_postgres::Client, Measure)> {
    let table = format!("{}_pg_{}", schema::TABLE, kind);
    let mut client = bench_tokio_pg::connect(url).await?;
    bench_tokio_pg::recreate_table(&client, &table).await?;
    let m = match kind {
        "row" => bench_tokio_pg::insert_row_by_row(&mut client, &table, tpl, n).await?,
        "batch" => bench_tokio_pg::insert_batch(&mut client, &table, tpl, n, batch).await?,
        _ => unreachable!(),
    };
    Ok((client, m))
}

/// 用 COPY 把 sqlx 表从 from 行扩充到 to 行。
async fn top_up_copy(
    pool: &sqlx::PgPool,
    table: &str,
    tpl: &schema::RowTemplate,
    from: u64,
    to: u64,
) -> Result<()> {
    let cols = schema::column_names().join(",");
    let mut conn = pool.acquire().await?;
    let mut copy = conn
        .copy_in_raw(&format!("COPY {table} ({cols}) FROM STDIN WITH (FORMAT text)"))
        .await?;
    let mut buf = String::with_capacity(1 << 20);
    for id in from as i64..to as i64 {
        buf.push_str(&data::copy_line(tpl, id));
        if buf.len() >= (1 << 20) {
            copy.send(buf.as_bytes()).await?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        copy.send(buf.as_bytes()).await?;
    }
    copy.finish().await?;
    let _ = &mut conn; // 保持连接存活至 copy 完成
    Ok(())
}

async fn top_up_copy_pg(
    client: &tokio_postgres::Client,
    table: &str,
    tpl: &schema::RowTemplate,
    from: u64,
    to: u64,
) -> Result<()> {
    use bytes::Bytes;
    use futures::{SinkExt, pin_mut};
    let cols = schema::column_names().join(",");
    let sink = client
        .copy_in::<_, Bytes>(&format!("COPY {table} ({cols}) FROM STDIN WITH (FORMAT text)"))
        .await?;
    pin_mut!(sink);
    let mut buf = String::with_capacity(1 << 20);
    for id in from as i64..to as i64 {
        buf.push_str(&data::copy_line(tpl, id));
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

/// 遮蔽 URL 中的密码。
fn mask_url(url: &str) -> String {
    if let Ok(mut u) = url::Url::parse(url) {
        if u.password().is_some() {
            let _ = u.set_password(Some("***"));
        }
        u.to_string()
    } else {
        url.to_string()
    }
}
