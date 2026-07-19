//! rule — 单据校验规则执行（方案 §14.2 后端二次校验 / §15 T1，落地 Phase 9）
//!
//! 读单据定义的 `validationRules`（`{ code, name, level, expr, message, severity? }`），
//! 用后端 formula 求值器（与前端 `formula-eval` 语义对齐）在行 scope 上逐条求值。
//!
//! 定位：§14.2「前端校验不可信，后端保存前二次把关」。
//!   - expr 可求值且为 false → 违规（收集 message）
//!   - expr 不可求值（描述性文本、含前端未实现的跨层函数）→ 跳过（降级，不误判）
//!   - severity=error 阻断保存；warn 仅提示
//!
//! 说明：真实 validationRules 里有跨层聚合语法（`sum(account_lines.local_dr)`、`has(...)`），
//! 本 T1 求值器覆盖行内标量校验；跨层聚合校验需先在 scope 里预铺平（调用方职责）或走 §15 T3。

use serde_json::Value;

use super::formula::{Scope, eval_formula};

/// 一条校验违规。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Violation {
    pub code: String,
    pub message: String,
    pub severity: String, // "error" | "warn"
    pub level: Option<String>,
}

/// 校验结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidateResult {
    pub ok: bool,
    pub violations: Vec<Violation>,
    /// 因表达式不可求值而跳过的规则数（诊断用）。
    pub skipped: usize,
}

impl ValidateResult {
    /// 是否有 error 级违规（阻断保存）。
    pub fn has_error(&self) -> bool {
        self.violations.iter().any(|v| v.severity == "error")
    }
}

/// 执行一组校验规则。
///
/// - `rules`：单据定义的 `validationRules` 数组
/// - `scope`：行字段铺平后的求值上下文
pub fn validate(rules: &[Value], scope: &Scope) -> ValidateResult {
    let mut violations = Vec::new();
    let mut skipped = 0usize;

    for rule in rules {
        let expr = match rule.get("expr").and_then(|v| v.as_str()) {
            Some(e) if !e.trim().is_empty() => e,
            _ => {
                skipped += 1;
                continue;
            }
        };
        match eval_formula(expr, scope) {
            Ok(v) => {
                if !v.as_bool() {
                    violations.push(Violation {
                        code: rule
                            .get("code")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        message: rule
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("校验未通过")
                            .to_string(),
                        severity: rule
                            .get("severity")
                            .and_then(|v| v.as_str())
                            .unwrap_or("error")
                            .to_string(),
                        level: rule.get("level").and_then(|v| v.as_str()).map(String::from),
                    });
                }
            }
            // 不可求值（描述性文本 / 跨层聚合未铺平）→ 跳过，不误判
            Err(_) => skipped += 1,
        }
    }

    ValidateResult {
        ok: !violations.iter().any(|v| v.severity == "error"),
        violations,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{FValue, scope_from_json};
    use serde_json::json;

    #[test]
    fn balance_rule_passes_and_fails() {
        let rules = vec![json!({
            "code": "balance", "expr": "total_dr == total_cr",
            "message": "借贷不平", "severity": "error"
        })];

        // 平衡 → ok
        let s = scope_from_json(&json!({ "total_dr": 100, "total_cr": 100 }));
        let r = validate(&rules, &s);
        assert!(r.ok);
        assert_eq!(r.violations.len(), 0);

        // 不平 → error 违规
        let s2 = scope_from_json(&json!({ "total_dr": 100, "total_cr": 90 }));
        let r2 = validate(&rules, &s2);
        assert!(!r2.ok);
        assert!(r2.has_error());
        assert_eq!(r2.violations[0].message, "借贷不平");
    }

    #[test]
    fn descriptive_expr_is_skipped_not_failed() {
        // 真实定义里有描述性文本 expr，不可求值 → 跳过（不误判为违规）
        let rules = vec![json!({
            "code": "aux", "expr": "dim_type/dim_value_id 与对应专列字段不得冲突",
            "message": "维度冲突"
        })];
        let s = Scope::new();
        let r = validate(&rules, &s);
        assert!(r.ok);
        assert_eq!(r.violations.len(), 0);
        assert_eq!(r.skipped, 1);
    }

    #[test]
    fn warn_severity_does_not_block() {
        let rules = vec![json!({
            "code": "w", "expr": "amount > 0", "message": "金额应为正", "severity": "warn"
        })];
        let s = scope_from_json(&json!({ "amount": -5 }));
        let r = validate(&rules, &s);
        // warn 违规存在但不阻断
        assert_eq!(r.violations.len(), 1);
        assert!(!r.has_error());
        assert!(r.ok);
    }

    #[test]
    fn empty_expr_skipped() {
        let rules = vec![json!({ "code": "x", "expr": "" })];
        let r = validate(&rules, &scope_from_json(&json!({})));
        assert_eq!(r.skipped, 1);
        let _ = FValue::Null;
    }
}
