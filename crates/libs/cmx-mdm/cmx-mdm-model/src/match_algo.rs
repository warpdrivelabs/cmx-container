//! 匹配算法（M3，纯逻辑 DB-free，可单测）：分块 → 比较 → 双阈值裁决。
//!
//! - 分块：多簇键按优先级；NULL/空簇键不进共享块（防 NULL 巨簇）；块上限 500（截断+warn）。
//! - 比较：字段级加权得分 0-100；中间量 u32 累加防溢出；Σweight=0 / max_len=0 显式设防。
//! - 裁决：≥95 AutoMerge / 80–94 Review / <80 NoMatch。

use serde_json::{Map, Value};

/// 候选记录（cm_* published 行的精简投影）。
#[derive(Debug, Clone)]
pub struct MatchRecord {
    pub id: i64,
    pub fields: Map<String, Value>,
}

/// 比较字段种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// 全等得分（相等满分，不等 0；两侧均空 0 分——空值不构成匹配证据）
    Exact,
    /// 归一化编辑距离得分（两侧均空视为相等满分）
    EditDistance,
}

/// 比较字段配置（weight 为权重，u32 防溢出）。
#[derive(Debug, Clone)]
pub struct MatchFieldSpec {
    pub field: String,
    pub weight: u32,
    pub kind: FieldKind,
}

/// 双阈值裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// ≥95 自动合并
    AutoMerge,
    /// 80–94 人工评审
    Review,
    /// <80 不匹配
    NoMatch,
}

/// 匹配候选。
#[derive(Debug, Clone)]
pub struct MatchCandidate {
    pub record_id: i64,
    pub score: u8,
    pub decision: Decision,
}

/// 块上限护栏（防 N×N 爆炸）。
pub const BLOCK_CAP: usize = 500;

/// 分块：按簇键优先级（cluster_keys 序）把记录分块。
///
/// 护栏：
/// - 簇键 NULL/空 的记录**不进共享块**（各自独立块，防 NULL 巨簇）；
/// - 块超 [`BLOCK_CAP`] 截断并 `tracing::warn`（M3 记日志，M4 降级次簇键再分块）。
///
/// 分块策略：取第一个非空簇键值作块键（高优先级簇键主导）。
pub fn blocking<'a>(
    records: &'a [MatchRecord],
    cluster_keys: &[&str],
) -> Vec<Vec<&'a MatchRecord>> {
    use std::collections::BTreeMap;
    let mut blocks: BTreeMap<String, Vec<&'a MatchRecord>> = BTreeMap::new();

    for r in records {
        // 取第一个非空簇键（名+值）作块键；全空 → 孤儿，不进共享块（防 NULL 巨簇）
        if let Some((kname, kval)) = cluster_keys.iter().find_map(|k| {
            field_str(&r.fields, k)
                .filter(|s| !s.is_empty())
                .map(|v| (*k, v))
        }) {
            blocks.entry(format!("{kname}:{kval}")).or_default().push(r);
        }
    }

    let mut out = Vec::new();
    for (key, mut blk) in blocks {
        if blk.len() > BLOCK_CAP {
            tracing::warn!(
                target: "cmx_mdm::match", block = %key, size = blk.len(), cap = BLOCK_CAP,
                "块超上限截断（被截记录本块内不参与比较，M4 降级次簇键）"
            );
            blk.truncate(BLOCK_CAP);
        }
        // 单元素块无比较意义，不输出
        if blk.len() > 1 {
            out.push(blk);
        }
    }
    out
}

/// 比较：target vs other 加权得分 0-100。
///
/// 公式（审查重要-3 + 单测修正）：`score = Σ(field_score × weight) / Σweight`，中间量 u32。
/// - **两侧均空的字段跳过**（空值不构成匹配证据，不计入分子分母）
/// - 跳过后 Σweight == 0 → 0（NoMatch，不除零）
/// - Exact：相等=100，不等=0
/// - EditDistance：`100 × (1 - dist/max_len)`，max_len=0 显式分支
pub fn compare(target: &MatchRecord, other: &MatchRecord, specs: &[MatchFieldSpec]) -> u8 {
    let mut acc: u32 = 0;
    let mut total_w: u32 = 0;
    for s in specs {
        let a = field_str(&target.fields, &s.field);
        let b = field_str(&other.fields, &s.field);
        let a_empty = a.as_deref().map(|s| s.is_empty()).unwrap_or(true);
        let b_empty = b.as_deref().map(|s| s.is_empty()).unwrap_or(true);
        // 两侧均空 → 跳过（不参与评分）
        if a_empty && b_empty {
            continue;
        }
        let field_score: u32 = match s.kind {
            FieldKind::Exact => match (a.as_deref(), b.as_deref()) {
                (Some(x), Some(y)) if !x.is_empty() && x == y => 100,
                _ => 0,
            },
            FieldKind::EditDistance => match (a.as_deref(), b.as_deref()) {
                (Some(x), Some(y)) => {
                    let max_len = x.chars().count().max(y.chars().count()) as u32;
                    if max_len == 0 {
                        100
                    } else {
                        let dist = levenshtein(x, y).min(max_len);
                        (100 * (max_len - dist) / max_len).min(100)
                    }
                }
                _ => 0,
            },
        };
        acc += field_score * s.weight;
        total_w += s.weight;
    }
    if total_w == 0 {
        return 0;
    }
    (acc / total_w) as u8
}

