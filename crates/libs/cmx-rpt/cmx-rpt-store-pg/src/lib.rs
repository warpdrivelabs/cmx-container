//! cmx-rpt-store-pg —— 报表模块的 PostgreSQL 持久化/服务层。
//!
//! 承载全部 cr_* 报表数据字典物理表（fico-db）的读写：报表主档/版本/类别/元素/会计日历/
//! 合并组织的查询，版式 BLOB(cr_report_fmt) + 关系投影(sheet/region/row/col/cell_element_map)
//! 的事务重建，单元格数据(cr_cell_data) 按 org+period 的批量 UPSERT。读走 ZmcDataSet 零拷贝
//! 省内存，写走强类型 DataValue 绑定。表由 data/meta 定义建，本层不建表（无 DDL）。
//!
//! 服务函数返回 `serde_json::Value` 载荷或语义结果（如 `SaveLayoutOutcome`），由 cmx-rpt-api
//! 的薄 handler 包装成 HTTP 响应。错误统一走 `cmx_api_types::Error`（经 `api_err`/BizError 桥接）。

use std::collections::HashMap;

use serde_json::{Value, json};
use tracing::debug;

use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_database_pg::{ZmcRowSource, get_default_pg_db_manager};

use cmx_rpt_model::{CreateVersionBody, DEFAULT_REGION, LayoutQuery, RPT_DB_ID, ReportListQuery};

pub use cmx_api_types::{Error, Result};

pub mod compute;
pub use compute::compute_report_service;

/// 构造报表业务错误（BizError → cmx_api_types::Error）。
pub fn api_err(msg: &str) -> Error {
    cmx_biz::BizError::business(msg.to_string()).into()
}

// ============================================================================
// DB 门面 helper
// ============================================================================

/// JSON 参数查询 → 行数组（sqlx/json 链路；报表读取多为小体量关系投影）。
pub(crate) async fn query_rows(sql: &str, params: Value, label: &str) -> Result<Vec<Value>> {
    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_with_json(RPT_DB_ID, None, sql, params, label)
        .await
        .map_err(|e| api_err(&format!("报表设计数据查询失败: {e}")))?;
    let v = serde_json::to_value(&ds).map_err(|e| api_err(&format!("查询结果序列化失败: {e}")))?;
    Ok(v.get("rows")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default())
}

/// JSON 参数执行（非事务，单语句）。
async fn execute(sql: &str, params: Value) -> Result<()> {
    let mm = get_default_pg_db_manager();
    mm.execute_sql_with_json(RPT_DB_ID, None, sql, params)
        .await
        .map_err(|e| api_err(&format!("报表设计数据写入失败: {e}")))?;
    Ok(())
}

/// 强类型执行（事务内），参数按 DataValue 变体精确绑定（String→text, Binary→bytea, Null→NULL…）。
pub(crate) async fn exec_dv(txn_id: &str, sql: &str, params: Vec<DataValue>) -> Result<()> {
    let mm = get_default_pg_db_manager();
    mm.execute_sql_with_datavalues(RPT_DB_ID, Some(txn_id), sql, params)
        .await
        .map_err(|e| api_err(&format!("报表写入失败: {e}")))?;
    Ok(())
}

// ============================================================================
// JSON 字段/DataValue 绑定 helper
// ============================================================================

fn str_field(body: &Value, name: &str) -> Option<String> {
    body.get(name)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn int_field(body: &Value, name: &str) -> Option<i64> {
    body.get(name).and_then(|v| v.as_i64())
}

fn bool_int_field(body: &Value, name: &str) -> Option<i64> {
    body.get(name).and_then(|v| match v {
        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
        Value::Number(n) => n.as_i64(),
        Value::String(s) => match s.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" => Some(1),
            "0" | "false" | "FALSE" | "no" | "NO" => Some(0),
            _ => None,
        },
        _ => None,
    })
}

fn dv_str(s: Option<&str>) -> DataValue {
    match s {
        Some(v) if !v.is_empty() => DataValue::String(v.to_string()),
        // 强类型 Text NULL：tokio-postgres 按列 OID 校验 NULL 类型（裸 Null 会当 text 绑到
        // 非 text 列报错；这里列本就是 varchar 故 Text NULL 正确）。
        _ => DataValue::NullTyped(SqlTypeMarker::Text),
    }
}

/// NOT NULL 文本列：缺失时用给定默认值（避免 null 约束冲突）。
fn dv_str_def(s: Option<&str>, def: &str) -> DataValue {
    match s {
        Some(v) if !v.is_empty() => DataValue::String(v.to_string()),
        _ => DataValue::String(def.to_string()),
    }
}

fn dv_i64(v: Option<i64>) -> DataValue {
    // 强类型 Int NULL：宽度自适应 INT2/4/8，绑到 bigint 列的 NULL 正确（裸 Null=text 会报错）。
    v.map(DataValue::Int)
        .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Int))
}

/// NOT NULL 整数列：缺失时用给定默认值。
fn dv_i64_def(v: Option<i64>, def: i64) -> DataValue {
    DataValue::Int(v.unwrap_or(def))
}

fn s(body: &Value, k: &str) -> Option<String> {
    body.get(k).and_then(|v| v.as_str()).map(str::to_owned)
}

fn i(body: &Value, k: &str) -> Option<i64> {
    body.get(k).and_then(|v| match v {
        Value::Number(n) => n.as_i64(),
        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
        _ => None,
    })
}

fn arr<'a>(body: &'a Value, k: &str) -> &'a [Value] {
    body.get(k)
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
}

/// 0基列号 → 字母（0→A, 26→AA），列 code 缺失时的兜底。
fn col_letter_of(idx: usize) -> String {
    let mut n = idx + 1;
    let mut s = String::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    if s.is_empty() { "A".to_string() } else { s }
}

