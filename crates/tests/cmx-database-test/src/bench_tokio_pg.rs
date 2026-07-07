//! tokio-postgres 驱动的基准操作（含 pipelining）。

use crate::data::copy_line;
use crate::report::Measure;
use crate::schema::{self, RowTemplate};
use anyhow::Result;
use bytes::Bytes;
use futures::{SinkExt, pin_mut};
use postgres_types::ToSql;
use std::time::Instant;
use tokio_postgres::{Client, NoTls};

const DRIVER: &str = "tokio-postgres";

/// 建立单个 tokio-postgres 连接（后台 spawn 其 connection future）。
pub async fn connect(url: &str) -> Result<Client> {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::error!("tokio-postgres connection error: {e}");
        }
    });
    Ok(client)
}

/// 重建表。
pub async fn recreate_table(client: &Client, table: &str) -> Result<()> {
    client
        .batch_execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await?;
    client.batch_execute(&schema::create_table_ddl(table)).await?;
    Ok(())
}

/// 把一行模板打包为 tokio-postgres 参数（id + 49 列，列顺序与 schema 一致）。
fn row_params(tpl: &RowTemplate, id: i64) -> Vec<Box<dyn ToSql + Sync + Send>> {
    let mut p: Vec<Box<dyn ToSql + Sync + Send>> = Vec::with_capacity(50);
    p.push(Box::new(id));
    for v in &tpl.ints {
        p.push(Box::new(*v));
    }
    for v in &tpl.texts {
        p.push(Box::new(v.clone()));
    }
    for v in &tpl.nums {
        p.push(Box::new(*v));
    }
    for v in &tpl.times {
        p.push(Box::new(*v));
    }
    for v in &tpl.flags {
        p.push(Box::new(*v));
    }
    for v in &tpl.uuids {
        p.push(Box::new(*v));
    }
    p.push(Box::new(tpl.json.clone()));
    p
}

fn as_refs(boxed: &[Box<dyn ToSql + Sync + Send>]) -> Vec<&(dyn ToSql + Sync)> {
    boxed
        .iter()
        .map(|b| b.as_ref() as &(dyn ToSql + Sync))
        .collect()
}

/// 场景 A：逐行 INSERT（事务内 prepare 一次，逐行 execute）。
pub async fn insert_row_by_row(
    client: &mut Client,
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
    let tx = client.transaction().await?;
    let stmt = tx.prepare(&sql).await?;
    for id in 0..n as i64 {
        let boxed = row_params(tpl, id);
        let refs = as_refs(&boxed);
        tx.execute(&stmt, &refs).await?;
    }
    tx.commit().await?;
    Ok(Measure::new("insert/逐行", DRIVER, n, start.elapsed()))
}

