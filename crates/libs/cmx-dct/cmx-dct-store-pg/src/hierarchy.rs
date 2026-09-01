//! cmx-dct-store-pg 分级字典层级字段（`level_no` / `full_path` / `is_leaf`）级联维护。
//!
//! 分级字典（`self_hierarchy=true` + `parent_field` + `is_leaf` 列齐全）保存时需维护三字段：
//! - 新增子节点 / 节点移入新父 → 新父 `is_leaf=0`
//! - 删除子节点 / 节点移出旧父 → 旧父若无其他子节点则 `is_leaf=1`
//! - 任何 parent 变更 → 受影响节点及整棵子树的 `level_no` / `full_path` 重算
//!
//! 与 cmx-iam 的差异：iam 在事务提交后用独立连接重算（规避删除可见性竞态）；
//! 此处 dct 在 save_apply 同事务内重算（dct 三段 apply 顺序 deleted→inserted→updated，
//! 重算在最后统一执行，此时所有增删改已落地，同事务内可见性一致，无竞态）。
//!
//! 内部维护函数均 `pub(crate)`（供 [`crate::write`] 模块调用）；跨 crate 调用方
//! （如 MDM 激活器等外部直写路径）用 [`recompute_dict_hierarchy`] 补偿入口。

use cmx_api_types::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::{DatabaseManager, get_default_pg_db_manager};
use cmx_dct_model::{DctQuery, DictView, to_dv_by_col, valid_col};
use serde_json::Value;

use crate::error::map_db_err;
use crate::resolve::resolve_dict;

/// 分级字典的 parent 列名。仅当 self_hierarchy=true + parent_field 非空 + is_leaf 是合法列时返回。
/// 不满足任一条件（非分级字典或缺列）返回 None，调用方据此跳过所有层级维护。
pub(crate) fn hierarchy_parent_field(view: &DictView) -> Option<String> {
    if !view.self_hierarchy {
        return None;
    }
    let pf = view.parent_field.as_ref()?;
    if !valid_col(view, pf) || !valid_col(view, "is_leaf") {
        return None;
    }
    Some(pf.clone())
}

/// 取 JSON 值作为非空 id 字符串。null / 空串 / 空对象返回 None（表示无父或父置空）。
pub(crate) fn non_empty_id_str(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// 在事务内查某行的 parent_id 值。返回 None 表示无父（null/空/行不存在）。
pub(crate) async fn select_parent_id(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    parent_field: &str,
    id: &Value,
) -> Result<Option<String>> {
    let sql = format!(
        "SELECT \"{}\" AS pid FROM \"{}\" WHERE \"{}\" = $1",
        parent_field, view.table_name, view.pk
    );
    let params = vec![to_dv_by_col(view, &view.pk, id)];
    let ds = mm
        .query_sql_with_datavalues(db_id, Some(txn_id), &sql, params, "pid")
        .await
        .map_err(|e| map_db_err(e, "select_parent_id", view, None, &sql))?;
    let ds_val = serde_json::to_value(&ds).ok();
    let pid = ds_val
        .as_ref()
        .and_then(|v| v.get("rows"))
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
        .and_then(|r0| r0.get("pid"))
        .and_then(non_empty_id_str);
    Ok(pid)
}

/// 把一组 id 字符串去重（保序），并生成 `$1,$2,...` 占位符列表。
/// 抽成不依赖 view 的纯函数，方便单元测试去重 + 占位符拼接逻辑。
fn dedup_with_placeholders(ids: &[String]) -> (Vec<String>, String) {
    let mut uniq: Vec<String> = Vec::with_capacity(ids.len());
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if seen.insert(id.clone()) {
            uniq.push(id.clone());
        }
    }
    let placeholders: Vec<String> = (1..=uniq.len()).map(|i| format!("${i}")).collect();
    let ph_list = placeholders.join(", ");
    (uniq, ph_list)
}

/// 把一组 id 字符串去重，并按 pk 列类型派发成 DataValue + 生成 `$1,$2,...` 占位符列表。
///
/// PK 类型派发很关键：cf_gl_account.id 是 BIGINT，client 传 snowflake 字符串 "1785..."
/// 需走 `to_dv_by_col` 按 `view.columns[pk].dataType` 转 `DataValue::Int`（→ PgInt 宽度
/// 自适应 INT2/INT4/INT8），否则裸绑 `DataValue::String` 在 PG prepare 阶段就报
/// "error serializing parameter 0"（OID 期望 bigint，实际收到 text）。
/// PK 为 String/Text/UUID 时 `to_dv_by_col` 保持 `DataValue::String`，不破坏现有行为。
fn build_unique_ids(
    view: &DictView,
    ids: &[String],
) -> Result<(Vec<String>, Vec<DataValue>, String)> {
    let (uniq, ph_list) = dedup_with_placeholders(ids);
    let params: Vec<DataValue> = uniq
        .iter()
        .map(|s| to_dv_by_col(view, &view.pk, &Value::String(s.clone())))
        .collect();
    Ok((uniq, params, ph_list))
}