/// 解析行列 id：真号(纯数字/数字)原样用；临时(空/非数字串/缺失)铸新 next_pk_id。
fn resolve_id(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or_else(cmx_utils::next_pk_id),
        Some(Value::String(s)) if s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty() => {
            s.parse::<i64>().unwrap_or_else(|_| cmx_utils::next_pk_id())
        }
        _ => cmx_utils::next_pk_id(),
    }
}

// ============================================================================
// 报表主档 / 版本 / 类别 / 元素 / 日历 / 组织：读服务
// ============================================================================

/// 报表设计工作台总览：类别 + 期间类型 + 全量报表。
pub async fn overview() -> Result<Value> {
    debug!("{:<12} - overview db={}", "RPT-STORE", RPT_DB_ID);
    let categories = query_rows(
        r#"SELECT code, name, sort_no, status, remark
           FROM cr_report_category
           WHERE COALESCE(status, 1) = 1
           ORDER BY COALESCE(sort_no, 999999), code"#,
        json!([]),
        "report_design_categories",
    )
    .await?;
    let periods = query_rows(
        r#"SELECT code, name, sort_no, status, remark
           FROM cr_period_type
           WHERE COALESCE(status, 1) = 1
           ORDER BY COALESCE(sort_no, 999999), code"#,
        json!([]),
        "report_design_periods",
    )
    .await?;
    let reports = report_rows(&ReportListQuery::default()).await?;
    Ok(json!({
        "dbId": RPT_DB_ID,
        "categories": categories,
        "periods": periods,
        "reports": reports,
    }))
}

/// 过滤报表列表（供 overview + /reports 复用）。
pub async fn report_rows(q: &ReportListQuery) -> Result<Vec<Value>> {
    let mut wheres = Vec::new();
    let mut params = Vec::new();
    let mut n = 0usize;
    if let Some(category) = q
        .category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        n += 1;
        wheres.push(format!("rl.report_category = ${n}"));
        params.push(json!(category));
    }
    if let Some(period_type) = q
        .period_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        n += 1;
        wheres.push(format!("rl.period_type = ${n}"));
        params.push(json!(period_type));
    }
    if let Some(kw) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        n += 1;
        wheres.push(format!(
            "(rl.code ILIKE ${n} OR rl.name ILIKE ${n} OR COALESCE(rl.remark, '') ILIKE ${n})"
        ));
        params.push(json!(format!("%{kw}%")));
    }
    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };
    let sql = format!(
        r#"SELECT
             rl.code, rl.name, rl.report_type, rl.report_category, rl.format_code,
             rl.period_type, rl.currency_code, rl.amount_unit, rl.entity_scope,
             rl.template_version, rl.data_source, rl.is_statutory, rl.remark,
             rl.sort_no, rl.status, rl.create_time, rl.update_time,
             COALESCE((SELECT COUNT(*) FROM cr_report_version rv WHERE rv.report_code = rl.code), 0) AS version_count,
             (SELECT rv.code FROM cr_report_version rv WHERE rv.report_code = rl.code AND COALESCE(rv.is_current, 0) = 1 ORDER BY rv.version_no DESC, rv.code DESC LIMIT 1) AS current_version_code,
             COALESCE((
               SELECT jsonb_agg(jsonb_build_object(
                 'code', rv.code,
                 'name', rv.name,
                 'version_no', rv.version_no,
                 'version_status', rv.version_status,
                 'is_current', rv.is_current,
                 'base_version_code', rv.base_version_code,
                 'effective_from', rv.effective_from,
                 'effective_to', rv.effective_to,
                 'change_summary', rv.change_summary,
                 'publish_time', rv.publish_time,
                 'remark', rv.remark
               ) ORDER BY rv.version_no DESC, rv.code DESC)
               FROM cr_report_version rv
               WHERE rv.report_code = rl.code
             ), '[]'::jsonb)::text AS versions
           FROM cr_report_list rl
           {where_sql}
           ORDER BY COALESCE(rl.sort_no, 999999), rl.code"#
    );
    query_rows(&sql, Value::Array(params), "report_design_reports").await
}

/// 报表列表（含 dbId 信封）。
pub async fn reports(q: &ReportListQuery) -> Result<Value> {
    let reports = report_rows(q).await?;
    Ok(json!({ "dbId": RPT_DB_ID, "items": reports }))
}

/// 数据元素：元素分类 + 数据元素。
pub async fn elements() -> Result<Value> {
    debug!("{:<12} - elements db={}", "RPT-STORE", RPT_DB_ID);
    let categories = query_rows(
        r#"SELECT code, name, sort_no, status, remark
           FROM cr_element_category
           WHERE COALESCE(status, 1) = 1
           ORDER BY COALESCE(sort_no, 999999), code"#,
        json!([]),
        "report_design_element_categories",
    )
    .await?;
    let elements = query_rows(
        r#"SELECT code, name, category_code, data_type, unit, decimals,
                  value_source, formula_type, calc_formula, check_formula,
                  sort_no, status, remark
           FROM cr_data_element
           WHERE COALESCE(status, 1) = 1
           ORDER BY COALESCE(sort_no, 999999), code"#,
        json!([]),
        "report_design_data_elements",
    )
    .await?;
    Ok(json!({
        "dbId": RPT_DB_ID,
        "categories": categories,
        "elements": elements,
    }))
}

/// 会计日历字典（cr_acct_calendar 年度→月度自分级）。
pub async fn calendar() -> Result<Value> {
    debug!("{:<12} - calendar db={}", "RPT-STORE", RPT_DB_ID);
    let periods = query_rows(
        r#"SELECT code, name, calendar_type, fiscal_year, period_no, parent_code,
                  full_path, level_no, is_leaf, period_type, start_date, end_date,
                  quarter, period_status, is_year_end, days, sort_no, status
           FROM cr_acct_calendar
           WHERE COALESCE(status, 1) = 1
           ORDER BY COALESCE(sort_no, 999999), code"#,
        json!([]),
        "report_design_calendar",
    )
    .await?;
    Ok(json!({ "dbId": RPT_DB_ID, "periods": periods }))
}

