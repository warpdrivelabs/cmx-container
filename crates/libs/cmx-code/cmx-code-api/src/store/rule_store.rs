//! `cmx_code_rule` 表 CRUD + 规则选优。

use cmx_code_model::error::{CodeError, Result};
use cmx_code_model::spec::RuleSpec;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::get_default_pg_db_manager;
use cmx_utils::next_pk_id;

use crate::handlers::Dam;

/// DAM 全为空 → 不按模块过滤（返回全部规则）；任一非空 → 按非空维度过滤。
///
/// `start_idx` 是占位符起始编号（列表查询无前置参数 → 1；单条查询 rule_code 占 $1 → 2）。
///
/// 设计：规则管理页带 DAM（只看本模块规则）；字典/单据挂规则时的下拉不带 DAM（看全部，支持跨模块引用）。
fn dam_where(dam: &Dam, start_idx: usize) -> (String, Vec<DataValue>) {
    let mut conds = Vec::new();
    let mut params = Vec::new();
    let mut idx = start_idx;
    if !dam.domain_code.is_empty() {
        conds.push(format!("domain_code = ${idx}"));
        params.push(DataValue::String(dam.domain_code.clone()));
        idx += 1;
    }
    if !dam.application_code.is_empty() {
        conds.push(format!("application_code = ${idx}"));
        params.push(DataValue::String(dam.application_code.clone()));
        idx += 1;
    }
    if !dam.module_code.is_empty() {
        conds.push(format!("module_code = ${idx}"));
        params.push(DataValue::String(dam.module_code.clone()));
    }
    if conds.is_empty() {
        (String::new(), params)
    } else {
        (format!(" AND {}", conds.join(" AND ")), params)
    }
}

/// 创建规则。
pub async fn create_rule(rule: &RuleSpec, db_id: &str) -> Result<()> {
    let mm = get_default_pg_db_manager();
    let id = next_pk_id();
    let sql = r#"INSERT INTO cmx_code_rule
        (id, rule_code, rule_name, mode, org_scope, condition, segments, joiner, pattern,
         enable_gap, use_sequence, priority, is_active, domain_code, application_code, module_code)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"#;
    let params = vec![
        DataValue::Int(id as i64),
        DataValue::String(rule.rule_code.clone()),
        DataValue::String(rule.rule_name.clone()),
        DataValue::String(rule.mode.clone()),
        rule.org_scope.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        rule.condition.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        DataValue::Json(serde_json::to_string(&rule.segments).unwrap_or_else(|_| "[]".into())),
        DataValue::String(rule.joiner.clone()),
        rule.pattern.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        DataValue::Bool(rule.enable_gap),
        DataValue::Bool(rule.use_sequence),
        DataValue::Int(rule.priority as i64),
        DataValue::Bool(rule.is_active),
        DataValue::String(rule.domain_code.clone()),
        DataValue::String(rule.application_code.clone()),
        DataValue::String(rule.module_code.clone()),
    ];
    mm.execute_sql_with_datavalues(db_id, None, sql, params)
        .await
        .map_err(|e| CodeError::Database(format!("创建规则失败：{e}")))?;
    Ok(())
}

/// 按 rule_code 查询单条规则（DAM 非空时同时按模块过滤）。
pub async fn get_rule(rule_code: &str, db_id: &str, dam: &Dam) -> Result<RuleSpec> {
    let mm = get_default_pg_db_manager();
    let (dam_clause, dam_params) = dam_where(dam, 2);
    let sql = format!(
        r#"SELECT rule_code, rule_name, mode, org_scope, condition, segments, joiner,
        pattern, enable_gap, use_sequence, priority, is_active, domain_code, application_code, module_code
        FROM cmx_code_rule WHERE rule_code = $1 AND archived = 0{dam_clause}"#
    );
    let mut params = vec![DataValue::String(rule_code.into())];
    params.extend(dam_params);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "code_rule")
        .await
        .map_err(|e| CodeError::Database(format!("查询规则失败：{e}")))?;

    let row = ds.rows.first().ok_or_else(|| {
        CodeError::NoMatchingRule(format!("规则 {rule_code} 不存在"))
    })?;

    parse_rule_from_row(row)
}

