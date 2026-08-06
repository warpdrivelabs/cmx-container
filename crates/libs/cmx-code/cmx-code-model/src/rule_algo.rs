//! 段序列求值主逻辑（rule_algo）。
//!
//! 核心函数：
//! - [`resolve_fixed_segments`]：求固定段前缀（const/date/ref/custom + serial/dateSerial 的 reset_key），用于反查 max。
//! - [`evaluate_segments`]：完整铸号（固定段 + 流水段反查 max + UNIQUE 重试）。
//!
//! ## reset_key 进 prefix（方案 §4.8.1 路径 A）
//!
//! serial/dateSerial 段求值出的 `reset_key`（编码依据，如日期串 `20260805`、分类值 `raw`）
//! 会拼进反查 max 的 prefix，使 `WHERE code LIKE '{prefix}{reset_key}%'` 天然按 reset 维度分组。
//! 这样「按日重置」「按组织重置」「按分类重置」无需改 SQL 签名即可生效。

use crate::advance::Advance;
use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::pad;
use crate::registry::SegmentRegistry;
use crate::spec::{RuleSpec, SegmentSpec, SegmentValue, Target};

/// 最大重试次数（UNIQUE 冲突重试上限）。
const MAX_RETRY: u32 = 8;

/// 全局重置占位符（resetBy 为空时的 reset_key）。
///
/// 该值不拼进 prefix（避免污染全局连续号的 LIKE 模式）——全局连续时 prefix 仅含固定段。
const GLOBAL_RESET: &str = "_global_";

/// 求固定段前缀（含 serial/dateSerial 的 reset_key），用于反查 max 的 `WHERE code LIKE 'prefix%'`。
///
/// - const/date/ref/custom 段：Literal 直接拼
/// - serial/dateSerial 段：把 `reset_key` 拼进前缀（reset_key 非 `_global_` 时），width 部分不拼
/// - random 段：跳过（前缀不含）
///
/// 这样反查 max 的 LIKE 子串天然覆盖 reset 维度分组（方案 §4.8.1 路径 A）。
pub fn resolve_fixed_segments(rule: &RuleSpec, ctx: &ResolveContext) -> Result<String> {
    Ok(build_prefix_and_specs(rule, ctx)?.0)
}

/// 完整铸号：固定段 + 流水段（反查 max + UNIQUE 重试）。
///
/// 对应方案 §4.4 `mint_single`。单条独立铸号（minted_buffer 从 ctx 取）。
///
/// prefix 已含 reset_key（方案 §4.8.1 路径 A），反查 max 天然按 reset 维度分组。
pub async fn evaluate_segments(
    rule: &RuleSpec,
    target: &Target,
    ctx: &ResolveContext,
    advance: &dyn Advance,
) -> Result<String> {
    let (prefix, serial_spec, random_segs) = build_prefix_and_specs(rule, ctx)?;

    // ② 无流水段但有随机段 → 随机段 UNIQUE 冲突重试（方案 §6.3，换种子重试）
    // serial 与 random 互斥（设计文档 §12 的示例里两者从不混用）
    if serial_spec.is_some() && !random_segs.is_empty() {
        return Err(CodeError::InvalidSegment(
            "规则不能同时包含流水段(serial/dateSerial)和随机段(random)".into(),
        ));
    }

    if serial_spec.is_none() {
        if !random_segs.is_empty() {
            return mint_random_code(&prefix, &random_segs, target, advance, ctx).await;
        }
        // 无流水无随机：纯固定码
        return Ok(prefix);
    }

    // ③ 有流水段：反查 max + UNIQUE 重试
    let Some(SegmentValue::NeedsSerial {
        width,
        step,
        start,
        pad_char,
        pad_side,
        ..
    }) = serial_spec
    else {
        // unreachable：② 已处理 serial_spec.is_none()
        return Ok(prefix);
    };
    let effective_enable_gap = ctx.effective_enable_gap(rule.enable_gap);

    for attempt in 1..=MAX_RETRY {
        // 优先取断号（enable_gap=true 且断号表有货）
        if effective_enable_gap {
            if let Some(gap) = advance.take_gap(&prefix, width).await? {
                let code = format!(
                    "{prefix}{}",
                    pad::format_serial(gap, width, pad_char, pad_side)
                );
                match advance.try_insert(target, &code).await {
                    Ok(_) => return Ok(code),
                    Err(_) => continue,
                }
            }
        }

        // 反查 max（带 minted_buffer）
        let max = advance
            .query_max_serial(target, &prefix, width, ctx.minted_buffer())
            .await?;
        let candidate = next_after(max, start, step);
        let code = format!(
            "{prefix}{}",
            pad::format_serial(candidate, width, pad_char, pad_side)
        );

        match advance.try_insert(target, &code).await {
            Ok(_) => return Ok(code),
            Err(_) if attempt < MAX_RETRY => continue,
            Err(e) => return Err(e),
        }
    }
    Err(CodeError::MaxRetryExceeded(MAX_RETRY))
}