/// 合并组织架构（cr_consol_org parent_id 自分级树）。
pub async fn consol_org() -> Result<Value> {
    debug!("{:<12} - consol_org db={}", "RPT-STORE", RPT_DB_ID);
    let orgs = query_rows(
        r#"SELECT id, code, name, consol_scheme, org_type, parent_id, full_path,
                  level_no, is_leaf, entity_code, consol_method, ownership_pct, voting_pct,
                  consol_currency, is_parent, offset_flag, remark, sort_no, status
           FROM cr_consol_org
           WHERE COALESCE(status, 1) = 1
           ORDER BY COALESCE(sort_no, 999999), id"#,
        json!([]),
        "report_design_consol_org",
    )
    .await?;
    Ok(json!({ "dbId": RPT_DB_ID, "orgs": orgs }))
}

/// 报表详情：主档 + 版本 + 选中版本统计。
pub async fn report_detail(code: &str, version: Option<String>) -> Result<Value> {
    let report = query_rows(
        r#"SELECT code, name, report_type, report_category, format_code, period_type,
                  currency_code, amount_unit, entity_scope, template_version, data_source,
                  is_statutory, remark, sort_no, status, create_time, update_time
           FROM cr_report_list
           WHERE code = $1"#,
        json!([code]),
        "report_design_report",
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| api_err("报表不存在"))?;

    let code = report
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let versions = query_rows(
        r#"SELECT code, name, report_code, version_no, version_status, is_current,
                  base_version_code, effective_from, effective_to, change_summary,
                  publish_time, publish_by, remark, sort_no, status, create_time, update_time
           FROM cr_report_version
           WHERE report_code = $1
           ORDER BY version_no DESC, code DESC"#,
        json!([code]),
        "report_design_versions",
    )
    .await?;
    let selected_version = version
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            versions
                .iter()
                .find(|v| v.get("is_current").and_then(|x| x.as_i64()).unwrap_or(0) == 1)
                .and_then(|v| v.get("code").and_then(|x| x.as_str()).map(ToOwned::to_owned))
        })
        .or_else(|| {
            versions
                .first()
                .and_then(|v| v.get("code").and_then(|x| x.as_str()).map(ToOwned::to_owned))
        });

    let stats = if let Some(ver) = selected_version.as_deref() {
        query_rows(
            r#"SELECT
                 (SELECT COUNT(*) FROM cr_report_sheet WHERE report_code = $1 AND version_code = $2) AS sheet_count,
                 (SELECT COUNT(*) FROM cr_report_region WHERE report_code = $1 AND version_code = $2) AS region_count,
                 (SELECT COUNT(*) FROM cr_report_row WHERE report_code = $1 AND version_code = $2) AS row_count,
                 (SELECT COUNT(*) FROM cr_report_col WHERE report_code = $1 AND version_code = $2) AS col_count,
                 (SELECT COUNT(*) FROM cr_report_fmt WHERE report_code = $1 AND version_code = $2) AS format_count"#,
            json!([code, ver]),
            "report_design_detail_stats",
        )
        .await?
        .into_iter()
        .next()
        .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };

    Ok(json!({
        "dbId": RPT_DB_ID,
        "report": report,
        "versions": versions,
        "selectedVersion": selected_version,
        "stats": stats,
    }))
}

// ============================================================================
// 报表主档 / 版本：写服务
// ============================================================================

/// 新建报表（含初始版本）。
pub async fn create_report(body: &Value) -> Result<Value> {
    let code = str_field(body, "code").ok_or_else(|| api_err("报表编码不能为空"))?;
    let name = str_field(body, "name").ok_or_else(|| api_err("报表名称不能为空"))?;
    let report_type = str_field(body, "report_type").unwrap_or_else(|| "CUSTOM".to_string());
    let report_category =
        str_field(body, "report_category").unwrap_or_else(|| "management".to_string());
    let period_type = str_field(body, "period_type").unwrap_or_else(|| "month".to_string());
    let sort_no = int_field(body, "sort_no").unwrap_or(100);
    let status = bool_int_field(body, "status").unwrap_or(1);
    let is_statutory = bool_int_field(body, "is_statutory").unwrap_or(0);

    execute(
        r#"INSERT INTO cr_report_list
           (code, name, report_type, report_category, format_code, period_type,
            currency_code, amount_unit, entity_scope, template_version, data_source,
            is_statutory, remark, sort_no, status, create_time, update_time)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
        json!([
            code.clone(),
            name.clone(),
            report_type,
            report_category,
            str_field(body, "format_code"),
            period_type,
            str_field(body, "currency_code").unwrap_or_else(|| "CNY".to_string()),
            str_field(body, "amount_unit").unwrap_or_else(|| "yuan".to_string()),
            str_field(body, "entity_scope").unwrap_or_else(|| "single".to_string()),
            str_field(body, "template_version").unwrap_or_else(|| "V1".to_string()),
            str_field(body, "data_source"),
            is_statutory,
            str_field(body, "remark"),
            sort_no,
            status
        ]),
    )
    .await?;

    let version_code = str_field(body, "version_code").unwrap_or_else(|| "V1".to_string());
    create_version_row(
        &code,
        &version_code,
        &str_field(body, "version_name").unwrap_or_else(|| "初始版本".to_string()),
        1,
        None,
        "draft",
        1,
        str_field(body, "change_summary").as_deref(),
    )
    .await?;

    Ok(json!({ "code": code, "name": name, "version": version_code }))
}

