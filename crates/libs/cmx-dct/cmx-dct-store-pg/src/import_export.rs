//! 字典数据流式导入导出服务。
//!
//! - 导出 `export_stream`：keyset 分页拉取全表 → 按 JSON(NDJSON) / CSV 编码 →
//!   `mpsc::Receiver<Bytes>` 供 handler 包装为 axum 流式响应
//! - 导入 `import_stream`：从异步流读取 JSON(NDJSON) / CSV → 累积 batch → 列校验 →
//!   多行 INSERT + 单批事务；replace 模式前置 TRUNCATE
//!
//! 性能：导出每批 5000 行；导入每批 1000 行；峰值内存 ≤ 64MB。

use bytes::Bytes;
use serde::Serialize;
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use cmx_api_types::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::{DatabaseManager, get_default_pg_db_manager};

use cmx_dct_model::{
    BatchConflictMode, DctQuery, DictView, build_batch_insert_sql, build_truncate_sql, extract_pk,
};

use crate::error::{api_err, map_db_err};
use crate::resolve::resolve_dict;
use cmx_biz::pg_detail;

// ============================================================================
// 公共类型
// ============================================================================

/// 导入导出文件格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    /// NDJSON 流式（每行一个 JSON 对象，推荐大数据量）
    Json,
    /// CSV（含表头，Excel 友好）
    Csv,
}

impl ImportFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }

    /// HTTP Content-Type（导出响应头用）。
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Json => "application/x-ndjson; charset=utf-8",
            Self::Csv => "text/csv; charset=utf-8",
        }
    }

    /// 默认文件扩展名（不含 `.`）。
    pub fn ext(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

/// 导入摘要：成功行数 + 跳过行数 + 错误清单（不中断）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    /// 已解析的总行数（不含表头）
    pub total: u64,
    /// 实际写入受影响行数（INSERT/UPDATE 的 affected rows 总和）
    pub affected: u64,
    /// 跳过行数（主键冲突 DO NOTHING、列校验失败）
    pub skipped: u64,
    /// 错误清单（最多前 100 条，避免响应膨胀）
    pub errors: Vec<ImportError>,
}

impl ImportSummary {
    fn add_error(&mut self, row: usize, col: Option<String>, msg: impl Into<String>) {
        if self.errors.len() < 100 {
            self.errors.push(ImportError {
                row,
                col,
                message: msg.into(),
            });
        }
    }
}

/// 导入错误条目（含行号、列名、消息）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportError {
    /// 行号（1-based，不含表头）
    pub row: usize,
    /// 列名（None=行级错误）
    pub col: Option<String>,
    /// 错误消息
    pub message: String,
}

// ============================================================================
// 导出：keyset 分页 + mpsc 流
// ============================================================================

/// 启动导出流。
///
/// 内部 `resolve_dict` 解析字典视图，再 spawn tokio task：
/// 1. keyset 分页查询（首批 `last_pk=None`，后续 `WHERE pk > $1`，每批 `batch_size` 行）
/// 2. 按 `fmt` 序列化为 `Bytes`（JSON NDJSON 每行一个 JSON + `\n` / CSV 含表头）
/// 3. 通过 `mpsc::Sender<Bytes>` 发送
/// 4. 全部完成或出错时关闭 Sender
///
/// **错误处理**：`resolve_dict` 失败直接返回 `Err`（handler 可回 500）；流内错误无法回传
/// status code（响应头已发出），通过 `tracing::error!` 记录 + 流提前关闭。
///
/// # Arguments
///
/// - `q`：字典定位（内部 resolve_dict 解析视图）
/// - `db_id`：数据源 ID
/// - `fmt`：导出格式
/// - `batch_size`：每批行数（推荐 5000）
/// - `buffer`：mpsc channel 容量（推荐 8，避免内存堆积）
pub async fn export_stream(
    q: &DctQuery,
    db_id: String,
    fmt: ImportFormat,
    batch_size: i64,
    buffer: usize,
) -> Result<mpsc::Receiver<Bytes>> {
    let view = resolve_dict(q, false).await?;
    let (tx, rx) = mpsc::channel::<Bytes>(buffer);
    tokio::spawn(async move {
        match run_export(view.clone(), db_id, fmt, batch_size, tx.clone()).await {
            Ok(total) => info!(
                target: "cmx_dct::export",
                dict_code = %view.dict_code, table = %view.table_name,
                fmt = fmt.as_str(), total,
                "export_done"
            ),
            Err(e) => {
                error!(
                    target: "cmx_dct::export",
                    dict_code = %view.dict_code, table = %view.table_name,
                    fmt = fmt.as_str(), error = %e,
                    "export_failed"
                );
                // 尝试发送一个错误标记 chunk（已写头后才生效，作为 best-effort）
                let _ = tx.send(Bytes::from_static(b"")).await;
            }
        }
        // tx drop 自动关闭 channel
    });
    Ok(rx)
}

