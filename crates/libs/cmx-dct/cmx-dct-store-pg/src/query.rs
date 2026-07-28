//! cmx-dct-store-pg 数据装载服务——分页查询（`search`）与零拷贝列式（`search_zmc`）。
//!
//! 两个函数共用 [`build_search`] 前置（构造 data + count SQL + 参数，附带脱敏 debug 日志），
//! 避免重复。zmc 路径不执行 count（流式列式导出不分页计数）。

use cmx_api_types::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::{ZmcRowSource, get_default_pg_db_manager};
use cmx_dct_model::{DictView, build_search_sql};
use serde_json::{Value, json};

use crate::error::api_err;

/// 构造 search 的 (sql, count_sql, params)，附带结构化 debug 日志（脱敏：不打印 raw 全文）。
///
/// [`search`]（执行 data + count）与 [`search_zmc`]（只执行 data）共用此前置，
/// 避免 SQL 构造 + 日志重复。zmc 不执行 count（流式列式导出不分页计数）。
fn build_search(view: &DictView, raw: &Value) -> (String, String, Vec<DataValue>) {
    let (sql, count_sql, params) = build_search_sql(view, raw);
    tracing::debug!(
        target: "cmx_dct::search",
        dict_code = %view.dict_code, table = %view.table_name,
        sql_len = sql.len(), params_len = params.len(),
        "build search sql"
    );
    (sql, count_sql, params)
}

/// 装载字典数据（分页 + 计数）。返回 `{rows,total,page,pageSize}`。
pub async fn search(view: &DictView, raw: &Value, db_id: &str) -> Result<Value> {
    let (sql, count_sql, params) = build_search(view, raw);

    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            &sql,
            params.clone(),
            &view.dict_code,
        )
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search data query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %sql, "failed sql");
            api_err(&format!("字典查询失败: {e}"))
        })?;
    let total_ds = mm
        .query_sql_with_datavalues(db_id, None, &count_sql, params, "cnt")
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search count query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %count_sql, "failed sql");
            api_err(&format!("字典计数失败: {e}"))
        })?;

    // DataSet → rows JSON。
    let rows_val = serde_json::to_value(&ds).map_err(|e| api_err(&format!("序列化失败: {e}")))?;
    let rows = rows_val.get("rows").cloned().unwrap_or_else(|| json!([]));
    let total = serde_json::to_value(&total_ds)
        .ok()
        .and_then(|v| {
            v.get("rows")
                .and_then(|r| r.get(0))
                .and_then(|r0| r0.get("cnt"))
                .cloned()
        })
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let (page, page_size) = cmx_dct_model::parse_paging(raw);
    Ok(json!({
        "rows": rows,
        "total": total,
        "page": page,
        "pageSize": page_size,
    }))
}

/// 零拷贝装载：tokio-postgres + ZmcDataSet + 列式二进制。返回列式包字节（handler 包 msgpack 信封）。
///
/// 与 [`search`] 对齐：跑一次 COUNT(*) 把总条数挂到 `zmc.total`，编码进列式包的 `total` 字段，
/// 供前端分页工具栏算总页数（前端 `pkg.total` 读取，缺省 null）。COUNT 与主 SELECT 共用同一份
/// filter 下推（同一 where_sql + params），看到的行集一致。
pub async fn search_zmc(view: &DictView, raw: &Value, db_id: &str) -> Result<Vec<u8>> {
    let (sql, count_sql, params) = build_search(view, raw);

    let mm = get_default_pg_db_manager();
    // 零拷贝：ZmcDataSet 持有原始 tokio-postgres Row，惰性列式二进制编码。
    let mut zmc = mm
        .query_sql_zmc_with_datavalues(db_id, &sql, params.clone(), &view.dict_code)
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search_zmc query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %sql, "failed sql");
            api_err(&format!("字典零拷贝查询失败: {e}"))
        })?;

    // COUNT(*) → zmc.total（与 search 端点契约对齐：zmc 路径也回传 total 供前端分页）。
    let count_ds = mm
        .query_sql_zmc_with_datavalues(db_id, &count_sql, params, "cnt")
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search_zmc count query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %count_sql, "failed sql");
            api_err(&format!("字典零拷贝计数失败: {e}"))
        })?;
    if let Some(row0) = count_ds.rows.first()
        && let Some(n) = row0.get_i64(0)
    {
        zmc.total = Some(n);
    }

    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    Ok(buf)
}
