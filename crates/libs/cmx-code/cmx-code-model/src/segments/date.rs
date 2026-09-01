//! ③ 日期段（date）：日期格式化。

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 日期段 resolver。
pub struct DateResolver;

#[async_trait(?Send)]
impl SegmentResolver for DateResolver {
    fn seg_type(&self) -> &str {
        "date"
    }

    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue> {
        let format = seg
            .get_str("format")
            .ok_or_else(|| CodeError::InvalidSegment("date 段缺 format 参数".into()))?;
        let formatted = format_date(&ctx.now, format)?;
        Ok(SegmentValue::Literal(formatted))
    }
}

/// 日期格式化（支持常见格式串）。
///
/// 支持的格式（借鉴 chrono 但简化为常见编码用例）：
/// - `YYYY` → 2026
/// - `YY` → 26
/// - `YYYYMM` → 202608
/// - `YYMM` → 2608
/// - `YYYYMMDD` → 20260804
/// - `YYMMDD` → 260804
pub fn format_date(now: &chrono::DateTime<chrono::Utc>, format: &str) -> Result<String> {
    let result = match format {
        "YYYY" => now.format("%Y").to_string(),
        "YY" => now.format("%y").to_string(),
        "YYYYMM" => now.format("%Y%m").to_string(),
        "YYMM" => now.format("%y%m").to_string(),
        "YYYYMMDD" => now.format("%Y%m%d").to_string(),
        "YYMMDD" => now.format("%y%m%d").to_string(),
        "MM" => now.format("%m").to_string(),
        "DD" => now.format("%d").to_string(),
        other => {
            return Err(CodeError::InvalidSegment(format!(
                "不支持的日期格式：{other}（支持 YYYY/YY/YYYYMM/YYMM/YYYYMMDD/YYMMDD/MM/DD）"
            )))
        }
    };
    Ok(result)
}