/// 构造前缀 + 收集 serial/random 段（resolve_fixed_segments 和 evaluate_segments 的共用逻辑）。
///
/// 返回 `(prefix, serial_spec, random_segs)`：
/// - prefix：固定段 + reset_key + 段间 joiner 拼接（单条/批量/预览共用，保证格式一致）
/// - serial_spec：第一个 serial/dateSerial 段的求值结果（None=无流水段）
/// - random_segs：所有 random 段的声明（换种子重试用）
fn build_prefix_and_specs(
    rule: &RuleSpec,
    ctx: &ResolveContext,
) -> Result<(String, Option<SegmentValue>, Vec<SegmentSpec>)> {
    let registry = SegmentRegistry::new();
    let mut parts: Vec<String> = Vec::new();
    let mut serial_spec: Option<SegmentValue> = None;
    let mut random_segs: Vec<SegmentSpec> = Vec::new();
    let mut ctx = ctx.clone();

    for (idx, seg) in rule.segments.iter().enumerate() {
        let val = registry.resolve(seg, &ctx)?;
        match val {
            SegmentValue::Literal(s) => {
                parts.push(s.clone());
                ctx.resolved_so_far.push(s);
            }
            SegmentValue::NeedsSerial { ref reset_key, .. } => {
                if reset_key != GLOBAL_RESET {
                    parts.push(reset_key.clone());
                }
                if serial_spec.is_none() {
                    serial_spec = Some(val);
                }
            }
            SegmentValue::NeedsUniqueCheck { .. } => {
                random_segs.push(seg.clone());
            }
        }
        // 段间连接符（除最后一段）
        if idx + 1 < rule.segments.len() {
            parts.push(rule.joiner.clone());
        }
    }

    Ok((parts.join(""), serial_spec, random_segs))
}

/// 随机段铸号：固定段前缀 + 随机候选，UNIQUE 冲突重试（方案 §6.3）。
///
/// 与流水段不同：不反查 max（无 max 概念），**每次重新 resolve 随机段换种子**。
async fn mint_random_code(
    prefix: &str,
    random_segs: &[SegmentSpec],
    target: &Target,
    advance: &dyn Advance,
    ctx: &ResolveContext,
) -> Result<String> {
    const MAX_RETRY_RANDOM: u32 = 16;
    let registry = SegmentRegistry::new();

    for attempt in 0..MAX_RETRY_RANDOM {
        // 每次重试重新 resolve 随机段（换种子）
        let mut random_parts: Vec<String> = Vec::new();
        for seg in random_segs {
            if let SegmentValue::NeedsUniqueCheck { candidate } =
                registry.resolve(seg, ctx)?
            {
                random_parts.push(candidate);
            }
        }
        let code = format!("{prefix}{}", random_parts.join(""));

        match advance.try_insert(target, &code).await {
            Ok(_) => return Ok(code),
            Err(_) if attempt + 1 < MAX_RETRY_RANDOM => continue,
            Err(e) => return Err(e),
        }
    }
    Err(CodeError::RandomSpaceExhausted(MAX_RETRY_RANDOM))
}