/// 删除报表（级联删 7 张表）。
pub async fn delete_report(code: &str) -> Result<Value> {
    debug!("{:<12} - delete_report {}", "RPT-STORE", code);
    for sql in [
        "DELETE FROM cr_report_fmt WHERE report_code = $1",
        "DELETE FROM cr_report_col WHERE report_code = $1",
        "DELETE FROM cr_report_row WHERE report_code = $1",
        "DELETE FROM cr_report_region WHERE report_code = $1",
        "DELETE FROM cr_report_sheet WHERE report_code = $1",
        "DELETE FROM cr_report_version WHERE report_code = $1",
        "DELETE FROM cr_report_list WHERE code = $1",
    ] {
        execute(sql, json!([code])).await?;
    }
    Ok(json!({ "code": code }))
}

/// 建版本。
pub async fn create_version(report_code: &str, body: &CreateVersionBody) -> Result<Value> {
    let rows = query_rows(
        "SELECT COALESCE(MAX(version_no), 0) AS max_no FROM cr_report_version WHERE report_code = $1",
        json!([report_code]),
        "report_design_version_no",
    )
    .await?;
    let next_no = rows
        .first()
        .and_then(|r| r.get("max_no"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        + 1;
    let version_code = body
        .code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("V{next_no}"));
    let version_name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("版本 {next_no}"));
    let is_current = if body.is_current.unwrap_or(false) { 1 } else { 0 };
    if is_current == 1 {
        execute(
            "UPDATE cr_report_version SET is_current = 0, update_time = CURRENT_TIMESTAMP WHERE report_code = $1",
            json!([report_code]),
        )
        .await?;
    }
    create_version_row(
        report_code,
        &version_code,
        &version_name,
        next_no,
        body.base_version_code.as_deref(),
        "draft",
        is_current,
        body.change_summary.as_deref(),
    )
    .await?;
    Ok(json!({
        "reportCode": report_code,
        "version": version_code,
        "versionNo": next_no
    }))
}

/// 设置默认（当前生效）版本。
pub async fn set_default_version(report_code: &str, version: &str) -> Result<Value> {
    let exists = query_rows(
        "SELECT code FROM cr_report_version WHERE report_code = $1 AND code = $2",
        json!([report_code, version]),
        "report_design_default_version_check",
    )
    .await?;
    if exists.is_empty() {
        return Err(api_err("版本不存在"));
    }
    execute(
        "UPDATE cr_report_version SET is_current = CASE WHEN code = $2 THEN 1 ELSE 0 END, update_time = CURRENT_TIMESTAMP WHERE report_code = $1",
        json!([report_code, version]),
    )
    .await?;
    Ok(json!({ "reportCode": report_code, "version": version }))
}

#[allow(clippy::too_many_arguments)]
async fn create_version_row(
    report_code: &str,
    version_code: &str,
    version_name: &str,
    version_no: i64,
    base_version_code: Option<&str>,
    version_status: &str,
    is_current: i64,
    change_summary: Option<&str>,
) -> Result<()> {
    execute(
        r#"INSERT INTO cr_report_version
           (code, name, report_code, version_no, version_status, is_current,
            base_version_code, change_summary, remark, sort_no, status, create_time, update_time)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
        json!([
            version_code,
            version_name,
            report_code,
            version_no,
            version_status,
            is_current,
            base_version_code,
            change_summary,
            change_summary,
            version_no
        ]),
    )
    .await
}

// ============================================================================
// 模式一 · 版式加载/存储
// ============================================================================

/// 读版式：cr_report_fmt(BLOB, ZmcDataSet 零拷贝→base64) + 关系投影。
pub async fn load_layout(code: &str, q: &LayoutQuery) -> Result<Value> {
    let version = q
        .version
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_default()
        .to_string();

    // —— 版式 BLOB：ZmcDataSet 零拷贝，直接从行借出 bytes 编 base64 ——
    let mm = get_default_pg_db_manager();
    let fmt = {
        let ds = mm
            .query_sql_zmc_with_datavalues(
                RPT_DB_ID,
                r#"SELECT doc_content, doc_format, mime_type, file_size, content_hash, storage_type, external_uri
                   FROM cr_report_fmt WHERE report_code = $1 AND version_code = $2"#,
                vec![
                    DataValue::String(code.to_string()),
                    DataValue::String(version.clone()),
                ],
                "rpt_fmt",
            )
            .await
            .map_err(|e| api_err(&format!("读取报表格式失败: {e}")))?;
        if ds.row_count() == 0 {
            Value::Null
        } else {
            use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
            let sc = &ds.schema;
            let row = &ds.rows[0];
            let idx = |n: &str| sc.col_index(n);
            let content_b64 = idx("doc_content")
                .and_then(|c| row.get_bytes(c))
                .map(|b| BASE64.encode(b));
            let get_s = |n: &str| idx(n).and_then(|c| row.get_str(c)).map(str::to_owned);
            let get_i = |n: &str| idx(n).and_then(|c| row.get_i64(c));
            json!({
                "docContent": content_b64,
                "docFormat": get_s("doc_format"),
                "mimeType": get_s("mime_type"),
                "fileSize": get_i("file_size"),
                "contentHash": get_s("content_hash"),
                "storageType": get_s("storage_type"),
                "externalUri": get_s("external_uri"),
            })
        }
    };

    // —— 关系投影：小体量，直接 query_rows(JSON) 即可 ——
    let p = json!([code, version]);
    let sheets = query_rows(
        r#"SELECT report_code, version_code, sheet_index, name, sheet_type, tab_color,
                  row_count, col_count, header_rows, fixed_rows, fixed_cols, paper_size,
                  orientation, font_family, font_size, show_gridline, is_hidden,
                  title_style, header_style, sort_no, status
           FROM cr_report_sheet WHERE report_code=$1 AND version_code=$2
           ORDER BY sheet_index"#,
        p.clone(),
        "rpt_sheets",
    )
    .await?;
    let regions = query_rows(
        r#"SELECT report_code, version_code, sheet_code, region_code, region_name, region_type,
                  start_row, start_col, end_row, end_col, start_cell, end_cell, row_span, col_span,
                  direction, is_repeatable, data_source, is_merged, freeze_flag, bg_color,
                  border_style, style, sort_no, status
           FROM cr_report_region WHERE report_code=$1 AND version_code=$2
           ORDER BY sheet_code, region_code"#,
        p.clone(),
        "rpt_regions",
    )
    .await?;
    let rows = query_rows(
        r#"SELECT id, code, name, report_code, version_code, sheet_code, region_code, row_no,
                  line_no, row_type, parent_id, full_path, level_no, is_leaf, formula,
                  account_range, balance_dir, indent, is_bold, sort_no, status
           FROM cr_report_row WHERE report_code=$1 AND version_code=$2
           ORDER BY sheet_code, region_code, row_no"#,
        p.clone(),
        "rpt_rows",
    )
    .await?;
    let cols = query_rows(
        r#"SELECT id, code, name, report_code, version_code, sheet_code, region_code, col_no,
                  col_letter, col_type, parent_id, full_path, level_no, is_leaf, value_type,
                  period_offset, width, decimals, align, formula, is_hidden, sort_no, status
           FROM cr_report_col WHERE report_code=$1 AND version_code=$2
           ORDER BY sheet_code, region_code, col_no"#,
        p.clone(),
        "rpt_cols",
    )
    .await?;
    let cell_map = query_rows(
        r#"SELECT id, code, name, report_code, version_code, sheet_code, region_code, row_id,
                  col_id, cell_ref, element_code, value_type, data_source, calc_formula,
                  check_formula, is_editable, number_format, sort_no, status
           FROM cr_cell_element_map WHERE report_code=$1 AND version_code=$2
           ORDER BY sheet_code, region_code"#,
        p,
        "rpt_cell_map",
    )
    .await?;

    Ok(json!({
        "dbId": RPT_DB_ID,
        "reportCode": code,
        "version": version,
        "fmt": fmt,
        "sheets": sheets,
        "regions": regions,
        "rows": rows,
        "cols": cols,
        "cellMap": cell_map,
    }))
}