async fn run_export(
    view: DictView,
    db_id: String,
    fmt: ImportFormat,
    batch_size: i64,
    tx: mpsc::Sender<Bytes>,
) -> Result<u64> {
    let mm = get_default_pg_db_manager();
    let dict_code_for_ds = view.dict_code.clone();

    // CSV 首批先发送表头
    if fmt == ImportFormat::Csv {
        let header = view
            .columns
            .iter()
            .map(|c| csv_escape(&c.name))
            .collect::<Vec<_>>()
            .join(",");
        let header_line = format!("{}\n", header);
        tx.send(Bytes::from(header_line))
            .await
            .map_err(|_| api_err("导出 channel 已关闭"))?;
    }

    let mut last_pk: Option<DataValue> = None;
    let mut total: u64 = 0;
    loop {
        let (sql, params) =
            cmx_dct_model::build_export_sql(&view, last_pk.as_ref(), batch_size);
        let ds = mm
            .query_sql_with_datavalues(&db_id, None, &sql, params, &dict_code_for_ds)
            .await
            .map_err(|e| map_db_err(e, "export", &view, None, &sql))?;
        // DataSet → rows JSON
        let rows_val = serde_json::to_value(&ds)
            .map_err(|e| api_err(&format!("导出序列化失败: {e}")))?;
        let rows_arr = rows_val
            .get("rows")
            .and_then(|v| v.as_array())
            .ok_or_else(|| api_err("导出结果缺 rows"))?;
        if rows_arr.is_empty() {
            break;
        }
        let batch: Vec<Map<String, Value>> = rows_arr
            .iter()
            .filter_map(|r| r.as_object().cloned())
            .collect();
        let batch_len = batch.len() as u64;

        // 编码为 Bytes 并发送
        let chunk = encode_batch(&view, &batch, fmt)?;
        if !chunk.is_empty() {
            tx.send(Bytes::from(chunk))
                .await
                .map_err(|_| api_err("导出 channel 已关闭"))?;
        }

        // 提取下一批的 last_pk
        if let Some(last_row) = batch.last() {
            last_pk = Some(extract_pk(&view, last_row));
        }

        total += batch_len;
        debug!(
            target: "cmx_dct::export",
            dict_code = %view.dict_code, table = %view.table_name,
            batch = batch_len, total, pk = ?last_pk,
            "export_batch"
        );

        // 不足一批视为末尾
        if (batch_len as i64) < batch_size {
            break;
        }
    }
    Ok(total)
}

/// 把一批行编码为字节（JSON NDJSON 或 CSV）。
fn encode_batch(
    view: &DictView,
    batch: &[Map<String, Value>],
    fmt: ImportFormat,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(batch.len() * 128);
    match fmt {
        ImportFormat::Json => {
            for row in batch {
                // NDJSON：每行一个紧凑 JSON + \n
                let line = serde_json::to_string(row)
                    .map_err(|e| api_err(&format!("JSON 编码失败: {e}")))?;
                out.extend_from_slice(line.as_bytes());
                out.push(b'\n');
            }
        }
        ImportFormat::Csv => {
            // 按 view.columns 顺序输出每行（表头由首批发送，这里不重复）
            for row in batch {
                let mut cells: Vec<String> = Vec::with_capacity(view.columns.len());
                for c in &view.columns {
                    let v = row.get(&c.name).cloned().unwrap_or(Value::Null);
                    let s = value_to_csv_cell(&v);
                    cells.push(csv_escape(&s));
                }
                out.extend_from_slice(cells.join(",").as_bytes());
                out.push(b'\n');
            }
        }
    }
    Ok(out)
}

