//! cmx-dct-store-pg 数据装载服务——分页查询（[`dict_search`]）与零拷贝列式（[`dict_search_zmc`]）。
//!
//! 两个场景函数共用 [`build_search`] 前置（构造 data + count SQL + 参数，附带脱敏 debug 日志），
//! 避免重复。zmc 路径返回原始 `ZmcDataSet`（未编码），由 handler 层自行决定序列化方式
//! （msgpack / JSON）。

use cmx_api_types::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::{ZmcDataSet, ZmcRowSource, get_default_pg_db_manager};
use cmx_dct_model::{DctQuery, DictView, build_search_sql};

use crate::error::api_err;
use crate::meta::{SearchQuery, SearchResult};
use crate::resolve::resolve_dict;

/// 构造 search 的 (sql, count_sql, params)，附带结构化 debug 日志（脱敏：不打印 raw 全文）。
///
/// [`dict_search`]（执行 data + count）与 [`dict_search_zmc`]（只执行 data + count 挂 total）
/// 共用此前置，避免 SQL 构造 + 日志重复。
fn build_search(view: &DictView, q: &SearchQuery) -> (String, String, Vec<DataValue>) {
    let raw = q.to_raw();
    let (sql, count_sql, params) = build_search_sql(view, &raw);
    tracing::debug!(
        target: "cmx_dct::search",
        dict_code = %view.dict_code, table = %view.table_name,
        sql_len = sql.len(), params_len = params.len(),
        "build search sql"
    );
    (sql, count_sql, params)
}

/// 装载字典数据（分页 + 计数）。返回 [`SearchResult`]（rows + total + page + pageSize）。
///
/// 一步到位：内部 `resolve_dict` 解析字典视图 → 构造 SQL → 执行 data + count → 组装结果。
/// 调用方只需提供 [`DctQuery`]（定位）+ [`SearchQuery`]（查询参数）+ db_id。
pub async fn dict_search(
    q: &DctQuery,
    search: &SearchQuery,
    db_id: &str,
) -> Result<SearchResult> {
    let view = resolve_dict(q, true).await?;
    let (sql, count_sql, params) = build_search(&view, search);

    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params.clone(), &view.dict_code)
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
    let rows = serde_json::to_value(&ds)
        .map_err(|e| api_err(&format!("序列化失败: {e}")))?
        .get("rows")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
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

    Ok(SearchResult {
        rows,
        total,
        page: search.page,
        page_size: search.page_size,
    })
}

/// 零拷贝装载：tokio-postgres + ZmcDataSet + COUNT 挂 total。返回原始 [`ZmcDataSet`]（未编码）。
///
/// 与 [`dict_search`] 对齐：跑一次 COUNT(*) 把总条数挂到 `zmc.total`，供前端分页工具栏算总页数
/// （前端 `pkg.total` 读取，缺省 null）。COUNT 与主 SELECT 共用同一份 filter 下推
/// （同一 where_sql + params），看到的行集一致。
///
/// 调用方（通常是 handler）拿到 ZmcDataSet 后自行决定序列化方式（msgpack 列式二进制 / JSON 等）。
pub async fn dict_search_zmc(
    q: &DctQuery,
    search: &SearchQuery,
    db_id: &str,
) -> Result<ZmcDataSet> {
    let view = resolve_dict(q, true).await?;
    let (sql, count_sql, params) = build_search(&view, search);

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

    // COUNT(*) → zmc.total（与 dict_search 端点契约对齐：zmc 路径也回传 total 供前端分页）。
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
    Ok(zmc)
}