/// 分级字典的 code 列名（供 full_path 拼接用）。
///
/// 正常字典 `code_field` 非空（如 gl_account.code）；极少数无 code 字段的字典
/// 回退到 pk（用 id 字符串当路径段，保证 full_path 非空兜底 NOT NULL）。
fn hierarchy_code_field(view: &DictView) -> &str {
    if view.code_field.is_empty() {
        &view.pk
    } else {
        &view.code_field
    }
}

/// 重算一组节点 id 及其整棵子树的 `level_no` / `full_path` / `is_leaf`。
///
/// 三字段统一在一次递归 CTE 中维护，避免分多步产生中间不一致状态。
///
/// # 语义
/// - **anchor**：每个受影响节点自身，读其父当前 `level_no` / `full_path` 推导自身新值。
///   根节点（无父）level_no=1，full_path=自身 code。
///   非根节点 level_no=父+1，full_path=父.full_path || '.' || 本行 code。
///   is_leaf 按 EXISTS (有子) 判定：有子→0，无子→1。
/// - **subtree**：从 anchor 用 `UNION ALL` 递归向下展开所有后代，逐层 level+1 / 路径拼 code /
///   按是否有子判叶。
/// - **UPDATE**：把 subtree 结果一次性写回三列。
///
/// # 幂等性
/// 重算只依赖当前父子拓扑（parent_id + code），不依赖历史值——重复执行结果一致，
/// 因此同一 id 多次进入 ids 集合也安全（`build_unique_ids` 先去重）。
///
/// # 环保护
/// PG 递归 CTE 默认无内置深度上限，但 CMX 字典实际层级 ≤ 10（会计科目最深 4-5 级），
/// 远低于触发栈深风险。如未来出现异常深的脏数据，应在导入路径前置环检测。
///
/// # 用法
/// 由 `save_apply` 末尾调用，传入所有受影响节点 id：被删行的旧父、新增行自身、
/// 改了 parent 的行自身。一次 SQL 重算所有受影响子树。
pub(crate) async fn recompute_hierarchy_subtree(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    parent_field: &str,
    ids: &[String],
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let (_uniq, params, ph_list) = build_unique_ids(view, ids)?;
    let code_col = hierarchy_code_field(view);
    // code/full_path/level_no/is_leaf/parent_field 任一缺失即不应走到此函数
    // （`hierarchy_parent_field` 已保证 parent_field + is_leaf 合法；code/level_no/full_path
    // 由分级字典 DDL 隐含，但此处仍作 valid_col 防御，缺则跳过该列的 UPDATE）。
    let has_level = valid_col(view, "level_no");
    let has_path = valid_col(view, "full_path");
    // 至少要能算出 is_leaf（hierarchy_parent_field 已保证），否则无意义。
    if !has_level && !has_path {
        // 无 level_no 也无 full_path，退化为仅 is_leaf（与旧行为等价）。
        let sql = format!(
            "UPDATE \"{tbl}\" SET \"is_leaf\" = CASE \
             WHEN EXISTS (SELECT 1 FROM \"{tbl}\" c WHERE c.\"{pf}\" = \"{tbl}\".\"{pk}\") THEN 0 \
             ELSE 1 END \
             WHERE \"{pk}\" IN ({ph})",
            tbl = view.table_name,
            pk = view.pk,
            pf = parent_field,
            ph = ph_list
        );
        mm.execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
            .await
            .map_err(|e| map_db_err(e, "recompute_hierarchy", view, None, &sql))?;
        return Ok(());
    }
    // 完整递归 CTE：anchor 推每个受影响节点自身，subtree 递归展开后代。
    // 用 COALESCE(p.level_no + 1, 1) 处理"父不存在 / 父 level_no 为 NULL"——回退为根级 1。
    // full_path 同理：父路径空或 NULL 时回退为自身 code（根级）。
    let set_clause = match (has_level, has_path) {
        (true, true) => "\"level_no\" = s.new_level, \"full_path\" = s.new_path, \"is_leaf\" = s.new_leaf",
        (true, false) => "\"level_no\" = s.new_level, \"is_leaf\" = s.new_leaf",
        (false, true) => "\"full_path\" = s.new_path, \"is_leaf\" = s.new_leaf",
        // 上面已 early return，逻辑上不可达；保留分支让 match 穷尽
        (false, false) => "\"is_leaf\" = s.new_leaf",
    };
    let sql = format!(
        "WITH RECURSIVE \
         anchor AS ( \
           SELECT t.\"{pk}\" AS id, \
                  COALESCE(p.\"level_no\" + 1, 1) AS new_level, \
                  CASE WHEN p.\"full_path\" IS NOT NULL AND p.\"full_path\" <> '' \
                       THEN p.\"full_path\" || '.' || t.\"{code}\" \
                       ELSE t.\"{code}\" END AS new_path, \
                  CASE WHEN EXISTS (SELECT 1 FROM \"{tbl}\" c WHERE c.\"{pf}\" = t.\"{pk}\") \
                       THEN 0 ELSE 1 END AS new_leaf \
           FROM \"{tbl}\" t LEFT JOIN \"{tbl}\" p ON p.\"{pk}\" = t.\"{pf}\" \
           WHERE t.\"{pk}\" IN ({ph}) \
         ), \
         subtree AS ( \
           SELECT id, new_level, new_path, new_leaf FROM anchor \
           UNION ALL \
           SELECT c.\"{pk}\", s.new_level + 1, \
                  CASE WHEN s.new_path IS NOT NULL AND s.new_path <> '' \
                       THEN s.new_path || '.' || c.\"{code}\" ELSE c.\"{code}\" END, \
                  CASE WHEN EXISTS (SELECT 1 FROM \"{tbl}\" gc WHERE gc.\"{pf}\" = c.\"{pk}\") THEN 0 ELSE 1 END \
           FROM \"{tbl}\" c JOIN subtree s ON c.\"{pf}\" = s.id \
         ) \
         UPDATE \"{tbl}\" t SET {set_clause} \
         FROM subtree s WHERE t.\"{pk}\" = s.id",
        pk = view.pk,
        pf = parent_field,
        code = code_col,
        tbl = view.table_name,
        ph = ph_list,
        set_clause = set_clause
    );
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
        .await
        .map_err(|e| map_db_err(e, "recompute_hierarchy", view, None, &sql))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_dct_model::DictColumn;
    use serde_json::json;

    /// 造一个最小化分级字典视图（含 pk/code/parent_id/is_leaf/level_no/full_path 列）。
    /// spec 用空 TableSpec（被测函数不读 spec）。
    fn hierarchy_view(pk: &str, code: &str) -> DictView {
        let mut cols = vec![
            DictColumn {
                name: pk.to_string(),
                caption: pk.to_string(),
                data_type: "BIGINT".to_string(),
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
                name: code.to_string(),
                caption: code.to_string(),
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
        ];
        // 分级字典的标准列（hierarchy_parent_field 要求 parent_field + is_leaf 合法）
        for name in ["parent_id", "is_leaf", "level_no", "full_path"] {
            cols.push(DictColumn {
                name: name.to_string(),
                caption: name.to_string(),
                data_type: if name == "parent_id" { "BIGINT".to_string() }
                    else if name == "full_path" { "VARCHAR".to_string() }
                    else { "TINYINT".to_string() },
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
            });
        }
        DictView {
            dict_code: "test_dct".to_string(),
            dict_name: "Test".to_string(),
            table_name: "cf_test".to_string(),
            id_field: pk.to_string(),
            code_field: code.to_string(),
            label_field: "name".to_string(),
            parent_field: Some("parent_id".to_string()),
            self_hierarchy: true,
            columns: cols,
            pk: pk.to_string(),
            spec: std::sync::Arc::new(cmx_biz::validation::TableSpec {
                table: "test_dct".to_string(),
                columns: std::collections::HashMap::new(),
                order: vec![],
            }),
            code_rule: None,
            unique_keys: vec![],
        }
    }

    #[test]
    fn dedup_with_placeholders_preserves_first_occurrence_order() {
        // 重复 id 只保留首次出现位置（保序），占位符从 $1 起连续编号。
        let ids = vec!["3".to_string(), "1".to_string(), "3".to_string(), "2".to_string()];
        let (uniq, ph) = dedup_with_placeholders(&ids);
        assert_eq!(uniq, vec!["3", "1", "2"]);
        assert_eq!(ph, "$1, $2, $3");
    }

    #[test]
    fn dedup_with_placeholders_empty_returns_empty() {
        let (uniq, ph) = dedup_with_placeholders(&[]);
        assert!(uniq.is_empty());
        assert!(ph.is_empty());
    }

    #[test]
    fn dedup_with_placeholders_single() {
        let (uniq, ph) = dedup_with_placeholders(&["42".to_string()]);
        assert_eq!(uniq, vec!["42"]);
        assert_eq!(ph, "$1");
    }

    #[test]
    fn build_unique_ids_dispatches_bigint_pk_to_int() {
        // cf_gl_account.id 是 BIGINT：字符串 id 应被 to_dv_by_col 转 DataValue::Int，
        // 否则 PG prepare 阶段报 OID 类型不匹配。
        let v = hierarchy_view("id", "code");
        let (uniq, params, ph) = build_unique_ids(&v, &["1001".to_string(), "1002".to_string()]).unwrap();
        assert_eq!(uniq, vec!["1001", "1002"]);
        assert_eq!(ph, "$1, $2");
        // BIGINT 列：字符串数字 → DataValue::Int
        assert!(matches!(params[0], DataValue::Int(_)), "BIGINT pk should dispatch to Int");
        assert!(matches!(params[1], DataValue::Int(_)));
    }

    #[test]
    fn hierarchy_code_field_returns_code_when_present() {
        let v = hierarchy_view("id", "code");
        assert_eq!(hierarchy_code_field(&v), "code");
    }

    #[test]
    fn hierarchy_code_field_falls_back_to_pk_when_code_empty() {
        // 无 code 字段的字典：full_path 路径段回退为 pk，保证非空兜底
        let mut v = hierarchy_view("id", "");
        v.code_field = String::new();
        assert_eq!(hierarchy_code_field(&v), "id");
    }

    #[test]
    fn hierarchy_parent_field_requires_hierarchy_plus_valid_cols() {
        // 完整分级字典 → Some(parent_id)
        let v = hierarchy_view("id", "code");
        assert_eq!(hierarchy_parent_field(&v).as_deref(), Some("parent_id"));

        // self_hierarchy=false → None（即使列齐全）
        let mut v2 = v.clone();
        v2.self_hierarchy = false;
        assert!(hierarchy_parent_field(&v2).is_none());

        // self_hierarchy=true 但缺 is_leaf 列 → None
        let mut v3 = v.clone();
        v3.columns.retain(|c| c.name != "is_leaf");
        assert!(hierarchy_parent_field(&v3).is_none());

        // self_hierarchy=true 但 parent_field 列被移除 → None
        let mut v4 = v.clone();
        v4.columns.retain(|c| c.name != "parent_id");
        assert!(hierarchy_parent_field(&v4).is_none());
    }

    #[test]
    fn non_empty_id_str_handles_null_empty_and_numbers() {
        assert_eq!(non_empty_id_str(&json!(null)), None);
        assert_eq!(non_empty_id_str(&json!("")), None);
        assert_eq!(non_empty_id_str(&json!("1001")), Some("1001".to_string()));
        assert_eq!(non_empty_id_str(&json!(1001)), Some("1001".to_string()));
        assert_eq!(non_empty_id_str(&json!(true)), Some("true".to_string()));
        assert_eq!(non_empty_id_str(&json!({})), None);
    }
}

/// 外部直写路径（如 MDM 激活器 `dict_upsert` / 按 id 直改）后的层级列补偿重算。
///
/// `save` 路径已在 `save_apply` 末尾统一维护 level_no/full_path/is_leaf；但经
/// `dict_upsert` 等单行直写的调用方绕过了该维护——`build_upsert_sql_dv` 的 backfill
/// 只能给出根级兜底值（level_no=1、full_path=code），子节点的真实层级/物化路径会失真
/// （parent_id 本身正确，父子拓扑无损）。本函数为这类调用方提供"写入后补偿重算"入口。
///
/// - `ids`：受影响节点（行自身 + 其父；父的 is_leaf 需随子节点增删翻转）
/// - 非分级字典（self_hierarchy=false / parent 列缺失）→ no-op，返回 `Ok(false)`
/// - 分级字典 → 在 `txn_id` 事务内执行 [`recompute_hierarchy_subtree`]，返回 `Ok(true)`
///
/// 幂等：重算只依赖当前父子拓扑（parent_id + code），不依赖历史值——重复执行结果一致。
pub async fn recompute_dict_hierarchy(
    q: &DctQuery,
    ids: &[i64],
    db_id: &str,
    txn_id: &str,
) -> Result<bool> {
    if ids.is_empty() {
        return Ok(false);
    }
    let view = resolve_dict(q, false).await?;
    let Some(pf) = hierarchy_parent_field(&view) else {
        return Ok(false);
    };
    let id_strs: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
    let mm = get_default_pg_db_manager();
    recompute_hierarchy_subtree(mm, db_id, txn_id, &view, &pf, &id_strs).await?;
    Ok(true)
}