/// 列出规则（仅 rule_code + rule_name，供下拉源用）。DAM 非空时按模块过滤。
pub async fn list_rules(db_id: &str, dam: &Dam) -> Result<Vec<serde_json::Value>> {
    let mm = get_default_pg_db_manager();
    let (dam_clause, dam_params) = dam_where(dam, 1);
    let sql = format!(
        r#"SELECT rule_code, rule_name FROM cmx_code_rule
        WHERE archived = 0 AND is_active = true{dam_clause} ORDER BY priority DESC, rule_code"#
    );
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, dam_params, "code_rules")
        .await
        .map_err(|e| CodeError::Database(format!("列出规则失败：{e}")))?;

    let mut result = Vec::new();
    for row in &ds.rows {
        let rule_code = dv_as_str(row.get(0)).unwrap_or("").to_string();
        let rule_name = dv_as_str(row.get(1)).unwrap_or("").to_string();
        result.push(serde_json::json!({ "ruleCode": rule_code, "ruleName": rule_name }));
    }
    Ok(result)
}

/// 更新规则（按 rule_code 定位）。DAM 列一并更新。
pub async fn update_rule(rule_code: &str, rule: &RuleSpec, db_id: &str) -> Result<()> {
    let mm = get_default_pg_db_manager();
    let sql = r#"UPDATE cmx_code_rule SET
        rule_name = $1, mode = $2, org_scope = $3, condition = $4, segments = $5,
        joiner = $6, pattern = $7, enable_gap = $8, use_sequence = $9, priority = $10,
        is_active = $11, domain_code = $12, application_code = $13, module_code = $14,
        update_time = NOW()
        WHERE rule_code = $15 AND archived = 0"#;
    let params = vec![
        DataValue::String(rule.rule_name.clone()),
        DataValue::String(rule.mode.clone()),
        rule.org_scope.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        rule.condition.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        DataValue::Json(serde_json::to_string(&rule.segments).unwrap_or_else(|_| "[]".into())),
        DataValue::String(rule.joiner.clone()),
        rule.pattern.clone().map(DataValue::String).unwrap_or(DataValue::Null),
        DataValue::Bool(rule.enable_gap),
        DataValue::Bool(rule.use_sequence),
        DataValue::Int(rule.priority as i64),
        DataValue::Bool(rule.is_active),
        DataValue::String(rule.domain_code.clone()),
        DataValue::String(rule.application_code.clone()),
        DataValue::String(rule.module_code.clone()),
        DataValue::String(rule_code.into()),
    ];
    mm.execute_sql_with_datavalues(db_id, None, sql, params)
        .await
        .map_err(|e| CodeError::Database(format!("更新规则失败：{e}")))?;
    Ok(())
}

/// 删除规则（软删除 archived=1）。DAM 非空时按模块限定（防误删他模块规则）。
pub async fn delete_rule(rule_code: &str, db_id: &str, dam: &Dam) -> Result<()> {
    let mm = get_default_pg_db_manager();
    let (dam_clause, dam_params) = dam_where(dam, 2);
    let sql = format!(
        r#"UPDATE cmx_code_rule SET archived = 1, is_active = false, update_time = NOW()
        WHERE rule_code = $1 AND archived = 0{dam_clause}"#
    );
    let mut params = vec![DataValue::String(rule_code.into())];
    params.extend(dam_params);
    mm.execute_sql_with_datavalues(db_id, None, &sql, params)
        .await
        .map_err(|e| CodeError::Database(format!("删除规则失败：{e}")))?;
    Ok(())
}

/// 从 DataSet row 解析 RuleSpec（列序与 get_rule/query_rules 的 SELECT 一致）。
fn parse_rule_from_row(row: &cmx_core::model::data::dataset::rds::Row) -> Result<RuleSpec> {
    let rule_code = dv_as_str(row.get(0)).unwrap_or("").to_string();
    let rule_name = dv_as_str(row.get(1)).unwrap_or("").to_string();
    let mode = dv_as_str(row.get(2)).unwrap_or("auto").to_string();
    let org_scope = dv_as_str(row.get(3)).map(|s| s.to_string());
    let condition = dv_as_str(row.get(4)).map(|s| s.to_string());
    let segments_json = dv_as_str(row.get(5)).unwrap_or("[]");
    let segments: Vec<_> = serde_json::from_str(segments_json).unwrap_or_default();
    let joiner = dv_as_str(row.get(6)).unwrap_or("").to_string();
    let pattern = dv_as_str(row.get(7)).map(|s| s.to_string());
    let enable_gap = dv_as_bool(row.get(8)).unwrap_or(false);
    let use_sequence = dv_as_bool(row.get(9)).unwrap_or(false);
    let priority = dv_as_i64(row.get(10)).unwrap_or(100) as i32;
    let is_active = dv_as_bool(row.get(11)).unwrap_or(true);
    let domain_code = dv_as_str(row.get(12)).unwrap_or("").to_string();
    let application_code = dv_as_str(row.get(13)).unwrap_or("").to_string();
    let module_code = dv_as_str(row.get(14)).unwrap_or("").to_string();

    Ok(RuleSpec {
        rule_code,
        rule_name,
        mode,
        org_scope,
        condition,
        segments,
        joiner,
        pattern,
        enable_gap,
        use_sequence,
        priority,
        is_active,
        domain_code,
        application_code,
        module_code,
    })
}