/// 场景 B：批量多值 INSERT。
pub async fn insert_batch(
    client: &mut Client,
    table: &str,
    tpl: &RowTemplate,
    n: u64,
    batch: usize,
) -> Result<Measure> {
    let cols = schema::column_names().join(",");
    let start = Instant::now();
    let tx = client.transaction().await?;
    let mut id: i64 = 0;
    let total = n as i64;
    while id < total {
        let this_batch = std::cmp::min(batch as i64, total - id);
        let mut groups = Vec::with_capacity(this_batch as usize);
        for row in 0..this_batch {
            let base = (row as usize) * 50;
            let ph: Vec<String> = (1..=50).map(|c| format!("${}", base + c)).collect();
            groups.push(format!("({})", ph.join(",")));
        }
        let sql = format!("INSERT INTO {table} ({cols}) VALUES {}", groups.join(","));
        let mut boxed: Vec<Box<dyn ToSql + Sync + Send>> = Vec::with_capacity(this_batch as usize * 50);
        for row in 0..this_batch {
            boxed.extend(row_params(tpl, id + row));
        }
        let refs = as_refs(&boxed);
        tx.execute(sql.as_str(), &refs).await?;
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

/// 场景 C：COPY（文本格式）。
pub async fn insert_copy(
    client: &Client,
    table: &str,
    tpl: &RowTemplate,
    n: u64,
) -> Result<Measure> {
    let cols = schema::column_names().join(",");
    let start = Instant::now();
    let sink = client
        .copy_in::<_, Bytes>(&format!(
            "COPY {table} ({cols}) FROM STDIN WITH (FORMAT text)"
        ))
        .await?;
    pin_mut!(sink);
    let mut buf = String::with_capacity(1 << 20);
    for id in 0..n as i64 {
        buf.push_str(&copy_line(tpl, id));
        if buf.len() >= (1 << 20) {
            sink.send(Bytes::from(std::mem::take(&mut buf))).await?;
        }
    }
    if !buf.is_empty() {
        sink.send(Bytes::from(buf)).await?;
    }
    sink.finish().await?;
    Ok(Measure::new("insert/COPY", DRIVER, n, start.elapsed()))
}

/// 场景 D：全量查询 query（一次性物化所有行）。
pub async fn query_fetch_all(client: &Client, table: &str, limit: u64) -> Result<Measure> {
    let sql = format!("SELECT * FROM {table} LIMIT {limit}");
    let start = Instant::now();
    let rows = client.query(sql.as_str(), &[]).await?;
    let mut sink: i64 = 0;
    for r in &rows {
        let v: i64 = r.get("id");
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

/// 场景 E：流式查询（query_raw 逐行读取）。
pub async fn query_stream(client: &Client, table: &str, limit: u64) -> Result<Measure> {
    use futures::TryStreamExt;
    let sql = format!("SELECT * FROM {table} LIMIT {limit}");
    let start = Instant::now();
    let params: Vec<i32> = vec![];
    let stream = client.query_raw(sql.as_str(), params).await?;
    pin_mut!(stream);
    let mut count: u64 = 0;
    let mut sink: i64 = 0;
    while let Some(row) = stream.try_next().await? {
        let v: i64 = row.get("id");
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

/// 场景 F：点查延迟采样（返回原始单次延迟微秒数组）。
pub async fn point_query_latency_raw(
    client: &Client,
    table: &str,
    max_id: i64,
    samples: usize,
) -> Result<Vec<f64>> {
    let sql = format!("SELECT * FROM {table} WHERE id = $1");
    let stmt = client.prepare(&sql).await?;
    let mut micros = Vec::with_capacity(samples);
    for i in 0..samples {
        let id = (i as i64 * 7919) % max_id;
        let t = Instant::now();
        let _row = client.query_opt(&stmt, &[&id]).await?;
        micros.push(t.elapsed().as_secs_f64() * 1_000_000.0);
    }
    Ok(micros)
}

/// 场景 G：pipelining（tokio-postgres 独有）—— 并发发起 N 个独立点查，一次性 join。
///
/// 对比“串行发 N 个点查”的往返节省。返回 (串行 Measure, 管道化 Measure)。
pub async fn pipelining_compare(
    client: &Client,
    table: &str,
    max_id: i64,
    queries: u64,
) -> Result<(Measure, Measure)> {
    let sql = format!("SELECT * FROM {table} WHERE id = $1");
    let stmt = client.prepare(&sql).await?;

    // 串行
    let start = Instant::now();
    for i in 0..queries as i64 {
        let id = (i * 7919) % max_id;
        let _ = client.query_opt(&stmt, &[&id]).await?;
    }
    let serial = Measure::new("pipelining/串行", DRIVER, queries, start.elapsed());

    // 管道化：并发 poll 所有 future
    let start = Instant::now();
    let futs: Vec<_> = (0..queries as i64)
        .map(|i| {
            let id = (i * 7919) % max_id;
            let stmt = stmt.clone();
            async move { client.query_opt(&stmt, &[&id]).await }
        })
        .collect();
    let results = futures::future::join_all(futs).await;
    for r in results {
        r?;
    }
    let pipelined = Measure::new("pipelining/管道化", DRIVER, queries, start.elapsed());

    Ok((serial, pipelined))
}