/// JSON Value → CSV 单元格字符串（NULL → 空，Bool → true/false，Number → 字面量，
/// String → 原文，其他 → JSON 序列化）。
fn value_to_csv_cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// CSV 字段转义：含 `,` / `"` / `\n` / `\r` 时用双引号包裹，内部双引号双写。
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        s.to_string()
    }
}

// ============================================================================
// 导入：流式解析 + 批量 INSERT
// ============================================================================

/// 流式导入。
///
/// 内部：
/// 1. 若 `mode == Replace`，先 `TRUNCATE TABLE ... RESTART IDENTITY`（独立事务）
/// 2. 按 `fmt` 流式解析（`csv::AsyncReader` / `serde_json::into_iter` 逐行读）
/// 3. 累积 `batch_size` 行后调用 `apply_import_batch` 写入
/// 4. 列校验失败 / 主键冲突 DO NOTHING → 累计到 `skipped`，不中断
/// 5. DB 错误 → 累计到 `errors`，不中断（除非错误数过多）
///
/// # Arguments
///
/// - `q`：字典定位（内部 resolve_dict 解析视图）
/// - `db_id`：数据源 ID
/// - `fmt`：文件格式
/// - `mode`：冲突处理模式（upsert / insert_only / replace）
/// - `batch_size`：每批行数（推荐 1000）
/// - `data`：异步读取流（multipart 字段内容）
pub async fn import_stream<R: tokio::io::AsyncRead + Unpin>(
    q: &DctQuery,
    db_id: String,
    fmt: ImportFormat,
    mode: BatchConflictMode,
    batch_size: usize,
    data: R,
) -> Result<ImportSummary> {
    let view = resolve_dict(q, false).await?;
    // replace 模式前置 TRUNCATE（独立事务）
    if mode == BatchConflictMode::Replace {
        truncate_for_replace(&view, &db_id).await?;
    }

    let mm = get_default_pg_db_manager();
    let mut summary = ImportSummary::default();
    let mut batch: Vec<Map<String, Value>> = Vec::with_capacity(batch_size);
    let mut row_idx: usize = 0;

    // 流式解析
    // CSV：一次性读入内存（导入是低频管理操作，100w 行 CSV ~150MB 可接受；同步 csv::Reader
    // 在 Cursor 上跑解析，每行 < 1μs，不会阻塞 tokio worker）。
    // JSON：按行流式 BufReader（NDJSON 每行一个对象，天然流式）。
    match fmt {
        ImportFormat::Csv => {
            let mut buf = Vec::new();
            let mut reader = data;
            reader
                .read_to_end(&mut buf)
                .await
                .map_err(|e| api_err(&format!("CSV 读取失败: {e}")))?;
            let cursor = std::io::Cursor::new(buf);
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(true)
                .from_reader(cursor);
            let headers = rdr.headers().map_err(|e| api_err(&format!("CSV 表头读取失败: {e}")))?.clone();
            // 用 ByteRecord 迭代：csv 1.x StringRecord 未暴露 is_quoted，ByteRecord.range(i)
            // 给出字段在原始字节缓冲中的范围，可自检首字节是否为 `"`（带引号）。
            let mut byte_rec = csv::ByteRecord::new();
            while rdr.read_byte_record(&mut byte_rec).map_err(|e| api_err(&format!("CSV 记录读取失败: {e}")))? {
                row_idx += 1;
                let mut obj = Map::new();
                let raw_buf = byte_rec.as_slice();
                for (i, field) in byte_rec.iter().enumerate() {
                    if let Some(h) = headers.get(i) {
                        // 区分 NULL vs 空字符串（对齐 PostgreSQL COPY 的 CSV 模式）：
                        //   无引号空字段（首字节非 `"`）→ NULL（跳过 key，等价 row 不含该列，走 backfill / DB NULL）
                        //   带引号空字段（首字节 `"`，原文 `""`）→ 空字符串 ""
                        // 这样用户可在 CSV 中显式区分两种语义；整数列无引号空字段也不再报 TypeMismatch
                        if field.is_empty() {
                            let is_quoted = byte_rec
                                .range(i)
                                .and_then(|r| raw_buf.get(r.start..r.start.saturating_add(1)))
                                .map(|b| b == b"\"")
                                .unwrap_or(false);
                            if !is_quoted {
                                continue;
                            }
                        }
                        // UTF-8 容错：非法字节用 replacement char 替换（与原 StringRecord 行为一致）
                        let s = String::from_utf8_lossy(field).into_owned();
                        obj.insert(h.to_string(), Value::String(s));
                    }
                }
                push_row(
                    &view, &db_id, mm, mode, batch_size, &mut batch,
                    &mut summary, &mut row_idx, obj,
                )
                .await?;
            }
        }
        ImportFormat::Json => {
            let mut reader = BufReader::new(data);
            let mut line = String::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await
                    .map_err(|e| api_err(&format!("JSON 读取失败: {e}")))?;
                if n == 0 {
                    break;
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                row_idx += 1;
                let obj: Map<String, Value> = match serde_json::from_str(trimmed) {
                    Ok(m) => m,
                    Err(e) => {
                        summary.add_error(row_idx, None, format!("JSON 解析失败: {e}"));
                        summary.skipped += 1;
                        continue;
                    }
                };
                push_row(
                    &view, &db_id, mm, mode, batch_size, &mut batch,
                    &mut summary, &mut row_idx, obj,
                )
                .await?;
            }
        }
    }

    // 末批不足 batch_size 也写出
    if !batch.is_empty() {
        apply_import_batch(mm, &view, &db_id, std::mem::take(&mut batch), mode, &mut summary)
            .await?;
    }

    summary.total = row_idx as u64;
    info!(
        target: "cmx_dct::import",
        dict_code = %view.dict_code, table = %view.table_name,
        fmt = fmt.as_str(), mode = ?mode,
        total = summary.total, affected = summary.affected, skipped = summary.skipped,
        errors = summary.errors.len(),
        "import_done"
    );
    Ok(summary)
}

