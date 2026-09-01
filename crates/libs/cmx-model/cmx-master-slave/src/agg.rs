//! 层间汇总引擎 —— 即统一建模方案点名的 **cmx-agg**。
//!
//! 语义与前端 `cmx-master-slave.js` 的 `AGG_FUNCS` + `_resolveSourcesForTarget` + `_cascade`
//! **逐字对齐**，作为服务端权威：saver 落库前对变更树 [`rollup`]，父层承接字段随子层一并 UPSERT。
//!
//! 两件事：
//! 1. [`topo_sort`]：把 `(from→to)` 规则按依赖建 DAG 拓扑排序，成环即拒（对标前端 16 层安全阀）。
//! 2. [`rollup`]：按拓扑序逐条规则，对每个 target 按三种作用域收集 source → 聚合 → 回写。

use crate::error::{MsError, Result};
use crate::schema::{AggFn, AggRule, Scope};
use crate::tree::{MsTree, NodeId};
use crate::value::{as_f64, to_value};
use std::collections::HashSet;

/// 对整棵树按规则做写时上卷。原地回写各 target 的 `to_field`。
pub fn rollup(tree: &mut MsTree, rules: &[AggRule]) -> Result<()> {
    let order = topo_sort(rules)?;
    for &ri in &order {
        execute_rule(tree, &rules[ri]);
    }
    Ok(())
}

/// 读时上卷：不改树，返回每条规则 `(target 业务键, 值)`。
pub fn rollup_read(tree: &MsTree, rules: &[AggRule]) -> Result<Vec<ReadResult>> {
    let order = topo_sort(rules)?;
    let mut out = Vec::new();
    for &ri in &order {
        let rule = &rules[ri];
        let mut per_target = Vec::new();
        for target in resolve_targets(tree, rule) {
            let sources = resolve_sources(tree, rule, target);
            per_target.push((tree.node(target).key.clone(), apply_agg(tree, rule, &sources)));
        }
        out.push(ReadResult {
            to: rule.to.clone(),
            to_field: rule.to_field.clone(),
            values: per_target,
        });
    }
    Ok(out)
}

/// 读时上卷的一条规则结果。
#[derive(Debug, Clone)]
pub struct ReadResult {
    pub to: String,
    pub to_field: String,
    pub values: Vec<(String, serde_json::Value)>,
}

fn execute_rule(tree: &mut MsTree, rule: &AggRule) {
    for target in resolve_targets(tree, rule) {
        let sources = resolve_sources(tree, rule, target);
        let value = apply_agg(tree, rule, &sources);
        tree.node_mut(target).set(&rule.to_field, value);
    }
}

fn resolve_targets(tree: &MsTree, rule: &AggRule) -> Vec<NodeId> {
    tree.collect_path(&rule.to)
}

/// 对一个 target 行 t 计算 source 集合（对齐前端 `_resolveSourcesForTarget`）。
fn resolve_sources(tree: &MsTree, rule: &AggRule, target: NodeId) -> Vec<NodeId> {
    if rule.scope == Scope::All {
        return tree.collect_path(&rule.from);
    }
    let from_segs: Vec<&str> = rule.from.split('.').collect();
    let to_segs: Vec<&str> = rule.to.split('.').collect();

    // case A: to 是 from 的祖先（或相等）
    if from_segs.len() >= to_segs.len() && to_segs.iter().enumerate().all(|(i, s)| from_segs[i] == *s)
    {
        let rest = &from_segs[to_segs.len()..];
        if rest.is_empty() {
            return vec![target];
        }
        return tree.descend_from(target, rest);
    }

    // case B: 最近公共 list 祖先
    let mut common = 0usize;
    while common < from_segs.len() && common < to_segs.len() && from_segs[common] == to_segs[common] {
        common += 1;
    }
    if common == 0 {
        return tree.collect_path(&rule.from);
    }
    let ancestor = match ancestor_at_depth(tree, target, common) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let from_rest = &from_segs[common..];
    if from_rest.is_empty() {
        return vec![ancestor];
    }
    tree.descend_from(ancestor, from_rest)
}

/// 从节点沿 parent 上溯到深度 `depth`（段数）的祖先（对齐前端 `_ancestorRowOf`）。
fn ancestor_at_depth(tree: &MsTree, node: NodeId, depth: usize) -> Option<NodeId> {
    let mut cur = node;
    loop {
        let n = tree.node(cur).path.split('.').count();
        if n == depth {
            return Some(cur);
        }
        if n < depth {
            return None;
        }
        cur = tree.node(cur).parent?;
    }
}

fn apply_agg(tree: &MsTree, rule: &AggRule, sources: &[NodeId]) -> serde_json::Value {
    if rule.agg == AggFn::Count {
        return to_value(sources.len() as f64);
    }
    let field = match &rule.field {
        Some(f) => f,
        None => return to_value(0.0),
    };
    let vals: Vec<f64> = sources
        .iter()
        .map(|&id| tree.node(id).get(field).map(as_f64).unwrap_or(0.0))
        .collect();
    let result = if vals.is_empty() {
        0.0
    } else {
        match rule.agg {
            AggFn::Sum => vals.iter().sum(),
            AggFn::Avg => vals.iter().sum::<f64>() / vals.len() as f64,
            AggFn::Min => vals.iter().cloned().fold(f64::INFINITY, f64::min),
            AggFn::Max => vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            AggFn::Count => unreachable!(),
        }
    };
    to_value(result)
}