/// 存版式的语义结果：冲突（乐观锁）或成功（含返回载荷）。api 层据此映射 409 / 200。
pub enum SaveLayoutOutcome {
    /// content_hash 与库内不一致（他人已更新）。
    Conflict,
    /// 成功，携带 { ok, contentHash, fileSize, idMap } 载荷。
    Ok(Value),
}

/// 存版式：乐观锁校验 → 事务内 UPSERT BLOB + 重建关系投影。自管事务。
pub async fn save_layout(code: &str, body: &Value) -> Result<SaveLayoutOutcome> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use sha2::{Digest, Sha256};

    let version = s(body, "version").unwrap_or_default();
    let fmt = body.get("fmt").cloned().unwrap_or(Value::Null);
    let doc_b64 = fmt.get("docContent").and_then(|v| v.as_str()).unwrap_or("");
    let doc_bytes = BASE64
        .decode(doc_b64)
        .map_err(|e| api_err(&format!("docContent 非法 base64: {e}")))?;
    let doc_format = fmt
        .get("docFormat")
        .and_then(|v| v.as_str())
        .unwrap_or("ssjson")
        .to_string();
    let mime_type = fmt
        .get("mimeType")
        .and_then(|v| v.as_str())
        .unwrap_or("application/json")
        .to_string();
    let client_hash = fmt
        .get("contentHash")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let new_hash = format!("{:x}", Sha256::digest(&doc_bytes));
    let file_size = doc_bytes.len() as i64;

    let mm = get_default_pg_db_manager();

    // —— 乐观锁：比对 DB 现存 content_hash 与前端携带的 hash ——
    let cur = query_rows(
        "SELECT content_hash FROM cr_report_fmt WHERE report_code=$1 AND version_code=$2",
        json!([code, version]),
        "rpt_fmt_hash",
    )
    .await?;
    if let Some(row) = cur.first() {
        let db_hash = row
            .get("content_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if !db_hash.is_empty() && client_hash.as_deref().map(|h| h != db_hash).unwrap_or(false) {
            return Ok(SaveLayoutOutcome::Conflict);
        }
    }

    let tx = mm.get_transaction_context();
    let txn_id = tx
        .begin(RPT_DB_ID)
        .await
        .map_err(|e| api_err(&format!("开启事务失败: {e}")))?;

    let result = save_layout_apply(
        &txn_id,
        code,
        &version,
        doc_bytes,
        &doc_format,
        &mime_type,
        file_size,
        &new_hash,
        body,
    )
    .await;
    match result {
        Ok(id_map) => {
            tx.commit(&txn_id)
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(SaveLayoutOutcome::Ok(json!({
                "ok": true,
                "contentHash": new_hash,
                "fileSize": file_size,
                "idMap": id_map,
            })))
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn save_layout_apply(
    txn_id: &str,
    code: &str,
    version: &str,
    doc_bytes: Vec<u8>,
    doc_format: &str,
    mime_type: &str,
    file_size: i64,
    new_hash: &str,
    body: &Value,
) -> Result<Value> {
    // 1) UPSERT 版式 BLOB（DataValue::Binary → bytea）
    exec_dv(
        txn_id,
        r#"INSERT INTO cr_report_fmt
           (report_code, version_code, doc_content, doc_format, mime_type, file_size,
            content_hash, storage_type, sort_no, status, create_time, update_time)
           VALUES ($1,$2,$3,$4,$5,$6,$7,'inline',0,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)
           ON CONFLICT (report_code, version_code) DO UPDATE SET
             doc_content=EXCLUDED.doc_content, doc_format=EXCLUDED.doc_format,
             mime_type=EXCLUDED.mime_type, file_size=EXCLUDED.file_size,
             content_hash=EXCLUDED.content_hash, update_time=CURRENT_TIMESTAMP"#,
        vec![
            DataValue::String(code.to_string()),
            DataValue::String(version.to_string()),
            DataValue::Binary(doc_bytes),
            DataValue::String(doc_format.to_string()),
            DataValue::String(mime_type.to_string()),
            DataValue::Int(file_size),
            DataValue::String(new_hash.to_string()),
        ],
    )
    .await?;

    // 2) 重建关系投影：先删本 report+version 全部，再批量插入（幂等）
    for tbl in [
        "cr_report_sheet",
        "cr_report_region",
        "cr_report_row",
        "cr_report_col",
        "cr_cell_element_map",
    ] {
        exec_dv(
            txn_id,
            &format!("DELETE FROM {tbl} WHERE report_code=$1 AND version_code=$2"),
            vec![
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
            ],
        )
        .await?;
    }

    // 2a) sheets
    for (idx, sh) in arr(body, "sheets").iter().enumerate() {
        exec_dv(
            txn_id,
            r#"INSERT INTO cr_report_sheet
               (report_code, version_code, sheet_index, name, sheet_type, tab_color, row_count,
                col_count, header_rows, fixed_rows, fixed_cols, font_family, font_size,
                show_gridline, is_hidden, title_style, header_style, sort_no, status,
                create_time, update_time)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
            vec![
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
                dv_i64(i(sh, "sheetIndex").or(Some(idx as i64))),
                dv_str_def(s(sh, "name").as_deref(), &format!("Sheet{}", idx + 1)),
                dv_str_def(s(sh, "sheetType").as_deref(), "data"),
                dv_str(s(sh, "tabColor").as_deref()),
                dv_i64_def(i(sh, "rowCount"), 60),
                dv_i64_def(i(sh, "colCount"), 18),
                dv_i64(i(sh, "headerRows")),
                dv_i64(i(sh, "fixedRows")),
                dv_i64(i(sh, "fixedCols")),
                dv_str(s(sh, "fontFamily").as_deref()),
                dv_i64(i(sh, "fontSize")),
                dv_i64_def(i(sh, "showGridline"), 1),
                dv_i64_def(i(sh, "isHidden"), 0),
                dv_str(s(sh, "titleStyle").as_deref()),
                dv_str(s(sh, "headerStyle").as_deref()),
                dv_i64(i(sh, "sortNo").or(Some(idx as i64))),
            ],
        )
        .await?;
    }

    // 2b) regions（含默认区域）
    for (idx, rg) in arr(body, "regions").iter().enumerate() {
        let region_code = s(rg, "code")
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| DEFAULT_REGION.to_string());
        exec_dv(
            txn_id,
            r#"INSERT INTO cr_report_region
               (report_code, version_code, sheet_code, region_code, region_name, region_type,
                start_row, start_col, end_row, end_col, start_cell, end_cell, row_span, col_span,
                direction, is_repeatable, data_source, is_merged, freeze_flag, bg_color,
                border_style, style, sort_no, status, create_time, update_time)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
            vec![
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
                dv_str(s(rg, "sheetCode").as_deref()),
                DataValue::String(region_code),
                dv_str_def(s(rg, "name").as_deref(), "默认区域"),
                dv_str_def(s(rg, "type").as_deref(), "data"),
                dv_i64(i(rg, "startRow")),
                dv_i64(i(rg, "startCol")),
                dv_i64(i(rg, "endRow")),
                dv_i64(i(rg, "endCol")),
                dv_str(s(rg, "startCell").as_deref()),
                dv_str(s(rg, "endCell").as_deref()),
                dv_i64(i(rg, "rowSpan")),
                dv_i64(i(rg, "colSpan")),
                dv_str(s(rg, "direction").as_deref()),
                dv_i64_def(i(rg, "isRepeatable"), 0),
                dv_str(s(rg, "dataSource").as_deref()),
                dv_i64_def(i(rg, "isMerged"), 0),
                dv_i64_def(i(rg, "freezeFlag"), 0),
                dv_str(s(rg, "bgColor").as_deref()),
                dv_str(s(rg, "borderStyle").as_deref()),
                dv_str(s(rg, "style").as_deref()),
                dv_i64(i(rg, "sortNo").or(Some(idx as i64))),
            ],
        )
        .await?;
    }

    // 2c) rows —— id 铸真号 + 临时id映射
    let mut id_map = serde_json::Map::new();
    let mut temp_id_map: HashMap<String, i64> = HashMap::new();
    for (idx, rw) in arr(body, "rows").iter().enumerate() {
        let sheet_code = s(rw, "sheetCode").unwrap_or_default();
        let region_code = s(rw, "regionCode")
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| DEFAULT_REGION.to_string());
        let row_code = s(rw, "code").unwrap_or_default();
        let real_id = resolve_id(rw.get("id"));
        id_map.insert(
            format!("row|{sheet_code}|{region_code}|{row_code}"),
            json!(real_id),
        );
        if let Some(tid) = rw.get("id").and_then(|v| v.as_str()) {
            temp_id_map.insert(tid.to_string(), real_id);
        }
        exec_dv(
            txn_id,
            r#"INSERT INTO cr_report_row
               (id, code, name, report_code, version_code, sheet_code, region_code, row_no,
                line_no, row_type, parent_id, full_path, level_no, is_leaf, formula, account_range,
                balance_dir, indent, is_bold, sort_no, status, create_time, update_time)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
            vec![
                DataValue::Int(real_id),
                dv_str_def(Some(&row_code), &format!("R{}", idx + 1)),
                dv_str_def(s(rw, "name").as_deref(), ""),
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
                dv_str(Some(&sheet_code)),
                DataValue::String(region_code),
                dv_i64(i(rw, "rowNo").or(Some(idx as i64))),
                dv_str(s(rw, "lineNo").as_deref()),
                dv_str_def(s(rw, "rowType").as_deref(), "data"),
                dv_i64(i(rw, "parentId")),
                dv_str_def(s(rw, "fullPath").as_deref(), &row_code),
                dv_i64_def(i(rw, "levelNo"), 1),
                dv_i64_def(i(rw, "isLeaf"), 1),
                dv_str(s(rw, "formula").as_deref()),
                dv_str(s(rw, "accountRange").as_deref()),
                dv_str(s(rw, "balanceDir").as_deref()),
                dv_i64(i(rw, "indent")),
                dv_i64_def(i(rw, "isBold"), 0),
                dv_i64(i(rw, "sortNo").or(Some(idx as i64))),
            ],
        )
        .await?;
    }

    // 2d) cols
    for (idx, cl) in arr(body, "cols").iter().enumerate() {
        let sheet_code = s(cl, "sheetCode").unwrap_or_default();
        let region_code = s(cl, "regionCode")
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| DEFAULT_REGION.to_string());
        let col_code = s(cl, "code").unwrap_or_default();
        let real_id = resolve_id(cl.get("id"));
        id_map.insert(
            format!("col|{sheet_code}|{region_code}|{col_code}"),
            json!(real_id),
        );
        if let Some(tid) = cl.get("id").and_then(|v| v.as_str()) {
            temp_id_map.insert(tid.to_string(), real_id);
        }
        exec_dv(
            txn_id,
            r#"INSERT INTO cr_report_col
               (id, code, name, report_code, version_code, sheet_code, region_code, col_no,
                col_letter, col_type, parent_id, full_path, level_no, is_leaf, value_type, period_offset,
                width, decimals, align, formula, is_hidden, sort_no, status, create_time, update_time)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
            vec![
                DataValue::Int(real_id),
                dv_str_def(Some(&col_code), &col_letter_of(idx)),
                dv_str_def(s(cl, "name").as_deref(), ""),
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
                dv_str(Some(&sheet_code)),
                DataValue::String(region_code),
                dv_i64(i(cl, "colNo").or(Some(idx as i64))),
                dv_str(s(cl, "colLetter").as_deref()),
                dv_str_def(s(cl, "colType").as_deref(), "data"),
                dv_i64(i(cl, "parentId")),
                dv_str_def(s(cl, "fullPath").as_deref(), &col_code),
                dv_i64_def(i(cl, "levelNo"), 1),
                dv_i64_def(i(cl, "isLeaf"), 1),
                dv_str(s(cl, "valueType").as_deref()),
                dv_i64(i(cl, "periodOffset")),
                dv_i64(i(cl, "width")),
                dv_i64(i(cl, "decimals")),
                dv_str(s(cl, "align").as_deref()),
                dv_str(s(cl, "formula").as_deref()),
                dv_i64_def(i(cl, "isHidden"), 0),
                dv_i64(i(cl, "sortNo").or(Some(idx as i64))),
            ],
        )
        .await?;
    }

    // 2e) cell_element_map —— row_id/col_id 把前端临时id串解引用成真号
    let resolve_ref = |v: Option<&Value>| -> DataValue {
        match v {
            Some(Value::Number(n)) => n.as_i64().map(DataValue::Int).unwrap_or(DataValue::Int(0)),
            Some(Value::String(t)) => temp_id_map
                .get(t)
                .copied()
                .map(DataValue::Int)
                .unwrap_or(DataValue::Int(0)),
            _ => DataValue::Int(0),
        }
    };
    for (idx, cm) in arr(body, "cellMap").iter().enumerate() {
        exec_dv(
            txn_id,
            r#"INSERT INTO cr_cell_element_map
               (id, code, report_code, version_code, sheet_code, region_code, row_id, col_id, cell_ref,
                element_code, value_type, data_source, calc_formula, check_formula, is_editable,
                number_format, sort_no, status, create_time, update_time)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#,
            vec![
                DataValue::Int(cmx_utils::next_pk_id()),
                dv_str_def(s(cm, "cellRef").as_deref(), &format!("CM{}", idx + 1)),
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
                dv_str(s(cm, "sheetCode").as_deref()),
                dv_str_def(
                    s(cm, "regionCode").filter(|c| !c.is_empty()).as_deref(),
                    DEFAULT_REGION,
                ),
                resolve_ref(cm.get("rowId")),
                resolve_ref(cm.get("colId")),
                dv_str(s(cm, "cellRef").as_deref()),
                dv_str(s(cm, "elementCode").as_deref()),
                dv_str(s(cm, "valueType").as_deref()),
                dv_str(s(cm, "dataSource").as_deref()),
                dv_str(s(cm, "calcFormula").as_deref()),
                dv_str(s(cm, "checkFormula").as_deref()),
                dv_i64_def(i(cm, "isEditable"), 1),
                dv_str(s(cm, "numberFormat").as_deref()),
                dv_i64(i(cm, "sortNo").or(Some(idx as i64))),
            ],
        )
        .await?;
    }

    Ok(Value::Object(id_map))
}