/// 双阈值裁决。
pub fn decide(score: u8) -> Decision {
    if score >= 95 {
        Decision::AutoMerge
    } else if score >= 80 {
        Decision::Review
    } else {
        Decision::NoMatch
    }
}

/// 查重主流程：target vs all（先分块再块内比较）。排除自身。
///
/// 返回候选（score≥80 的 Review/AutoMerge，NoMatch 不返回）。
pub fn find_candidates(
    target: &MatchRecord,
    all: &[MatchRecord],
    specs: &[MatchFieldSpec],
    cluster_keys: &[&str],
) -> Vec<MatchCandidate> {
    // target 自身所在块：把 target 也放进分块输入，保证同块记录可比
    let mut input: Vec<MatchRecord> = Vec::with_capacity(all.len() + 1);
    input.push(target.clone());
    input.extend_from_slice(all);
    let blocks = blocking(&input, cluster_keys);
    let mut out = Vec::new();
    for blk in blocks {
        if !blk.iter().any(|r| r.id == target.id) {
            continue;
        }
        for r in blk {
            if r.id == target.id {
                continue;
            }
            let score = compare(target, r, specs);
            let decision = decide(score);
            if decision != Decision::NoMatch {
                out.push(MatchCandidate { record_id: r.id, score, decision });
            }
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.score));
    out
}

/// 取字段字符串值（非字符串类型转字符串表示；Null/缺失 → None）。
fn field_str(fields: &Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(v) => Some(v.to_string()),
    }
}

/// Levenshtein 编辑距离（字符级，两行 DP 空间 O(n)）。
fn levenshtein(a: &str, b: &str) -> u32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len() as u32;
    }
    if b.is_empty() {
        return a.len() as u32;
    }
    let mut prev: Vec<u32> = (0..=b.len() as u32).collect();
    let mut curr = vec![0u32; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i as u32 + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec(id: i64, credit: &str, tax: &str, name: &str) -> MatchRecord {
        let mut fields = Map::new();
        fields.insert("credit_code".into(), json!(credit));
        fields.insert("tax_no".into(), json!(tax));
        fields.insert("name".into(), json!(name));
        MatchRecord { id, fields }
    }

    fn specs() -> Vec<MatchFieldSpec> {
        vec![
            MatchFieldSpec { field: "credit_code".into(), weight: 40, kind: FieldKind::Exact },
            MatchFieldSpec { field: "tax_no".into(), weight: 30, kind: FieldKind::Exact },
            MatchFieldSpec { field: "name".into(), weight: 30, kind: FieldKind::EditDistance },
        ]
    }

    #[test]
    fn blocking_same_credit_code_same_block() {
        let rs = vec![rec(1, "C1", "", "甲"), rec(2, "C1", "", "甲乙"), rec(3, "C2", "", "丙")];
        let blocks = blocking(&rs, &["credit_code", "tax_no", "name"]);
        assert!(blocks.iter().any(|b| b.len() == 2));
    }

    #[test]
    fn blocking_null_key_orphan() {
        let rs = vec![rec(1, "", "", "甲"), rec(2, "", "", "甲"), rec(3, "C1", "", "丙")];
        let blocks = blocking(&rs, &["credit_code", "tax_no", "name"]);
        // 空 credit/tax 走 name 簇键 → 甲甲同块
        assert!(blocks.iter().any(|b| b.len() == 2));
    }

    #[test]
    fn compare_identical_100() {
        let a = rec(1, "C1", "T1", "华东钢铁");
        let b = rec(2, "C1", "T1", "华东钢铁");
        assert_eq!(compare(&a, &b, &specs()), 100);
        assert_eq!(decide(100), Decision::AutoMerge);
    }

    #[test]
    fn compare_name_block_one_char_diff_review() {
        // name 簇键块场景（credit/tax 空被跳过）：name 7 字差 1 → 85 → Review
        let a = rec(1, "", "", "华东钢铁集团");
        let b = rec(2, "", "", "华东钢铁集团公");
        let s = compare(&a, &b, &specs());
        assert!((80..=94).contains(&s), "score={s}");
        assert_eq!(decide(s), Decision::Review);
    }

    #[test]
    fn compare_credit_tax_eq_name_near_automerge() {
        // credit+tax 相等 + name 10 字差 1 → 97 → AutoMerge
        let a = rec(1, "C1", "T1", "华东钢铁集团有限公司");
        let b = rec(2, "C1", "T1", "华东钢铁集团有限公");
        let s = compare(&a, &b, &specs());
        assert!(s >= 95, "score={s}");
        assert_eq!(decide(s), Decision::AutoMerge);
    }

    #[test]
    fn compare_zero_weight_no_panic() {
        let a = rec(1, "C1", "", "甲");
        let b = rec(2, "C1", "", "甲");
        let empty: Vec<MatchFieldSpec> = vec![];
        assert_eq!(compare(&a, &b, &empty), 0);
    }

    #[test]
    fn compare_both_empty_name_no_div_zero() {
        // credit 相等、tax/name 双空被跳过 → 100（同信用代码=同主体）
        let a = rec(1, "C1", "", "");
        let b = rec(2, "C1", "", "");
        let s = compare(&a, &b, &specs());
        assert_eq!(s, 100);
    }
}