/// 计算下一个流水号：`start + (max - start) / step * step + step`。
///
/// 考虑 start/step，保证号连续不跳。
pub fn next_after(max: i64, start: i64, step: i64) -> i64 {
    if max < start {
        return start;
    }
    // (max - start) / step * step + step + start
    let offset = max - start;
    let steps = offset / step;
    start + (steps + 1) * step
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advance::StubAdvance;

    #[test]
    fn test_next_after() {
        assert_eq!(next_after(0, 1, 1), 1);
        assert_eq!(next_after(7, 1, 1), 8);
        assert_eq!(next_after(6, 1, 2), 7); // step=2: 1,3,5,7 → max=6 下一个是 7
        assert_eq!(next_after(0, 100, 1), 100); // start=100
    }

    #[tokio::test]
    async fn test_evaluate_simple_serial() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "V"},
                    {"type": "serial", "width": 4}
                ]
            }"#,
        )
        .unwrap();
        let target = Target::dct("bus_partner", "code");
        let ctx = ResolveContext::for_test();
        let advance = StubAdvance;
        let code = evaluate_segments(&rule, &target, &ctx, &advance).await.unwrap();
        assert_eq!(code, "V0001"); // max=0, start=1 → 0001
    }

    #[tokio::test]
    async fn test_evaluate_const_only() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "PREFIX"}
                ]
            }"#,
        )
        .unwrap();
        let target = Target::dct("t", "code");
        let ctx = ResolveContext::for_test();
        let code = evaluate_segments(&rule, &target, &ctx, &StubAdvance).await.unwrap();
        assert_eq!(code, "PREFIX");
    }

    /// reset_key（非全局）拼进 prefix —— 反查 max 的 LIKE 天然含 reset 维度。
    #[test]
    fn test_resolve_fixed_segments_with_reset_key() {
        // dateSerial：日期串应进 prefix
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "FV"},
                    {"type": "dateSerial", "format": "YYYYMMDD", "width": 4}
                ]
            }"#,
        )
        .unwrap();
        let ctx = ResolveContext::for_test();
        let prefix = resolve_fixed_segments(&rule, &ctx).unwrap();
        // dateSerial 的 reset_key = 日期串（YYYYMMDD），应拼进 prefix
        let today = ctx.now.format("%Y%m%d").to_string();
        assert_eq!(prefix, format!("FV{today}"));
    }

    /// 全局 serial（无 resetBy）的 reset_key=_global_，不拼进 prefix。
    #[test]
    fn test_resolve_fixed_segments_global_serial() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "V"},
                    {"type": "serial", "width": 4}
                ]
            }"#,
        )
        .unwrap();
        let ctx = ResolveContext::for_test();
        let prefix = resolve_fixed_segments(&rule, &ctx).unwrap();
        assert_eq!(prefix, "V"); // _global_ 不拼进 prefix
    }

    /// serial + resetBy=字段名：字段值应进 prefix。
    #[test]
    fn test_resolve_fixed_segments_reset_by_field() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "V"},
                    {"type": "serial", "width": 4, "resetBy": "category"}
                ]
            }"#,
        )
        .unwrap();
        let ctx = ResolveContext::for_test().with(serde_json::json!({"category": "raw"}));
        let prefix = resolve_fixed_segments(&rule, &ctx).unwrap();
        assert_eq!(prefix, "Vraw"); // category 值 raw 拼进 prefix
    }

    /// dateSerial 铸号：码应含日期 + 流水。
    #[tokio::test]
    async fn test_evaluate_date_serial() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "FV"},
                    {"type": "dateSerial", "format": "YYYYMMDD", "width": 4}
                ]
            }"#,
        )
        .unwrap();
        let target = Target::doc("cv_header", "doc_no");
        let ctx = ResolveContext::for_test();
        let code = evaluate_segments(&rule, &target, &ctx, &StubAdvance).await.unwrap();
        let today = ctx.now.format("%Y%m%d").to_string();
        assert_eq!(code, format!("FV{today}0001")); // 日期 + 首号 0001
    }

    /// 回归：单条铸号（evaluate_segments）与批量铸号前缀（resolve_fixed_segments）格式必须一致。
    /// 修复 P0 bug：evaluate_segments 曾漏拼 joiner，导致同一规则产出两种码格式。
    #[test]
    fn test_prefix_consistency_single_vs_batch() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "joiner": "-",
                "segments": [
                    {"type": "const", "value": "V"},
                    {"type": "serial", "width": 4}
                ]
            }"#,
        )
        .unwrap();
        let ctx = ResolveContext::for_test();
        let prefix_batch = resolve_fixed_segments(&rule, &ctx).unwrap();
        let (prefix_single, _, _) = build_prefix_and_specs(&rule, &ctx).unwrap();
        assert_eq!(prefix_batch, prefix_single, "单条与批量 prefix 必须一致");
        // const "V" + joiner "-"（serial 的 reset_key=_global_ 不进 prefix，但段间 joiner 保留）
        assert_eq!(prefix_batch, "V-");
    }

    /// random 段不进 prefix（只有 const/date/ref/custom + reset_key 进）。
    #[test]
    fn test_prefix_excludes_random() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "INV"},
                    {"type": "random", "mode": "charset", "width": 6, "charset": "alnum"}
                ]
            }"#,
        )
        .unwrap();
        let ctx = ResolveContext::for_test();
        let prefix = resolve_fixed_segments(&rule, &ctx).unwrap();
        assert_eq!(prefix, "INV"); // random 段不进 prefix
    }

    /// serial + random 混用应报错（互斥）。
    #[tokio::test]
    async fn test_serial_random_mutex() {
        let rule: RuleSpec = serde_json::from_str(
            r#"{
                "segments": [
                    {"type": "const", "value": "X"},
                    {"type": "serial", "width": 4},
                    {"type": "random", "mode": "charset", "width": 4}
                ]
            }"#,
        )
        .unwrap();
        let target = Target::dct("t", "code");
        let ctx = ResolveContext::for_test();
        let result = evaluate_segments(&rule, &target, &ctx, &StubAdvance).await;
        assert!(result.is_err(), "serial + random 混用必须报错");
    }
}