// ============================================================================
// 模式二 · 数据加载/存储
// ============================================================================

/// 取数：cr_cell_data 按 org+period ZmcDataSet 零拷贝读。
pub async fn query_data(code: &str, body: &Value) -> Result<Value> {
    let version = s(body, "version").unwrap_or_default();
    let org = s(body, "orgCode").unwrap_or_default();
    let period = s(body, "periodCode").unwrap_or_default();

    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_zmc_with_datavalues(
            RPT_DB_ID,
            r#"SELECT sheet_code, region_code, row_id, col_id, cell_ref, element_code,
                      value_type, text_value, num_value, currency_code, amount_unit,
                      data_status, is_manual
               FROM cr_cell_data
               WHERE report_code=$1 AND version_code=$2 AND org_code=$3 AND period_code=$4
               ORDER BY sheet_code, region_code, row_id, col_id"#,
            vec![
                DataValue::String(code.to_string()),
                DataValue::String(version.clone()),
                DataValue::String(org.clone()),
                DataValue::String(period.clone()),
            ],
            "rpt_cell_data",
        )
        .await
        .map_err(|e| api_err(&format!("读取报表数据失败: {e}")))?;

    // 逐行零拷贝取值：只在结果 payload 里生成需要的小对象，峰值内存 O(结果集)。
    let sc = &ds.schema;
    let c_sheet = sc.col_index("sheet_code");
    let c_region = sc.col_index("region_code");
    let c_row = sc.col_index("row_id");
    let c_col = sc.col_index("col_id");
    let c_ref = sc.col_index("cell_ref");
    let c_el = sc.col_index("element_code");
    let c_vt = sc.col_index("value_type");
    let c_txt = sc.col_index("text_value");
    let c_num = sc.col_index("num_value");
    let c_cur = sc.col_index("currency_code");
    let c_unit = sc.col_index("amount_unit");
    let c_ds = sc.col_index("data_status");
    let c_man = sc.col_index("is_manual");

    let mut cells = Vec::with_capacity(ds.row_count());
    for row in &ds.rows {
        let gs = |c: Option<usize>| c.and_then(|i| row.get_str(i)).map(str::to_owned);
        let gi = |c: Option<usize>| c.and_then(|i| row.get_i64(i));
        let num = c_num.and_then(|i| row.get_decimal(i)).map(|d| d.to_string());
        cells.push(json!({
            "sheetCode": gs(c_sheet),
            "regionCode": gs(c_region),
            "rowId": gi(c_row),
            "colId": gi(c_col),
            "cellRef": gs(c_ref),
            "elementCode": gs(c_el),
            "valueType": gs(c_vt),
            "textValue": gs(c_txt),
            "numValue": num,
            "currencyCode": gs(c_cur),
            "amountUnit": gs(c_unit),
            "dataStatus": gs(c_ds),
            "isManual": gi(c_man),
        }));
    }

    Ok(json!({
        "dbId": RPT_DB_ID,
        "reportCode": code,
        "version": version,
        "orgCode": org,
        "periodCode": period,
        "count": cells.len(),
        "cells": cells,
    }))
}