/// 把一行推入 batch；满 batch 时触发写入。
#[allow(clippy::too_many_arguments)]
async fn push_row(
    view: &DictView,
    db_id: &str,
    mm: &DatabaseManager,
    mode: BatchConflictMode,
    batch_size: usize,
    batch: &mut Vec<Map<String, Value>>,
    summary: &mut ImportSummary,
    row_idx: &mut usize,
    row: Map<String, Value>,
) -> Result<()> {
    // 编码引擎铸号：若字典配置了 auto codeRule 且 code 为空，先铸号再校验
    // （与 write.rs save 路径一致：铸号在 NOT NULL 校验之前）。
    //
    // 逐行铸号（单行 slice）：
    // - use_sequence=true：每次走发号序列表 FOR UPDATE 取号，原子安全，逐行无重号风险。
    // - use_sequence=false（默认反查 max）：serial 段每行单独反查 max，若导入大量同 prefix 行
    //   可能因前一行未落库取到同一 max 号。random 段无此问题（每次 resolve 换种子）。
    //   导入场景通常 CSV 自带 code，留空行少；若需大批量导入 auto 铸号，建议开启 use_sequence=true。
    let mut row = row;
    if needs_mint_code(view, &row) {
        let rows_slice = std::slice::from_mut(&mut row);
        crate::write::mint_codes_for_inserts(view, rows_slice, db_id, None).await;
    }
    // 列校验：通过才入 batch；否则记录 skipped + error
    if let Some(violation) = validate_row(view, &row) {
        summary.add_error(*row_idx, violation.0, violation.1);
        summary.skipped += 1;
        return Ok(());
    }
    batch.push(row);
    if batch.len() >= batch_size {
        let batch_taken = std::mem::take(batch);
        apply_import_batch(mm, view, db_id, batch_taken, mode, summary).await?;
    }
    Ok(())
}

/// 判断某行是否需要编码引擎铸号（有 auto codeRule 且 code 字段为空）。
fn needs_mint_code(view: &DictView, row: &Map<String, Value>) -> bool {
    let Some(code_rule) = &view.code_rule else { return false };
    let mode = code_rule.get("mode").and_then(|v| v.as_str()).unwrap_or("manual");
    if mode != "auto" { return false }
    let code_field = &view.code_field;
    match row.get(code_field) {
        None => true,
        Some(Value::Null) => true,
        Some(v) => v.as_str().map(|s| s.is_empty()).unwrap_or(false),
    }
}

