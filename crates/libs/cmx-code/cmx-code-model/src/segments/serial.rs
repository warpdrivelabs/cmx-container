//! ② 流水段（serial）：自增序列，返回 NeedsSerial 交推进器反查 max。

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 流水段 resolver。
///
/// 返回 `NeedsSerial`，由 `rule_algo::evaluate_segments` 交流水推进器（Advance）反查 max + 重试。
pub struct SerialResolver;

#[async_trait(?Send)]
impl SegmentResolver for SerialResolver {
    fn seg_type(&self) -> &str {
        "serial"
    }

    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue> {
        let width = seg
            .width()
            .ok_or_else(|| CodeError::InvalidSegment("serial 段缺 width 参数".into()))?;

        // 重置依据（resetBy）：求值成字符串作为 reset_key
        let reset_key = match seg.reset_by() {
            Some(rb) => evaluate_reset_by(rb, ctx)?,
            None => "_global_".to_string(),
        };

        Ok(SegmentValue::NeedsSerial {
            reset_key,
            width,
            step: seg.step().unwrap_or(1),
            start: seg.start().unwrap_or(1),
            pad_char: seg.pad_char(),
            pad_side: seg.pad_side(),
        })
    }
}

/// 求值 resetBy（编码依据）。
///
/// 支持：
/// - `"date"` → 当前日期 `YYYYMMDD`
/// - `"category+date"` → 分类值 + 日期
/// - `"org_code"` → 组织码
/// - 字段名 → 取该字段值
pub fn evaluate_reset_by(rb: &str, ctx: &ResolveContext) -> Result<String> {
    // 复合依据：用 + 分隔，逐段求值拼接
    let parts: Vec<&str> = rb.split('+').collect();
    let mut result = String::new();
    for part in parts {
        let v = match part {
            "date" => ctx.now.format("%Y%m%d").to_string(),
            "org_code" => ctx.org.org_code.clone(),
            "_global_" => "_global_".to_string(),
            field => ctx
                .attr_str(field)
                .ok_or_else(|| CodeError::RefFieldMissing(field.into()))?
                .to_string(),
        };
        result.push_str(&v);
    }
    Ok(result)
}
