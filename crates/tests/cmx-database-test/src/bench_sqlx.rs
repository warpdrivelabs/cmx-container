//! sqlx 驱动的基准操作。

use crate::data::copy_line;
use crate::report::Measure;
use crate::schema::{self, RowTemplate};
use anyhow::Result;
use futures::TryStreamExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, Row};
use std::time::Instant;

const DRIVER: &str = "sqlx";

/// 建连接池（单连接，避免池调度噪声干扰对比）。
pub async fn connect(url: &str, max_conn: u32) -> Result<sqlx::PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(max_conn)
        .min_connections(1)
        .connect(url)
        .await?;
    Ok(pool)
}

/// 重建表（DROP + CREATE）。
pub async fn recreate_table(pool: &sqlx::PgPool, table: &str) -> Result<()> {
    pool.execute(sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS {table}"))))
        .await?;
    pool.execute(sqlx::query(sqlx::AssertSqlSafe(schema::create_table_ddl(table))))
        .await?;
    Ok(())
}

/// 给一条 INSERT query 绑定一行（id + 49 列），列顺序与 schema 一致。
fn bind_row<'q>(
    mut q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    tpl: &'q RowTemplate,
    id: i64,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    q = q.bind(id);
    for v in &tpl.ints {
        q = q.bind(*v);
    }
    for v in &tpl.texts {
        q = q.bind(v.as_str());
    }
    for v in &tpl.nums {
        q = q.bind(*v);
    }
    for v in &tpl.times {
        q = q.bind(*v);
    }
    for v in &tpl.flags {
        q = q.bind(*v);
    }
    for v in &tpl.uuids {
        q = q.bind(*v);
    }
    q = q.bind(tpl.json.clone());
    q
}

/// 场景 A：逐行 INSERT（每行一次 execute），全部包在一个事务里。
pub async fn insert_row_by_row(
    pool: &sqlx::PgPool,
    table: &str,
    tpl: &RowTemplate,
    n: u64,
) -> Result<Measure> {
    let cols = schema::column_names().join(",");
    let sql = format!(
        "INSERT INTO {table} ({cols}) VALUES ({})",
        schema::placeholders(50)
    );
    let start = Instant::now();
    let mut tx = pool.begin().await?;
    for id in 0..n as i64 {
        let q = bind_row(sqlx::query(sqlx::AssertSqlSafe(sql.clone())), tpl, id);
        q.execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Measure::new("insert/逐行", DRIVER, n, start.elapsed()))
}

/// 场景 B：批量多值 INSERT（每批 batch 行拼成一条 INSERT ... VALUES (..),(..)...）。
pub async fn insert_batch(
    pool: &sqlx::PgPool,
    table: &str,
    tpl: &RowTemplate,
    n: u64,
    batch: usize,
) -> Result<Measure> {
    let cols = schema::column_names().join(",");
    let start = Instant::now();
    let mut tx = pool.begin().await?;
    let mut id: i64 = 0;
    let total = n as i64;
    while id < total {
        let this_batch = std::cmp::min(batch as i64, total - id);
        // 构造多值占位符
        let mut groups = Vec::with_capacity(this_batch as usize);
        for row in 0..this_batch {
            let base = (row as usize) * 50;
            let ph: Vec<String> = (1..=50).map(|c| format!("${}", base + c)).collect();
            groups.push(format!("({})", ph.join(",")));
        }
        let sql = format!("INSERT INTO {table} ({cols}) VALUES {}", groups.join(","));
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.clone()));
        for row in 0..this_batch {
            q = bind_row_append(q, tpl, id + row);
        }
        q.execute(&mut *tx).await?;
        id += this_batch;
    }
    tx.commit().await?;
    Ok(Measure::new(
        format!("insert/批量(batch={batch})"),
        DRIVER,
        n,
        start.elapsed(),
    ))
}

/// 与 bind_row 相同，但用于多值批量（不重置 query）。
fn bind_row_append<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    tpl: &'q RowTemplate,
    id: i64,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    bind_row(q, tpl, id)
}

/// 场景 C：COPY（PG 最快批量导入路径），文本格式。
pub async fn insert_copy(
    pool: &sqlx::PgPool,
    table: &str,
    tpl: &RowTemplate,
    n: u64,
) -> Result<Measure> {
    let cols = schema::column_names().join(",");
    let start = Instant::now();
    let mut conn = pool.acquire().await?;
    let mut copy = conn
        .copy_in_raw(&format!(
            "COPY {table} ({cols}) FROM STDIN WITH (FORMAT text)"
        ))
        .await?;
    // 分块发送，避免单个巨大 buffer
    let mut buf = String::with_capacity(1 << 20);
    for id in 0..n as i64 {
        buf.push_str(&copy_line(tpl, id));
        if buf.len() >= (1 << 20) {
            copy.send(buf.as_bytes()).await?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        copy.send(buf.as_bytes()).await?;
    }
    copy.finish().await?;
    Ok(Measure::new("insert/COPY", DRIVER, n, start.elapsed()))
}

/// 场景 D：全量查询 fetch_all（一次性物化所有行）。
pub async fn query_fetch_all(pool: &sqlx::PgPool, table: &str, limit: u64) -> Result<Measure> {
    let sql = format!("SELECT * FROM {table} LIMIT {limit}");
    let start = Instant::now();
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.clone())).fetch_all(pool).await?;
    // 触碰每行第一列，防止编译器/驱动优化掉解码
    let mut sink: i64 = 0;
    for r in &rows {
        let v: i64 = r.try_get("id").unwrap_or(0);
        sink = sink.wrapping_add(v);
    }
    std::hint::black_box(sink);
    Ok(Measure::new(
        format!("query/fetch_all({limit})"),
        DRIVER,
        rows.len() as u64,
        start.elapsed(),
    ))
}

/// 场景 E：流式查询（逐行读取，不全量物化）。
pub async fn query_stream(pool: &sqlx::PgPool, table: &str, limit: u64) -> Result<Measure> {
    let sql = format!("SELECT * FROM {table} LIMIT {limit}");
    let start = Instant::now();
    let mut stream = sqlx::query(sqlx::AssertSqlSafe(sql.clone())).fetch(pool);
    let mut count: u64 = 0;
    let mut sink: i64 = 0;
    while let Some(row) = stream.try_next().await? {
        let v: i64 = row.try_get("id").unwrap_or(0);
        sink = sink.wrapping_add(v);
        count += 1;
    }
    std::hint::black_box(sink);
    Ok(Measure::new(
        format!("query/流式({limit})"),
        DRIVER,
        count,
        start.elapsed(),
    ))
}

/// 场景 F：点查延迟采样（返回原始单次延迟微秒数组，供多轮合并后统一算分位数）。
pub async fn point_query_latency_raw(
    pool: &sqlx::PgPool,
    table: &str,
    max_id: i64,
    samples: usize,
) -> Result<Vec<f64>> {
    let sql = format!("SELECT * FROM {table} WHERE id = $1");
    let mut micros = Vec::with_capacity(samples);
    for i in 0..samples {
        let id = (i as i64 * 7919) % max_id; // 伪随机散布，避免顺序缓存
        let t = Instant::now();
        let _row = sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
            .bind(id)
            .fetch_optional(pool)
            .await?;
        micros.push(t.elapsed().as_secs_f64() * 1_000_000.0);
    }
    Ok(micros)
}