/// 存数：批量 UPSERT cr_cell_data（8元唯一键幂等），自管事务。返回 { ok, saved }。
pub async fn save_data(code: &str, body: &Value) -> Result<Value> {
    let version = s(body, "version").unwrap_or_default();
    let org = s(body, "orgCode").unwrap_or_default();
    let period = s(body, "periodCode").unwrap_or_default();
    let cells = arr(body, "cells").to_vec();

    let mm = get_default_pg_db_manager();
    let tx = mm.get_transaction_context();
    let txn_id = tx
        .begin(RPT_DB_ID)
        .await
        .map_err(|e| api_err(&format!("开启事务失败: {e}")))?;

    let result = save_data_apply(&txn_id, code, &version, &org, &period, &cells).await;
    match result {
        Ok(n) => {
            tx.commit(&txn_id)
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(json!({ "ok": true, "saved": n }))
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            Err(e)
        }
    }
}

async fn save_data_apply(
    txn_id: &str,
    code: &str,
    version: &str,
    org: &str,
    period: &str,
    cells: &[Value],
) -> Result<usize> {
    for cell in cells {
        let region_code = s(cell, "regionCode")
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| DEFAULT_REGION.to_string());
        // 数值：DataValue::Decimal（前端传字符串数值）；空→Null。
        let num_val = cell
            .get("numValue")
            .and_then(|v| match v {
                Value::String(x) if !x.is_empty() => x.parse::<rust_decimal::Decimal>().ok(),
                Value::Number(n) => n
                    .as_f64()
                    .and_then(rust_decimal::Decimal::from_f64_retain),
                _ => None,
            })
            .map(DataValue::Decimal)
            .unwrap_or(DataValue::NullTyped(SqlTypeMarker::Decimal));

        exec_dv(
            txn_id,
            r#"INSERT INTO cr_cell_data
               (id, org_code, period_code, report_code, version_code, sheet_code, region_code,
                row_id, col_id, cell_ref, element_code, value_type, text_value, num_value,
                currency_code, amount_unit, data_status, is_manual, compute_time, sort_no, status,
                create_time, update_time)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,CURRENT_TIMESTAMP,0,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)
               ON CONFLICT (org_code, period_code, report_code, version_code, sheet_code, region_code, row_id, col_id)
               DO UPDATE SET cell_ref=EXCLUDED.cell_ref, element_code=EXCLUDED.element_code,
                 value_type=EXCLUDED.value_type, text_value=EXCLUDED.text_value,
                 num_value=EXCLUDED.num_value, currency_code=EXCLUDED.currency_code,
                 amount_unit=EXCLUDED.amount_unit, data_status=EXCLUDED.data_status,
                 is_manual=EXCLUDED.is_manual, compute_time=CURRENT_TIMESTAMP,
                 update_time=CURRENT_TIMESTAMP"#,
            vec![
                DataValue::Int(cmx_utils::next_pk_id()),
                DataValue::String(org.to_string()),
                DataValue::String(period.to_string()),
                DataValue::String(code.to_string()),
                DataValue::String(version.to_string()),
                dv_str(s(cell, "sheetCode").as_deref()),
                DataValue::String(region_code),
                dv_i64_def(i(cell, "rowId"), 0),
                dv_i64_def(i(cell, "colId"), 0),
                dv_str(s(cell, "cellRef").as_deref()),
                dv_str(s(cell, "elementCode").as_deref()),
                dv_str(s(cell, "valueType").as_deref()),
                dv_str(s(cell, "textValue").as_deref()),
                num_val,
                dv_str(s(cell, "currencyCode").as_deref()),
                dv_str(s(cell, "amountUnit").as_deref()),
                dv_str_def(s(cell, "dataStatus").as_deref(), "manual"),
                dv_i64_def(i(cell, "isManual"), 1),
            ],
        )
        .await?;
    }
    Ok(cells.len())
}