/// DataValue → &str 辅助。
fn dv_as_str(dv: Option<&DataValue>) -> Option<&str> {
    match dv {
        Some(DataValue::String(s)) => Some(s),
        Some(DataValue::Json(s)) => Some(s),
        _ => None,
    }
}

/// DataValue → bool 辅助。
fn dv_as_bool(dv: Option<&DataValue>) -> Option<bool> {
    match dv {
        Some(DataValue::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// DataValue → i64 辅助。
fn dv_as_i64(dv: Option<&DataValue>) -> Option<i64> {
    match dv {
        Some(DataValue::Int(n)) => Some(*n),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 规则选优（orgScope / condition / priority 三维）
// ═══════════════════════════════════════════════════════════════════════════════

/// 查询同 ruleCode 的全部候选规则（active + archived=0），返回 Vec 供 select_best 选优。
///
/// DAM 非空时按模块过滤（铸号只选本模块规则）；DAM 全空时返回全部（兼容旧调用方）。
pub async fn query_rules(rule_code: &str, db_id: &str, dam: &Dam) -> Result<Vec<RuleSpec>> {
    let mm = get_default_pg_db_manager();
    let (dam_clause, dam_params) = dam_where(dam, 2);
    let sql = format!(
        r#"SELECT rule_code, rule_name, mode, org_scope, condition, segments, joiner,
        pattern, enable_gap, use_sequence, priority, is_active, domain_code, application_code, module_code
        FROM cmx_code_rule
        WHERE rule_code = $1 AND archived = 0 AND is_active = true{dam_clause}"#
    );
    let mut params = vec![DataValue::String(rule_code.into())];
    params.extend(dam_params);
    let ds = mm
        .query_sql_with_datavalues(db_id, None, &sql, params, "code_rules")
        .await
        .map_err(|e| CodeError::Database(format!("查询规则候选失败：{e}")))?;

    let mut rules = Vec::new();
    for row in &ds.rows {
        if let Ok(rule) = parse_rule_from_row(row) {
            rules.push(rule);
        }
    }
    Ok(rules)
}

/// 规则选优：orgScope 命中 + condition 匹配 + priority 取大（方案 §3.4.3）。
///
/// priority 相同时取候选列表中**最后一个**（Rust `max_by_key` 语义，建议用不同 priority 显式排序避免歧义）。
/// - `org_code`：当前组织码（None 视为全局，匹配 org_scope=None 的规则）
/// - `attrs`：行属性（condition 表达式求值用）
pub fn select_best<'a>(
    candidates: &'a [RuleSpec],
    org_code: Option<&str>,
    attrs: &serde_json::Value,
) -> Option<&'a RuleSpec> {
    candidates
        .iter()
        .filter(|r| org_matches(r, org_code))
        .filter(|r| cond_matches(r, attrs))
        .max_by_key(|r| r.priority)
}

/// orgScope 匹配（方案 §3.4.1）。
///
/// - `None` / 空 → 全局命中
/// - `"CODE"` → 精确匹配
/// - `"A,B,C"` → 逗号分隔多组织，精确匹配其一
fn org_matches(rule: &RuleSpec, org_code: Option<&str>) -> bool {
    let Some(scope) = &rule.org_scope else {
        return true;
    };
    let scope = scope.trim();
    if scope.is_empty() {
        return true;
    }
    let Some(code) = org_code else {
        return false;
    };
    // 逗号分隔多组织匹配（方案 §3.4.1 数组多组织的字符串编码形式）
    scope.split(',').any(|s| s.trim() == code)
}

/// condition 匹配（方案 §3.4.2）。
///
/// 优先尝试 JSON 算子表达式（`{"eq":[field,value]}` 等），解析失败回退字符串表达式
/// （`field==value` / `!=`，向后兼容）。无 condition 恒 true。
///
/// JSON 算子白名单：eq / ne / in / exists / and / or / not。未知算子判 **false**（严格）。
fn cond_matches(rule: &RuleSpec, attrs: &serde_json::Value) -> bool {
    let Some(cond) = &rule.condition else {
        return true;
    };
    let cond = cond.trim();
    if cond.is_empty() {
        return true;
    }

    // ① 尝试 JSON 算子表达式（方案 §3.4.2 目标形态）
    if cond.starts_with('{') {
        if let Ok(expr) = serde_json::from_str::<serde_json::Value>(cond) {
            if let Some(result) = eval_json_condition(&expr, attrs) {
                return result;
            }
            // JSON 解析成功但算子未知 → 严格判 false（方案 §3.4.2）
            tracing::warn!(
                target: "cmx_code::cond",
                condition = %cond,
                "condition JSON 算子不识别，判 false（严格模式）"
            );
            return false;
        }
        // JSON 解析失败 → 落到字符串兼容分支
    }

    // ② 字符串表达式兼容（向后兼容，方案 §12.1 示例格式）
    let expr = cond.strip_prefix("attrs.").unwrap_or(cond);
    if let Some(idx) = expr.find("==") {
        let field = expr[..idx].trim();
        let expected = expr[idx + 2..].trim().trim_matches('\'').trim_matches('"');
        let actual = attrs.get(field).and_then(|v| v.as_str()).unwrap_or("");
        return actual == expected;
    }
    if let Some(idx) = expr.find("!=") {
        let field = expr[..idx].trim();
        let expected = expr[idx + 2..].trim().trim_matches('\'').trim_matches('"');
        let actual = attrs.get(field).and_then(|v| v.as_str()).unwrap_or("");
        return actual != expected;
    }

    // 不识别 → 严格判 false（与 JSON 算子一致，修复附录 C.2.8 的默认 true 安全隐患）
    tracing::warn!(
        target: "cmx_code::cond",
        condition = %cond,
        "condition 表达式不识别，判 false（严格模式）"
    );
    false
}

/// JSON 算子条件求值（方案 §3.4.2 白名单）。
///
/// 返回 `Some(bool)` 表示算子已知并已求值；`None` 表示算子未知。
fn eval_json_condition(expr: &serde_json::Value, attrs: &serde_json::Value) -> Option<bool> {
    let obj = expr.as_object()?;
    // 单算子对象：取第一个 key
    let (op, args) = obj.iter().next()?;

    match op.as_str() {
        "eq" => {
            // {"eq": [field, value]} → attrs[field] == value
            let arr = args.as_array()?;
            let field = arr.first()?.as_str()?;
            let expected = arr.get(1)?;
            let actual = attrs.get(field);
            Some(actual == Some(expected))
        }
        "ne" => {
            // {"ne": [field, value]} → attrs[field] != value
            let arr = args.as_array()?;
            let field = arr.first()?.as_str()?;
            let expected = arr.get(1)?;
            let actual = attrs.get(field);
            Some(actual != Some(expected))
        }
        "in" => {
            // {"in": [field, [v1,v2,...]]} → attrs[field] ∈ 数组
            let arr = args.as_array()?;
            let field = arr.first()?.as_str()?;
            let values = arr.get(1)?.as_array()?;
            let actual = attrs.get(field)?;
            Some(values.contains(actual))
        }
        "exists" => {
            // {"exists": field} → attrs 有该字段
            let field = args.as_str()?;
            Some(attrs.get(field).is_some())
        }
        "and" => {
            // {"and": [expr, expr, ...]} → 全部为 true
            let arr = args.as_array()?;
            let mut all_true = true;
            for sub in arr {
                match eval_json_condition(sub, attrs) {
                    Some(true) => {}
                    Some(false) => {
                        all_true = false;
                        break;
                    }
                    None => {
                        all_true = false;
                        break;
                    }
                }
            }
            Some(all_true)
        }
        "or" => {
            // {"or": [expr, expr, ...]} → 任一为 true
            let arr = args.as_array()?;
            let mut any_true = false;
            for sub in arr {
                if let Some(true) = eval_json_condition(sub, attrs) {
                    any_true = true;
                    break;
                }
            }
            Some(any_true)
        }
        "not" => {
            // {"not": expr} → 取反
            Some(!eval_json_condition(args, attrs).unwrap_or(false))
        }
        _ => None, // 未知算子
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_rule(code: &str, org_scope: Option<&str>, condition: Option<&str>, priority: i32) -> RuleSpec {
        RuleSpec {
            rule_code: code.into(),
            rule_name: code.into(),
            mode: "auto".into(),
            org_scope: org_scope.map(String::from),
            condition: condition.map(String::from),
            segments: vec![],
            joiner: String::new(),
            pattern: None,
            enable_gap: false,
            use_sequence: false,
            priority,
            is_active: true,
            domain_code: String::new(),
            application_code: String::new(),
            module_code: String::new(),
        }
    }

    #[test]
    fn test_org_matches_global() {
        let r = make_rule("r", None, None, 100);
        assert!(org_matches(&r, None));
        assert!(org_matches(&r, Some("HQ")));
    }

    #[test]
    fn test_org_matches_scoped() {
        let r = make_rule("r", Some("HQ"), None, 100);
        assert!(org_matches(&r, Some("HQ")));
        assert!(!org_matches(&r, Some("BR1")));
        assert!(!org_matches(&r, None));
    }

    #[test]
    fn test_cond_matches_eq() {
        let r = make_rule("r", None, Some("attrs.bp_role=='supplier'"), 100);
        let attrs = serde_json::json!({"bp_role": "supplier"});
        assert!(cond_matches(&r, &attrs));
        let attrs2 = serde_json::json!({"bp_role": "customer"});
        assert!(!cond_matches(&r, &attrs2));
    }

    #[test]
    fn test_select_best_priority() {
        let candidates = vec![
            make_rule("r", None, None, 50),
            make_rule("r", None, None, 200),
            make_rule("r", None, None, 100),
        ];
        let best = select_best(&candidates, None, &serde_json::json!({}));
        assert_eq!(best.unwrap().priority, 200);
    }

    // ── orgScope 多组织（逗号分隔，方案 §3.4.1）──
    #[test]
    fn test_org_matches_multi_scope() {
        let r = make_rule("r", Some("EAST,WEST"), None, 100);
        assert!(org_matches(&r, Some("EAST")));
        assert!(org_matches(&r, Some("WEST")));
        assert!(!org_matches(&r, Some("HQ")));
    }

    // ── condition JSON 算子（方案 §3.4.2）──
    #[test]
    fn test_cond_json_eq() {
        let r = make_rule("r", None, Some(r#"{"eq":["bp_role","supplier"]}"#), 100);
        assert!(cond_matches(&r, &json!({"bp_role": "supplier"})));
        assert!(!cond_matches(&r, &json!({"bp_role": "customer"})));
    }

    #[test]
    fn test_cond_json_in() {
        let r = make_rule("r", None, Some(r#"{"in":["bp_role",["supplier","vendor"]]}"#), 100);
        assert!(cond_matches(&r, &json!({"bp_role": "supplier"})));
        assert!(cond_matches(&r, &json!({"bp_role": "vendor"})));
        assert!(!cond_matches(&r, &json!({"bp_role": "customer"})));
    }

    #[test]
    fn test_cond_json_and() {
        let r = make_rule(
            "r",
            None,
            Some(r#"{"and":[{"eq":["t","SA"]},{"exists":"ref_no"}]}"#),
            100,
        );
        assert!(cond_matches(&r, &json!({"t":"SA","ref_no":"R1"})));
        assert!(!cond_matches(&r, &json!({"t":"SA"}))); // 缺 ref_no
        assert!(!cond_matches(&r, &json!({"t":"AR","ref_no":"R1"}))); // t 不对
    }

    #[test]
    fn test_cond_json_unknown_op_strict_false() {
        // 未知算子 → 严格判 false（非默认 true，修复 C.2.8 安全隐患）
        let r = make_rule("r", None, Some(r#"{"frob":["x","y"]}"#), 100);
        assert!(!cond_matches(&r, &json!({"x": "y"})));
    }

    #[test]
    fn test_cond_json_or() {
        let r = make_rule("r", None, Some(r#"{"or":[{"eq":["t","SA"]},{"eq":["t","AR"]}]}"#), 100);
        assert!(cond_matches(&r, &json!({"t":"SA"})));
        assert!(cond_matches(&r, &json!({"t":"AR"})));
        assert!(!cond_matches(&r, &json!({"t":"GL"})));
    }

    #[test]
    fn test_cond_json_not() {
        let r = make_rule("r", None, Some(r#"{"not":{"eq":["t","SA"]}}"#), 100);
        assert!(!cond_matches(&r, &json!({"t":"SA"})));
        assert!(cond_matches(&r, &json!({"t":"AR"})));
    }

    #[test]
    fn test_cond_json_exists() {
        let r = make_rule("r", None, Some(r#"{"exists":"ref_no"}"#), 100);
        assert!(cond_matches(&r, &json!({"ref_no":"R1"})));
        assert!(!cond_matches(&r, &json!({"other":"x"})));
    }

    #[test]
    fn test_cond_json_ne() {
        let r = make_rule("r", None, Some(r#"{"ne":["t","SA"]}"#), 100);
        assert!(!cond_matches(&r, &json!({"t":"SA"})));
        assert!(cond_matches(&r, &json!({"t":"AR"})));
    }
}
