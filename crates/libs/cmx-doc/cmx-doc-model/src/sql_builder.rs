//! sql_builder —— 通用单据**每层 SELECT 构建内核**（元数据驱动，零单据专属假设）。
//!
//! 输入：一层的 [`LayerView`]（来自 `DocMetaView` 解析）+ 该层的 [`LayerQuery`] + 可选父作用域。
//! 输出：`(sql_text, Vec<DataValue>)` —— 参数化 SQL（`$1..$N` 占位）+ 顺序绑定值。
//!
//! **通用性铁律**：本模块**不认识任何具体单据**（不出现 cv_batch/local_dr 等词）。
//! 列名、类型、表名全部来自 `LayerView`/`ColumnView`；任何用到的列都先经 `has_column` 白名单
//! 校验（防注入）；值按列 `data_type` 类型化成 [`DataValue`] 走参数绑定（两驱动都完备，
//! 且绕开 sea-query Values 的 decimal/array feature 缺口）。
//!
//! 生成的 SQL 对 tokio-postgres 与 sqlx 通用（都用 `$N` 占位 + DataValue 绑定）。

use cmx_core::model::cell::DataValue;
use serde_json::Value;

use super::meta::LayerView;
use super::query::{Cond, Filter, LayerQuery, Op, OrderBy, json_to_datavalue};
use cmx_biz::{BizError, Result};

/// 参数收集器：顺序累积 DataValue，产出 `$N` 占位符。
struct Params {
    vals: Vec<DataValue>,
}
impl Params {
    fn new() -> Self {
        Params { vals: Vec::new() }
    }
    /// 追加一个值，返回其占位符 `$N`（N=1-based）。
    fn push(&mut self, v: DataValue) -> String {
        self.vals.push(v);
        format!("${}", self.vals.len())
    }
    /// 追加一个数组值（用于 `= ANY($N)`），返回 `$N`。
    fn push_array(&mut self, vs: Vec<DataValue>) -> String {
        self.vals.push(DataValue::Array(vs));
        format!("${}", self.vals.len())
    }
}