/// 单行校验：返回 `(col_name, message)` 表示首个违规；`None` 表示通过。
///
/// 复用 `cmx_biz::validation` 但只取首个违规（批量导入场景不需要全部违规清单）。
fn validate_row(view: &DictView, row: &Map<String, Value>) -> Option<(Option<String>, String)> {
    use cmx_dct_model::SERVER_FILLED_COLS;
    use cmx_dct_model::SERVER_REPLACED_COLS;
    let vopts = cmx_biz::validation::ValidateOptions::insert(
        SERVER_FILLED_COLS,
        SERVER_REPLACED_COLS,
    );
    let violations = cmx_biz::validation::validate_insert_row(
        &view.spec,
        row,
        None,
        &vopts,
    );
    violations.into_iter().next().map(|v| {
        let col = v
            .column
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        (col, v.message)
    })
}

/// 写入一批行（单事务，多行 INSERT）。
async fn apply_import_batch(
    mm: &DatabaseManager,
    view: &DictView,
    db_id: &str,
    rows: Vec<Map<String, Value>>,
    mode: BatchConflictMode,
    summary: &mut ImportSummary,
) -> Result<()> {
    let batch_len = rows.len() as u64;
    let (sql, params) = match build_batch_insert_sql(view, &rows, mode) {
        Some(x) => x,
        None => {
            // 无有效用户列：全部 skipped
            let base = summary.total as usize;
            for i in 0..batch_len {
                summary.add_error(base + i as usize + 1, None, "无有效用户列");
                summary.skipped += 1;
            }
            return Ok(());
        }
    };

    // 单事务执行
    let tx = mm.get_transaction_context();
    let txn_id = tx.begin(db_id).await.map_err(|e| {
        map_db_err(e, "import_begin", view, None, &sql)
    })?;

    let exec_result = mm
        .execute_sql_with_datavalues(db_id, Some(&txn_id), &sql, params)
        .await;

    match exec_result {
        Ok(n) => {
            tx.commit(&txn_id).await.map_err(|e| {
                map_db_err(e, "import_commit", view, None, &sql)
            })?;
            // upsert/insert_only：affected 可能 < batch_len（冲突 DO NOTHING）
            // replace：affected 应等于 batch_len
            summary.affected += n;
            // 若 affected < batch_len，差额计为 skipped（主键冲突）
            if n < batch_len && mode != BatchConflictMode::Replace {
                summary.skipped += batch_len - n;
            }
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            // 错误细化：
            //   - `cmx_biz::BizError::from_db_error` 用子串匹配 SQLSTATE 关键词翻译成中文
            //     （duplicate key / null value / not-null / foreign key / check constraint），
            //     需要传入包含 PG 原文的字符串。`tokio_postgres::Error` 顶层 Display 恒为
            //     "db error"（真错藏在 `as_db_error()`），故统一走 `pg_detail` 抽真实
            //     message/detail/constraint（与 `map_db_err` / cmx-rpt-store-pg 一致）。
            //   - 日志同时打印 raw_error（Display）+ pg_detail（真实 PG 明细）+ first_row +
            //     sql_preview，便于定位是哪一列/哪一行数据触发约束。
            let detail = pg_detail(&e);
            let biz = cmx_biz::BizError::from_db_error(&detail);
            let translated = biz.to_string();
            let msg = format!("批次写入失败：{}", translated);
            error!(
                target: "cmx_dct::import",
                dict_code = %view.dict_code, table = %view.table_name,
                batch_len,
                raw_error = %e,
                pg_detail = %detail,
                translated = %translated,
                first_row = ?rows.first(),
                sql_preview = %sql.chars().take(300).collect::<String>(),
                "batch_failed"
            );
            let base = summary.total as usize;
            for i in 0..batch_len {
                summary.add_error(base + i as usize + 1, None, &msg);
                summary.skipped += 1;
            }
        }
    }
    summary.total += batch_len;
    Ok(())
}

