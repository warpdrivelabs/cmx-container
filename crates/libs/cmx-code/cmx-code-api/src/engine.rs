//! 主引擎：组合 rule_algo + advance，落地 DB。
//!
//! 对外暴露 `mint`（单条铸号）和 `mint_batch`（批量取号）。
//! [`CodeEngine`] 实现 cmx-traits 的 `CodeMinter` trait，供 DCT/DOC 钩子全局调用。

use async_trait::async_trait;
use cmx_code_model::{
    advance::Advance,
    context::ResolveContext,
    error::Result,
    pad, rule_algo,
    spec::{CodeRule, RuleSpec, Target},
};
use cmx_traits::code::CodeMinter;
use std::collections::HashMap;

use crate::store::{rule_store, PgAdvance};

/// 编码引擎实例（实现 CodeMinter trait，供全局注入）。
pub struct CodeEngine;

#[async_trait]
impl CodeMinter for CodeEngine {
    async fn mint(
        &self,
        code_rule: &serde_json::Value,
        target: &serde_json::Value,
        attrs: &serde_json::Value,
        db_id: &str,
        txn_id: Option<&str>,
    ) -> std::result::Result<String, String> {
        mint_via_minter(code_rule, target, attrs, db_id, txn_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn mint_batch(
        &self,
        code_rule: &serde_json::Value,
        target: &serde_json::Value,
        rows: &[serde_json::Value],
        db_id: &str,
        txn_id: Option<&str>,
    ) -> std::result::Result<Vec<String>, String> {
        mint_via_minter_batch(code_rule, target, rows, db_id, txn_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn record_gap_for_code(
        &self,
        code_rule: &serde_json::Value,
        code: &str,
        attrs: &serde_json::Value,
        db_id: &str,
    ) -> bool {
        record_gap_for_code_impl(code_rule, code, attrs, db_id).await
    }
}

/// 反序列化 codeRule + target + 查规则表 + 合并局部覆盖（mint_via_minter / _batch 共用）。
///
/// 返回 `(rule, target, advance)` 三元组，供后续单条/批量铸号使用。
async fn load_rule_for_mint(
    code_rule: &serde_json::Value,
    target: &serde_json::Value,
    db_id: &str,
    txn_id: Option<&str>,
) -> Result<(RuleSpec, Target, PgAdvance)> {
    let cr: CodeRule = serde_json::from_value(code_rule.clone())
        .map_err(|e| cmx_code_model::error::CodeError::Internal(format!("codeRule 反序列化失败：{e}")))?;
    let tgt: Target = serde_json::from_value(target.clone())
        .map_err(|e| cmx_code_model::error::CodeError::Internal(format!("target 反序列化失败：{e}")))?;
    let rule_code = cr.rule_code.as_deref().ok_or_else(|| {
        cmx_code_model::error::CodeError::NoMatchingRule("codeRule 缺 ruleCode".into())
    })?;
    // 铸号时不按 DAM 过滤——ruleCode 全局唯一
    let rule_spec = rule_store::get_rule(rule_code, db_id, &crate::handlers::Dam::default()).await?;
    let rule = cr.merge_with(rule_spec);
    let advance = PgAdvance::new(db_id, txn_id);
    Ok((rule, tgt, advance))
}

/// 删行记断号实现：查规则表 → merge → 判 enable_gap → parse_code_serial → record_gap。
///
/// 返回 true=已记录，false=无需记录（非连号域/无流水段/解析失败）。
/// 失败记 warn 日志不阻断删行主流程。
async fn record_gap_for_code_impl(
    code_rule: &serde_json::Value,
    code: &str,
    attrs: &serde_json::Value,
    db_id: &str,
) -> bool {
    // 反序列化挂载点声明
    let cr: CodeRule = match serde_json::from_value(code_rule.clone()) {
        Ok(cr) => cr,
        Err(e) => {
            tracing::warn!(target: "cmx_code::gap", error = %e, "record_gap: codeRule 反序列化失败");
            return false;
        }
    };
    // 查规则表拿 merge 后的 RuleSpec（含 segments）
    let Some(rule_code) = cr.rule_code.as_deref() else {
        return false;
    };
    let rule_spec = match rule_store::get_rule(rule_code, db_id, &crate::handlers::Dam::default()).await {
        Ok(rs) => rs,
        Err(e) => {
            tracing::warn!(target: "cmx_code::gap", rule_code = rule_code, error = %e, "record_gap: 查规则表失败");
            return false;
        }
    };
    let rule = cr.merge_with(rule_spec);
    // 仅连号域（enable_gap 生效）才记断号
    let effective_gap = attrs
        .get("enableGap")
        .and_then(|v| v.as_bool())
        .unwrap_or(rule.enable_gap);
    if !effective_gap {
        return false;
    }
    // 反解析 code → (prefix, serial_val, width)
    let Some((prefix, serial_val, width)) = rule_algo::parse_code_serial(code, &rule, attrs) else {
        return false;
    };
    // 记录断号（失败不阻断删行）
    if let Err(e) = crate::store::gap_store::record_gap(&prefix, serial_val, width, db_id).await {
        tracing::warn!(
            target: "cmx_code::gap", prefix = %prefix, serial = serial_val, error = %e,
            "record_gap: 写断号表失败（不阻断删行）"
        );
        return false;
    }
    tracing::debug!(
        target: "cmx_code::gap", prefix = %prefix, serial = serial_val, width = width,
        code = code, "record_gap: 删行记断号"
    );
    true
}

/// 内部铸号桥接：Value 参数 → 强类型 → engine.mint。
async fn mint_via_minter(
    code_rule: &serde_json::Value,
    target: &serde_json::Value,
    attrs: &serde_json::Value,
    db_id: &str,
    txn_id: Option<&str>,
) -> Result<String> {
    let (rule, tgt, advance) = load_rule_for_mint(code_rule, target, db_id, txn_id).await?;
    let ctx = ResolveContext::new(db_id, txn_id).with(attrs.clone());
    mint(&rule, &tgt, &ctx, &advance).await
}

/// 批量铸号桥接：Value 参数 → 强类型 → 按 prefix 分组批量取号（方案 §4.5 + §4.1 buffer 推进）。
///
/// **核心机制**（修复附录 C.2.10/C.2.11）：
/// 1. 反序列化 codeRule + 查规则表**一次**（所有行共享规则算法）
/// 2. 对每行算 prefix（resolve_fixed_segments，含 reset_key），按 prefix 分组
/// 3. 同 prefix 组：用 `engine::mint_batch` 一次反查 max 取 N 个连续号（buffer 注入已铸号，
///    保证同事务多行号连续不重，方案 §4.1）
/// 4. 不同 prefix（如不同日期/类型的行）各自独立取号
///
/// **为什么不直接一次 mint_batch 全部**：不同行的 attrs 可能不同（ref 段取不同值），
/// 产生不同 prefix。批量取号要求同 prefix 才能取连续号段，不同 prefix 必须分开取。
async fn mint_via_minter_batch(
    code_rule: &serde_json::Value,
    target: &serde_json::Value,
    rows: &[serde_json::Value],
    db_id: &str,
    txn_id: Option<&str>,
) -> Result<Vec<String>> {
    // 反序列化 codeRule + target + 查规则表（共用 load_rule_for_mint）
    let (rule, tgt, advance) = load_rule_for_mint(code_rule, target, db_id, txn_id).await?;

    let mut results: Vec<Option<String>> = vec![None; rows.len()];

    // 按 prefix 分组：同 prefix 的行索引归一组，保留行序用于结果对齐
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    let mut group_order: Vec<String> = Vec::new(); // 保持首次出现顺序（确定性）

    for (idx, attrs) in rows.iter().enumerate() {
        let ctx = ResolveContext::new(db_id, txn_id).with(attrs.clone());
        let prefix = rule_algo::resolve_fixed_segments(&rule, &ctx)?;
        if !groups.contains_key(&prefix) {
            group_order.push(prefix.clone());
        }
        groups.entry(prefix).or_default().push(idx);
    }

    // H1 修复：跨 prefix 组推进 minted_buffer。
    // use_sequence=false 路径需要 buffer 推进（不同 prefix 组的尾部数字段可能碰撞）；
    // use_sequence=true 路径走序列表取号，天然原子不依赖 buffer，但维护也无害。
    let mut all_minted: Vec<String> = Vec::new();

    // 每个 prefix 组：一次取 N 个连续号
    for prefix in &group_order {
        let indices = &groups[prefix];
        let count = indices.len();
        if count == 0 {
            continue;
        }

        // 用该组第一行的 attrs 构造 ctx（同 prefix 的行 attrs 仅在流水段前的固定段可能不同，
        // 但既然 prefix 相同，固定段求值结果一致，取任一行的 attrs 都可以）
        let first_idx = indices[0];
        let first_attrs = &rows[first_idx];
        let ctx = ResolveContext::new(db_id, txn_id)
            .with(first_attrs.clone())
            .with_minted(&all_minted);

        // 批量取号：use_sequence=true 走 seq_store 原子取号段；false 走反查 max + buffer union
        let codes = mint_batch(&rule, &tgt, &ctx, &advance, count, txn_id).await?;

        // 把号写回结果（按 indices 顺序对齐）+ 推进 buffer（供下一组用）
        for (i, &row_idx) in indices.iter().enumerate() {
            if let Some(code) = codes.get(i) {
                results[row_idx] = Some(code.clone());
                all_minted.push(code.clone());
            }
        }
    }

    // 收集结果：所有行都应铸到号，出现 None 说明 prefix 分组逻辑有 bug
    let final_codes: Vec<String> = results
        .into_iter()
        .map(|opt| opt.ok_or_else(|| {
            cmx_code_model::error::CodeError::Internal("批量铸号：某行未分配到号（prefix 分组异常）".into())
        }))
        .collect::<Result<Vec<_>>>()?;
    Ok(final_codes)
}

/// 单条铸号（方案 §4.4 mint_single）。
pub async fn mint(
    rule: &RuleSpec,
    target: &Target,
    ctx: &ResolveContext,
    advance: &dyn Advance,
) -> Result<String> {
    rule_algo::evaluate_segments(rule, target, ctx, advance).await
}

/// 批量取号（方案 §4.5 batch_generate）。
///
/// 取号路径由 `rule.use_sequence` 决定：
/// - **`use_sequence=true`**（集群安全）：走 `cmx_code_seq` 发号序列表，`FOR UPDATE` 行锁
///   取连续号段，集群并发不重。首启（current_val=0）从业务表探测真实 max 作基线。
///   必须在事务内调用（`txn_id` 非 None），使行锁与落库形成原子段。
/// - **`use_sequence=false`**（默认）：走反查业务表 max + minted_buffer union（老路径，向后兼容）。
///
/// 补位 / 步长 / 起始值 / reset_key 进 prefix 全部与单条铸号（`evaluate_segments`）一致，
/// 保证批量取号与逐条铸号语义统一（方案附录 C.2.3 修复）。
pub async fn mint_batch(
    rule: &RuleSpec,
    target: &Target,
    ctx: &ResolveContext,
    advance: &dyn Advance,
    count: usize,
    txn_id: Option<&str>,
) -> Result<Vec<String>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    // 用 build_prefix_and_specs 一次性拿到 prefix + serial_spec + random_segs，
    // 以区分「纯固定码」「serial 流水」「random-only」三种场景。
    let (prefix, serial_spec, random_segs) = rule_algo::build_prefix_and_specs(rule, ctx)?;
    let width = rule.serial_width();
    let start = rule.serial_start();
    let step = rule.serial_step();
    let pad_char = rule.serial_pad_char();
    let pad_side = rule.serial_pad_side();

    if width == 0 {
        if !random_segs.is_empty() {
            // random-only 规则：逐条铸号（每条 resolve 换种子），不做 UNIQUE 查重
            // （DCT try_insert 是 no-op，靠 DB UNIQUE 约束兜底）。
            let mut codes = Vec::with_capacity(count);
            for _ in 0..count {
                codes.push(rule_algo::evaluate_segments(rule, target, ctx, advance).await?);
            }
            return Ok(codes);
        }
        // 无流水段也无随机段：纯固定码重复 count 份（罕见场景）
        return Ok(vec![prefix; count]);
    }

    let _ = serial_spec; // 已通过 width 判断确认存在，下文直接用 prefix+width 取号

    // ── use_sequence=true：发号序列表原子取号段（集群安全） ──
    if rule.use_sequence() {
        // 首启基线探测：先反查业务表真实 max，传给 seq_store 作为 current_val=0 时的起点。
        let probed_max = advance
            .query_max_serial(target, &prefix, width, ctx.minted_buffer())
            .await?;
        let serials = crate::store::seq_store::alloc_serial_segment(
            &rule.rule_code,
            &prefix,
            count,
            start,
            step,
            probed_max,
            width,
            &ctx.db_id,
            txn_id,
        )
        .await?;
        let codes = serials
            .iter()
            .map(|n| format!("{prefix}{}", pad::format_serial(*n, width, pad_char, pad_side)))
            .collect();
        return Ok(codes);
    }

    // ── use_sequence=false（默认）：反查业务表 max + minted_buffer union ──
    let max = advance
        .query_max_serial(target, &prefix, width, ctx.minted_buffer())
        .await?;

    let codes: Vec<String> = (0..count as i64)
        .map(|i| {
            // 第 i 个号 = next_after(max, start, step) 基础上再走 i 步
            let base = rule_algo::next_after(max, start, step);
            let n = base + i * step;
            format!("{prefix}{}", pad::format_serial(n, width, pad_char, pad_side))
        })
        .collect();

    Ok(codes)
}

/// 预览编码（不落库不占号）。
///
/// 与定稿（`evaluate_segments`）共用前缀构造（含 reset_key）+ `next_after` + `format_serial`，
/// 保证预览码 = 定稿码（方案附录 C.2.4 修复）。
/// 区别：预览不调 `try_insert`（不占号、不触发 UNIQUE 重试）。
pub async fn preview(
    rule: &RuleSpec,
    target: &Target,
    ctx: &ResolveContext,
    advance: &dyn Advance,
) -> Result<String> {
    let (prefix, _serial_spec, random_segs) = rule_algo::build_prefix_and_specs(rule, ctx)?;
    let width = rule.serial_width();
    let start = rule.serial_start();
    let step = rule.serial_step();
    let pad_char = rule.serial_pad_char();
    let pad_side = rule.serial_pad_side();

    if width == 0 {
        if !random_segs.is_empty() {
            // random-only 规则：走 evaluate_segments 生成一条预览码（不落库不占号）
            return rule_algo::evaluate_segments(rule, target, ctx, advance).await;
        }
        return Ok(prefix);
    }

    let max = advance
        .query_max_serial(target, &prefix, width, &[])
        .await?;

    let next = rule_algo::next_after(max, start, step);
    Ok(format!("{prefix}{}", pad::format_serial(next, width, pad_char, pad_side)))
}

/// 创建 PgAdvance（handler 调用入口）。
pub fn pg_advance(db_id: &str, txn_id: Option<&str>) -> PgAdvance {
    PgAdvance::new(db_id, txn_id)
}