/// 双引号包裹标识符（列名/表名），防关键字冲突。列名已白名单校验，安全。
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// 构建一层的 SELECT。
///
/// - `layer`：本层视图（列/表名/schema）。
/// - `lq`：本层查询（过滤/排序/分页/游标）。
/// - `parent_scope`：`Some((child_key, parent_ids))` 时生成 `WHERE child_key = ANY($n)`（子层驱动）。
///
/// 返回 `(sql, params)`。默认排序：有 parent_scope → `[child_key, line_no?]`，否则 → `[id?]`。
pub fn build_layer_select(
    layer: &LayerView,
    lq: &LayerQuery,
    parent_scope: Option<(&str, &[DataValue])>,
) -> Result<(String, Vec<DataValue>)> {
    // 列名白名单 + 类型化前置校验
    lq.validate_against(layer)?;

    let mut p = Params::new();
    let cols = column_list(layer);
    let mut sql = format!("SELECT {cols} FROM {}", quote_ident(&layer.table_name));

    // ── WHERE ─────────────────────────────────────────────────────────
    let mut conds: Vec<String> = Vec::new();

    // 父作用域：child_key = ANY($n)
    if let Some((child_key, parent_ids)) = parent_scope {
        if !layer.has_column(child_key) {
            return Err(BizError::business(format!(
                "子键 {child_key} 不在层 {} 中",
                layer.table_name
            )));
        }
        let ph = p.push_array(parent_ids.to_vec());
        conds.push(format!("{} = ANY({ph})", quote_ident(child_key)));
    }

    // 过滤树下推
    if let Some(filter) = &lq.filter {
        conds.push(build_filter(layer, filter, &mut p)?);
    }

    // keyset 游标谓词
    if let Some(cursor) = &lq.cursor {
        let order = effective_order(layer, lq, parent_scope);
        conds.push(build_cursor_pred(layer, &order, cursor, &mut p)?);
    }

    if !conds.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conds.join(" AND "));
    }

    // ── ORDER BY ──────────────────────────────────────────────────────
    let order = effective_order(layer, lq, parent_scope);
    if !order.is_empty() {
        let ob = order
            .iter()
            .map(|o| {
                format!(
                    "{} {}",
                    quote_ident(&o.col),
                    if o.desc { "DESC" } else { "ASC" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(" ORDER BY ");
        sql.push_str(&ob);
    }

    // ── LIMIT / OFFSET ────────────────────────────────────────────────
    if let Some(n) = lq.limit {
        sql.push_str(&format!(" LIMIT {n}"));
    }
    if let Some(off) = lq.offset {
        // 游标与 offset 二选一：有游标时忽略 offset（游标已定位）
        if lq.cursor.is_none() {
            sql.push_str(&format!(" OFFSET {off}"));
        }
    }

    Ok((sql, p.vals))
}

/// 构造根层 COUNT 查询：`SELECT COUNT(*) FROM <table> [WHERE <filter>]`。
///
/// 与 [`build_layer_select`] 的核心差异：COUNT 不需要 ORDER BY / LIMIT / OFFSET / cursor /
/// parent_scope——只复用 filter 下推以保证 COUNT 与分页 SELECT 看到同一行集。
///
/// # 参数
/// - `layer`：本层视图（用于表名 + filter 列校验）
/// - `lq`：本层查询（仅复用 `lq.filter`；其它字段忽略）
///
/// # 返回
/// `(sql, params)`，参数顺序与 `$N` 占位符对齐。
pub fn build_layer_count(
    layer: &LayerView,
    lq: &LayerQuery,
) -> Result<(String, Vec<DataValue>)> {
    // 列名白名单前置校验（与 build_layer_select 一致）
    lq.validate_against(layer)?;

    let mut p = Params::new();
    let mut sql = format!("SELECT COUNT(*) FROM {}", quote_ident(&layer.table_name));

    // 仅复用 filter 下推（不含 parent_scope/cursor；COUNT 只针对根层独立计数）
    if let Some(filter) = &lq.filter {
        let f = build_filter(layer, filter, &mut p)?;
        sql.push_str(" WHERE ");
        sql.push_str(&f);
    }

    Ok((sql, p.vals))
}

/// 逗号分隔的**带引号**列名列表（按定义 schema 顺序）。
fn column_list(layer: &LayerView) -> String {
    layer
        .schema
        .fields
        .iter()
        .map(|f| quote_ident(&f.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 有效排序：显式 order_by 优先；否则默认（子层 [child_key, line_no?]，根层 []）。
/// **始终以 id 收尾**作为稳定 tie-breaker（keyset 游标依赖它，且保证结果稳定），除非已含 id。
fn effective_order(
    layer: &LayerView,
    lq: &LayerQuery,
    parent_scope: Option<(&str, &[DataValue])>,
) -> Vec<OrderBy> {
    let mut out: Vec<OrderBy> = if !lq.order_by.is_empty() {
        lq.order_by.clone()
    } else {
        let mut d = Vec::new();
        if let Some((child_key, _)) = parent_scope {
            if layer.has_column(child_key) {
                d.push(OrderBy {
                    col: child_key.to_string(),
                    desc: false,
                });
            }
            if layer.has_column("line_no") {
                d.push(OrderBy {
                    col: "line_no".to_string(),
                    desc: false,
                });
            }
        }
        d
    };
    // 始终以 id 收尾（稳定 tie-breaker），若有 id 列且未包含
    if layer.has_column("id") && !out.iter().any(|o| o.col == "id") {
        out.push(OrderBy {
            col: "id".to_string(),
            desc: false,
        });
    }
    out
}

/// 递归把过滤树编成 SQL 片段（带括号），值累积到 params。
fn build_filter(layer: &LayerView, f: &Filter, p: &mut Params) -> Result<String> {
    match f {
        Filter::And(children) => {
            let parts = children
                .iter()
                .map(|c| build_filter(layer, c, p))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("({})", parts.join(" AND ")))
        }
        Filter::Or(children) => {
            let parts = children
                .iter()
                .map(|c| build_filter(layer, c, p))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("({})", parts.join(" OR ")))
        }
        Filter::Leaf(cond) => build_leaf(layer, cond, p),
    }
}

/// 单个叶子条件 → SQL 片段。列名已在 validate_against 校验过，这里再取列做类型化。
fn build_leaf(layer: &LayerView, c: &Cond, p: &mut Params) -> Result<String> {
    let col = layer
        .column(&c.col)
        .ok_or_else(|| BizError::business(format!("列 {} 不在层 {}", c.col, layer.table_name)))?;
    let qcol = quote_ident(&c.col);

    let type_val = |v: &Value| json_to_datavalue(col, v);

    Ok(match c.op {
        Op::Eq => format!("{qcol} = {}", p.push(type_val(&c.val)?)),
        Op::Ne => format!("{qcol} <> {}", p.push(type_val(&c.val)?)),
        Op::Gt => format!("{qcol} > {}", p.push(type_val(&c.val)?)),
        Op::Gte => format!("{qcol} >= {}", p.push(type_val(&c.val)?)),
        Op::Lt => format!("{qcol} < {}", p.push(type_val(&c.val)?)),
        Op::Lte => format!("{qcol} <= {}", p.push(type_val(&c.val)?)),
        Op::In | Op::NotIn => {
            let arr = c.val.as_array().ok_or_else(|| {
                BizError::business(format!("{} 的 $in/$notIn 值必须是数组", c.col))
            })?;
            let vals = arr.iter().map(type_val).collect::<Result<Vec<_>>>()?;
            let ph = p.push_array(vals);
            let neg = if c.op == Op::NotIn { "NOT " } else { "" };
            // 用 = ANY / <> ALL（数组参数，两驱动都支持）
            if c.op == Op::NotIn {
                format!("{qcol} <> ALL({ph})")
            } else {
                let _ = neg;
                format!("{qcol} = ANY({ph})")
            }
        }
        Op::Like => format!("{qcol} LIKE {}", p.push(text_val(&c.val))),
        Op::Ilike => format!("{qcol} ILIKE {}", p.push(text_val(&c.val))),
        Op::Contains => format!("{qcol} LIKE {}", p.push(like_wrap(&c.val, "%", "%"))),
        Op::StartsWith => format!("{qcol} LIKE {}", p.push(like_wrap(&c.val, "", "%"))),
        Op::EndsWith => format!("{qcol} LIKE {}", p.push(like_wrap(&c.val, "%", ""))),
        Op::IsNull => {
            let is_null = c.val.as_bool().unwrap_or(true);
            if is_null {
                format!("{qcol} IS NULL")
            } else {
                format!("{qcol} IS NOT NULL")
            }
        }
    })
}

/// LIKE/ILIKE 值统一当文本。
fn text_val(v: &Value) -> DataValue {
    match v {
        Value::String(s) => DataValue::String(s.clone()),
        Value::Number(n) => DataValue::String(n.to_string()),
        _ => DataValue::String(v.to_string()),
    }
}

/// contains/startsWith/endsWith：包裹 % 生成 LIKE 模式（转义 % _ \）。
fn like_wrap(v: &Value, pre: &str, suf: &str) -> DataValue {
    let raw = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => v.to_string(),
    };
    let esc = raw
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    DataValue::String(format!("{pre}{esc}{suf}"))
}

/// keyset 游标谓词：`ORDER BY c1 a1, c2 a2, ... id` 下，取「> 上一页末值」。
///
/// 生成行值比较的展开式（不用 SQL row-value，兼容混合升降序）：
///   (c1 OP1 v1)
///   OR (c1 = v1 AND c2 OP2 v2)
///   OR (c1 = v1 AND c2 = v2 AND id > id_last)
/// 其中 OPk 依该列升/降序取 `>`/`<`。末列恒以 id 收尾（tie-breaker）。
fn build_cursor_pred(
    layer: &LayerView,
    order: &[OrderBy],
    cursor: &super::query::Cursor,
    p: &mut Params,
) -> Result<String> {
    // order 末列应为 id（effective_order 保证）；游标 vals 与「order 除 id 外」对齐。
    // 组装比较列序列（含最终 id）。
    let mut keys: Vec<(&str, bool)> = order.iter().map(|o| (o.col.as_str(), o.desc)).collect();
    if keys.is_empty() {
        // 无排序无法游标；退化：id > cursor.id
        if layer.has_column("id") {
            keys.push(("id", false));
        } else {
            return Err(BizError::business("无排序列，无法使用游标分页"));
        }
    }

    // 值序列：与 keys 对齐。除末列 id 外用 cursor.vals；末列用 cursor.id。
    // keys 里最后一项若是 id，用 cursor.id；其余用 vals（按序）。
    let last_is_id = keys.last().map(|(c, _)| *c == "id").unwrap_or(false);
    let non_id_count = if last_is_id {
        keys.len() - 1
    } else {
        keys.len()
    };
    if cursor.vals.len() < non_id_count {
        return Err(BizError::business("游标值数量与排序列不匹配"));
    }

    // 预取每个 key 的类型化值
    let mut typed: Vec<DataValue> = Vec::with_capacity(keys.len());
    for (i, (col, _)) in keys.iter().enumerate() {
        let cv = layer
            .column(col)
            .ok_or_else(|| BizError::business(format!("游标列 {col} 不在层")))?;
        let jv = if last_is_id && i == keys.len() - 1 {
            &cursor.id
        } else {
            &cursor.vals[i]
        };
        typed.push(json_to_datavalue(cv, jv)?);
    }

    // 展开成 OR 链
    let mut ors: Vec<String> = Vec::new();
    for i in 0..keys.len() {
        let mut ands: Vec<String> = Vec::new();
        // 前 i 列相等
        for j in 0..i {
            let ph = p.push(typed[j].clone());
            ands.push(format!("{} = {ph}", quote_ident(keys[j].0)));
        }
        // 第 i 列严格大/小
        let (col, desc) = keys[i];
        let cmp = if desc { "<" } else { ">" };
        let ph = p.push(typed[i].clone());
        ands.push(format!("{} {cmp} {ph}", quote_ident(col)));
        ors.push(format!("({})", ands.join(" AND ")));
    }
    Ok(format!("({})", ors.join(" OR ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{ColumnView, LayerView};
    use cmx_core::model::cell::{Field, FieldType};
    use cmx_core::model::data::dataset::Schema;
    use serde_json::json;
    use std::sync::Arc;

    // 造一个通用 mock 层（列名/类型任意——证明零单据专属）。
    fn mock_layer(table: &str, cols: &[(&str, &str)]) -> LayerView {
        let columns: Vec<ColumnView> = cols
            .iter()
            .map(|(n, dt)| ColumnView {
                name: n.to_string(),
                data_type: dt.to_string(),
                nullable: true,
                is_primary_key: *n == "id",
                caption: n.to_string(),
                dim_type: String::new(),
                agg: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
            })
            .collect();
        let fields: Vec<Field> = cols
            .iter()
            .map(|(n, dt)| Field {
                name: n.to_string(),
                field_type: match *dt {
                    "BIGINT" | "INT" => FieldType::Int,
                    "DECIMAL" => FieldType::Decimal,
                    "DATE" => FieldType::Date,
                    _ => FieldType::String,
                },
                label: String::new(),
            })
            .collect();
        LayerView {
            id: table.to_string(),
            table_name: table.to_string(),
            level: "L1".to_string(),
            level_name: String::new(),
            parent_table: String::new(),
            columns,
            summaries: Vec::new(),
            agg_fields: Vec::new(),
            schema: Arc::new(Schema::new_unchecked(table, fields)),
            spec: Arc::new(cmx_biz::validation::TableSpec {
                table: table.to_string(),
                columns: std::collections::HashMap::new(),
                order: Vec::new(),
            }),
        }
    }

    fn lq(filter: Value) -> LayerQuery {
        LayerQuery {
            filter: Filter::from_json(&filter).unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn root_eq_and_range() {
        let layer = mock_layer(
            "t",
            &[("id", "BIGINT"), ("code", "VARCHAR"), ("amt", "DECIMAL")],
        );
        let q = lq(json!({ "code": "A", "amt": { "$gte": 100, "$lt": 500 } }));
        let (sql, params) = build_layer_select(&layer, &q, None).unwrap();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("\"code\" = $1") || sql.contains("\"code\" = $"));
        assert!(sql.contains("ORDER BY \"id\" ASC"));
        assert_eq!(params.len(), 3); // code, amt gte, amt lt
    }

    #[test]
    fn count_reuses_filter() {
        let layer = mock_layer(
            "t",
            &[("id", "BIGINT"), ("code", "VARCHAR"), ("amt", "DECIMAL")],
        );
        let q = lq(json!({ "code": "A", "amt": { "$gte": 100, "$lt": 500 } }));
        let (sql, params) = build_layer_count(&layer, &q).unwrap();
        // 形如 SELECT COUNT(*) FROM "t" WHERE (...)
        assert!(
            sql.starts_with("SELECT COUNT(*) FROM \"t\""),
            "unexpected sql: {sql}"
        );
        assert!(sql.contains("WHERE"), "应含 WHERE 复用 filter: {sql}");
        // 与 build_layer_select 同一套 filter → 同一份参数（code, amt gte, amt lt）
        assert_eq!(params.len(), 3);
        // COUNT 不该带 ORDER BY / LIMIT / OFFSET
        assert!(!sql.contains("ORDER BY"), "COUNT 不应含 ORDER BY: {sql}");
        assert!(!sql.contains("LIMIT"), "COUNT 不应含 LIMIT: {sql}");
        assert!(!sql.contains("OFFSET"), "COUNT 不应含 OFFSET: {sql}");

        // 无 filter 时：裸 COUNT(*)，无 WHERE
        let empty = LayerQuery::default();
        let (sql2, params2) = build_layer_count(&layer, &empty).unwrap();
        assert_eq!(sql2, "SELECT COUNT(*) FROM \"t\"");
        assert!(params2.is_empty());
    }

    #[test]
    fn child_scope_any() {
        let layer = mock_layer(
            "c",
            &[("id", "BIGINT"), ("upper_id", "BIGINT"), ("line_no", "INT")],
        );
        let parents = vec![DataValue::Int(1), DataValue::Int(2)];
        let (sql, params) =
            build_layer_select(&layer, &LayerQuery::default(), Some(("upper_id", &parents)))
                .unwrap();
        assert!(sql.contains("\"upper_id\" = ANY($1)"));
        // 默认排序 child_key, line_no, id
        assert!(sql.contains("ORDER BY \"upper_id\" ASC, \"line_no\" ASC, \"id\" ASC"));
        assert_eq!(params.len(), 1);
        assert!(matches!(params[0], DataValue::Array(_)));
    }

    #[test]
    fn in_and_isnull_and_like() {
        let layer = mock_layer(
            "t",
            &[("id", "BIGINT"), ("st", "VARCHAR"), ("nm", "VARCHAR")],
        );
        let q = lq(json!({ "st": { "$in": ["a","b"] }, "nm": { "$contains": "x" } }));
        let (sql, params) = build_layer_select(&layer, &q, None).unwrap();
        assert!(sql.contains("\"st\" = ANY("));
        assert!(sql.contains("\"nm\" LIKE "));
        // contains 包 %
        assert!(
            params
                .iter()
                .any(|v| matches!(v, DataValue::String(s) if s == "%x%"))
        );
    }

    #[test]
    fn or_group() {
        let layer = mock_layer("t", &[("id", "BIGINT"), ("st", "VARCHAR")]);
        let q = lq(json!({ "$or": [ {"st":"a"}, {"st":"b"} ] }));
        let (sql, _p) = build_layer_select(&layer, &q, None).unwrap();
        assert!(sql.contains(" OR "));
    }

    #[test]
    fn illegal_column_rejected() {
        let layer = mock_layer("t", &[("id", "BIGINT")]);
        let q = lq(json!({ "evil; DROP TABLE": "x" }));
        assert!(build_layer_select(&layer, &q, None).is_err());
    }

    #[test]
    fn cursor_pred_expands() {
        let layer = mock_layer("t", &[("id", "BIGINT"), ("d", "DATE")]);
        let mut q = LayerQuery {
            order_by: vec![OrderBy {
                col: "d".into(),
                desc: false,
            }],
            ..Default::default()
        };
        q.cursor = Some(super::super::query::Cursor {
            vals: vec![json!("2026-01-01")],
            id: json!(100),
        });
        let (sql, params) = build_layer_select(&layer, &q, None).unwrap();
        // 展开：(d > $) OR (d = $ AND id > $)
        assert!(sql.contains(" OR "));
        assert!(sql.contains("\"d\" > $"));
        assert!(sql.contains("\"id\" > $"));
        assert_eq!(params.len(), 3); // d(gt) + d(eq) + id(gt)
    }
}