/// 把汇总规则按 `(from→to)` 依赖拓扑排序。返回规则下标执行序；成环 → [`MsError::AggCycle`]。
pub fn topo_sort(rules: &[AggRule]) -> Result<Vec<usize>> {
    let n = rules.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for i in 0..n {
        for j in 0..n {
            if i != j && produces_input_for(&rules[i], &rules[j]) {
                adj[i].push(j);
                indeg[j] += 1;
            }
        }
    }
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    queue.sort_unstable();
    let mut order = Vec::with_capacity(n);
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        order.push(u);
        let mut newly = Vec::new();
        for &v in &adj[u] {
            indeg[v] -= 1;
            if indeg[v] == 0 {
                newly.push(v);
            }
        }
        newly.sort_unstable();
        queue.extend(newly);
    }
    if order.len() != n {
        let done: HashSet<usize> = order.iter().copied().collect();
        let stuck: Vec<String> = (0..n)
            .filter(|i| !done.contains(i))
            .map(|i| format!("{}→{}", rules[i].from, rules[i].to))
            .collect();
        return Err(MsError::AggCycle(stuck.join(", ")));
    }
    Ok(order)
}

/// rule `a` 是否为 rule `b` 生产输入（rollup 依赖边 a→b，要求 a 先于 b）。
/// 依赖成立当且仅当 a **写到的层与字段** 正是 b **收集的层与字段**：
/// `a.to == b.from` 且 `a.to_field == b.field`。这样两条串联的上卷
/// （子明细→明细 写 X，明细→主表 读 X）才连边；而同一层转换的多字段并列规则
/// （aux→acc 的 dr/cr/local… 各字段）彼此独立、不成环。
///
/// 注意：**必须用层相等而非前缀**。前缀会把「父层写」误判为「子层读」的输入
/// （acc 是 aux 的路径前缀），导致同一批逐层上卷规则被误判成环。
fn produces_input_for(a: &AggRule, b: &AggRule) -> bool {
    if a.to != b.from {
        return false;
    }
    match (&b.field, b.agg) {
        (_, AggFn::Count) => true,
        (Some(bf), _) => bf == &a.to_field,
        (None, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::MsTree;
    use serde_json::{json, Map, Value};

    fn row(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }
    fn rule(from: &str, to: &str, field: Option<&str>, tf: &str, agg: AggFn) -> AggRule {
        AggRule {
            from: from.into(),
            to: to.into(),
            field: field.map(|s| s.into()),
            to_field: tf.into(),
            agg,
            scope: Scope::Siblings,
        }
    }
    fn voucher() -> MsTree {
        let mut t = MsTree::new();
        let h = t.add("h1", "head", row(json!({"id":"h1"})));
        t.add_root(h);
        let i1 = t.add("i1", "head.items", row(json!({"id":"i1","debit":100})));
        let i2 = t.add("i2", "head.items", row(json!({"id":"i2","debit":50})));
        t.link(h, i1);
        t.link(h, i2);
        let x1 = t.add("t1", "head.items.taxes", row(json!({"id":"t1","tax":6})));
        let x2 = t.add("t2", "head.items.taxes", row(json!({"id":"t2","tax":4})));
        t.link(i1, x1);
        t.link(i1, x2);
        t
    }

    #[test]
    fn sum_child_to_parent() {
        let mut t = voucher();
        rollup(&mut t, &[rule("head.items", "head", Some("debit"), "totalDebit", AggFn::Sum)]).unwrap();
        assert_eq!(t.node(t.collect_path("head")[0]).get("totalDebit"), Some(&json!(150)));
    }

    #[test]
    fn case_a_limits_to_subtree() {
        let mut t = voucher();
        rollup(&mut t, &[rule("head.items.taxes", "head.items", Some("tax"), "totalTax", AggFn::Sum)]).unwrap();
        let items = t.collect_path("head.items");
        let i1 = items.iter().find(|&&n| t.node(n).key == "i1").unwrap();
        let i2 = items.iter().find(|&&n| t.node(n).key == "i2").unwrap();
        assert_eq!(t.node(*i1).get("totalTax"), Some(&json!(10)));
        assert_eq!(t.node(*i2).get("totalTax"), Some(&json!(0)));
    }

    #[test]
    fn chained_via_toposort() {
        let mut t = voucher();
        let rules = vec![
            rule("head.items", "head", Some("totalTax"), "grand", AggFn::Sum),
            rule("head.items.taxes", "head.items", Some("tax"), "totalTax", AggFn::Sum),
        ];
        rollup(&mut t, &rules).unwrap();
        assert_eq!(t.node(t.collect_path("head")[0]).get("grand"), Some(&json!(10)));
    }

    #[test]
    fn count_avg_min_max() {
        let mut t = voucher();
        let rules = vec![
            rule("head.items", "head", None, "cnt", AggFn::Count),
            rule("head.items", "head", Some("debit"), "avg", AggFn::Avg),
            rule("head.items", "head", Some("debit"), "mn", AggFn::Min),
            rule("head.items", "head", Some("debit"), "mx", AggFn::Max),
        ];
        rollup(&mut t, &rules).unwrap();
        let h = t.collect_path("head")[0];
        assert_eq!(t.node(h).get("cnt"), Some(&json!(2)));
        assert_eq!(t.node(h).get("avg"), Some(&json!(75)));
        assert_eq!(t.node(h).get("mn"), Some(&json!(50)));
        assert_eq!(t.node(h).get("mx"), Some(&json!(100)));
    }

    #[test]
    fn cycle_rejected() {
        let rules = vec![
            rule("head.items", "head.other", Some("x"), "y", AggFn::Sum),
            rule("head.other", "head.items", Some("y"), "x", AggFn::Sum),
        ];
        assert!(matches!(topo_sort(&rules), Err(MsError::AggCycle(_))));
    }
}