/// TRUNCATE 目标表（replace 模式前置）。
pub(crate) async fn truncate_for_replace(view: &DictView, db_id: &str) -> Result<()> {
    let sql = build_truncate_sql(view);
    let mm = get_default_pg_db_manager();
    let tx = mm.get_transaction_context();
    let txn_id = tx.begin(db_id).await.map_err(|e| {
        map_db_err(e, "import_truncate_begin", view, None, &sql)
    })?;
    match mm.execute_sql_with_datavalues(db_id, Some(&txn_id), &sql, Vec::new()).await {
        Ok(_) => {
            tx.commit(&txn_id).await.map_err(|e| {
                map_db_err(e, "import_truncate_commit", view, None, &sql)
            })?;
            info!(
                target: "cmx_dct::import",
                dict_code = %view.dict_code, table = %view.table_name,
                "truncate_done"
            );
            Ok(())
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            Err(map_db_err(e, "import_truncate", view, None, &sql))
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_basic() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn value_to_csv_cell_types() {
        assert_eq!(value_to_csv_cell(&Value::Null), "");
        assert_eq!(value_to_csv_cell(&Value::Bool(true)), "true");
        assert_eq!(value_to_csv_cell(&serde_json::Number::from(42).into()), "42");
        assert_eq!(value_to_csv_cell(&Value::String("hello".to_string())), "hello");
    }

    #[test]
    fn import_format_as_str() {
        assert_eq!(ImportFormat::Json.as_str(), "json");
        assert_eq!(ImportFormat::Csv.as_str(), "csv");
    }

    #[test]
    fn import_format_content_type() {
        assert!(ImportFormat::Json.content_type().contains("ndjson"));
        assert!(ImportFormat::Csv.content_type().contains("text/csv"));
    }

    #[test]
    fn import_summary_default() {
        let s = ImportSummary::default();
        assert_eq!(s.total, 0);
        assert_eq!(s.affected, 0);
        assert_eq!(s.skipped, 0);
        assert!(s.errors.is_empty());
    }

    #[test]
    fn import_summary_errors_capped() {
        let mut s = ImportSummary::default();
        for i in 0..200 {
            s.add_error(i, None, "test");
        }
        assert_eq!(s.errors.len(), 100);
    }

    #[test]
    fn encode_batch_json_lines() {
        let view = mock_view();
        let mut row = Map::new();
        row.insert("id".to_string(), Value::String("CNY".to_string()));
        row.insert("code".to_string(), Value::String("CNY".to_string()));
        let batch = vec![row];
        let out = encode_batch(&view, &batch, ImportFormat::Json).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"id\":\"CNY\""));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn encode_batch_csv_no_header_in_batch() {
        // CSV 批次不应含表头（表头由首批单独发送）
        let view = mock_view();
        let mut row = Map::new();
        row.insert("id".to_string(), Value::String("CNY".to_string()));
        row.insert("code".to_string(), Value::String("CNY".to_string()));
        row.insert("name".to_string(), Value::String("人民币".to_string()));
        let batch = vec![row];
        let out = encode_batch(&view, &batch, ImportFormat::Csv).unwrap();
        let s = String::from_utf8(out).unwrap();
        // 单行 CSV：3 列值，不含表头名
        assert_eq!(s.lines().count(), 1);
        assert!(s.contains("CNY"));
    }

    /// 构造测试用 DictView（与 cmx-dct-model::bulk::tests 一致结构，独立构造避免循环依赖）。
    fn mock_view() -> DictView {
        use cmx_dct_model::DictColumn;
        let columns = vec![
            DictColumn {
                name: "id".to_string(),
                caption: "ID".to_string(),
                data_type: "VARCHAR".to_string(),
                is_pk: true,
                nullable: false,
                dim_type: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                physical_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
                extra: None,
            },
            DictColumn {
                name: "code".to_string(),
                caption: "编码".to_string(),
                data_type: "VARCHAR".to_string(),
                is_pk: false,
                nullable: false,
                dim_type: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                physical_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
                extra: None,
            },
            DictColumn {
                name: "name".to_string(),
                caption: "名称".to_string(),
                data_type: "VARCHAR".to_string(),
                is_pk: false,
                nullable: true,
                dim_type: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                physical_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
                extra: None,
            },
        ];
        DictView {
            dict_code: "test".to_string(),
            dict_name: "测试".to_string(),
            table_name: "cf_test".to_string(),
            id_field: "id".to_string(),
            code_field: "code".to_string(),
            label_field: "name".to_string(),
            parent_field: None,
            self_hierarchy: false,
            columns,
            pk: "id".to_string(),
            spec: std::sync::Arc::new(cmx_biz::validation::TableSpec {
                table: "cf_test".to_string(),
                columns: std::collections::HashMap::new(),
                order: Vec::new(),
            }),
            code_rule: None,
        }
    }
}
